# Registry Root 신뢰성 및 사용구조 문서화 개선 기획서

> 작성일: 2026-07-04
> 상태: 구현 대기 (Codex 작업용)
> 대상 저장소: github.com/Seosiju/my-skills
> 관련 문서: docs/2026-07-03-code-quality-improvement-plan.md, docs/2026-07-01-seed-implementation-spec.md, docs/2026-06-30-my-skills-open-source-roadmap.md

## 0. 배경: 용어 정의 (이 문서 전체에서 고정)

`my-skills`라는 이름은 네 가지 실체를 가리킬 수 있고, 이 혼동이 이번 문제의 뿌리다.
이 문서는 아래 용어만 사용한다.

| 용어 | 실체 | 예시 경로 |
|---|---|---|
| **tool repo** | 공개 CLI 패키지 저장소 (이 저장소). seed 스킬 원본과 테스트 픽스처 포함 | `~/git/my-skills` (contributor clone) |
| **CLI** | `uv tool install`로 설치된 실행 파일 | `~/.local/bin/my-skills` |
| **registry** | 사용자의 canonical 스킬 저장소. `my-skills.toml` + `skills/` | 기본값 `~/my-agent-skills` |
| **active root** | CLI가 "현재 registry"로 해석하는 경로. `~/.config/my-skills/root`에 캐시됨 | — |
| **host 디렉터리** | 각 에이전트의 스킬 설치 위치 (build output) | `~/.claude/skills` 등 |
| **machine-local data** | 비밀값·계정·메모리 등 커밋 금지 데이터 | `~/.local/share/my-skills/` |

registry의 **코드상 기본값은 `~/my-agent-skills`** 이며
(`init_registry_commands.py:15`의 `DEFAULT_REGISTRY`), 사용자는 임의 경로를 쓸 수
있다. 이 기획의 계기가 된 머신은 커스텀 경로 `~/git/my-agent-skills`를 쓰는
**예외 케이스**다.

> **구현 가드레일 (Codex 필독):** §2(진단)와 §7(런북)에 나오는 구체 경로
> (`~/git/my-agent-skills`, `/Users/example/git/my-skills` 등)는 **특정 머신의
> 증거·정리 절차일 뿐이다.** 코드·테스트·README·seed 스킬 어디에도 이 경로들을
> 기본값이나 예시로 넣지 않는다. 코드가 아는 기본값은 `~/my-agent-skills` 하나이고,
> 문서 예시도 기본값 또는 `/Users/you/...` 플레이스홀더만 쓴다.

지원 호스트는 Claude Code, Codex, Hermes 셋뿐이다. **gemini는 지원 호스트가 아니며,
이 작업의 어떤 문서/코드에도 gemini를 지원 호스트로 기재해서는 안 된다**
(`cli-inventory`의 `scan_tools.py`에 있는 `"gemini"`는 CLI *탐지* 라벨이므로 수정 금지).

## 1. 문제 정의

두 가지 증상이 보고되었다.

1. **active root가 tool repo를 가리킴.** 사용자 머신에서 registry는
   `~/git/my-agent-skills`(기본값이 아닌 이 머신의 커스텀 경로)인데 root 캐시는
   tool repo clone을 가리키고 있었다.
   registry를 올바르게 재지정해도 다시 어긋나는 구조다.
2. **에이전트 세션들이 사용구조를 오해함.** 다른 AI 세션이 "CLI는 uv로 설치하고,
   스킬은 별도 registry 저장소에서 관리한다"는 모델을 이해하지 못하고 tool repo를
   registry로 취급했다.

## 2. 진단 — 검증된 사실 (2026-07-04, 전부 실측)

각 항목은 검증 방법과 함께 기록한다. Codex는 이 사실들을 전제로 구현하면 된다.

### F1. root 캐시가 tool repo를 가리킴
`~/.config/my-skills/root` 내용 = `/Users/example/git/my-skills` = tool repo clone
(cat으로 확인; 이하 개인 경로는 기존 문서 관례대로 `/Users/example`로 익명화).
실제 설치 state의 모든 레코드도 `source: /Users/example/git/my-skills/skills/...` —
즉 지금까지 **tool repo가 사실상의 registry로 사용**되어 왔다.

### F2. cwd 발견이 캐시를 무조건 덮어씀 (설계 결함)
`src/my_skills/cli_runtime.py`의 `find_repo_root()` 해석 순서는
`MY_SKILLS_ROOT` env → cwd 상향 탐색 → 캐시 순인데, **cwd에서 발견되면 항상
캐시를 덮어쓴다** (`cli_runtime.py:61-65`). tool repo 루트에는 `my-skills.toml`과
`skills/`(seed 원본·wheel force-include 소스)가 있으므로, tool repo 안에서 명령을
한 번만 실행해도 active root가 tool repo로 재포획된다. 캐시를 고쳐도 재발한다.

### F3. 테스트가 실제 사용자 파일을 오염시킴 (버그, 재현 완료)
`tests/conftest.py`가 없어 전역 격리가 없고, 테스트별 `monkeypatch.setenv("XDG_*")`는
수동·비일관적이다 (`XDG_CONFIG_HOME`을 설정하는 테스트 파일은 4개뿐).

격리된 가짜 `HOME`에서 재현한 결과:

- `pytest tests/test_share.py::test_share_apply_disable_registers_disabled_skill
  tests/test_share.py::test_share_apply_blocks_different_canonical_without_force`
  실행만으로 → `$HOME/.local/state/my-skills/state.json`에 가짜 `brand -> claude`
  설치 레코드가 기록되고, `$HOME/.config/my-skills/root`가 pytest 임시 경로로 덮임.
  (두 테스트 모두 `XDG_STATE_HOME`/`XDG_CONFIG_HOME` 미설정 — `tests/test_share.py:140,155`)
- **전체 스위트(235개) 실행 후**: state 누출 1건(`brand`), root 캐시는 마지막으로
  실행된 누출 테스트(`test_sync_update_failure_prese...`)의 임시 경로로 종료.
  즉 `pytest`를 돌릴 때마다 실사용 root 캐시가 존재하지 않는 경로로 클로버된다.
- 사용자의 실제 `state.json`에도 pytest 임시 경로를 가리키는 `brand` 레코드가
  실존한다 (2026-07-03 생성) — 과거 로컬 pytest 실행의 잔재.

오픈소스 관점에서 치명적: **저장소를 clone해서 `pytest`를 실행한 모든 기여자의
실제 registry 설정이 조용히 망가진다.**

### F4. doctor가 root 관련 정보를 전혀 보여주지 않음
`src/my_skills/inspection_commands.py:64-94` — 출력은 OS/Shell/Python, 호스트 3종,
Manifest valid 여부뿐. active root 경로, root가 어떻게 결정됐는지(env/cwd/cache),
CLI 버전, 등록 스킬 수, state/data 경로 모두 부재. F1~F2 같은 상황을 사용자가
진단할 수단이 없다.

### F5. `--version` 부재
`my-skills --version` → `error: unrecognized arguments` (exit 2). 실측 확인.

### F6. registry 폴더 자체는 정상, git만 미정리
`~/git/my-agent-skills`는 manifest/skills 구조·validate 모두 정상. 단 git은
`README.md`만 tracked이고 `my-skills.toml`/`skills/`/`.gitignore`는 untracked.
원인: `init_registry_commands.py:139-143` — 기존 `.git`이 있으면 git init뿐 아니라
**초기 add/commit까지 통째로 스킵**한다 (사용자는 GitHub에서 만든 repo를 clone한
뒤 init-registry를 실행했으므로 이 경로를 탔다).

### F7. 두 "registry 후보"의 스킬 내용은 동일
tool repo `skills/` vs registry `skills/` diff 차이는 tool repo 쪽
`skills/cli-inventory/.omc`(에이전트 스크래치, gitignored)와 빈 `skills/my-test-skill`
디렉터리뿐. **따라서 active root를 registry로 전환해도 데이터 손실이 없다.**

### F8. 이전 세션 주장 팩트체크
| 주장 (이전 세션) | 검증 결과 |
|---|---|
| root 캐시가 dev clone을 가리킴 | 맞음 (F1) |
| doctor에 registry 정보 부재 | 맞음 (F4) |
| `--version` 부재 | 맞음 (F5) |
| `my-jira`(codex)·`my-skills`(hermes) CONFLICT | 맞음 — 호스트 복사본에 실제 로컬 편집 존재 (회수 대상, 덮어쓰기 금지) |
| `uninstall`(스킬명 없이)이 exit 0 | **틀림** — 실측 exit 2, `install_commands.py:232-234`도 2 반환 |

### F9. 기타
- 설치된 CLI는 v0.1.0으로, main의 Unreleased 개선사항이 반영되지 않은 상태.
- `status` ⊂ `skills` 중복, `bootstrap` 노출 문제 등은 실재하나 **이번 기획의
  범위에서 제외**한다 (§8).

## 3. 근본 원인 (3개)

- **RC1 — 캐시 덮어쓰기 정책**: cwd 발견이 기존 캐시를 조용히 덮어쓴다. tool repo가
  registry 모양(manifest+skills)을 하고 있는 것은 seed 배포·CI 스모크를 위한 의도된
  구조이므로, 고칠 것은 repo 모양이 아니라 **캐시 기록 시맨틱**이다.
- **RC2 — 테스트 격리 부재**: conftest 전역 격리가 없어 테스트가 실사용
  state/캐시를 오염시킨다.
- **RC3 — 문서의 모델 전달 실패**: README의 "How it works"와 "Layout" 섹션이
  `skills/<name>/`를 무한정으로 표기해 "이 repo가 registry"라는 오독을 유도하고,
  root 해석 규칙(env→cwd→cache)은 어디에도 문서화되어 있지 않으며, tool repo에
  에이전트용 오리엔테이션(AGENTS.md)이 없다.

## 4. 설계 원칙

- **P0. 테스트·개발 활동은 사용자 실데이터를 절대 건드리지 않는다.**
  (state.json, root 캐시, data root 전부)
- **P1. active root의 영구 변경은 명시적 행위로만 일어난다.**
  `init-registry`와 신설 `set-root`만 캐시를 덮어쓸 수 있다. cwd 발견은 해당
  호출에만 유효하다. 단 **캐시가 아예 없거나 무효일 때의 최초 기록은 허용**한다
  (first-run 편의 유지 — 기존 문서화된 동작 "run once from your clone" 보존).
- **P2. root 상태는 항상 조회 가능해야 한다.** doctor가 경로와 결정 경위를 보여준다.
- **P3. 문서는 §0의 용어 모델을 일관되게 사용한다.** 신규 사용자·에이전트가
  "clone == registry"로 오해할 수 없어야 한다.
- **P4. 계약 유지.** `--json` 출력 스키마, exit code 계약(0/1/2)은 변경하지 않는다.
  stdout의 JSON 무결성을 위해 새 안내문은 stderr로만 낸다. 사용자 가시 동작 변경은
  CHANGELOG Unreleased에 기재한다.

## 5. 작업 목록 (PR 단위, 순서 고정)

### PR1 — 테스트 격리 (최우선, 독립 머지 가능)

**T1-1. `tests/conftest.py` 신설**

```python
from __future__ import annotations

import pytest


@pytest.fixture(autouse=True)
def _isolate_user_dirs(tmp_path, monkeypatch):
    """Tests must never read or write the real user's config/state/data."""
    monkeypatch.setenv("XDG_CONFIG_HOME", str(tmp_path / "xdg-config"))
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "xdg-state"))
    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path / "xdg-data"))
    monkeypatch.delenv("MY_SKILLS_ROOT", raising=False)
```

- 기존 테스트의 개별 `setenv("XDG_*", ...)`는 **그대로 둔다** (autouse 이후에
  적용되므로 우선하며, diff를 최소화한다). 새로 작성하는 테스트는 conftest에
  의존하고 개별 setenv를 넣지 않는다.
- 주의: autouse fixture는 `tmp_path`를 쓰므로 모든 테스트에 tmp 디렉터리가
  생긴다. 무해하다.

**T1-2. 격리 가드 테스트 추가** — `tests/test_isolation.py`

- `default_state_path()`(`state.py:22`)와 `_root_cache_path()`(`cli_runtime.py:10`),
  `data_root()`(`data.py:27`)가 모두 `tmp_path` 하위를 가리키는지 단언하는 테스트.
  conftest가 실수로 제거·약화되면 즉시 실패하도록 하는 안전핀이다.

**T1-3. 검증 절차 (완료 게이트)** — 아래 스크립트로 누출 0을 확인한다:

```bash
scratch="$(mktemp -d)"
env -u XDG_STATE_HOME -u XDG_CONFIG_HOME -u XDG_DATA_HOME -u MY_SKILLS_ROOT \
  HOME="$scratch" .venv/bin/python -m pytest -q
test ! -e "$scratch/.local/state/my-skills" || { echo "STATE LEAK"; exit 1; }
test ! -e "$scratch/.config/my-skills" || { echo "CACHE LEAK"; exit 1; }
```

(uv 캐시(`$scratch/.cache/uv`)는 bootstrap 테스트가 uv를 호출한 흔적이므로 누출로
치지 않는다.)

**T1-4. CHANGELOG** — Unreleased `### Fixed`에 한 줄:
"Running the test suite no longer overwrites the real user's my-skills state
file and active-root cache."

### PR2 — root 캐시 시맨틱 변경 + `set-root` 명령

**T2-1. `find_repo_root()` 시맨틱 변경** (`src/my_skills/cli_runtime.py`)

상황별 정확한 동작 (변경점은 굵게):

| 상황 | 반환 | 캐시 동작 |
|---|---|---|
| `MY_SKILLS_ROOT` 유효 | env root | 기록 안 함 (현행 유지) |
| `MY_SKILLS_ROOT` 무효 | `ManifestError` | — (현행 유지) |
| cwd에서 발견, 캐시 없음 또는 무효(파일 없음/가리키는 경로가 registry 아님) | cwd root | **기록 (first-run)** |
| cwd에서 발견, 캐시 == cwd root | cwd root | no-op |
| cwd에서 발견, 캐시가 **다른 유효 root**를 가리킴 | cwd root | **기록하지 않음** + stderr 1줄 안내: `note: using ./my-skills.toml for this command; active registry is <cached>. Run 'my-skills set-root' here to switch.` |
| cwd 미발견, 캐시 유효 | cache root | — |
| 둘 다 없음 | `ManifestError` | 에러 메시지에 `set-root` 언급 추가 |

- `write_cache=` 파라미터는 유지하되 의미를 "first-run 기록 허용 여부"로 좁힌다.
  외부 호출자는 `bootstrap_commands.py:64`
  (`find_repo_root(write_cache=not args.dry_run)`) 하나뿐이며, 새 시맨틱과
  호환되므로 수정 불필요 (dry-run은 기록 안 함, 실제 실행은 first-run 기록 허용).
- stderr 안내는 **stdout JSON 계약을 깨지 않는다** (P4). CI 스모크는 fresh HOME이라
  캐시가 없으므로 안내가 출력되지 않는다.
- 구현 편의를 위해 내부에 `resolve_root() -> RootResolution` (fields: `root: Path`,
  `source: Literal["env", "cwd", "cache"]`, `cached: Path | None`)를 신설하고
  `find_repo_root()`는 이를 감싼다. doctor(PR3)가 `source`를 사용한다.
  기존 공개 시그니처는 유지한다.

**T2-2. `set-root` 명령 신설** (`cli.py` + `init_registry_commands.py` 또는 신규 모듈)

- 사용법: `my-skills set-root [path]` — path 생략 시 cwd 상향 탐색으로 발견된 root.
- 동작: 대상이 registry(`my-skills.toml` 존재)인지 검증 후 기존
  `cache_repo_root()`(`cli_runtime.py:31`)를 호출. 성공 시
  `Active registry root set to <path>` 출력, exit 0.
- 오류: registry가 아니면 `error: <path> does not contain my-skills.toml` (stderr,
  exit 2).
- help 문구: `Set the active registry root used when running outside a registry`.

**T2-3. 테스트**

- 위 표의 각 행을 커버하는 단위 테스트 (특히: 다른 유효 캐시가 있을 때 덮어쓰지
  않음 + stderr 안내 검증, first-run 기록, set-root 성공/실패).
- **기존 테스트 수정 필요**: cwd 발견이 항상 캐시를 쓴다고 가정한 테스트
  (`tests/test_cli_runtime.py` 등)는 새 시맨틱으로 갱신한다. 이는 의도된 동작
  변경이며 이 기획서가 근거다.

**T2-4. CHANGELOG** — `### Changed`:
"Running a command inside a my-skills directory no longer silently repoints the
machine-wide active registry; use `my-skills set-root` (or `init-registry`) to
switch. First-run discovery still records the root when none is cached." /
`### Added`: "`set-root` command."

### PR3 — doctor 확장 + `--version`

**T3-1. `--version`** (`cli.py::build_parser`)

```python
from . import __version__
parser.add_argument("--version", action="version", version=f"my-skills {__version__}")
```

**T3-2. doctor 출력 확장** (`inspection_commands.py::cmd_doctor`)

목표 출력 형식 (기존 항목은 유지, 신규 항목 추가):

```
my-skills 0.1.0
OS:     macOS-...
Shell:  /bin/zsh
Python: 3.11.x

Registry: /Users/you/my-agent-skills (source: cache)
Skills:   4 registered, 3 enabled
State:    /Users/you/.local/state/my-skills/state.json
Data:     /Users/you/.local/share/my-skills

Hosts:
  Claude Code  exe=found (claude)  enabled=True  writable=True  path=...
  ...

Manifest: valid
```

- `Registry:` 라인은 `resolve_root()`의 `source`(env/cwd/cache)를 표기한다.
  root 미발견 시: `Registry: not configured (run 'my-skills init-registry' or 'my-skills set-root')`.
- cwd root ≠ 캐시 root인 경우 경고 라인 추가:
  `warning: this directory is not the active registry (active: <cached>)`.
- `Skills:` 카운트는 manifest 로드 성공 시에만; 실패 시 생략.
- `State:`/`Data:` 는 각각 `default_state_path()`(`state.py`)와
  `data_root()`(`data.py`) 재사용. 존재하지 않아도 경로는 표시한다.
- exit code 계약 유지: manifest INVALID → 1, 그 외 0 (현행과 동일).

**T3-3. 테스트** — doctor 출력에 Registry/버전/State/Data 라인이 포함되는지,
root 미설정·불일치 시 문구가 나오는지. CI 스모크(`ci.yml`의 `my-skills doctor`)는
출력 형식만 늘어나므로 영향 없음(확인만).

**T3-4. CHANGELOG** — `### Added`: "--version flag; doctor now reports the active
registry root (and how it was resolved), CLI version, skill counts, and
state/data paths."

### PR4 — 사용구조 문서 재구성

**T4-1. `README.md` / `README.ko.md`** (두 파일 패리티 필수)

1. "How it works" 앞 또는 안에 **§0의 표를 요약한 "One tool, two repos" 단락**을
   추가한다. 핵심 문장(취지): *"The tool repo (this repository) is what you
   install; your registry (default `~/my-agent-skills`, created by
   `init-registry`) is where your skills live. Cloning this repository is only
   for contributors — it is not your registry."*
2. 본문 전체에서 무한정 `skills/<name>/` 표기를 "your registry's
   `skills/<name>/`"로 한정한다 (mermaid 다이어그램 라벨 포함:
   `registry skills/<name>` 등).
3. **"Layout" 섹션 분리**: 현재는 tool repo 루트의 `my-skills.toml`+`skills/`를
   보여줘 오독을 유도한다. "Repository layout (the CLI project)"과 "Your registry
   layout"으로 나누고, tool repo의 `skills/`는 "seed sources packaged into the
   wheel; also a dev fixture for tests/CI — not your registry"로 명시한다.
4. **"Where commands look for your registry" 섹션 신설**: 해석 순서
   `MY_SKILLS_ROOT` env → 현재 디렉터리(그 호출에만 적용) → cached active root,
   확인 방법(`my-skills doctor`), 변경 방법(`set-root`, `init-registry`)을
   문서화한다. PR2/PR3의 새 동작과 일치해야 한다.
5. Quick start에 `my-skills doctor`로 "Registry:" 라인을 확인하는 단계를 한 줄
   추가한다.

**T4-2. `skills/my-skills/SKILL.md` (seed 스킬 — 에이전트가 실제로 읽는 문서)**

- "Mental model" 아래에 **"Which registry am I operating on?"** 소절 추가:
  작업 전 `my-skills doctor`의 `Registry:` 라인으로 active root를 확인하고,
  기대와 다르면 `my-skills set-root <path>`(영구) 또는
  `MY_SKILLS_ROOT=<path>`(일회)로 교정하라는 지침.
- "Do not point at a clone" 경고를 강화: tool repo clone(`my-skills.toml`이 있는
  CLI 소스 저장소)은 registry가 아니며, 그 안에서 명령을 실행해도 active root는
  바뀌지 않는다(PR2 이후 동작)는 설명.
- seed 파일 목록(`DEFAULT_SEED_FILES`)에 파일 추가는 없다 — 기존 SKILL.md 내용
  수정만이므로 `pyproject.toml` force-include 변경 불필요.

**T4-3. `AGENTS.md` 신설 (tool repo 루트)**

에이전트/기여자 오리엔테이션 문서. 필수 내용:

- 첫 문단: *"This repository is the my-skills CLI project — it is NOT a user's
  skill registry. The `my-skills.toml` and `skills/` here are seed sources and
  test fixtures."* (§0 용어 표 축약 포함)
- 사용자의 registry를 조작해야 할 때: `my-skills doctor`로 active root 확인 후
  registry 쪽에서 작업하라는 지침.
- 개발 명령: `uv run pytest`, `uv build`, `uv run my-skills <cmd>` (repo 안에서
  실행해도 active root를 바꾸지 않음 — PR2 이후).
- 지원 호스트는 Claude Code / Codex / Hermes 셋. gemini 지원 기재 금지.

**T4-4. `init-registry`의 기존-git 경로 개선 (F6 대응, 소규모)**

`init_registry_commands.py::_init_git` — 기존 `.git`이 있을 때 init만 스킵하고
**staging/commit은 시도**하도록 변경한다 (스캐폴드 파일 add → commit, 실패 시
현행처럼 안내만 출력). 사용자가 GitHub에서 만든 빈 repo를 clone해 init-registry를
실행하는 흔한 경로에서 untracked 방치를 없앤다. 테스트: 기존 `.git`이 있는
디렉터리에서 스캐폴드가 커밋되는지 검증 (`tests/test_init_registry.py` 확장).
CHANGELOG `### Fixed` 한 줄.

### 머지 순서와 의존성

`PR1` (독립) → `PR2` (PR1의 격리 위에서 캐시 테스트 안전) → `PR3` (PR2의
`resolve_root()` 사용) → `PR4` (PR2/PR3의 최종 동작을 문서화). PR4는 코드 동작이
확정된 뒤 작성해야 문서-코드 불일치가 없다.

## 6. 완료 게이트 (전 PR 공통)

1. `uv run pytest` 전체 통과 (기존 235 + 신규; 감소 금지).
2. §5 T1-3 격리 검증 스크립트 누출 0.
3. CI (`.github/workflows/ci.yml`) green — doctor 출력 확장·stderr 안내가 스모크를
   깨지 않는지 확인.
4. `README.md`/`README.ko.md` 내용 패리티 (섹션 구조 동일).
5. CHANGELOG Unreleased에 각 PR의 사용자 가시 변경 기재.
6. 어떤 문서에도 gemini가 지원 호스트로 등장하지 않음 (`grep -ri gemini` 검사).
   허용되는 등장: `scan_tools.py`의 CLI 탐지 라벨, CHANGELOG의 과거 수정 기록,
   docs/ 기획서들의 "gemini는 지원 호스트가 아니다"류 제약 서술. 그 외 —
   특히 README 양 언어, `AGENTS.md`, seed 스킬 — 에서는 0건이어야 한다.

## 7. 부록: 이 머신의 정리 런북 (코드 작업 아님 — repo에 커밋할 내용 없음)

이 절의 경로는 전부 **이 머신의 커스텀 설정**이다 (registry가 기본값
`~/my-agent-skills`가 아닌 `~/git/my-agent-skills`에 있음). 코드/문서 구현에
반영할 내용이 아니며, PR1~PR4 머지 후 사용자(또는 로컬 세션)가 수행:

1. CLI 재설치: `uv tool install --force git+https://github.com/Seosiju/my-skills.git`
   (현재 설치본은 v0.1.0으로 구식).
2. 오염 정리: `~/.local/state/my-skills/state.json`에서 pytest 경로를 가리키는
   `brand` 레코드 제거.
3. active root 전환: `my-skills set-root ~/git/my-agent-skills`
   (PR2 이전이라면 임시로 `export MY_SKILLS_ROOT=~/git/my-agent-skills`).
4. 로컬 편집 회수 (CONFLICT 2건 — 덮어쓰지 말 것):
   `my-skills share --from codex --plan` 검토 후 `my-skills share --from codex my-jira`,
   같은 방식으로 `my-skills share --from hermes my-skills`.
5. registry git 정리: `~/git/my-agent-skills`에서 `my-skills.toml`, `.gitignore`,
   `skills/`, `README.md` 커밋.
6. tool repo 로컬 잔재 삭제: 빈 `skills/my-test-skill/` 디렉터리,
   `skills/cli-inventory/.omc/` (둘 다 untracked/ignored).

## 8. 범위 제외 (이번에 하지 않음)

- **seed `skills/`를 패키지 내부로 이동** — RC1은 캐시 시맨틱 변경으로 해소되며,
  seed 이동은 build/CI/seed-spec 문서 연쇄 수정이 커서 별도 기획으로 미룬다.
- `status` 명령 정리(≒`skills` 중복), `skills --with-status` 삭제, `bootstrap`
  help 숨김, `remove`/`new` 명령 신설 — 실재하는 개선점이나 root 신뢰성과 무관.
  후속 기획서로 다룬다.
- 버전 범프(0.2.0) 및 릴리스 — 릴리스 시점에 `docs/release-checklist.md`를 따른다.

## 9. 구현 착수 판정 (CodeGraph 리뷰 반영)

**판정: 조건부 승인.**

이 기획의 핵심 진단(RC1 root 캐시 덮어쓰기, RC2 테스트 격리 부재, RC3 문서 모델
혼동)은 현재 코드와 일치한다. `find_repo_root()`는 cwd에서 `my-skills.toml`을
찾으면 캐시를 바로 덮어쓰고, 테스트 전역 격리는 없으며, `doctor`는 active root,
state/data 경로, CLI 버전을 보여주지 않는다. 따라서 **PR1은 즉시 착수 가능**하다.

단, **PR2 착수 전** 아래 정책을 문서와 테스트 계약에 반영해야 한다.

1. **`bootstrap`의 active root 기록 정책을 확정한다.**
   현재 `bootstrap_commands.py::cmd_bootstrap()`은 `cache_repo_root(root)`를 직접
   호출하고, `tests/test_cli_bootstrap.py`도 root 캐시 기록을 기대한다. P1 원칙을
   엄격히 적용하려면 `bootstrap`은 더 이상 active root를 기록하지 않아야 한다.
   유지한다면 `bootstrap`을 명시적 active-root 변경 명령의 예외로 문서화하고
   테스트도 그 계약을 유지한다.

   **권장:** 기록 제거. `bootstrap`은 contributor/dev editable install 경로이고,
   registry 선택은 `init-registry`와 `set-root`가 맡는 편이 역할 분리가 명확하다.

2. **fresh machine에서 tool repo 실행 시 first-run 캐시 정책을 명확히 한다.**
   T2-1의 현재 설계는 캐시가 없거나 무효일 때 cwd root를 first-run으로 기록한다.
   tool repo도 `my-skills.toml`을 가지므로, fresh machine에서 tool repo 안에서
   명령을 실행하면 여전히 tool repo가 active root로 기록될 수 있다. 이는 T4-2/T4-3의
   "tool repo 안에서 명령을 실행해도 active root는 바뀌지 않는다"는 문구와 충돌한다.

   **권장:** 이번 PR에서는 tool repo 식별/차단을 새로 만들지 말고, 문서를 정확히
   고친다. 즉 "캐시가 없으면 cwd registry가 최초 active root로 기록될 수 있다"는
   사실을 README/seed skill/AGENTS.md에 반영한다. tool repo 자동 식별과 차단은 seed
   배치 변경과 맞물릴 수 있으므로 별도 후속 기획으로 분리한다.

3. **`set-root [path]`에서 path 생략 동작은 cache fallback을 금지한다.**
   path 생략 시 `find_repo_root()`를 그대로 쓰면 cwd 밖에서 기존 cache root를 찾아
   같은 값을 다시 기록하는 no-op 성공이 될 수 있다. path 생략은 cwd 상향 탐색만
   수행해야 하며, cwd와 부모 어디에도 `my-skills.toml`이 없으면 exit 2로 실패해야
   한다.

   **권장 구현:** `find_cwd_root()` 또는 `resolve_root(allow_cache=False)`처럼
   cache fallback을 끈 내부 API를 둔다. `set-root [path]`는 이 API를 사용하고,
   `find_repo_root()`는 기존 공개 시그니처를 유지한다.

4. **`doctor`의 root 미설정 exit code를 테스트로 고정한다.**
   현재 `cmd_doctor()`는 manifest를 못 읽으면 `Manifest: INVALID (...)`와 exit 1을
   반환한다. PR3는 `Registry: not configured`를 출력하도록 바꾸므로, root 미설정을
   진단 가능한 정상 상태(exit 0)로 볼지 manifest invalid(exit 1)로 볼지 결정해야
   한다. 어느 쪽이든 `tests/test_doctor.py` 또는 기존 CLI 테스트에 명시적으로 고정한다.

위 네 항목이 반영되면 PR2~PR4도 구현 착수 가능하다. 반영 전에는 PR2 구현자가
`bootstrap`, first-run 캐시, `set-root` 생략 인자, `doctor` exit code를 각자 판단하게
되어 문서-코드 불일치가 남을 위험이 크다.
