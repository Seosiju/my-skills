# my-skills

**Agent Skill을 한 번 작성하고, 모든 에이전트에서 사용하세요.**

_스킬을 위한 하나의 정규(canonical) 위치 — Claude Code·Codex·Hermes 전반에 설치·동기화·공유._

[English](README.md) | **한국어**

[![Python](https://img.shields.io/badge/python-3.11+-blue.svg)](https://www.python.org/)
[![built with uv](https://img.shields.io/badge/built%20with-uv-purple.svg)](https://docs.astral.sh/uv/)
[![spec: Agent Skills](https://img.shields.io/badge/spec-Agent%20Skills-green.svg)](https://agentskills.io/specification)
[![License: MIT](https://img.shields.io/badge/license-MIT-yellow.svg)](LICENSE)

`my-skills`는 당신의 [Agent Skills](https://agentskills.io/specification)를
한곳에 보관하고, 같은 스킬을 사용하는 모든 AI 코딩 에이전트에 설치하는 작은
CLI입니다. 스킬을 한 번 편집하면 `sync`가 어디든 전파하며, 복사본이 변경(drift)됐을
때는 조용히 덮어쓰지 않고 알려줍니다.

## 왜 쓰나요

- **한 번 작성, 어디서든 실행** — 같은 스킬이 Claude Code·Codex·Hermes에서 동작합니다.
- **조용한 덮어쓰기 없음** — 설치는 기본적으로 복사하며, 건드리기 전에 로컬 편집(drift)을 감지합니다.
- **머신-로컬은 로컬에** — 비밀값·경로·계정은 정규 스킬이나 git에 절대 들어가지 않습니다.
- **CI 친화적** — `validate`, `install`, `sync --check`는 오류나 드리프트 시 비정상 종료코드를 반환합니다.

## 어떻게 동작하나요

스킬은 `skills/<name>/` 아래의 디렉터리이며, YAML frontmatter(`name`,
`description`)를 가진 `SKILL.md`를 포함합니다 —
[Agent Skills](https://agentskills.io/specification) 표준입니다. `skills/`
디렉터리가 **정규(canonical)** 진실의 원천입니다.

각 에이전트(Claude Code, Codex, Hermes)는 **호스트**입니다. `install`은 정규
스킬을 호스트로 복사하고, `sync`는 그 복사본을 최신으로 유지합니다. 복사본은
제자리에서 편집될 수 있으므로, `my-skills`는 **드리프트**를 추적해 `sync`가 로컬
변경을 알리지 않고 덮어쓰는 일이 없도록 합니다.

## 빠른 시작

요구 사항: **Python 3.11+** 와 [uv](https://docs.astral.sh/uv/).

```bash
git clone https://github.com/Seosiju/my-skills.git
cd my-skills

uv run my-skills doctor     # 환경, 감지된 호스트, 대상 경로 확인
uv run my-skills skills     # 스킬 목록 + 호스트별 설치 상태
uv run my-skills install    # 활성 스킬을 에이전트에 설치
```

이게 전부입니다 — 이제 감지된 모든 에이전트에서 스킬을 사용할 수 있습니다.

`skills`는 모든 스킬과 호스트별 설치 위치를 보여줍니다:

```text
SKILL             ENABLED  CLAUDE   CODEX    HERMES
----------------  -------  -------  -------  -------
cli-inventory     yes      fresh    fresh    missing
personal-profile  yes      fresh    stale    missing
my-skills         yes      fresh    fresh    fresh
```

## 포함된 스킬

| 스킬 | 하는 일 |
|------|---------|
| `cli-inventory` | 이 머신에 설치된 CLI 도구를 발견(PATH + Homebrew/npm/pipx/cargo/gem/pip)해 머신-로컬 인벤토리로 기록하고 빠르게 다시 읽기. |
| `personal-profile` | 사용자에 대한 지속적인 사실(정체성, 선호)을 기억하고 에이전트 전반에 적용. |
| `my-skills` | 카탈로그·share·install·sync 워크플로를 CLI로 안내하는 에이전트용 스킬. |

## 일상 명령어

```bash
# 무엇이 있고 어디에 설치됐는지 본다.
uv run my-skills skills              # 에이전트/UI용은 --json 추가
uv run my-skills status              # 스킬별·호스트별 설치 상태

# 설치 / 갱신.
uv run my-skills install --dry-run   # 계획만 미리 보기, 아무것도 안 씀
uv run my-skills install             # 활성 스킬 -> 활성 호스트
uv run my-skills install cli-inventory --host claude
uv run my-skills install cli-inventory --host all --yes  # 명시적인 다중 호스트 쓰기
uv run my-skills sync                # 정규 편집을 관리되는 설치본으로 전파
uv run my-skills sync --check        # 드리프트만 감지 (fresh 아니면 비정상 종료코드)

# 관리되는 설치본 제거 (기록된 대상만).
uv run my-skills uninstall cli-inventory --host claude

# 기본 install/sync 선택을 위해 스킬 켜기/끄기.
uv run my-skills enable cli-inventory
uv run my-skills disable cli-inventory
```

### 이미 작성한 스킬 가져오기

```bash
# 외부 스킬 디렉터리를 정규 skills/로 가져온다.
uv run my-skills import ~/.hermes/skills/cli-inventory

# 또는 호스트의 로컬 스킬을 검토한 뒤 하나를 my-skills로 승격한다.
uv run my-skills share --from claude --plan --json
uv run my-skills share --from claude cli-inventory --enable
uv run my-skills sync cli-inventory
```

### 스킬을 실시간으로 개발하기

```bash
# 호스트 복사본을 정규에 심볼릭 링크해 sync 없이도 편집이 반영되게 한다.
uv run my-skills install cli-inventory --host claude --mode link
```

링크된 설치본은 항상 `FRESH`로 보고됩니다. `uninstall`은 심볼릭 링크만 제거하며
정규 소스는 절대 삭제하지 않습니다. 복사 모드가 기본값입니다.

## 안전성은 이렇게 지켜집니다

- **기본은 복사.** 설치는 정규 디렉터리를 복사하며, 다음 `install` 또는 `sync` 전까지 아무것도 바뀌지 않습니다.
- **다중 호스트 쓰기는 확인이 필요합니다.** `--host all` 등 여러 호스트에 쓰는
  명령은 dry-run 계획을 확인한 뒤 `--yes`를 붙여야 합니다. 읽기 전용 확인은
  `--yes` 없이 실행됩니다.
- **충돌 = 차단.** 이미 존재하는 비관리 대상은 절대 덮어쓰지 않습니다.
- **드리프트 보호.** 로컬에서 편집된 복사본은 덮어쓰지 않고 보고합니다.
- **원자적 쓰기.** 설치는 임시 디렉터리에 준비한 뒤 제자리로 교체하며, 실패 시 롤백합니다.

설치 state는 머신-로컬(`$XDG_STATE_HOME` 또는 `~/.local/state/my-skills/`)이며
절대 커밋되지 않습니다.

`sync`와 `status`는 각 (스킬, 호스트)를 분류해 쓰기 작업이 무엇을 할지 항상 알 수 있게 합니다:

| 상태 | 의미 |
|------|------|
| `FRESH` | 설치본이 정규본과 일치 |
| `STALE` | 정규본이 변경됨; `sync`가 갱신 |
| `DRIFTED` | 설치본이 로컬에서 편집됨; `sync`가 건드리지 않음 |
| `CONFLICT` | 양쪽 모두 변경됨 — 자동 병합 불가 |
| `MISSING` | 등록됐지만 설치되지 않음 |
| `UNMANAGED` | my-skills가 설치하지 않은 복사본이 존재 |

`sync`는 안전한 경우만 씁니다. `DRIFTED`, `CONFLICT`, `UNMANAGED`는 비정상 종료코드로 차단됩니다.

### 머신-로컬 데이터

정규 스킬은 머신별 데이터를 절대 저장하지 않습니다. 실제 로컬 데이터가 필요한
스킬(예: `personal-profile` 메모리)은 대신 모든 호스트가 공유하는 단일 데이터
루트를 읽고 씁니다:

```bash
uv run my-skills data-path personal-profile          # 경로 해석
uv run my-skills data-path personal-profile --create # 그리고 생성
```

데이터 루트는 머신-로컬이며 절대 커밋되지 않습니다.

## 구조

```text
my-skills.toml            # 매니페스트: 호스트 + 스킬 + 기본값
skills/<name>/SKILL.md    # 정규 스킬
src/my_skills/            # CLI 패키지
tests/                    # 단위 + 픽스처 기반 테스트
```

매니페스트의 `enabled` 플래그가 기본 선택을 제어합니다. 인자 없는
`install` / `sync`는 `enabled = true` 스킬만 대상으로 하며, `--all`을 넘기면
등록된 모든 스킬을 대상으로 합니다.

## 테스트

```bash
uv run pytest
```

## 라이선스

[MIT](LICENSE)
