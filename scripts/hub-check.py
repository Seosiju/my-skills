#!/usr/bin/env python3
"""Verify real-registry visibility, sandbox Git hooks and agent tasks.
Run with Python 3 on Windows or macOS: python -I scripts/hub-check.py [options].
"""

from __future__ import annotations

import argparse
import dataclasses
import datetime as dt
import hashlib
import json
import os
import platform
import secrets
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
from contextlib import contextmanager, suppress
from pathlib import Path
from typing import Any

ALL_AGENTS: tuple[str, ...] = ("claude", "codex", "agy")
CLAUDE_CHECK_MODEL = "haiku"
CLAUDE_TASK_MODEL = "sonnet"
AGY_SETTINGS = Path(".gemini/antigravity-cli/settings.json")
QUERY_TIMEOUT_S = 90
TASK_TIMEOUT_S = 600
TASK_EDIT_PROMPT = "내 스킬 hubcheck-alpha 설명(description) 끝에 '주간 보고 형식도 지원한다'는 내용 한 줄 추가해줘"
TASK_NEW_PROMPT = "CSV 열 이름 정리해 주는 새 스킬 hubcheck-gamma 만들어줘. claude랑 codex에서만 쓸 거야"


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
        text = self.status
        if self.status == "실패":
            text += f": {sanitize_cell_text(self.reason or '알 수 없는 오류')}"
        return f"{text} ({sanitize_cell_text(self.note)})" if self.note else text


def sanitize_cell_text(text: str) -> str:
    clean = " ".join(text.replace("|", "/").splitlines()).strip()
    return clean[:157] + "..." if len(clean) > 160 else clean

ROW_LABELS = {
    "1": "저장소 스킬이 다 보이나 (실제, 읽기 전용)",
    "2": "정본 수정(v1→v2) 커밋 후 반영",
    "3": "새 스킬(`my-skills new`) 커밋 후 표시",
    "4": "비활성화(`my-skills disable`) 커밋 후 제거",
    "5a": "에이전트 과제: 스킬 수정 (`hubcheck-alpha`)",
    "5b": "에이전트 과제: 새 스킬 생성 (`hubcheck-gamma`)",
}
ALPHA, BETA, GAMMA = "hubcheck-alpha", "hubcheck-beta", "hubcheck-gamma"
V1, V2 = "hubcheck-token-v1", "hubcheck-token-v2"
DESC_PROMPT = "이름이 hubcheck-로 시작하는 스킬을 `이름: 설명` 형식으로 한 줄씩만 출력해."
LIST_PROMPT = (
    "지금 네가 쓸 수 있는 스킬 중 이름이 hubcheck-로 시작하는 스킬 이름만 "
    "한 줄에 하나씩 출력해. 설명이나 다른 문장은 쓰지 말고, 없으면 NONE만 출력해."
)
REAL_LIST_PROMPT = (
    "네가 쓸 수 있는 스킬 이름을 빠짐없이 한 줄에 하나씩만 출력해. 생략하거나 요약하지 마."
)
OK_PROMPT = "OK라고만 답해."


def clean_env() -> dict[str, str]:
    # os.environ already normalizes Windows keys to uppercase.
    return {k: v for k, v in os.environ.items()
            if not k.upper().startswith(("GIT_", "HERDR_")) and k.upper() != "MY_SKILLS_ROOT"}


def is_reparse_or_link(path: Path) -> bool:
    with suppress(FileNotFoundError, NotADirectoryError):
        info = path.lstat()
        return stat.S_ISLNK(info.st_mode) or bool(
            getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT
        )
    return False

def _unlink_links_in_tree(path: Path) -> None:
    """Unlink junctions and symlinks before recursive deletion."""
    if is_reparse_or_link(path):
        try:
            path.unlink()
        except OSError:
            path.rmdir()
    elif path.is_dir():
        for entry in path.iterdir():
            _unlink_links_in_tree(entry)

def make_sandbox_dir(prefix: str = "msb-") -> Path:
    if os.name != "nt":
        return Path(tempfile.mkdtemp(prefix=prefix)).resolve()
    # mkdir inherits the parent ACL so files written by CodexSandboxOffline
    # can be cleaned up by the caller. mkdtemp uses a restrictive ACL on 3.12+.
    for _ in range(32):
        path = Path(tempfile.gettempdir()) / f"{prefix}{secrets.token_hex(4)}"
        try:
            path.mkdir()
        except FileExistsError:
            continue
        return path.resolve()
    raise RuntimeError("임시 샌드박스 디렉터리를 생성하지 못했습니다.")

def _handle_remove_readonly(func, path, _exc):
    if not is_reparse_or_link(Path(path)):
        os.chmod(path, stat.S_IWRITE | stat.S_IREAD)
    func(path)

def safe_rmtree(path: Path) -> bool:
    if path.parent != Path(tempfile.gettempdir()).resolve() or not path.name.startswith("msb-"):
        raise ValueError(f"샌드박스 밖 삭제 경로: {path}")
    handler = "onexc" if sys.version_info >= (3, 12) else "onerror"
    for attempt in range(4):
        with suppress(OSError):
            _unlink_links_in_tree(path)
            if path.exists():
                shutil.rmtree(path, **{handler: _handle_remove_readonly})
        if not path.exists() and not is_reparse_or_link(path):
            return True
        time.sleep(0.5 * (attempt + 1))
    return False


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
            ["taskkill", "/F", "/T", "/PID", str(proc.pid)], check=False,
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
    else:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            return
        except OSError:
            proc.kill()

def run_cmd(cmd, *, cwd, env, stdin_text=None, timeout=QUERY_TIMEOUT_S) -> CmdResult:
    start = time.monotonic()
    proc = subprocess.Popen(cmd, cwd=cwd, env=env, start_new_session=os.name != "nt",
        stdin=subprocess.DEVNULL if stdin_text is None else subprocess.PIPE,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    payload = stdin_text.encode("utf-8") if stdin_text is not None else None
    timed_out = False
    try:
        out, err = proc.communicate(payload, timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        kill_process_tree(proc)
        try:
            out, err = proc.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            out, err = proc.communicate()
    return CmdResult(proc.returncode, out.decode("utf-8", "replace"),
                     err.decode("utf-8", "replace"), timed_out, round(time.monotonic() - start, 2))

def save_log(log_dir, name, result, ext=".txt"):
    (log_dir / f"{name}{ext}").write_text(result.stdout, encoding="utf-8")
    if result.stderr.strip():
        (log_dir / f"{name}.stderr.txt").write_text(result.stderr, encoding="utf-8")

def resolve_my_skills_bin(bin_arg):
    found = bin_arg or shutil.which("my-skills")
    if not found:
        raise SystemExit("오류: PATH에서 my-skills 실행 파일을 찾을 수 없습니다.")
    path = Path(found).expanduser().resolve()
    if not path.is_file():
        raise SystemExit(f"오류: --bin 경로를 찾을 수 없습니다: {path}")
    return path

def resolve_agent_cmd(agent):
    preferred = shutil.which("claude.exe") if agent == "claude" and os.name == "nt" else None
    found = preferred or shutil.which(agent)
    if not found:
        return None
    if agent == "codex" and os.name == "nt" and found.lower().endswith((".cmd", ".bat")):
        js = Path(found).parent / "node_modules" / "@openai" / "codex" / "bin" / "codex.js"
        node = shutil.which("node")
        if js.is_file() and node:
            return [node, str(js)]
    return [found]

def read_agy_model() -> str | None:
    settings = Path.home() / AGY_SETTINGS
    with suppress(OSError, ValueError, AttributeError):
        model = json.loads(settings.read_text(encoding="utf-8")).get("model")
        if isinstance(model, str) and model.strip():
            return model
    return None


def make_real_env():
    return dict(clean_env(), MY_SKILLS_NO_UPDATE_CHECK="1", GIT_OPTIONAL_LOCKS="0")

def discover_real_env(binary, neutral):
    env = make_real_env()
    res = run_cmd([str(binary), "doctor", "--quick", "--json"], cwd=neutral, env=env, timeout=60)
    if res.returncode or res.timed_out:
        raise RuntimeError(f"my-skills doctor --quick --json 실패 (exit {res.returncode}): {res.stderr.strip()}")
    data = json.loads(res.stdout)
    root = (data.get("registry") or {}).get("path")
    if not root:
        raise RuntimeError("실제 활성 레지스트리를 찾지 못했습니다 (doctor에 Registry 경로 없음).")
    home = Path.home()
    defaults = dict(zip(ALL_AGENTS, (".claude/skills", ".agents/skills", ".gemini/config/skills")))
    hosts = {h["name"]: h for h in data["hosts"] if h["name"] in defaults}
    host_paths = [(host, path) for host, default in defaults.items()
                  for path in {home / default, Path(hosts[host].get("path") or home / default)}]
    config = Path(env.get("XDG_CONFIG_HOME") or Path(env.get("HOME") or home) / ".config")
    return {
        "registry_path": Path(root), "version": data.get("version", "unknown"),
        "state_path": Path(data["state_path"]), "data_root": Path(data["data_root"]),
        "root_cache_path": config / "my-skills" / "root", "enabled_count": data["skills"]["enabled"],
        "hosts": hosts, "host_paths": host_paths,
    }

def snapshot_host_dir(host_dir: Path) -> Any:
    if not host_dir.exists():
        return "<missing>"
    entries: list[list[str]] = []
    for entry in sorted(host_dir.iterdir(), key=lambda p: p.name.lower()):
        if is_reparse_or_link(entry):
            kind, value = "link", "<unreadable-link>"
            with suppress(OSError):
                value = os.readlink(entry)
        elif entry.is_dir():
            kind, value = "dir", sha256_file(entry / "SKILL.md")
        else:
            kind, value = "file", sha256_file(entry)
        entries.append([entry.name, kind, value])
    return entries

def take_real_snapshot(real_env: dict[str, Any]) -> dict[str, Any]:
    reg: Path = real_env["registry_path"]
    def git_read(*args):
        if not (reg / ".git").exists():
            return "<non-git>"
        res = run_cmd(["git", "-c", "core.fsmonitor=false", *args], cwd=reg, env=make_real_env(), timeout=30)
        if res.returncode or res.timed_out:
            raise RuntimeError("실제 레지스트리 Git 스냅샷 실패")
        return res.stdout

    return {
        "state_json_sha256": sha256_file(real_env["state_path"]),
        "root_cache_sha256": sha256_file(real_env["root_cache_path"]),
        "registry_head": git_read("rev-parse", "HEAD").strip(),
        "registry_git_status": git_read("status", "--porcelain=v1", "-uall"),
        "hosts": {f"{agent} ({path})": snapshot_host_dir(path) for agent, path in real_env["host_paths"]},
    }

def diff_snapshots(before, after):
    changes = [f"{key}: {value!r} -> {after[key]!r}" for key, value in before.items()
               if key != "hosts" and value != after[key]]
    for host, old in before["hosts"].items():
        new = after["hosts"][host]
        if old == new:
            continue
        changes.append(f"hosts.{host}: 항목 변경 감지")
        if isinstance(old, list) and isinstance(new, list):
            old, new = {tuple(row) for row in old}, {tuple(row) for row in new}
            for sign, entries in (("+", new - old), ("-", old - new)):
                if entries:
                    changes.append(f"  {sign} {sorted(entries)[:5]}")
    return changes


class Sandbox:
    def __init__(self, binary, real_registry):
        self.binary = binary
        self.root = make_sandbox_dir()
        self.registry = self.root / "registry"
        self.proj = self.root / "proj"
        self.remote = self.root / "remote.git"
        self.fakehome = self.root / "fakehome"
        self.host_dirs = {
            host: self.proj / subdir / "skills"
            for host, subdir in zip(ALL_AGENTS, (".claude", ".agents", ".agent"))
        }
        self.env = self.make_env()
        try:
            self.init_layout(real_registry)
        except BaseException:
            self.cleanup()
            raise

    def make_env(self):
        env = clean_env()
        for key in ("XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_DATA_HOME", "LOCALAPPDATA", "APPDATA"):
            env[key] = str(self.root / key.lower())
        env.update(GIT_CONFIG_GLOBAL=str(self.root / "gitconfig"),
                   MY_SKILLS_NO_UPDATE_CHECK="1", GIT_CONFIG_NOSYSTEM="1", GIT_ALLOW_PROTOCOL="file")
        env["PATH"] = os.pathsep.join(filter(None, (str(self.binary.parent), env.get("PATH"))))
        return env

    def command(self, tool, args, cwd, check):
        res = run_cmd([tool, *args], cwd=cwd or self.registry, env=self.env, timeout=60)
        if check and (res.returncode or res.timed_out):
            message = res.stderr.strip() or res.stdout.strip()
            raise RuntimeError(f"{tool} {' '.join(args)} 실패 (exit {res.returncode}): {message}")
        return res

    def git(self, *args, cwd=None, check=True):
        return self.command("git", args, cwd, check)

    def skills(self, *args, cwd=None, check=True):
        return self.command(str(self.binary), args, cwd, check)

    def init_layout(self, real_registry):
        for directory in (self.registry, self.proj):
            directory.mkdir()
        blackhole = self.root / "blackhole.git"
        safe = "".join(f"\tdirectory = {p.as_posix()}\n" for p in (self.registry, self.remote, self.proj))
        config = ('[user]\n\tname = HubCheck\n\temail = hubcheck@example.invalid\n[safe]\n'
                  f'{safe}[url "{blackhole.as_posix()}/"]\n'
                  '\tpushInsteadOf = https://\n\tpushInsteadOf = ssh://\n\tpushInsteadOf = git@\n')
        (self.root / "gitconfig").write_text(config, encoding="utf-8")
        for bare in (self.remote, blackhole):
            self.git("init", "--bare", str(bare), cwd=self.root)
        for repo in (self.proj, self.registry):
            self.git("init", "-b", "main", cwd=repo)
        (self.proj / "README.md").write_text("# HubCheck Sandbox Project\n", encoding="utf-8")
        self.commit("chore: init sandbox project", "README.md", cwd=self.proj)
        (self.registry / ".gitignore").write_text(
            "my-skills.local.toml\nlocal/\n.DS_Store\n__pycache__/\n*.pyc\n", encoding="utf-8"
        )
        manifest = ('schema_version = 1\nskills_root = "skills"\n\n[defaults]\n'
                    'install_mode = "link"\ncollision = "error"\nverify_after_install = true\n')
        local = ""
        for host, path in self.host_dirs.items():
            target = f'\n[targets.{host}]\nenabled = true\n'
            manifest += target + 'scope = "user"\n'
            mode = "copy" if host == "agy" else "link"
            local += target + f'path = {json.dumps(str(path))}\ninstall_mode = "{mode}"\n'
        (self.registry / "my-skills.toml").write_text(manifest, encoding="utf-8")
        (self.registry / "my-skills.local.toml").write_text(local, encoding="utf-8")
        staged = [".gitignore", "my-skills.toml"]
        guidance = real_registry / "AGENTS.md"
        if guidance.is_file():
            shutil.copy2(guidance, self.registry / "AGENTS.md")
            staged.append("AGENTS.md")
        self.commit("chore: init sandbox registry", *staged)
        self.git("remote", "add", "origin", self.remote.as_posix())
        self.git("push", "-u", "origin", "main")
        self.skills("set-root", str(self.registry), cwd=self.root)
        self.skills("hooks", "install")

    def commit(self, message, *paths, cwd=None):
        self.git("add", "--", *paths, cwd=cwd)
        return self.git("commit", "-m", message, cwd=cwd)

    def new_skill(self, name, description, message):
        self.skills("new", name, "--description", description)
        self.skills("enable", name)
        return self.commit(message, "my-skills.toml", f"skills/{name}")

    def setup_alpha(self):
        self.new_skill(ALPHA, f"일일 노트를 정리하는 내부 점검용 스킬 ({V1}).", "feat: add hubcheck-alpha v1")
        self.git("push", "origin", "main")

    def seed_agy(self):
        seed = Path(__file__).resolve().parent.parent / "skills" / "my-skills"
        shutil.copytree(seed, self.fakehome / ".gemini/config/skills/my-skills")

    def agent_env(self, agent):
        # Claude and Codex use their installed profiles, including their native login.
        if agent != "agy":
            return self.env
        if not self.fakehome.exists():
            (self.fakehome / AGY_SETTINGS.parent).mkdir(parents=True)
            for relative in (AGY_SETTINGS, Path(".gemini/GEMINI.md")):
                source = Path.home() / relative
                if source.is_file():
                    shutil.copy2(source, self.fakehome / relative)
        return dict(self.env, HOME=str(self.fakehome), USERPROFILE=str(self.fakehome))

    def cleanup(self):
        if not safe_rmtree(self.root):
            raise RuntimeError(f"샌드박스 정리 실패: {self.root}")

@contextmanager
def sandbox(binary, real_registry, kept):
    sb = Sandbox(binary, real_registry)
    try:
        yield sb
    finally:
        if kept is None:
            sb.cleanup()
        else:
            kept.append(sb.root)


def json_events(stdout):
    for line in stdout.splitlines():
        if line.lstrip().startswith("{"):
            with suppress(json.JSONDecodeError):
                yield json.loads(line)

def parse_claude(stdout):
    names, final, chunks, failed = None, "", [], False
    for event in json_events(stdout):
        kind = event.get("type")
        if kind == "system" and event.get("subtype") == "init":
            skills = event.get("skills")
            names = set(skills) if isinstance(skills, list) and all(isinstance(s, str) for s in skills) else None
        elif kind == "assistant":
            content = (event.get("message") or {}).get("content")
            for block in content if isinstance(content, list) else ():
                if isinstance(block, dict) and block.get("type") == "text" and isinstance(block.get("text"), str):
                    chunks.append(block["text"])
        elif kind == "result":
            if isinstance(event.get("result"), str):
                final = event["result"]
            failed |= bool(event.get("is_error"))
    return names, final or "\n".join(chunks), failed or names is None

def parse_codex(stdout):
    final, failed = "", False
    for event in json_events(stdout):
        kind = event.get("type")
        if kind == "item.completed":
            item = event.get("item") or {}
            if item.get("type") == "agent_message" and isinstance(item.get("text"), str):
                final = item["text"]
        elif kind == "error":
            failed |= isinstance(event.get("message"), str)
        elif kind == "turn.failed":
            failed |= isinstance((event.get("error") or {}).get("message"), str)
    return final, failed or not final.strip()

def execute_agent(agent, cmd, prompt, cwd, env, log_dir, name, *, task=False, real=False, model=None):
    stdin = None
    if agent == "claude":
        args = ["-p", "--model", CLAUDE_TASK_MODEL if task else CLAUDE_CHECK_MODEL,
                "--output-format", "stream-json", "--verbose", "--no-session-persistence",
                "--strict-mcp-config"]
        args += (["--permission-mode", "acceptEdits", "--allowedTools",
                  "Bash,PowerShell,Edit,Write,Read,Glob,Grep,Skill", "--add-dir", str(cwd.parent)]
                 if task else ["--tools", "Skill"])
        args += ["--", prompt]
    elif agent == "codex":
        args = ["exec", "--json", "--ephemeral", "--sandbox", "workspace-write" if task else "read-only",
                "-C", str(cwd), "-c", "notify=[]"]
        args += ["--add-dir", str(cwd.parent)] if task else ["-c", 'model_reasoning_effort="low"']
        if real:
            args.append("--skip-git-repo-check")
        args.append("-")
        stdin = prompt + "\n"
    elif task:
        args = ["--dangerously-skip-permissions", "--output-format", "stream-json"]
        if model:
            args += ["--model", model]
        args += ["-p", prompt]
    else:
        args = ["-p", "/skills"]

    agy_query = agent == "agy" and not task
    for attempt in range(4 if agy_query else 1):
        timeout = TASK_TIMEOUT_S if task else QUERY_TIMEOUT_S
        res = run_cmd([*cmd, *args], cwd=cwd, env=env, stdin_text=stdin, timeout=timeout)
        locked = agy_query and "database is locked" in f"{res.stdout}\n{res.stderr}".lower()
        if not locked or res.timed_out or attempt == 3:
            break
        time.sleep(2 * (attempt + 1))
    save_log(log_dir, name, res, ".txt" if agy_query else ".jsonl")
    return res, locked

def query(agent, cmd, prompt, cwd, env, log_dir, name, *, real=False):
    res, locked = execute_agent(agent, cmd, prompt, cwd, env, log_dir, name, real=real)
    if res.timed_out:
        return set(), "", {}, "시간 초과"
    if agent == "agy":
        descriptions = {}
        for line in res.stdout.splitlines():
            name, tab, desc = line.strip().partition("\t")
            if tab:
                descriptions[name.strip()] = desc.strip()
        error = f"agy /skills 실패 (exit {res.returncode})" if res.returncode or locked or not descriptions else ""
        return set(descriptions), "", descriptions, error
    if agent == "claude":
        names, final, failed = parse_claude(res.stdout)
        label = "Claude init.skills 확인"
    else:
        final, failed = parse_codex(res.stdout)
        names = {line.strip() for line in final.splitlines() if line.strip()}
        label = "codex"
    error = f"{label} 실패 (exit {res.returncode})" if failed or res.returncode else ""
    return names or set(), final, {}, error


def skipped(note):
    return {agent: CellResult("해당없음", note=note) for agent in ALL_AGENTS}

def selected_cells(agents, cmds):
    return {
        agent: CellResult("해당없음", note="옵션 제외") if agent not in agents
        else CellResult("실패", reason="CLI 미설치")
        for agent in ALL_AGENTS if agent not in agents or not cmds[agent]
    }

def run_real(agents, cmds, binary, real, cwd, logs):
    env = make_real_env()

    def read_registry(command, *args):
        result = run_cmd([str(binary), "--root", str(real["registry_path"]), command, *args, "--json"],
                         cwd=cwd, env=env, timeout=60)
        save_log(logs, f"row1-{command}", result, ".json")
        return result

    val = read_registry("validate")
    issues = []
    try:
        data = json.loads(val.stdout)
        if data["ok"] is not True or val.returncode or val.timed_out:
            issues.append(f"validate 실패 (exit {val.returncode})")
        for item in data["skills"]:
            for key, label in (("errors", "오류"), ("warnings", "경고")):
                issues.extend(f"{item.get('skill', '?')} {label}: {msg}" for msg in item.get(key, []))
    except (ValueError, TypeError, KeyError):
        issues.append(f"validate 응답 확인 실패 (exit {val.returncode})")
    res = read_registry("skills", "--enabled")
    if res.returncode or res.timed_out:
        raise RuntimeError(f"my-skills skills --enabled --json 실패: {res.stderr.strip()}")
    rows = json.loads(res.stdout)["skills"]
    if len(rows) != real["enabled_count"] or any(r["enabled"] is not True for r in rows):
        raise RuntimeError("skills --enabled 결과가 doctor의 활성 스킬과 불일치")
    expected = {
        agent: {r["name"] for r in rows if not r.get("hosts") or agent in r["hosts"]}
        if real["hosts"][agent]["enabled"] else set() for agent in ALL_AGENTS
    }
    note = f"참고: 스킬 내용 경고 {len(issues)}건, 스킬 주인이 판단 ({issues[0]})" if issues else ""
    cells = selected_cells(agents, cmds)
    for agent in agents:
        if not real["hosts"][agent]["enabled"]:
            cells[agent] = CellResult("해당없음", note="대상 비활성")
        if agent in cells:
            continue
        wanted = expected[agent]
        if not wanted and not issues:
            cells[agent] = CellResult("해당없음", note="대상 스킬 없음")
            continue
        missing = []
        for attempt in range(1, 3 if agent == "codex" else 2):
            name = f"row1-{agent}" + (f"-try{attempt}" if agent == "codex" else "")
            prompt = REAL_LIST_PROMPT if agent == "codex" else OK_PROMPT
            seen, _, _, error = query(agent, cmds[agent], prompt, cwd, env, logs, name, real=True)
            if not error:
                missing = sorted(wanted - seen)
                if not missing:
                    break
        reasons = [error] if error else []
        if missing:
            preview = ", ".join(missing[:4]) + (f" 외 {len(missing) - 4}개" if len(missing) > 4 else "")
            reasons.append(f"누락 {len(missing)}/{len(wanted)}개 ({preview})")
        details = {"expected": len(wanted), "validate_issues": issues}
        if reasons:
            details["missing"] = missing
        cells[agent] = CellResult("실패" if reasons else "통과", "; ".join(reasons), note, details)
    return cells, {"validate_issues": issues, "expected_counts": {a: len(v) for a, v in expected.items()}}

def check_visibility(agent, cmd, sb, logs, row, skill, *, token=None, old=None, present=True):
    prompt = DESC_PROMPT if token else LIST_PROMPT if agent == "codex" else OK_PROMPT
    attempts = 2 if agent == "codex" or (agent == "claude" and token) else 1
    env = sb.agent_env(agent)
    for attempt in range(1, attempts + 1):
        name = f"{row}-{agent}" + (f"-try{attempt}" if attempts == 2 else "")
        names, text, descriptions, error = query(agent, cmd, prompt, sb.proj, env, logs, name)
        if error:
            if agent == "claude" and error != "시간 초과":
                break
            continue
        if token:
            if agent != "codex" and skill not in names:
                label = "init.skills" if agent == "claude" else "/skills"
                return f"{label}에 {skill} 없음"
            desc = descriptions.get(skill, "") if agent == "agy" else "\n".join(
                value for line in text.splitlines() for key, _, value in [line.partition(":")]
                if key.strip() == skill
            )
            if old and old in desc and (agent != "agy" or token in desc):
                return f"이전 표식({old})이 여전히 보임"
            if token in desc:
                return ""
            error = f"응답에 {token} 없음"
        elif not present and agent == "codex" and skill in names:
            return f"{skill}가 여전히 보임"
        elif not present and BETA not in names:
            error = f"응답에 대조 스킬 {BETA} 없음"
        elif (skill in names) == present:
            return ""
        else:
            error = f"{skill} 미발견" if present else f"{skill}가 여전히 보임"
    return error

def run_hooks(agents, cmds, binary, registry, logs, kept, rows):
    defaults = selected_cells(agents, cmds)
    results = {row: dict(defaults) for row in rows}
    active = [agent for agent in agents if agent not in defaults]
    if not active:
        return results
    with sandbox(binary, registry, kept) as sb:
        try:
            sb.setup_alpha()
        except Exception as exc:
            for cells in results.values():
                cells.update({a: CellResult("실패", reason=f"셋업 실패 ({exc})") for a in active})
            return results
        ready = []
        for agent in active:
            error = check_visibility(agent, cmds[agent], sb, logs, "s1", ALPHA, token=V1)
            if not error:
                ready.append(agent)
                continue
            for cells in results.values():
                cells[agent] = CellResult("실패", reason=f"셋업 실패 ({error})")
        if not ready:
            return results
        for row in ("2", "3", "4"):
            if row > max(rows):
                break
            if row == "2":
                md = sb.registry / "skills" / ALPHA / "SKILL.md"
                md.write_text(md.read_text(encoding="utf-8").replace(V1, V2), encoding="utf-8")
                commit = sb.commit("chore: bump hubcheck-alpha to v2", f"skills/{ALPHA}/SKILL.md")
                skill, options = ALPHA, {"token": V2, "old": V1}
            elif row == "3":
                commit = sb.new_skill(BETA, "Hubcheck beta verification skill (hubcheck-beta-v1).",
                                      "feat: add hubcheck-beta")
                skill, options = BETA, {}
            else:
                sb.skills("disable", ALPHA)
                commit = sb.commit("chore: disable hubcheck-alpha", "my-skills.toml")
                skill, options = ALPHA, {"present": False}
            save_log(logs, f"row{row}-commit", commit)
            if row not in rows:
                continue
            for agent in ready:
                error = check_visibility(agent, cmds[agent], sb, logs, f"row{row}", skill, **options)
                if row == "2" and agent == "agy" and not error:
                    md = sb.host_dirs["agy"] / ALPHA / "SKILL.md"
                    if not md.is_file() or V2 not in md.read_text(encoding="utf-8"):
                        error = "agy copy 파일이 v2로 갱신되지 않음 (post-commit 누락)"
                results[row][agent] = CellResult("실패" if error else "통과", reason=error)
    return results

def check_task(sb, skill, base_head, result):
    details = {"duration_s": result.duration_s}
    if result.timed_out:
        return CellResult("실패", reason="시간 초과", details=details)
    details["exit_code"] = result.returncode
    failures = []
    res = sb.skills("skills", skill, "--json", check=False)
    row, states = {}, {}
    if res.returncode or res.timed_out:
        failures.append(f"{skill} 조회 실패 (exit {res.returncode})")
    else:
        try:
            row = json.loads(res.stdout)["skills"][0]
            states = row["status"]
            if row["name"] != skill or not isinstance(states, dict):
                raise ValueError("unexpected skills result")
        except (IndexError, KeyError, TypeError, ValueError):
            failures.append("skills JSON 파싱 실패")
            row, states = {}, {}
    edit = skill == ALPHA
    if not edit and not row:
        failures.append(f"매니페스트에 {skill} 미등록")
        return CellResult("실패", reason="; ".join(failures), details=details)
    description = row.get("description", "")
    details["description"] = description
    if sb.skills("validate", skill, "--json", check=False).returncode:
        failures.append("validate 실패")
    if edit:
        korean = "주간 보고" in description and ("형식" in description or "지원" in description)
        if not korean and "weekly report" not in description.lower():
            failures.append("정본 description에 요청 문구 없음")
    else:
        hosts = set(row.get("hosts") or [])
        details.update(hosts=sorted(hosts), enabled=bool(row.get("enabled")))
        if hosts != {"claude", "codex"}:
            failures.append(f"hosts 불일치 ({sorted(hosts)})")
        if not details["enabled"]:
            failures.append("스킬이 비활성 상태")
        if not description.strip() or "todo" in description.lower():
            failures.append("description에 TODO 남음 또는 비어 있음")
        elif "csv" not in description.lower():
            failures.append("description에 CSV 내용 없음")
        agy = sb.host_dirs["agy"] / skill
        host_files = {a: (sb.host_dirs[a] / skill / "SKILL.md").is_file() for a in ("claude", "codex")}
        host_files["agy_absent"] = not agy.exists() and not is_reparse_or_link(agy)
        details["host_files"] = host_files
        for host, ok in host_files.items():
            if not ok:
                failures.append("agy 대상에 불필요한 연결 존재" if host == "agy_absent" else f"{host} 대상에 연결 없음")
    details["status"] = states
    expected = dict.fromkeys(ALL_AGENTS, "FRESH")
    expected["agy"] = "FRESH" if edit else "UNSUPPORTED"
    if states != expected:
        failures.append(f"호스트 상태 불일치 ({states})")
    skill_md = f"skills/{skill}/SKILL.md"
    manifest = "my-skills.toml"
    after = sb.git("rev-parse", "HEAD").stdout.strip()
    changed = sb.git("diff", "--name-only", base_head, "HEAD", "--", skill_md, manifest).stdout.splitlines()
    if skill_md not in changed or (not edit and manifest not in changed):
        failures.append("관련 변경이 커밋되지 않음")
    dirty = sb.git("status", "--porcelain=v1", "--", f"skills/{skill}", manifest).stdout.strip()
    if dirty:
        failures.append("관련 파일에 미커밋 변경 남음")
    remote = sb.git("rev-parse", "refs/heads/main", cwd=sb.remote, check=False)
    pushed = not remote.returncode and after != base_head and after == remote.stdout.strip()
    details.update(base_head=base_head, after_head=after, related_dirty=dirty, pushed=pushed)
    return CellResult("실패" if failures else "통과", "; ".join(failures),
                      "push: 예" if pushed else "push: 아니오", details)

def run_tasks(agents, cmds, binary, registry, model, logs, kept, rows):
    defaults = selected_cells(agents, cmds)
    results = {row: dict(defaults) for row in rows}
    for agent in (a for a in agents if a not in defaults):
        for row in sorted(rows):
            cells = results[row]
            edit = row == "5a"
            label, kind = ("수정", "edit") if edit else ("새 스킬", "new")
            print(f"  - {row}({label}) {agent} 실행 중...", file=sys.stderr, flush=True)
            with sandbox(binary, registry, kept) as sb:
                try:
                    if edit:
                        sb.setup_alpha()
                    env = sb.agent_env(agent)
                    if agent == "agy":
                        sb.seed_agy()
                    base = sb.git("rev-parse", "HEAD").stdout.strip()
                    result, _ = execute_agent(
                        agent, cmds[agent], TASK_EDIT_PROMPT if edit else TASK_NEW_PROMPT, sb.proj,
                        env, logs, f"row{row}-{kind}-{agent}", task=True, model=model
                    )
                    cells[agent] = check_task(sb, ALPHA if edit else GAMMA, base, result)
                except Exception as exc:
                    cells[agent] = CellResult("실패", reason=f"실행 예외: {exc}")
            print(f"    -> {cells[agent].render()}", file=sys.stderr, flush=True)
    return results


def render_markdown(summary, logs, ordered, kept):
    os_label = f"{platform.system().lower()}-{platform.machine().lower()}"
    model = summary["agy_model"] or "기본"
    lines = ["# my-skills 허브 점검 결과", "",
             f"- 시각: {summary['timestamp']}", f"- OS: `{os_label}`",
             f"- `my-skills`: `{summary['my_skills_version']}` (`{summary['my_skills_bin']}`)",
             f"- 실제 레지스트리(읽기 전용): `{summary['real_registry']}`",
             f"- 모델: Claude `{CLAUDE_CHECK_MODEL}`(1~4줄) / `{CLAUDE_TASK_MODEL}`(5줄), "
             f"Codex 기본(1~4줄 `low` / 5줄 기본), agy `/skills`(1~4줄) / `{model}`(5줄)",
             f"- 원시 로그: `{logs}`"]
    if kept:
        lines.append("- 보존된 샌드박스: " + ", ".join(f"`{p}`" for p in kept))
    lines.extend(("", "| 줄 | 점검 항목 | Claude | Codex | agy |", "|---|---|---|---|---|"))
    counts = dict.fromkeys(("통과", "실패", "해당없음"), 0)
    for row, label, cells in ordered:
        columns = " | ".join(cells[agent].render() for agent in ALL_AGENTS)
        lines.append(f"| {row} | {label} | {columns} |")
        for cell in cells.values():
            counts[cell.status] += 1
    lines.append("")
    diffs = summary["snapshot_diffs"]
    safety = "실제 상태 변경 감지!" if diffs else "실제 상태 변경 없음"
    if diffs:
        lines.extend(("## 실제 상태 변경 감지", "", *(f"- {d}" for d in diffs), ""))
    lines.append("요약: " + " / ".join(f"{label} {count}" for label, count in counts.items()) + f" ({safety})")
    return "\n".join(lines) + "\n"

def parse_args(argv):
    parser = argparse.ArgumentParser(description="실제 에이전트(Claude, Codex, agy)로 my-skills 허브 동작을 점검합니다.")
    parser.add_argument("--real-only", action="store_true", help="1줄(실제 레지스트리 읽기 전용)만 실행")
    parser.add_argument("--skip-tasks", action="store_true", help="5줄(에이전트 과제 2종)을 건너뜀")
    parser.add_argument("--agents", default="claude,codex,agy", help="점검할 에이전트 목록(쉼표 구분, 기본: claude,codex,agy)")
    parser.add_argument("--bin", dest="bin_path", help="샌드박스에서 쓸 my-skills 바이너리 경로(기본: PATH의 my-skills)")
    parser.add_argument("--out", help="결과 Markdown을 저장할 파일 경로")
    parser.add_argument("--rows", help="실행할 줄 (쉼표 구분: 1,2,3,4,5 또는 5a,5b)")
    parser.add_argument("--keep", "--keep-sandbox", action="store_true", help="종료 후 샌드박스 폴더를 지우지 않고 남김")
    return parser.parse_args(argv)


def main(argv=None):
    for stream in (sys.stdout, sys.stderr):
        with suppress(AttributeError, OSError):
            stream.reconfigure(encoding="utf-8", errors="replace")
    args = parse_args(sys.argv[1:] if argv is None else argv)
    agents = list(dict.fromkeys(a.strip().lower() for a in args.agents.split(",") if a.strip()))
    unknown = set(agents) - set(ALL_AGENTS)
    if unknown or not agents:
        print(f"오류: 알 수 없는 에이전트 지정: {sorted(unknown) or args.agents} (허용: {', '.join(ALL_AGENTS)})",
              file=sys.stderr)
        return 2
    selected = set(ROW_LABELS) if args.rows is None else {r.strip().lower() for r in args.rows.split(",")}
    if "5" in selected:
        selected.remove("5")
        selected.update(("5a", "5b"))
    if selected - ROW_LABELS.keys():
        print(f"오류: 알 수 없는 점검 줄: {args.rows}", file=sys.stderr)
        return 2
    rows = {row: skipped("--rows") for row in ROW_LABELS if row not in selected}
    if args.skip_tasks:
        rows.update({row: skipped("--skip-tasks") for row in ("5a", "5b")})
    if args.real_only:
        rows.update({row: skipped("--real-only") for row in ROW_LABELS if row != "1"})
    selected -= rows.keys()
    binary = resolve_my_skills_bin(args.bin_path)
    cmds = {a: resolve_agent_cmd(a) for a in ALL_AGENTS}
    model = read_agy_model()
    now = dt.datetime.now()
    out = Path(args.out).expanduser().resolve() if args.out else None
    logs_parent = out.parent if out else Path(tempfile.gettempdir())
    logs_prefix = f"{out.stem}-logs-" if out else f"hubcheck-logs-{now:%Y%m%d-%H%M%S}-"
    neutral = make_sandbox_dir("msb-neutral-")
    before = None
    diffs = []
    kept = []
    meta = {}
    try:
        real = discover_real_env(binary, neutral)
        registry = real["registry_path"]
        protected = [registry, real["state_path"].parent, real["root_cache_path"].parent,
                     real["data_root"].parent, *(path for _, path in real["host_paths"])]
        if any(p and p.resolve().is_relative_to(root.resolve()) for p in (out, logs_parent) for root in protected):
            raise ValueError("실제 사용자 상태 아래에는 결과를 저장할 수 없습니다.")
        before = take_real_snapshot(real)
        logs_parent.mkdir(parents=True, exist_ok=True)
        logs = Path(tempfile.mkdtemp(prefix=logs_prefix, dir=logs_parent))
        if "1" in selected:
            print("[1/5] 실제 레지스트리 점검 중 (읽기 전용)...", file=sys.stderr, flush=True)
            rows["1"], meta = run_real(agents, cmds, binary, real, neutral, logs)
        hooks = selected & {"2", "3", "4"}
        if hooks:
            print("[2-4/5] 샌드박스 수정·생성·비활성화 점검 중...", file=sys.stderr, flush=True)
            rows.update(run_hooks(agents, cmds, binary, registry, logs, kept if args.keep else None, hooks))
        tasks = selected & {"5a", "5b"}
        if tasks:
            print("[5/5] 샌드박스 에이전트 과제 점검 중...", file=sys.stderr, flush=True)
            rows.update(run_tasks(agents, cmds, binary, registry, model, logs, kept if args.keep else None, tasks))
    finally:
        cleaned = safe_rmtree(neutral)
        if before is not None:
            after = take_real_snapshot(real)
            diffs = diff_snapshots(before, after)
            if diffs:
                print("!!! 실제 상태 변경 감지 (자동 복구하지 않음) !!!", *diffs, sep="\n", file=sys.stderr)
        if not cleaned:
            raise RuntimeError(f"샌드박스 정리 실패: {neutral}")
    ordered = [(row, label, rows[row]) for row, label in ROW_LABELS.items()]
    summary = {
        "timestamp": now.strftime("%Y-%m-%d %H:%M:%S"), "my_skills_bin": str(binary),
        "my_skills_version": real["version"],
        "real_registry": str(registry), "agy_model": model, "row1_meta": meta,
        "snapshot_before": before, "snapshot_after": after, "snapshot_diffs": diffs,
        "rows": [{"id": row, "label": label, "cells": {a: dataclasses.asdict(cells[a]) for a in ALL_AGENTS}}
                 for row, label, cells in ordered],
    }
    (logs / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    md = render_markdown(summary, logs, ordered, kept)
    if out:
        out.write_text(md, encoding="utf-8")
    print(md, end="")
    failed = any(cell.status == "실패" for cells in rows.values() for cell in cells.values())
    return 2 if diffs else int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
