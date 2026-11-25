#!/usr/bin/env python3
"""End-to-end hub verification for my-skills across Claude Code, Codex, and agy.

Runs real headless agents to verify:
  1. Real registry visibility + `my-skills validate` (read-only)
  2. Sandbox edit (v1 -> v2) reflected via post-commit hook
  3. Sandbox new skill (`my-skills new` -> enable -> commit) visible to all agents
  4. Sandbox disable (`my-skills disable` -> commit) removed from all agents
  5. Sandbox agent tasks (edit existing skill, create host-scoped new skill)

Uses Python 3 standard library only. Works on Windows and macOS.
Run with: python -I scripts/hub-check.py [options]
"""

from __future__ import annotations

import argparse
import dataclasses
import datetime as dt
import hashlib
import json
import os
import platform
import re
import secrets
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

ALL_AGENTS: tuple[str, ...] = ("claude", "codex", "agy")
AGENT_LABELS: dict[str, str] = {
    "claude": "Claude",
    "codex": "Codex",
    "agy": "agy",
}

CLAUDE_CHECK_MODEL = "haiku"
CLAUDE_TASK_MODEL = "sonnet"

QUERY_TIMEOUT_S = 90
AGY_SKILLS_TIMEOUT_S = 90
TASK_TIMEOUT_S = 600

TASK_EDIT_PROMPT = (
    "내 스킬 hubcheck-alpha 설명 끝에 '주간 보고 형식도 지원한다'는 내용 한 줄 추가해줘"
)
TASK_NEW_PROMPT = (
    "CSV 열 이름 정리해 주는 새 스킬 hubcheck-gamma 만들어줘. claude랑 codex에서만 쓸 거야"
)


@dataclasses.dataclass
class CmdResult:
    returncode: int
    stdout: str
    stderr: str
    timed_out: bool
    duration_s: float


@dataclasses.dataclass
class CellResult:
    status: str  # "통과" | "실패" | "해당없음"
    reason: str = ""
    note: str = ""
    details: dict[str, Any] = dataclasses.field(default_factory=dict)

    def render(self) -> str:
        if self.status == "실패":
            reason = sanitize_cell_text(self.reason or "알 수 없는 오류")
            if self.note:
                return f"실패: {reason} ({sanitize_cell_text(self.note)})"
            return f"실패: {reason}"
        if self.note:
            return f"{self.status} ({sanitize_cell_text(self.note)})"
        return self.status


def sanitize_cell_text(text: str) -> str:
    clean = " ".join(text.replace("|", "/").splitlines()).strip()
    if len(clean) > 160:
        clean = clean[:157] + "..."
    return clean


# ---------------------------------------------------------------------------
# Environment & filesystem helpers
# ---------------------------------------------------------------------------


def env_get(env: dict[str, str], key: str, default: str = "") -> str:
    if os.name == "nt":
        target = key.upper()
        for k, v in env.items():
            if k.upper() == target:
                return v
        return default
    return env.get(key, default)


def env_set(env: dict[str, str], key: str, value: str) -> None:
    if os.name == "nt":
        target = key.upper()
        for k in [k for k in env if k.upper() == target]:
            del env[k]
    env[key] = value


def env_pop(env: dict[str, str], key: str) -> None:
    if os.name == "nt":
        target = key.upper()
        for k in [k for k in env if k.upper() == target]:
            del env[k]
    else:
        env.pop(key, None)


def strip_herdr(env: dict[str, str]) -> None:
    for k in [k for k in env if k.upper().startswith("HERDR_")]:
        del env[k]


def prepend_path(env: dict[str, str], folder: Path) -> None:
    current = env_get(env, "PATH")
    folder_str = str(folder)
    new_path = f"{folder_str}{os.pathsep}{current}" if current else folder_str
    env_set(env, "PATH", new_path)


def is_reparse_or_link(path: Path) -> bool:
    try:
        if path.is_symlink():
            return True
    except OSError:
        pass
    isjunction = getattr(os.path, "isjunction", None)
    if isjunction is not None:
        try:
            return bool(isjunction(path))
        except OSError:
            return False
    return False


def readlink_safe(path: Path) -> str:
    try:
        return os.readlink(path)
    except OSError:
        return "<unreadable-link>"


def _unlink_links_in_tree(root: Path) -> None:
    """Remove junctions/symlinks without descending into their targets."""
    if not root.exists() and not is_reparse_or_link(root):
        return
    if is_reparse_or_link(root):
        try:
            root.unlink()
        except OSError:
            try:
                os.rmdir(root)
            except OSError:
                pass
        return
    try:
        entries = list(root.iterdir())
    except OSError:
        return
    for entry in entries:
        if is_reparse_or_link(entry):
            try:
                entry.unlink()
            except OSError:
                try:
                    os.rmdir(entry)
                except OSError:
                    pass
        elif entry.is_dir():
            _unlink_links_in_tree(entry)


def make_sandbox_dir(prefix: str = "msb-") -> Path:
    """Create a temp directory whose ACL inherits the parent user's access on Windows.

    Python 3.12+/3.13 `tempfile.mkdtemp()` uses mode 0o700, which on Windows replaces
    the inherited `<USER>:(OI)(CI)(F)` ACE with `OWNER RIGHTS` only. When Codex's
    `workspace-write` sandbox creates files as `CodexSandboxOffline`, those files then
    become `Access is denied` to the parent user. Plain `mkdir()` inherits `%TEMP%`'s
    ACL so both the sandbox user and the parent user keep access.
    """
    if os.name == "nt":
        base = Path(tempfile.gettempdir())
        for _ in range(32):
            candidate = base / f"{prefix}{secrets.token_hex(4)}"
            try:
                candidate.mkdir()
                return candidate.resolve()
            except FileExistsError:
                continue
        raise RuntimeError("임시 샌드박스 디렉터리를 생성하지 못했습니다.")
    return Path(tempfile.mkdtemp(prefix=prefix)).resolve()


def _handle_remove_readonly(func: Any, path: str, _exc: Any) -> None:
    try:
        os.chmod(path, stat.S_IWRITE | stat.S_IREAD)
        func(path)
    except OSError:
        pass


def safe_rmtree(path: Path) -> bool:
    if not path.exists() and not is_reparse_or_link(path):
        return True
    _unlink_links_in_tree(path)
    for attempt in range(4):
        try:
            if sys.version_info >= (3, 12):
                shutil.rmtree(path, onexc=_handle_remove_readonly)
            else:
                shutil.rmtree(path, onerror=_handle_remove_readonly)
        except OSError:
            pass
        if not path.exists():
            return True
        time.sleep(0.5 * (attempt + 1))
    return not path.exists()


def sha256_file(path: Path) -> str:
    if not path.is_file():
        return "<missing>"
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def kill_process_tree(proc: subprocess.Popen[Any]) -> None:
    if proc.poll() is not None:
        return
    if os.name == "nt":
        subprocess.run(
            ["taskkill", "/F", "/T", "/PID", str(proc.pid)],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
    else:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        except OSError:
            proc.kill()


def run_cmd(
    cmd: list[str],
    *,
    cwd: Path,
    env: dict[str, str],
    stdin_text: str | None = None,
    timeout: int = QUERY_TIMEOUT_S,
) -> CmdResult:
    start = time.monotonic()
    popen_kwargs: dict[str, Any] = {
        "cwd": str(cwd),
        "env": env,
        "stdout": subprocess.PIPE,
        "stderr": subprocess.PIPE,
    }
    if stdin_text is None:
        popen_kwargs["stdin"] = subprocess.DEVNULL
    else:
        popen_kwargs["stdin"] = subprocess.PIPE
    if os.name != "nt":
        popen_kwargs["start_new_session"] = True

    proc = subprocess.Popen(cmd, **popen_kwargs)
    timed_out = False
    stdout_bytes = b""
    stderr_bytes = b""
    try:
        input_bytes = stdin_text.encode("utf-8") if stdin_text is not None else None
        stdout_bytes, stderr_bytes = proc.communicate(input=input_bytes, timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        kill_process_tree(proc)
        try:
            stdout_bytes, stderr_bytes = proc.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            stdout_bytes, stderr_bytes = proc.communicate()

    duration = time.monotonic() - start
    return CmdResult(
        returncode=proc.returncode if proc.returncode is not None else -1,
        stdout=stdout_bytes.decode("utf-8", errors="replace"),
        stderr=stderr_bytes.decode("utf-8", errors="replace"),
        timed_out=timed_out,
        duration_s=round(duration, 2),
    )


def save_log(log_dir: Path, name: str, result: CmdResult, ext: str = ".txt") -> Path:
    log_dir.mkdir(parents=True, exist_ok=True)
    out_path = log_dir / f"{name}{ext}"
    out_path.write_text(result.stdout, encoding="utf-8")
    if result.stderr.strip():
        err_path = log_dir / f"{name}.stderr.txt"
        err_path.write_text(result.stderr, encoding="utf-8")
    return out_path


# ---------------------------------------------------------------------------
# Binary resolution
# ---------------------------------------------------------------------------


def resolve_my_skills_bin(bin_arg: str | None) -> Path:
    if bin_arg:
        candidate = Path(bin_arg).expanduser().resolve()
        if not candidate.is_file():
            raise SystemExit(f"오류: --bin 경로를 찾을 수 없습니다: {candidate}")
        return candidate
    found = shutil.which("my-skills")
    if not found:
        raise SystemExit("오류: PATH에서 my-skills 실행 파일을 찾을 수 없습니다.")
    return Path(found).resolve()


def resolve_agent_cmd(agent: str) -> list[str] | None:
    if agent == "claude":
        if os.name == "nt":
            exe = shutil.which("claude.exe")
            if exe:
                return [exe]
        found = shutil.which("claude")
        return [found] if found else None

    if agent == "codex":
        found = shutil.which("codex")
        if not found:
            return None
        if os.name == "nt" and found.lower().endswith((".cmd", ".bat")):
            js = Path(found).parent / "node_modules" / "@openai" / "codex" / "bin" / "codex.js"
            node = shutil.which("node")
            if js.is_file() and node:
                return [node, str(js)]
        return [found]

    if agent == "agy":
        found = shutil.which("agy")
        return [found] if found else None

    return None


def read_agy_model() -> str | None:
    settings_path = Path.home() / ".gemini" / "antigravity-cli" / "settings.json"
    if not settings_path.is_file():
        return None
    try:
        data = json.loads(settings_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    model = data.get("model")
    return model if isinstance(model, str) and model.strip() else None


# ---------------------------------------------------------------------------
# Real environment discovery & safety snapshot
# ---------------------------------------------------------------------------


def make_real_env() -> dict[str, str]:
    env = dict(os.environ)
    env_pop(env, "MY_SKILLS_ROOT")
    env_set(env, "MY_SKILLS_NO_UPDATE_CHECK", "1")
    strip_herdr(env)
    return env


def default_root_cache_path() -> Path:
    xdg = os.environ.get("XDG_CONFIG_HOME")
    base = Path(xdg) if xdg else (Path.home() / ".config")
    return base / "my-skills" / "root"


def default_state_path() -> Path:
    xdg = os.environ.get("XDG_STATE_HOME")
    base = Path(xdg) if xdg else (Path.home() / ".local" / "state")
    return base / "my-skills" / "state.json"


def discover_real_env(my_skills_bin: Path, neutral_dir: Path) -> dict[str, Any]:
    env = make_real_env()
    res = run_cmd([str(my_skills_bin), "doctor", "--quick", "--json"], cwd=neutral_dir, env=env, timeout=60)
    if res.returncode != 0:
        raise RuntimeError(f"my-skills doctor --quick --json 실패 (exit {res.returncode}): {res.stderr.strip()}")
    data = json.loads(res.stdout)
    reg_path_str = (data.get("registry") or {}).get("path")
    if not reg_path_str:
        raise RuntimeError("실제 활성 레지스트리를 찾지 못했습니다 (`my-skills doctor`에 Registry 경로 없음).")

    hosts_info: dict[str, dict[str, Any]] = {}
    defaults = {
        "claude": Path.home() / ".claude" / "skills",
        "codex": Path.home() / ".agents" / "skills",
        "agy": Path.home() / ".gemini" / "config" / "skills",
    }
    for h in data.get("hosts", []):
        name = h.get("name")
        if name in ALL_AGENTS:
            hosts_info[name] = {
                "enabled": bool(h.get("enabled", False)),
                "path": Path(h["path"]) if h.get("path") else defaults[name],
                "configured_mode": h.get("configured_mode"),
            }
    for name, default_path in defaults.items():
        if name not in hosts_info:
            hosts_info[name] = {"enabled": False, "path": default_path, "configured_mode": None}

    return {
        "version": data.get("version", "unknown"),
        "registry_path": Path(reg_path_str),
        "registry_source": (data.get("registry") or {}).get("source"),
        "state_path": Path(data["state_path"]) if data.get("state_path") else default_state_path(),
        "root_cache_path": default_root_cache_path(),
        "hosts": hosts_info,
    }


def snapshot_host_dir(host_dir: Path) -> Any:
    if not host_dir.exists():
        return "<missing>"
    entries: list[list[str]] = []
    for entry in sorted(host_dir.iterdir(), key=lambda p: p.name.lower()):
        if is_reparse_or_link(entry):
            entries.append([entry.name, "link", readlink_safe(entry)])
        elif entry.is_dir():
            entries.append([entry.name, "dir", sha256_file(entry / "SKILL.md")])
        else:
            entries.append([entry.name, "file", sha256_file(entry)])
    return entries


def take_real_snapshot(real_env: dict[str, Any]) -> dict[str, Any]:
    reg: Path = real_env["registry_path"]
    git_env = make_real_env()
    if (reg / ".git").exists():
        head_res = run_cmd(["git", "rev-parse", "HEAD"], cwd=reg, env=git_env, timeout=30)
        status_res = run_cmd(["git", "status", "--porcelain=v1"], cwd=reg, env=git_env, timeout=30)
        reg_head = head_res.stdout.strip() if head_res.returncode == 0 else f"<err:{head_res.returncode}>"
        reg_status = status_res.stdout if status_res.returncode == 0 else f"<err:{status_res.returncode}>"
    else:
        reg_head = "<non-git>"
        reg_status = "<non-git>"

    return {
        "state_json_sha256": sha256_file(real_env["state_path"]),
        "root_cache_sha256": sha256_file(real_env["root_cache_path"]),
        "registry_head": reg_head,
        "registry_git_status": reg_status,
        "hosts": {
            agent: snapshot_host_dir(real_env["hosts"][agent]["path"])
            for agent in ALL_AGENTS
        },
    }


def diff_snapshots(before: dict[str, Any], after: dict[str, Any]) -> list[str]:
    diffs: list[str] = []
    for key in ("state_json_sha256", "root_cache_sha256", "registry_head", "registry_git_status"):
        if before.get(key) != after.get(key):
            diffs.append(f"{key}: {before.get(key)!r} -> {after.get(key)!r}")
    for agent in ALL_AGENTS:
        b = before["hosts"].get(agent)
        a = after["hosts"].get(agent)
        if b != a:
            diffs.append(f"hosts.{agent}: 항목 변경 감지 (전 {len(b) if isinstance(b, list) else b}, 후 {len(a) if isinstance(a, list) else a})")
            if isinstance(b, list) and isinstance(a, list):
                b_set = {tuple(x) for x in b}
                a_set = {tuple(x) for x in a}
                added = sorted(a_set - b_set)
                removed = sorted(b_set - a_set)
                if added:
                    diffs.append(f"  + {added[:5]}")
                if removed:
                    diffs.append(f"  - {removed[:5]}")
    return diffs


# ---------------------------------------------------------------------------
# Sandbox management
# ---------------------------------------------------------------------------


class Sandbox:
    def __init__(self, my_skills_bin: Path, real_registry: Path) -> None:
        self.my_skills_bin = my_skills_bin
        self.real_registry = real_registry
        self.root = make_sandbox_dir(prefix="msb-")
        self.registry = self.root / "registry"
        self.proj = self.root / "proj"
        self.remote = self.root / "remote.git"
        self.blackhole = self.root / "blackhole.git"
        self.xdg_config = self.root / "xdg-config"
        self.xdg_state = self.root / "xdg-state"
        self.gitconfig = self.root / "gitconfig"
        self.fakehome = self.root / "fakehome"

        self.host_dirs: dict[str, Path] = {
            "claude": self.proj / ".claude" / "skills",
            "codex": self.proj / ".agents" / "skills",
            "agy": self.proj / ".agent" / "skills",
        }
        self._init_layout()

    def base_env(self) -> dict[str, str]:
        env = dict(os.environ)
        env_set(env, "XDG_CONFIG_HOME", str(self.xdg_config))
        env_set(env, "XDG_STATE_HOME", str(self.xdg_state))
        env_set(env, "MY_SKILLS_NO_UPDATE_CHECK", "1")
        env_set(env, "GIT_CONFIG_GLOBAL", str(self.gitconfig))
        env_pop(env, "MY_SKILLS_ROOT")
        strip_herdr(env)
        prepend_path(env, self.my_skills_bin.parent)
        return env

    def agy_env(self) -> dict[str, str]:
        env = self.base_env()
        env_set(env, "HOME", str(self.fakehome))
        env_set(env, "USERPROFILE", str(self.fakehome))
        return env

    def run_git(self, args: list[str], cwd: Path | None = None) -> CmdResult:
        res = run_cmd(["git", *args], cwd=cwd or self.registry, env=self.base_env(), timeout=60)
        if res.returncode != 0:
            raise RuntimeError(
                f"git {' '.join(args)} 실패 (exit {res.returncode}): {res.stderr.strip() or res.stdout.strip()}"
            )
        return res

    def run_my_skills(self, args: list[str], cwd: Path | None = None, check: bool = True) -> CmdResult:
        res = run_cmd([str(self.my_skills_bin), *args], cwd=cwd or self.registry, env=self.base_env(), timeout=60)
        if check and res.returncode != 0:
            raise RuntimeError(
                f"my-skills {' '.join(args)} 실패 (exit {res.returncode}): {res.stderr.strip() or res.stdout.strip()}"
            )
        return res

    def _init_layout(self) -> None:
        for d in (
            self.registry / "skills",
            self.proj,
            self.remote,
            self.blackhole,
            self.xdg_config,
            self.xdg_state,
            self.fakehome / ".gemini" / "antigravity-cli",
        ):
            d.mkdir(parents=True, exist_ok=True)

        # Copy non-secret agy config files into fakehome (settings.json + GEMINI.md).
        real_home = Path.home()
        real_settings = real_home / ".gemini" / "antigravity-cli" / "settings.json"
        if real_settings.is_file():
            shutil.copy2(real_settings, self.fakehome / ".gemini" / "antigravity-cli" / "settings.json")
        real_gemini_md = real_home / ".gemini" / "GEMINI.md"
        if real_gemini_md.is_file():
            shutil.copy2(real_gemini_md, self.fakehome / ".gemini" / "GEMINI.md")

        # Write sandbox gitconfig: fallback user, include real ~/.gitconfig, redirect all remote pushes to blackhole.
        real_gitconfig = real_home / ".gitconfig"
        lines = [
            "[user]",
            "\tname = HubCheck",
            "\temail = hubcheck@example.invalid",
        ]
        if real_gitconfig.is_file():
            lines.extend(["[include]", f"\tpath = {real_gitconfig.as_posix()}"])
        lines.extend(
            [
                "[safe]",
                f"\tdirectory = {self.registry.as_posix()}",
                f"\tdirectory = {self.remote.as_posix()}",
                f"\tdirectory = {self.proj.as_posix()}",
                f'[url "{self.blackhole.as_posix()}/"]',
                "\tpushInsteadOf = https://",
                "\tpushInsteadOf = ssh://",
                "\tpushInsteadOf = git@",
            ]
        )
        self.gitconfig.write_text("\n".join(lines) + "\n", encoding="utf-8")

        # Bare origin & blackhole repos.
        self.run_git(["init", "--bare", str(self.remote)], cwd=self.root)
        self.run_git(["init", "--bare", str(self.blackhole)], cwd=self.root)

        # Project git repo (agent cwd).
        self.run_git(["init", "-b", "main"], cwd=self.proj)
        (self.proj / "README.md").write_text("# HubCheck Sandbox Project\n", encoding="utf-8")
        self.run_git(["add", "README.md"], cwd=self.proj)
        self.run_git(["commit", "-m", "chore: init sandbox project"], cwd=self.proj)

        # Registry git repo.
        self.run_git(["init", "-b", "main"], cwd=self.registry)
        (self.registry / ".gitignore").write_text(
            "my-skills.local.toml\nlocal/\n.DS_Store\n__pycache__/\n*.pyc\n",
            encoding="utf-8",
        )
        (self.registry / "my-skills.toml").write_text(
            'schema_version = 1\n'
            'skills_root = "skills"\n\n'
            '[defaults]\n'
            'install_mode = "link"\n'
            'collision = "error"\n'
            'verify_after_install = true\n\n'
            '[targets.claude]\n'
            'enabled = true\n'
            'scope = "user"\n\n'
            '[targets.codex]\n'
            'enabled = true\n'
            'scope = "user"\n\n'
            '[targets.agy]\n'
            'enabled = true\n'
            'scope = "user"\n',
            encoding="utf-8",
        )
        # Local override keeps tracked my-skills.toml clean so post-commit hook runs.
        (self.registry / "my-skills.local.toml").write_text(
            f'[targets.claude]\n'
            f'enabled = true\n'
            f'path = {json.dumps(str(self.host_dirs["claude"]))}\n'
            f'install_mode = "link"\n\n'
            f'[targets.codex]\n'
            f'enabled = true\n'
            f'path = {json.dumps(str(self.host_dirs["codex"]))}\n'
            f'install_mode = "link"\n\n'
            f'[targets.agy]\n'
            f'enabled = true\n'
            f'path = {json.dumps(str(self.host_dirs["agy"]))}\n'
            f'install_mode = "copy"\n',
            encoding="utf-8",
        )

        # Copy real registry AGENTS.md for realistic agent guidance.
        commit_files = [".gitignore", "my-skills.toml"]
        real_agents_md = self.real_registry / "AGENTS.md"
        if real_agents_md.is_file():
            shutil.copy2(real_agents_md, self.registry / "AGENTS.md")
            commit_files.append("AGENTS.md")

        self.run_git(["add", *commit_files], cwd=self.registry)
        self.run_git(["commit", "-m", "chore: init sandbox registry"], cwd=self.registry)
        self.run_git(["remote", "add", "origin", self.remote.as_posix()], cwd=self.registry)
        self.run_git(["push", "-u", "origin", "main"], cwd=self.registry)

        # Cache sandbox registry as active root and install git hooks in sandbox registry only.
        self.run_my_skills(["set-root", str(self.registry)], cwd=self.root)
        self.run_my_skills(["hooks", "install"], cwd=self.registry)

    def setup_s1_alpha(self) -> None:
        """Create and enable hubcheck-alpha (hubcheck-token-v1), commit (hook installs), and push."""
        self.run_my_skills(
            [
                "new",
                "hubcheck-alpha",
                "--description",
                "일일 노트를 정리하는 허브 점검용 알파 스킬 (hubcheck-token-v1).",
            ]
        )
        self.run_my_skills(["enable", "hubcheck-alpha"])
        self.run_git(["add", "my-skills.toml", "skills/hubcheck-alpha"])
        self.run_git(["commit", "-m", "feat: add hubcheck-alpha v1"])
        self.run_git(["push", "origin", "main"])

    def install_agy_seed_skill(self, tool_repo_root: Path) -> None:
        """Place the my-skills seed skill in agy fakehome user-scope skills for Row 5 tasks."""
        seed_src = tool_repo_root / "skills" / "my-skills"
        if not seed_src.is_dir():
            raise RuntimeError(f"my-skills 시드 원본 폴더가 없습니다: {seed_src}")
        dest = self.fakehome / ".gemini" / "config" / "skills" / "my-skills"
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(seed_src, dest)

    def head_sha(self) -> str:
        return self.run_git(["rev-parse", "HEAD"]).stdout.strip()

    def remote_head_sha(self) -> str:
        res = run_cmd(
            ["git", "rev-parse", "refs/heads/main"],
            cwd=self.remote,
            env=self.base_env(),
            timeout=30,
        )
        return res.stdout.strip() if res.returncode == 0 else ""

    def cleanup(self) -> bool:
        return safe_rmtree(self.root)


# ---------------------------------------------------------------------------
# Agent runners and parsers
# ---------------------------------------------------------------------------


def parse_claude_stream(stdout: str) -> dict[str, Any]:
    init_skills: list[str] = []
    init_seen = False
    result_text = ""
    assistant_chunks: list[str] = []
    is_error = False

    for line in stdout.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            ev = json.loads(line)
        except json.JSONDecodeError:
            continue
        ev_type = ev.get("type")
        if ev_type == "system" and ev.get("subtype") == "init":
            init_seen = True
            skills = ev.get("skills")
            if isinstance(skills, list):
                init_skills = [str(s) for s in skills]
        elif ev_type == "assistant":
            msg = ev.get("message") or {}
            content = msg.get("content")
            if isinstance(content, list):
                for block in content:
                    if isinstance(block, dict) and block.get("type") == "text":
                        text = block.get("text")
                        if isinstance(text, str):
                            assistant_chunks.append(text)
        elif ev_type == "result":
            if isinstance(ev.get("result"), str):
                result_text = ev["result"]
            if ev.get("is_error"):
                is_error = True

    final_text = result_text or "\n".join(assistant_chunks)
    return {
        "init_seen": init_seen,
        "init_skills": init_skills,
        "final_text": final_text,
        "is_error": is_error,
    }


def query_claude(
    agent_cmd: list[str],
    prompt: str,
    cwd: Path,
    env: dict[str, str],
    log_dir: Path,
    log_name: str,
    timeout: int = QUERY_TIMEOUT_S,
) -> tuple[CmdResult, dict[str, Any]]:
    cmd = [
        *agent_cmd,
        "-p",
        "--model",
        CLAUDE_CHECK_MODEL,
        "--output-format",
        "stream-json",
        "--verbose",
        "--no-session-persistence",
        "--strict-mcp-config",
        "--",
        prompt,
    ]
    res = run_cmd(cmd, cwd=cwd, env=env, stdin_text=None, timeout=timeout)
    save_log(log_dir, log_name, res, ext=".jsonl")
    return res, parse_claude_stream(res.stdout)


def run_claude_task(
    agent_cmd: list[str],
    prompt: str,
    sb: Sandbox,
    log_dir: Path,
    log_name: str,
    timeout: int = TASK_TIMEOUT_S,
) -> CmdResult:
    cmd = [
        *agent_cmd,
        "-p",
        "--model",
        CLAUDE_TASK_MODEL,
        "--permission-mode",
        "acceptEdits",
        "--allowedTools",
        "Bash,PowerShell,Edit,Write,Read,Glob,Grep,Skill",
        "--add-dir",
        str(sb.root),
        "--strict-mcp-config",
        "--no-session-persistence",
        "--output-format",
        "stream-json",
        "--verbose",
        "--",
        prompt,
    ]
    res = run_cmd(cmd, cwd=sb.proj, env=sb.base_env(), stdin_text=None, timeout=timeout)
    save_log(log_dir, log_name, res, ext=".jsonl")
    return res


def parse_codex_jsonl(stdout: str) -> dict[str, Any]:
    messages: list[str] = []
    errors: list[str] = []
    for line in stdout.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            ev = json.loads(line)
        except json.JSONDecodeError:
            continue
        ev_type = ev.get("type")
        if ev_type == "item.completed":
            item = ev.get("item") or {}
            if item.get("type") == "agent_message":
                text = item.get("text")
                if isinstance(text, str):
                    messages.append(text)
        elif ev_type == "error":
            msg = ev.get("message")
            if isinstance(msg, str):
                errors.append(msg)
        elif ev_type == "turn.failed":
            err = (ev.get("error") or {}).get("message")
            if isinstance(err, str):
                errors.append(err)
    return {
        "messages": messages,
        "final_text": messages[-1] if messages else "",
        "errors": errors,
    }


def query_codex(
    agent_cmd: list[str],
    prompt: str,
    cwd: Path,
    env: dict[str, str],
    log_dir: Path,
    log_name: str,
    skip_git_check: bool = False,
    timeout: int = QUERY_TIMEOUT_S,
) -> tuple[CmdResult, dict[str, Any]]:
    cmd = [
        *agent_cmd,
        "exec",
        "--json",
        "--ephemeral",
        "--sandbox",
        "read-only",
        "-C",
        str(cwd),
        "-c",
        'model_reasoning_effort="low"',
        "-c",
        "notify=[]",
    ]
    if skip_git_check:
        cmd.append("--skip-git-repo-check")
    cmd.append("-")
    res = run_cmd(cmd, cwd=cwd, env=env, stdin_text=prompt + "\n", timeout=timeout)
    save_log(log_dir, log_name, res, ext=".jsonl")
    return res, parse_codex_jsonl(res.stdout)


def run_codex_task(
    agent_cmd: list[str],
    prompt: str,
    sb: Sandbox,
    log_dir: Path,
    log_name: str,
    timeout: int = TASK_TIMEOUT_S,
) -> CmdResult:
    cmd = [
        *agent_cmd,
        "exec",
        "--json",
        "--ephemeral",
        "--sandbox",
        "workspace-write",
        "-C",
        str(sb.proj),
        "--add-dir",
        str(sb.root),
        "-c",
        "notify=[]",
        "-",
    ]
    res = run_cmd(cmd, cwd=sb.proj, env=sb.base_env(), stdin_text=prompt + "\n", timeout=timeout)
    save_log(log_dir, log_name, res, ext=".jsonl")
    return res


def parse_agy_skills(stdout: str) -> dict[str, str]:
    skills: dict[str, str] = {}
    for raw in stdout.splitlines():
        line = raw.strip()
        if not line or "\t" not in line:
            continue
        name, desc = line.split("\t", 1)
        name = name.strip()
        if name:
            skills[name] = desc.strip()
    return skills


def query_agy_skills(
    agent_cmd: list[str],
    cwd: Path,
    env: dict[str, str],
    log_dir: Path,
    log_name: str,
    timeout: int = AGY_SKILLS_TIMEOUT_S,
) -> tuple[CmdResult, dict[str, str]]:
    last_res: CmdResult | None = None
    for attempt in range(4):
        res = run_cmd([*agent_cmd, "-p", "/skills"], cwd=cwd, env=env, stdin_text=None, timeout=timeout)
        last_res = res
        combined = f"{res.stdout}\n{res.stderr}".lower()
        if res.returncode == 0 and parse_agy_skills(res.stdout):
            save_log(log_dir, log_name, res, ext=".txt")
            return res, parse_agy_skills(res.stdout)
        if "database is locked" in combined and attempt < 3:
            time.sleep(2 * (attempt + 1))
            continue
        break
    assert last_res is not None
    save_log(log_dir, log_name, last_res, ext=".txt")
    return last_res, parse_agy_skills(last_res.stdout)


def run_agy_task(
    agent_cmd: list[str],
    prompt: str,
    sb: Sandbox,
    agy_model: str | None,
    log_dir: Path,
    log_name: str,
    timeout: int = TASK_TIMEOUT_S,
) -> CmdResult:
    cmd = [
        *agent_cmd,
        "--dangerously-skip-permissions",
        "--output-format",
        "stream-json",
    ]
    if agy_model:
        cmd.extend(["--model", agy_model])
    cmd.extend(["-p", prompt])

    last_res: CmdResult | None = None
    for attempt in range(3):
        res = run_cmd(cmd, cwd=sb.proj, env=sb.agy_env(), stdin_text=None, timeout=timeout)
        last_res = res
        combined = f"{res.stdout}\n{res.stderr}".lower()
        if res.returncode != 0 and "database is locked" in combined and attempt < 2:
            time.sleep(2 * (attempt + 1))
            continue
        break
    assert last_res is not None
    save_log(log_dir, log_name, last_res, ext=".jsonl")
    return last_res


def text_has_token(text: str, token: str) -> bool:
    pattern = rf"(?<![A-Za-z0-9_-]){re.escape(token)}(?![A-Za-z0-9_-])"
    return re.search(pattern, text) is not None


# ---------------------------------------------------------------------------
# Row 1: Real registry (read-only)
# ---------------------------------------------------------------------------


def run_row1_real(
    agents: list[str],
    agent_cmds: dict[str, list[str] | None],
    my_skills_bin: Path,
    real_env: dict[str, Any],
    neutral_dir: Path,
    log_dir: Path,
) -> tuple[dict[str, CellResult], dict[str, Any]]:
    env = make_real_env()
    reg_path = str(real_env["registry_path"])

    # 1. `my-skills validate --json`
    val_res = run_cmd(
        [str(my_skills_bin), "--root", reg_path, "validate", "--json"],
        cwd=neutral_dir,
        env=env,
        timeout=60,
    )
    save_log(log_dir, "row1-validate", val_res, ext=".json")
    validate_issues: list[str] = []
    if val_res.returncode != 0:
        try:
            val_data = json.loads(val_res.stdout)
        except json.JSONDecodeError:
            validate_issues.append(f"validate exit {val_res.returncode}")
            val_data = {}
    else:
        val_data = json.loads(val_res.stdout)

    for item in val_data.get("skills", []):
        sk = item.get("skill", "?")
        for err in item.get("errors", []):
            validate_issues.append(f"{sk} 오류: {err}")
        for warn in item.get("warnings", []):
            validate_issues.append(f"{sk} 경고: {warn}")

    # 2. Expected enabled skills per agent from `my-skills skills --enabled --json`
    sk_res = run_cmd(
        [str(my_skills_bin), "--root", reg_path, "skills", "--enabled", "--json"],
        cwd=neutral_dir,
        env=env,
        timeout=60,
    )
    save_log(log_dir, "row1-skills", sk_res, ext=".json")
    if sk_res.returncode != 0:
        raise RuntimeError(f"my-skills skills --enabled --json 실패: {sk_res.stderr.strip()}")
    sk_data = json.loads(sk_res.stdout)

    expected_by_agent: dict[str, list[str]] = {a: [] for a in ALL_AGENTS}
    for row in sk_data.get("skills", []):
        if not row.get("enabled"):
            continue
        name = row["name"]
        hosts = row.get("hosts") or []
        for agent in ALL_AGENTS:
            if not real_env["hosts"][agent]["enabled"]:
                continue
            if not hosts or agent in hosts:
                expected_by_agent[agent].append(name)

    results: dict[str, CellResult] = {}
    for agent in ALL_AGENTS:
        if agent not in agents:
            results[agent] = CellResult("해당없음", note="옵션 제외")
            continue
        if not real_env["hosts"][agent]["enabled"]:
            results[agent] = CellResult("해당없음", note="대상 비활성")
            continue
        cmd = agent_cmds.get(agent)
        if not cmd:
            results[agent] = CellResult("해당없음", note="CLI 미설치")
            continue

        expected = sorted(expected_by_agent[agent])
        missing: list[str] = []
        extra_err = ""

        if agent == "claude":
            res, parsed = query_claude(
                cmd,
                "OK라고만 답해.",
                neutral_dir,
                env,
                log_dir,
                "row1-claude",
            )
            if res.timed_out:
                extra_err = "시간 초과"
            elif not parsed["init_seen"]:
                extra_err = f"init 이벤트 없음 (exit {res.returncode})"
            else:
                seen = set(parsed["init_skills"])
                missing = [s for s in expected if s not in seen]

        elif agent == "codex":
            prompt = (
                "네가 쓸 수 있는 스킬 이름을 빠짐없이 한 줄에 하나씩만 출력해. "
                "생략하거나 요약하지 마."
            )
            for attempt in (1, 2):
                res, parsed = query_codex(
                    cmd,
                    prompt,
                    neutral_dir,
                    env,
                    log_dir,
                    f"row1-codex-try{attempt}",
                    skip_git_check=True,
                )
                if res.timed_out:
                    extra_err = "시간 초과"
                    continue
                if res.returncode != 0 and not parsed["final_text"]:
                    extra_err = f"codex 실패 (exit {res.returncode})"
                    continue
                extra_err = ""
                missing = [s for s in expected if not text_has_token(parsed["final_text"], s)]
                if not missing:
                    break

        elif agent == "agy":
            res, skills_map = query_agy_skills(cmd, neutral_dir, env, log_dir, "row1-agy")
            if res.timed_out:
                extra_err = "시간 초과"
            elif res.returncode != 0 and not skills_map:
                extra_err = f"agy /skills 실패 (exit {res.returncode})"
            else:
                missing = [s for s in expected if s not in skills_map]

        reasons: list[str] = []
        if extra_err:
            reasons.append(extra_err)
        if missing:
            preview = ", ".join(missing[:4]) + (f" 외 {len(missing) - 4}개" if len(missing) > 4 else "")
            reasons.append(f"누락 {len(missing)}/{len(expected)}개 ({preview})")
        if validate_issues:
            reasons.append(f"validate 문제 {len(validate_issues)}건 ({validate_issues[0]})")

        if reasons:
            results[agent] = CellResult(
                "실패",
                reason="; ".join(reasons),
                details={"expected": len(expected), "missing": missing, "validate_issues": validate_issues},
            )
        else:
            results[agent] = CellResult(
                "통과",
                details={"expected": len(expected), "validate_issues": []},
            )

    meta = {
        "validate_issues": validate_issues,
        "expected_counts": {a: len(expected_by_agent[a]) for a in ALL_AGENTS},
    }
    return results, meta


# ---------------------------------------------------------------------------
# Rows 2-4: Sandbox edit, new, disable via git hooks
# ---------------------------------------------------------------------------


def _check_agent_hubcheck_desc(
    agent: str,
    cmd: list[str],
    sb: Sandbox,
    log_dir: Path,
    log_prefix: str,
    skill_name: str,
    expected_token: str,
    forbidden_token: str | None = None,
) -> tuple[bool, str]:
    prompt = "이름이 hubcheck-로 시작하는 스킬을 `이름: 설명` 형식으로 한 줄씩만 출력해."

    if agent == "claude":
        for attempt in (1, 2):
            res, parsed = query_claude(
                cmd, prompt, sb.proj, sb.base_env(), log_dir, f"{log_prefix}-claude-try{attempt}"
            )
            if res.timed_out:
                if attempt == 2:
                    return False, "시간 초과"
                continue
            if not parsed["init_seen"]:
                return False, f"init 이벤트 없음 (exit {res.returncode})"
            if skill_name not in parsed["init_skills"]:
                return False, f"init.skills에 {skill_name} 없음"
            txt = parsed["final_text"]
            has_new = expected_token in txt
            has_old = bool(forbidden_token and forbidden_token in txt)
            if has_new and not has_old:
                return True, ""
            if attempt == 2:
                if has_old:
                    return False, f"이전 표식({forbidden_token})이 여전히 보임"
                return False, f"응답에 {expected_token} 없음"
        return False, "확인 실패"

    if agent == "codex":
        for attempt in (1, 2):
            res, parsed = query_codex(
                cmd, prompt, sb.proj, sb.base_env(), log_dir, f"{log_prefix}-codex-try{attempt}"
            )
            if res.timed_out:
                if attempt == 2:
                    return False, "시간 초과"
                continue
            if res.returncode != 0 and not parsed["final_text"]:
                if attempt == 2:
                    return False, f"codex 실패 (exit {res.returncode})"
                continue
            txt = parsed["final_text"]
            has_skill = text_has_token(txt, skill_name)
            has_new = expected_token in txt
            has_old = bool(forbidden_token and forbidden_token in txt)
            if has_skill and has_new and not has_old:
                return True, ""
            if attempt == 2:
                if not has_skill:
                    return False, f"{skill_name} 미발견"
                if has_old:
                    return False, f"이전 표식({forbidden_token})이 여전히 보임"
                return False, f"응답에 {expected_token} 없음"
        return False, "확인 실패"

    if agent == "agy":
        res, skills_map = query_agy_skills(cmd, sb.proj, sb.agy_env(), log_dir, f"{log_prefix}-agy")
        if res.timed_out:
            return False, "시간 초과"
        if res.returncode != 0 and not skills_map:
            return False, f"agy /skills 실패 (exit {res.returncode})"
        if skill_name not in skills_map:
            return False, f"/skills에 {skill_name} 없음"
        desc = skills_map[skill_name]
        if expected_token not in desc:
            return False, f"/skills 설명에 {expected_token} 없음"
        if forbidden_token and forbidden_token in desc:
            return False, f"/skills 설명에 이전 표식({forbidden_token}) 남음"
        return True, ""

    return False, f"알 수 없는 에이전트: {agent}"


def _check_agent_skill_presence(
    agent: str,
    cmd: list[str],
    sb: Sandbox,
    log_dir: Path,
    log_prefix: str,
    skill_name: str,
    should_exist: bool,
) -> tuple[bool, str]:
    if agent == "claude":
        res, parsed = query_claude(
            cmd, "OK라고만 답해.", sb.proj, sb.base_env(), log_dir, f"{log_prefix}-claude"
        )
        if res.timed_out:
            return False, "시간 초과"
        if not parsed["init_seen"]:
            return False, f"init 이벤트 없음 (exit {res.returncode})"
        present = skill_name in parsed["init_skills"]
        if present == should_exist:
            return True, ""
        return False, (f"{skill_name} 미발견" if should_exist else f"{skill_name}가 여전히 보임")

    if agent == "codex":
        prompt = (
            "지금 네가 쓸 수 있는 스킬 중 이름이 hubcheck-로 시작하는 스킬 이름만 "
            "한 줄에 하나씩 출력해. 설명이나 다른 문장은 쓰지 말고, 없으면 NONE만 출력해."
        )
        for attempt in (1, 2):
            res, parsed = query_codex(
                cmd, prompt, sb.proj, sb.base_env(), log_dir, f"{log_prefix}-codex-try{attempt}"
            )
            if res.timed_out:
                if attempt == 2:
                    return False, "시간 초과"
                continue
            if res.returncode != 0 and not parsed["final_text"]:
                if attempt == 2:
                    return False, f"codex 실패 (exit {res.returncode})"
                continue
            present = text_has_token(parsed["final_text"], skill_name)
            if present == should_exist:
                return True, ""
            if attempt == 2:
                return False, (f"{skill_name} 미발견" if should_exist else f"{skill_name}가 여전히 보임")
        return False, "확인 실패"

    if agent == "agy":
        res, skills_map = query_agy_skills(cmd, sb.proj, sb.agy_env(), log_dir, f"{log_prefix}-agy")
        if res.timed_out:
            return False, "시간 초과"
        if res.returncode != 0 and not skills_map:
            return False, f"agy /skills 실패 (exit {res.returncode})"
        present = skill_name in skills_map
        if present == should_exist:
            return True, ""
        return False, (f"{skill_name} 미발견" if should_exist else f"{skill_name}가 여전히 보임")

    return False, f"알 수 없는 에이전트: {agent}"


def run_rows2_to_4_sandbox(
    agents: list[str],
    agent_cmds: dict[str, list[str] | None],
    my_skills_bin: Path,
    real_env: dict[str, Any],
    log_dir: Path,
    keep: bool,
) -> tuple[dict[str, CellResult], dict[str, CellResult], dict[str, CellResult], list[Path]]:
    kept: list[Path] = []
    row2: dict[str, CellResult] = {}
    row3: dict[str, CellResult] = {}
    row4: dict[str, CellResult] = {}

    active_agents: list[str] = []
    for agent in ALL_AGENTS:
        if agent not in agents:
            for r in (row2, row3, row4):
                r[agent] = CellResult("해당없음", note="옵션 제외")
        elif not agent_cmds.get(agent):
            for r in (row2, row3, row4):
                r[agent] = CellResult("해당없음", note="CLI 미설치")
        else:
            active_agents.append(agent)

    if not active_agents:
        return row2, row3, row4, kept

    sb = Sandbox(my_skills_bin, real_env["registry_path"])
    try:
        # S1: setup hubcheck-alpha (hubcheck-token-v1)
        try:
            sb.setup_s1_alpha()
        except Exception as exc:
            msg = f"셋업 실패 ({exc})"
            for agent in active_agents:
                for r in (row2, row3, row4):
                    r[agent] = CellResult("실패", reason=msg)
            return row2, row3, row4, kept

        # Verify S1 visibility for each active agent.
        s1_ready: list[str] = []
        for agent in active_agents:
            cmd = agent_cmds[agent]
            assert cmd is not None
            ok, err = _check_agent_hubcheck_desc(
                agent, cmd, sb, log_dir, "s1", "hubcheck-alpha", "hubcheck-token-v1"
            )
            if ok:
                s1_ready.append(agent)
            else:
                msg = f"셋업 실패 ({err})"
                for r in (row2, row3, row4):
                    r[agent] = CellResult("실패", reason=msg)

        if not s1_ready:
            return row2, row3, row4, kept

        # S2 (Row 2): Edit canonical hubcheck-alpha v1 -> v2, commit (post-commit refreshes copy).
        alpha_md = sb.registry / "skills" / "hubcheck-alpha" / "SKILL.md"
        updated = alpha_md.read_text(encoding="utf-8").replace("hubcheck-token-v1", "hubcheck-token-v2")
        alpha_md.write_text(updated, encoding="utf-8")
        sb.run_git(["add", "skills/hubcheck-alpha/SKILL.md"])
        c2 = sb.run_git(["commit", "-m", "chore: bump hubcheck-alpha to v2"])
        save_log(log_dir, "row2-commit", c2, ext=".txt")

        for agent in s1_ready:
            cmd = agent_cmds[agent]
            assert cmd is not None
            ok, err = _check_agent_hubcheck_desc(
                agent,
                cmd,
                sb,
                log_dir,
                "row2",
                "hubcheck-alpha",
                "hubcheck-token-v2",
                forbidden_token="hubcheck-token-v1",
            )
            if ok and agent == "agy":
                copy_md = sb.host_dirs["agy"] / "hubcheck-alpha" / "SKILL.md"
                if not copy_md.is_file() or "hubcheck-token-v2" not in copy_md.read_text(encoding="utf-8"):
                    ok, err = False, "agy copy 파일이 v2로 갱신되지 않음 (post-commit 누락)"
            row2[agent] = CellResult("통과") if ok else CellResult("실패", reason=err)

        # S3 (Row 3): `my-skills new hubcheck-beta`, fill description, enable, commit.
        sb.run_my_skills(["new", "hubcheck-beta"])
        beta_md = sb.registry / "skills" / "hubcheck-beta" / "SKILL.md"
        beta_text = beta_md.read_text(encoding="utf-8")
        beta_text = re.sub(
            r'description:\s*".*?"',
            'description: "Hubcheck beta verification skill (hubcheck-beta-v1)."',
            beta_text,
            count=1,
        )
        beta_md.write_text(beta_text, encoding="utf-8")
        sb.run_my_skills(["enable", "hubcheck-beta"])
        sb.run_git(["add", "my-skills.toml", "skills/hubcheck-beta"])
        c3 = sb.run_git(["commit", "-m", "feat: add hubcheck-beta"])
        save_log(log_dir, "row3-commit", c3, ext=".txt")

        for agent in s1_ready:
            cmd = agent_cmds[agent]
            assert cmd is not None
            ok, err = _check_agent_skill_presence(
                agent, cmd, sb, log_dir, "row3", "hubcheck-beta", should_exist=True
            )
            row3[agent] = CellResult("통과") if ok else CellResult("실패", reason=err)

        # S4 (Row 4): `my-skills disable hubcheck-alpha`, commit.
        sb.run_my_skills(["disable", "hubcheck-alpha"])
        sb.run_git(["add", "my-skills.toml"])
        c4 = sb.run_git(["commit", "-m", "chore: disable hubcheck-alpha"])
        save_log(log_dir, "row4-commit", c4, ext=".txt")

        for agent in s1_ready:
            cmd = agent_cmds[agent]
            assert cmd is not None
            ok, err = _check_agent_skill_presence(
                agent, cmd, sb, log_dir, "row4", "hubcheck-alpha", should_exist=False
            )
            row4[agent] = CellResult("통과") if ok else CellResult("실패", reason=err)

    finally:
        if keep:
            kept.append(sb.root)
        else:
            sb.cleanup()

    return row2, row3, row4, kept


# ---------------------------------------------------------------------------
# Row 5: Agent tasks (end-state checkers only)
# ---------------------------------------------------------------------------


def _run_agent_task(
    agent: str,
    cmd: list[str],
    prompt: str,
    sb: Sandbox,
    agy_model: str | None,
    log_dir: Path,
    log_name: str,
) -> CmdResult:
    if agent == "claude":
        return run_claude_task(cmd, prompt, sb, log_dir, log_name)
    if agent == "codex":
        return run_codex_task(cmd, prompt, sb, log_dir, log_name)
    if agent == "agy":
        return run_agy_task(cmd, prompt, sb, agy_model, log_dir, log_name)
    raise ValueError(f"unknown agent: {agent}")


def _check_common_task_state(
    sb: Sandbox,
    skill_name: str,
    base_head: str,
) -> tuple[list[str], dict[str, Any]]:
    failures: list[str] = []
    details: dict[str, Any] = {}

    # 1. `my-skills status <skill> --json` all FRESH or UNSUPPORTED
    st_res = sb.run_my_skills(["status", skill_name, "--json"], check=False)
    status_map: dict[str, str] = {}
    if st_res.returncode != 0:
        failures.append(f"status 실패 (exit {st_res.returncode})")
    else:
        try:
            rows = json.loads(st_res.stdout)
            for row in rows:
                status_map[row["host"]] = row["status"]
        except (json.JSONDecodeError, KeyError) as exc:
            failures.append(f"status JSON 파싱 실패 ({exc})")
    details["status"] = status_map
    bad_hosts = [f"{h}={st}" for h, st in sorted(status_map.items()) if st not in ("FRESH", "UNSUPPORTED")]
    if bad_hosts:
        failures.append(f"status 미완료 ({', '.join(bad_hosts)})")

    # 2. Related changes committed
    after_head = sb.head_sha()
    details["base_head"] = base_head
    details["after_head"] = after_head
    if after_head == base_head:
        failures.append("관련 변경이 커밋되지 않음")

    porcelain = sb.run_git(["status", "--porcelain=v1", "--", f"skills/{skill_name}", "my-skills.toml"]).stdout.strip()
    details["related_dirty"] = porcelain
    if porcelain:
        failures.append("관련 파일에 미커밋 변경 남음")

    # 3. Record push status (informational only)
    remote_head = sb.remote_head_sha()
    pushed = bool(after_head and remote_head and after_head == remote_head and after_head != base_head)
    details["pushed"] = pushed
    return failures, details


def check_task_edit(sb: Sandbox, base_head: str, run_res: CmdResult) -> CellResult:
    if run_res.timed_out:
        return CellResult("실패", reason="시간 초과", details={"duration_s": run_res.duration_s})

    failures: list[str] = []
    details: dict[str, Any] = {"duration_s": run_res.duration_s, "exit_code": run_res.returncode}

    val_res = sb.run_my_skills(["validate", "hubcheck-alpha", "--json"], check=False)
    if val_res.returncode != 0:
        failures.append("validate 실패")

    sk_res = sb.run_my_skills(["skills", "hubcheck-alpha", "--json"], check=False)
    desc = ""
    if sk_res.returncode != 0:
        failures.append("hubcheck-alpha 조회 실패")
    else:
        try:
            rows = json.loads(sk_res.stdout).get("skills", [])
            if rows:
                desc = rows[0].get("description", "")
        except json.JSONDecodeError:
            failures.append("skills JSON 파싱 실패")
    details["description"] = desc

    has_ko = "주간 보고" in desc and ("형식" in desc or "지원" in desc)
    has_en = "weekly report" in desc.lower()
    if not (has_ko or has_en):
        failures.append("정본 description에 요청 문구 없음")

    common_failures, common_details = _check_common_task_state(sb, "hubcheck-alpha", base_head)
    failures.extend(common_failures)
    details.update(common_details)

    # Ensure all 3 hosts are specifically FRESH (hubcheck-alpha targets all 3).
    status_map = details.get("status", {})
    non_fresh = [h for h in ALL_AGENTS if status_map.get(h) != "FRESH"]
    if non_fresh and not any("status" in f for f in failures):
        failures.append(f"FRESH 아님 ({', '.join(non_fresh)})")

    pushed_label = "push: 예" if details.get("pushed") else "push: 아니오"
    if failures:
        return CellResult("실패", reason="; ".join(failures), note=pushed_label, details=details)
    return CellResult("통과", note=pushed_label, details=details)


def check_task_new(sb: Sandbox, base_head: str, run_res: CmdResult) -> CellResult:
    if run_res.timed_out:
        return CellResult("실패", reason="시간 초과", details={"duration_s": run_res.duration_s})

    failures: list[str] = []
    details: dict[str, Any] = {"duration_s": run_res.duration_s, "exit_code": run_res.returncode}

    sk_res = sb.run_my_skills(["skills", "--json"], check=False)
    gamma: dict[str, Any] | None = None
    if sk_res.returncode != 0:
        failures.append("skills 조회 실패")
    else:
        try:
            for row in json.loads(sk_res.stdout).get("skills", []):
                if row.get("name") == "hubcheck-gamma":
                    gamma = row
                    break
        except json.JSONDecodeError:
            failures.append("skills JSON 파싱 실패")

    if gamma is None:
        failures.append("매니페스트에 hubcheck-gamma 미등록")
        return CellResult("실패", reason="; ".join(failures), details=details)

    hosts = set(gamma.get("hosts") or [])
    enabled = bool(gamma.get("enabled"))
    desc = str(gamma.get("description") or "").strip()
    details["hosts"] = sorted(hosts)
    details["enabled"] = enabled
    details["description"] = desc

    if hosts != {"claude", "codex"}:
        failures.append(f"hosts 불일치 ({sorted(hosts)})")
    if not enabled:
        failures.append("스킬이 비활성 상태")

    val_res = sb.run_my_skills(["validate", "hubcheck-gamma", "--json"], check=False)
    if val_res.returncode != 0:
        failures.append("validate 실패")

    if not desc or "todo" in desc.lower():
        failures.append("description에 TODO 남음 또는 비어 있음")
    elif "csv" not in desc.lower():
        failures.append("description에 CSV 내용 없음")

    # Host filesystem checks
    claude_ok = (sb.host_dirs["claude"] / "hubcheck-gamma" / "SKILL.md").is_file()
    codex_ok = (sb.host_dirs["codex"] / "hubcheck-gamma" / "SKILL.md").is_file()
    agy_absent = not (sb.host_dirs["agy"] / "hubcheck-gamma").exists() and not is_reparse_or_link(
        sb.host_dirs["agy"] / "hubcheck-gamma"
    )
    details["host_files"] = {"claude": claude_ok, "codex": codex_ok, "agy_absent": agy_absent}
    if not claude_ok:
        failures.append("claude 대상에 연결 없음")
    if not codex_ok:
        failures.append("codex 대상에 연결 없음")
    if not agy_absent:
        failures.append("agy 대상에 불필요한 연결 존재")

    common_failures, common_details = _check_common_task_state(sb, "hubcheck-gamma", base_head)
    failures.extend(common_failures)
    details.update(common_details)

    status_map = details.get("status", {})
    if status_map:
        if status_map.get("claude") != "FRESH" or status_map.get("codex") != "FRESH":
            if not any("status" in f for f in failures):
                failures.append(f"claude/codex FRESH 아님 ({status_map})")
        if status_map.get("agy") != "UNSUPPORTED":
            if not any("status" in f for f in failures):
                failures.append(f"agy가 UNSUPPORTED 아님 ({status_map.get('agy')})")

    pushed_label = "push: 예" if details.get("pushed") else "push: 아니오"
    if failures:
        return CellResult("실패", reason="; ".join(failures), note=pushed_label, details=details)
    return CellResult("통과", note=pushed_label, details=details)


def run_row5_tasks(
    agents: list[str],
    agent_cmds: dict[str, list[str] | None],
    my_skills_bin: Path,
    real_env: dict[str, Any],
    agy_model: str | None,
    tool_repo_root: Path,
    log_dir: Path,
    keep: bool,
) -> tuple[dict[str, CellResult], dict[str, CellResult], list[Path]]:
    kept: list[Path] = []
    row5a: dict[str, CellResult] = {}
    row5b: dict[str, CellResult] = {}

    for agent in ALL_AGENTS:
        if agent not in agents:
            row5a[agent] = CellResult("해당없음", note="옵션 제외")
            row5b[agent] = CellResult("해당없음", note="옵션 제외")
            continue
        cmd = agent_cmds.get(agent)
        if not cmd:
            row5a[agent] = CellResult("해당없음", note="CLI 미설치")
            row5b[agent] = CellResult("해당없음", note="CLI 미설치")
            continue

        # Task 5a: Edit existing skill (S1 sandbox)
        print(f"  - 5a(수정) {agent} 실행 중...", file=sys.stderr, flush=True)
        sb_edit = Sandbox(my_skills_bin, real_env["registry_path"])
        try:
            sb_edit.setup_s1_alpha()
            if agent == "agy":
                sb_edit.install_agy_seed_skill(tool_repo_root)
            base_head = sb_edit.head_sha()
            res = _run_agent_task(
                agent, cmd, TASK_EDIT_PROMPT, sb_edit, agy_model, log_dir, f"row5a-edit-{agent}"
            )
            row5a[agent] = check_task_edit(sb_edit, base_head, res)
        except Exception as exc:
            row5a[agent] = CellResult("실패", reason=f"실행 예외: {exc}")
        finally:
            if keep:
                kept.append(sb_edit.root)
            else:
                sb_edit.cleanup()
        print(f"    -> {row5a[agent].render()}", file=sys.stderr, flush=True)

        # Task 5b: Create new skill (empty sandbox)
        print(f"  - 5b(새 스킬) {agent} 실행 중...", file=sys.stderr, flush=True)
        sb_new = Sandbox(my_skills_bin, real_env["registry_path"])
        try:
            if agent == "agy":
                sb_new.install_agy_seed_skill(tool_repo_root)
            base_head = sb_new.head_sha()
            res = _run_agent_task(
                agent, cmd, TASK_NEW_PROMPT, sb_new, agy_model, log_dir, f"row5b-new-{agent}"
            )
            row5b[agent] = check_task_new(sb_new, base_head, res)
        except Exception as exc:
            row5b[agent] = CellResult("실패", reason=f"실행 예외: {exc}")
        finally:
            if keep:
                kept.append(sb_new.root)
            else:
                sb_new.cleanup()
        print(f"    -> {row5b[agent].render()}", file=sys.stderr, flush=True)

    return row5a, row5b, kept


# ---------------------------------------------------------------------------
# Report rendering & CLI entrypoint
# ---------------------------------------------------------------------------


def render_markdown(
    *,
    timestamp: str,
    my_skills_bin: Path,
    my_skills_version: str,
    real_registry: Path,
    agy_model: str | None,
    log_dir: Path,
    rows: list[tuple[str, str, dict[str, CellResult]]],
    snapshot_diffs: list[str],
    kept_sandboxes: list[Path],
) -> str:
    os_label = f"{platform.system().lower()}-{platform.machine().lower()}"
    agy_task_label = agy_model or "기본"
    lines = [
        "# my-skills 허브 점검 결과",
        "",
        f"- 시각: {timestamp}",
        f"- OS: `{os_label}`",
        f"- `my-skills`: `{my_skills_version}` (`{my_skills_bin}`)",
        f"- 실제 레지스트리(읽기 전용): `{real_registry}`",
        (
            f"- 모델: Claude `{CLAUDE_CHECK_MODEL}`(1~4줄) / `{CLAUDE_TASK_MODEL}`(5줄), "
            f"Codex 기본(1~4줄 `low` / 5줄 기본), "
            f"agy `/skills`(1~4줄) / `{agy_task_label}`(5줄)"
        ),
        f"- 원시 로그: `{log_dir}`",
    ]
    if kept_sandboxes:
        lines.append("- 보존된 샌드박스: " + ", ".join(f"`{p}`" for p in kept_sandboxes))
    lines.extend(
        [
            "",
            "| 줄 | 점검 항목 | Claude | Codex | agy |",
            "|---|---|---|---|---|",
        ]
    )

    counts = {"통과": 0, "실패": 0, "해당없음": 0}
    for row_id, label, cells in rows:
        for agent in ALL_AGENTS:
            counts[cells[agent].status] += 1
        lines.append(
            f"| {row_id} | {label} | "
            f"{cells['claude'].render()} | {cells['codex'].render()} | {cells['agy'].render()} |"
        )

    lines.append("")
    if snapshot_diffs:
        lines.append("## 실제 상태 변경 감지")
        lines.append("")
        for d in snapshot_diffs:
            lines.append(f"- {d}")
        lines.append("")
        safety_note = "실제 상태 변경 감지!"
    else:
        safety_note = "실제 상태 변경 없음"

    lines.append(
        f"요약: 통과 {counts['통과']} / 실패 {counts['실패']} / 해당없음 {counts['해당없음']} ({safety_note})"
    )
    return "\n".join(lines) + "\n"


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="실제 에이전트(Claude, Codex, agy)로 my-skills 허브 동작을 점검합니다."
    )
    parser.add_argument("--real-only", action="store_true", help="1줄(실제 레지스트리 읽기 전용)만 실행")
    parser.add_argument("--skip-tasks", action="store_true", help="5줄(에이전트 과제 2종)을 건너뜀")
    parser.add_argument(
        "--agents",
        default="claude,codex,agy",
        help="점검할 에이전트 목록(쉼표 구분, 기본: claude,codex,agy)",
    )
    parser.add_argument(
        "--bin",
        dest="bin_path",
        default=None,
        help="샌드박스에서 쓸 my-skills 바이너리 경로(기본: PATH의 my-skills)",
    )
    parser.add_argument("--out", default=None, help="결과 Markdown을 저장할 파일 경로")
    parser.add_argument("--keep", action="store_true", help="종료 후 샌드박스 폴더를 지우지 않고 남김")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    if hasattr(sys.stdout, "reconfigure"):
        try:
            sys.stdout.reconfigure(encoding="utf-8", errors="replace")
            sys.stderr.reconfigure(encoding="utf-8", errors="replace")
        except OSError:
            pass

    args = parse_args(argv if argv is not None else sys.argv[1:])

    selected_agents = [a.strip().lower() for a in args.agents.split(",") if a.strip()]
    unknown = [a for a in selected_agents if a not in ALL_AGENTS]
    if unknown or not selected_agents:
        print(f"오류: 알 수 없는 에이전트 지정: {unknown or args.agents} (허용: {', '.join(ALL_AGENTS)})", file=sys.stderr)
        return 2

    tool_repo_root = Path(__file__).resolve().parent.parent
    my_skills_bin = resolve_my_skills_bin(args.bin_path)
    agent_cmds = {a: resolve_agent_cmd(a) for a in ALL_AGENTS}
    agy_model = read_agy_model()

    timestamp = dt.datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    stamp_slug = dt.datetime.now().strftime("%Y%m%d-%H%M%S")
    out_path = Path(args.out).expanduser().resolve() if args.out else None
    if out_path is not None:
        log_dir = out_path.parent / f"{out_path.stem}-logs"
    else:
        log_dir = Path(tempfile.gettempdir()) / f"hubcheck-logs-{stamp_slug}"
    log_dir.mkdir(parents=True, exist_ok=True)

    neutral_dir = make_sandbox_dir(prefix="msb-neutral-")
    real_env: dict[str, Any] | None = None
    before_snap: dict[str, Any] | None = None
    after_snap: dict[str, Any] | None = None
    snapshot_diffs: list[str] = []
    kept_sandboxes: list[Path] = []
    rows: list[tuple[str, str, dict[str, CellResult]]] = []
    row1_meta: dict[str, Any] = {}

    try:
        real_env = discover_real_env(my_skills_bin, neutral_dir)
        before_snap = take_real_snapshot(real_env)

        # Row 1: Real registry (read-only)
        print("[1/5] 실제 레지스트리 점검 중 (읽기 전용)...", file=sys.stderr, flush=True)
        row1, row1_meta = run_row1_real(
            selected_agents, agent_cmds, my_skills_bin, real_env, neutral_dir, log_dir
        )
        rows.append(("1", "저장소 스킬 표시 + validate (실제, 읽기 전용)", row1))

        if args.real_only:
            na = {a: CellResult("해당없음", note="--real-only") for a in ALL_AGENTS}
            rows.append(("2", "정본 수정(v1→v2) 커밋 후 반영", dict(na)))
            rows.append(("3", "새 스킬(`my-skills new`) 커밋 후 표시", dict(na)))
            rows.append(("4", "비활성화(`my-skills disable`) 커밋 후 제거", dict(na)))
            rows.append(("5a", "에이전트 과제: 스킬 수정 (`hubcheck-alpha`)", dict(na)))
            rows.append(("5b", "에이전트 과제: 새 스킬 생성 (`hubcheck-gamma`)", dict(na)))
        else:
            print("[2-4/5] 샌드박스 수정·생성·비활성화 점검 중...", file=sys.stderr, flush=True)
            row2, row3, row4, kept_24 = run_rows2_to_4_sandbox(
                selected_agents, agent_cmds, my_skills_bin, real_env, log_dir, args.keep
            )
            kept_sandboxes.extend(kept_24)
            rows.append(("2", "정본 수정(v1→v2) 커밋 후 반영", row2))
            rows.append(("3", "새 스킬(`my-skills new`) 커밋 후 표시", row3))
            rows.append(("4", "비활성화(`my-skills disable`) 커밋 후 제거", row4))

            if args.skip_tasks:
                na_t = {a: CellResult("해당없음", note="--skip-tasks") for a in ALL_AGENTS}
                rows.append(("5a", "에이전트 과제: 스킬 수정 (`hubcheck-alpha`)", dict(na_t)))
                rows.append(("5b", "에이전트 과제: 새 스킬 생성 (`hubcheck-gamma`)", dict(na_t)))
            else:
                print("[5/5] 샌드박스 에이전트 과제 2종 점검 중...", file=sys.stderr, flush=True)
                row5a, row5b, kept_5 = run_row5_tasks(
                    selected_agents,
                    agent_cmds,
                    my_skills_bin,
                    real_env,
                    agy_model,
                    tool_repo_root,
                    log_dir,
                    args.keep,
                )
                kept_sandboxes.extend(kept_5)
                rows.append(("5a", "에이전트 과제: 스킬 수정 (`hubcheck-alpha`)", row5a))
                rows.append(("5b", "에이전트 과제: 새 스킬 생성 (`hubcheck-gamma`)", row5b))
    finally:
        safe_rmtree(neutral_dir)
        if real_env is not None and before_snap is not None:
            after_snap = take_real_snapshot(real_env)
            snapshot_diffs = diff_snapshots(before_snap, after_snap)

    assert real_env is not None
    md = render_markdown(
        timestamp=timestamp,
        my_skills_bin=my_skills_bin,
        my_skills_version=real_env["version"],
        real_registry=real_env["registry_path"],
        agy_model=agy_model,
        log_dir=log_dir,
        rows=rows,
        snapshot_diffs=snapshot_diffs,
        kept_sandboxes=kept_sandboxes,
    )

    summary_payload = {
        "timestamp": timestamp,
        "my_skills_bin": str(my_skills_bin),
        "my_skills_version": real_env["version"],
        "real_registry": str(real_env["registry_path"]),
        "agy_model": agy_model,
        "row1_meta": row1_meta,
        "snapshot_before": before_snap,
        "snapshot_after": after_snap,
        "snapshot_diffs": snapshot_diffs,
        "rows": [
            {
                "id": rid,
                "label": label,
                "cells": {a: dataclasses.asdict(cells[a]) for a in ALL_AGENTS},
            }
            for rid, label, cells in rows
        ],
    }
    (log_dir / "summary.json").write_text(
        json.dumps(summary_payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )

    if out_path is not None:
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(md, encoding="utf-8")

    print(md, end="")

    if snapshot_diffs:
        banner = (
            "\n"
            "============================================================\n"
            "  !!! 실제 상태 변경 감지 (자동 복구하지 않음) !!!\n"
            "============================================================\n"
            + "\n".join(f"  - {d}" for d in snapshot_diffs)
            + "\n"
        )
        print(banner, file=sys.stderr)
        return 2

    has_failure = any(
        cells[a].status == "실패" for _, _, cells in rows for a in ALL_AGENTS
    )
    return 1 if has_failure else 0


if __name__ == "__main__":
    raise SystemExit(main())
