# seed 구현 스펙 (executor 태스크 분해)

> 작성일: 2026-07-01
> 상태: 구현 대기
> 상위 설계: `docs/2026-07-01-default-skills-seed-design.md` (§7 실행 순서를 태스크로 분해)
> 스킬 본문(교체용 SKILL.md 전문)은 T9에 인라인 포함.

각 태스크는 executor 하나가 독립적으로 집을 수 있는 단위다. **의존성 순서**를 지키고,
각 태스크는 자체 테스트(RED → GREEN)를 포함한다. 기존 install/sync/audit 런타임 경로는
건드리지 않는다(회귀 금지).

PR 묶음: PR1(패키징·resolver) → PR2(init 시딩·플래그·캐시·provenance) →
PR3(위치 프롬프트·git init) → PR4(문서·스킬·help·E2E). PR2와 PR3은 같은
`init_registry_commands.py`를 만지므로 순차로.

---

## PR1 — seed 패키징 + resolver

### T1. wheel에 seed 트리 포함
- 파일: `pyproject.toml`, `.gitignore`
- 변경:
  - `[tool.hatch.build.targets.wheel.force-include]`에 `"skills" = "my_skills/_defaults/skills"` 추가.
  - `.gitignore`에 `src/my_skills/_defaults/` 추가.
- 수용: `uv build` 후 wheel 압축 해제 시 `my_skills/_defaults/skills/<name>/SKILL.md`
  존재. `git status`에 `_defaults/`가 안 잡힘.
- 의존: 없음.

### T2. `defaults.py` — seed 목록 + resolver
- 파일: `src/my_skills/defaults.py` (신규), `tests/test_defaults.py` (신규)
- 변경:
  - `DEFAULT_SEED_SKILLS`: `[("cli-inventory", True), ("personal-profile", True),
    ("my-skills", True), ("my-jira", False)]` (name, enabled).
  - `seed_skills_dir() -> Path`: ① `importlib.resources.files("my_skills")/"_defaults"/"skills"`가
    있으면 그 경로 ② 없으면 repo 루트 `skills/` (editable/소스 체크아웃 fallback)
    ③ 둘 다 없으면 `SeedUnavailable` 예외.
  - `package_repo_root()`: editable/소스에선 모듈 위치(`<repo>/src/my_skills`)에서
    `parents[2]`로 repo 루트 유도. **packaged 경로(①)는 빌드된 wheel에만 존재하고
    editable에는 없으므로 ② fallback이 필수**(H2 참조 — 함수 직접호출 테스트는 ②만 탄다).
- 수용: packaged 존재 시 packaged 반환, 없을 때 repo `skills/` 반환, 둘 다 없을 때
  예외. 단위 테스트로 3분기 커버.
- 의존: T1(패키징 경로 규약 확정).

---

## PR2 — init-registry 시딩 + 플래그 + 캐시 + provenance

### T3. `--with-defaults` / `--no-defaults` + seed 복사
- 파일: `src/my_skills/init_registry_commands.py`, `src/my_skills/cli.py`,
  `tests/test_init_registry.py`
- 변경:
  - 서브파서에 `--with-defaults`(기본 True) / `--no-defaults`(store_false).
  - with-defaults일 때 `seed_skills_dir()`에서 `DEFAULT_SEED_SKILLS`의 각 스킬을
    새 registry `skills/<name>/`로 복사. **plain `shutil.copytree` per 스킬 사용**
    (installer.copy_install은 PlanItem→host 설치 + InstallRecord state 기록용이라
    canonical seed 복사엔 부적합).
- 수용: 기본 init → registry `skills/`에 4개 seed 존재. `--no-defaults` → 빈 `skills/`.
  **seed된 my-jira는 `config.example.json` 포함, `config.json`은 미포함**(실제 config는
  절대 복사 안 함 — tracked에 example만 있어 자연히 지켜지나 테스트로 못박음).
- 의존: T2.

### T4. 생성 매니페스트에 seed 등록
- 파일: `init_registry_commands.py` (MANIFEST 생성부), 테스트 동일
- 변경: 생성되는 `my-skills.toml`에 seed 스킬 `[skills.<name>]` 추가.
  enabled는 `DEFAULT_SEED_SKILLS` 값(`my-jira`=false, 나머지=true),
  `hosts = ["claude","codex","hermes"]`.
- 수용: 생성 toml에 `[skills.my-jira] enabled=false`, `[skills.cli-inventory]
  enabled=true`. `--no-defaults`면 `[skills.*]` 없음. init 직후
  `my-skills install --dry-run`이 **비어있지 않은** 계획 출력.
- 의존: T3.

### T5. 생성 직후 root 캐시
- 파일: `init_registry_commands.py` (`cache_repo_root` 사용, `cli_runtime`에서 import)
- 변경: registry 생성 성공 후 `cache_repo_root(target)` 호출.
- 수용: 임의 cwd에서 `init-registry <path>` 후, 다른 디렉터리에서 `my-skills skills`가
  방금 만든 registry를 root로 인식.
- 의존: T3.

### T6. seed provenance 저장 (manifest, capture-once)
- 파일: `src/my_skills/config.py`(`Skill` dataclass + `_resolve_skills`),
  `init_registry_commands.py`(매니페스트 생성부), `tests/test_init_registry.py`
- 근거: provenance는 seed 시점에만 얻는 capture-once 정보. `InstallRecord`(state.py)는
  (skill, host) host 설치 단위라 canonical seed 스킬 출처를 담을 수 없다 → **manifest에 저장.**
- 변경:
  - `Skill` dataclass에 `source_type: str = ""`, `source_revision: str = ""` 추가하고
    `_resolve_skills`에서 파싱(어휘는 InstallRecord와 동일).
  - init 생성 매니페스트의 seed 스킬 블록에 `source_type = "builtin-seed"`,
    `source_revision = <CLI 버전>` 기록.
- 수용: init 후 생성 `my-skills.toml`의 seed 스킬 블록에 두 필드 존재.
  `--no-defaults`면 없음. (사용자가 나중에 추가하는 스킬엔 `source_type` 없음 =
  local-authored로 구분 가능.)
- 의존: T4.
- 비범위(→ update-defaults): 콘텐츠 해시 diff, install 시 InstallRecord로의 승계.

---

## PR3 — 위치 프롬프트 + git init

### T7. registry 위치 입력 UX
- 파일: `init_registry_commands.py`, `cli.py`(path를 optional positional로), 테스트
- 변경:
  - path 인자 optional. 기본 상수 `DEFAULT_REGISTRY = "~/my-agent-skills"`.
  - path 생략 + `sys.stdin.isatty()` → 프롬프트 `Registry location [~/my-agent-skills]:`.
    빈 입력=기본값, 입력값은 절대/상대(cwd 기준) 모두 허용.
  - path 생략 + 비TTY → 기본값 사용 + 생성 위치 출력.
  - 대상이 이미 registry(`my-skills.toml` 존재): TTY면 "이미 있음 → 다른 경로/기존
    사용" 되물음, 비TTY면 기존처럼 거부(자동 suffix 금지).
- 수용: isatty mock으로 4분기(명시경로 / 빈엔터 / 비TTY 기본 / 기존 registry) 테스트.
- 의존: T3~T5(같은 명령).

### T8. git init 자동화
- 파일: `init_registry_commands.py`, `cli.py`(`--no-git`), 테스트
- 변경:
  - 생성 후 `git init` 실행(+ best-effort 첫 커밋은 생성한 scaffold/seed 파일만 stage).
  - `--no-git`이면 건너뜀.
  - 가드: `shutil.which("git")` 없으면 크래시 없이 "git 없어 건너뜀" 알림;
    이미 `.git` 있으면 건너뜀; 커밋 실패(identity 미설정 등)는 무시(`git init`은 보장).
- 수용: git 있음→`.git` 생성; `--no-git`→`.git` 없음; git-미설치 시뮬레이션→graceful.
- 의존: T7.

---

## PR4 — 문서 + 스킬 + help + E2E

### T9. 스킬 본문 교체
- 파일: `skills/my-skills/SKILL.md`
- 변경: 아래 "T9 본문"으로 `skills/my-skills/SKILL.md`를 통째로 교체.
  결정 반영: C1(멘탈모델 인트로), C2(bootstrap 한 줄 — `-h` 표기는 T10), C3(description
  셋업 트리거), C4(현행 섹션 순서). 관련 코드 변경은 T5(cache_repo_root)·T10(help)에 있음.
- 수용: 내용 일치. `my-skills validate my-skills` 통과.
- 의존: PR1~PR3(설명하는 동작이 실제로 존재해야 함).
- **순서 주의: 첫 배포 빌드 전 필수.** seed는 빌드 시점의 `skills/my-skills/`를
  패키징하므로, T9가 릴리스 빌드 전에 안 들어가면 seed된 my-skills가 옛 clone/bootstrap
  흐름을 가르친다.

#### T9 본문 (그대로 적용)

````markdown
---
name: my-skills
description: Manage your my-skills registry from an agent conversation. Use when setting up or creating a registry, listing available skills, sharing a host-local skill into the registry, enabling or disabling a skill, or installing/syncing skills into Claude Code, Codex, or Hermes.
---

# My Skills Registry

Use the `my-skills` CLI as the source of truth. Do not edit host skill
directories directly when a CLI command can plan, share, install, sync, enable,
or disable the skill.

**Mental model.** Your registry is a folder (default `~/my-agent-skills`) holding
`my-skills.toml` (the manifest) and `skills/<name>/SKILL.md` (the canonical
originals). Host directories (`~/.claude/skills`, `~/.agents/skills`,
`~/.hermes/skills`) are **build outputs**: `install` and `sync` copy the canonical
skill into them. Editing a host copy directly causes **drift**, which the CLI
detects and refuses to silently overwrite. git is optional — the registry works
as a plain folder; version control and remotes are the user's choice.

## First-time setup (the front door)

If the user has no registry yet, set one up. Install the CLI once:

```bash
uv tool install git+https://github.com/Seosiju/my-skills.git
```

Then create the registry. `init-registry` prompts for a location (default
`~/my-agent-skills`), seeds the public-safe default skills, and runs `git init`:

```bash
my-skills init-registry
```

- Pass a path to skip the prompt: `my-skills init-registry ~/my-agent-skills`.
- `--no-defaults` creates an empty registry (no seeded skills).
- `--no-git` skips git (local-only use; the whole system works without git).

`init-registry` records this registry as the active root, so later commands work
from any directory. Deploy the enabled skills into your agents:

```bash
my-skills install --dry-run   # preview the plan, write nothing
my-skills install
```

Secrets and real account config belong under `my-skills data-path <skill>`, not
in git (public or private).

## When there is no registry yet

If a command fails with `my-skills.toml not found`, no registry has been created
on this machine. Do not point at a clone — create a registry:

```bash
my-skills init-registry
```

As a temporary override, `export MY_SKILLS_ROOT=/path/to/registry` if one already
exists elsewhere (for example on another machine you cloned).

## List

```bash
my-skills skills
```

Shows each skill, whether it is enabled, and its install status per host
(`fresh`, `stale`, `drifted`, `missing`, or `-` when the skill does not target
that host). Add `--json` when another agent or UI needs a structured list:

```bash
my-skills skills --json
```

## Share From A Host

First produce a read-only plan:

```bash
my-skills share --from <claude|codex|hermes> --plan --json
```

Show the user the candidate skills, validation/audit risks, canonical status,
and available choices. Continue only after the user chooses to enable, disable,
or skip. Apply the selected choice:

```bash
my-skills share --from <host> <skill> --enable
my-skills share --from <host> <skill> --disable
```

Use `--force` only when the plan reports a *different* canonical skill and the
user explicitly confirms overwriting it.

Never use `--skip-audit` for share/import unless the user explicitly accepts the
audit risk. `--force` only answers "may overwrite?"; it does not bypass audit.

## Audit

Run audit before a write when the user asks about risk, provenance, or whether a
skill is safe to install:

```bash
my-skills audit <skill> --json
my-skills audit --all --json
my-skills skills --json --with-status
```

Use `audit --all --json` before applying multiple skills so bundle-level and
cross-skill findings are visible.

If audit returns `blocked: true`, stop and show the finding. Do not install,
sync, share, or import that skill. The only bypass is `--skip-audit`, and that
requires explicit user approval for the exact command.

## Install Or Sync

Default to the current host when the user asks to install a skill from inside a
specific agent. Use `--host all` only when the user explicitly asks for every
host or cross-agent installation.

A status table showing stale or missing entries across multiple hosts is
informational only. It is not permission to update every host. If the user says
"update" without saying "all hosts", "every host", or "cross-agent", update only
the current host.

Never add `--yes` yourself to a multi-host write. `--yes` means the user already
approved that exact multi-host plan. If the user has not explicitly approved all
hosts, stop after the dry-run and ask which host to update.

Before writing into host directories, show the dry-run plan:

```bash
my-skills install <skill> --host <host> --dry-run --json
```

Review both `actions` and `audit`. If audit would block, do not continue. If the
plan is acceptable, run the matching command:

```bash
my-skills install <skill> --host <host>
my-skills sync <skill> --host <host>
```

Multi-host writes require `--yes` after a reviewed dry-run plan. Read-only
checks such as `install --dry-run` and `sync --check` do not need `--yes`.
Only use `--host all --yes` when the user's request explicitly names all hosts
and the dry-run plan has been shown or otherwise reviewed.

## Enable Or Disable

Use manifest toggles instead of editing TOML by hand, then validate:

```bash
my-skills enable <skill>
my-skills disable <skill>
my-skills validate <skill>
```

## Other Commands

This skill covers the common flows. For the full command surface — including
`status`, `validate`, `import`, `data-path`, `uninstall`, and `doctor` — run:

```bash
my-skills --help
```

`bootstrap` is for **contributors/maintainers** working from a cloned repo (it
does an editable install); regular users never need it.
````

### T10. bootstrap help 표기
- 파일: `src/my_skills/cli.py`
- 변경: bootstrap 서브파서 help 문자열에 `(contributor/dev only)` 추가.
- 수용: `my-skills bootstrap -h` 출력에 표기 확인.
- 의존: 없음(독립).

### T11. README 온보딩 단일화
- 파일: `README.md`, `README.ko.md`
- 변경: 신규 사용자 경로를 `uv tool install` → `init-registry` → `install` 하나로.
  "Public CLI, Private Registry" 섹션에서 clone+bootstrap을 기여자 보조 경로로 격하.
  init 경로 예시를 `~/my-agent-skills`로.
- 수용: README만 따라 해도 신규 사용자가 registry 생성~install 도달. clone 전제 제거.
- 의존: PR1~PR3.

### T12. 콜드 E2E + 패키징 테스트 (분리)
- 파일: `tests/test_seed_e2e.py`, `tests/test_seed_packaging.py` (둘 다 신규)
- **왜 분리하나:** 함수 직접호출 E2E는 T2 resolver의 ② repo `skills/` fallback만 타서
  **packaged 경로(①)를 절대 안 밟는다.** 그래서 force-include(T1)가 깨져 wheel에 seed가
  0개여도 그 테스트는 통과한다 — 정작 잡으려던 위험을 못 잡음. 패키징을 따로 검증한다.
- (1) 패키징 테스트(`test_seed_packaging.py`): `uv build` → wheel 압축 해제 시
  `my_skills/_defaults/skills/<name>/SKILL.md` 존재 assert(또는 임시 HOME에 wheel 설치 후
  `seed_skills_dir()`가 ① 경로 반환). ← force-include = 진짜 위험 검증.
- (2) 기능 E2E(`test_seed_e2e.py`): 임시 HOME/XDG + CLI 함수 직접 호출 —
  init(기본 seed) → `skills/`에 seed 존재 → `install --dry-run` 비어있지 않음 →
  `install` → host 경로 사본 존재. `--no-defaults` 회귀(빈 registry)도 별도 케이스.
- (3) 실제 `uv tool install git+...` 스모크는 release-checklist 수동 게이트로 유지(설계 §6).
- 수용: (1)(2) 자동 통과, (3) 체크리스트 반영.
- 의존: PR1~PR3.

### T13. roadmap 참조 추가
- 파일: `docs/2026-06-30-my-skills-open-source-roadmap.md`
- 변경: 이 seed 설계/스펙 문서를 참조로 링크(§12 또는 §9 Phase 3 보완 메모).
- 수용: roadmap에서 seed 설계로 연결됨.
- 의존: 없음.

---

## 착수 순서 요약

1. T10, T13은 언제든(독립).
2. T1 → T2 → T3 → T4/T5/T6 → T7 → T8 → T9/T11/T12.
3. 각 태스크 RED 테스트 먼저, GREEN 후 `uv run pytest` + `uv build` 통과 확인.
