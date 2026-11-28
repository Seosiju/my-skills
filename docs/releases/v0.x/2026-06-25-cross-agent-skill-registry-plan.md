# my-skills 기획서

> 개인용 Cross-Agent Skill Registry 및 동기화 도구  
> 작성일: 2026-06-25  
> 상태: 구현 기준안 v1  
> 대상 저장소: `my-skills`

---

## 1. 요약

`my-skills`는 한 번 작성한 Agent Skill을 여러 AI agent에서 동일하게 사용할 수 있도록 관리, 설치, 동기화하는 개인용 로컬 registry다.

사용자는 Claude Code, Codex, Hermes 중 어느 환경에서든 skill을 만들 수 있다. 완성된 skill은 `my-skills` 저장소의 canonical skill directory로 가져오며, 이후 `my-skills`가 각 host의 discovery 경로에 안전하게 배포한다.

```text
어느 agent에서든 skill 작성
        ↓
my-skills의 canonical skill로 등록
        ↓
Agent Skills 표준 검증
        ↓
host별 설치 계획 생성
        ↓
Claude / Codex / Hermes에 설치
        ↓
수정 사항 동기화 및 drift 검사
```

이 프로젝트의 핵심은 `cli-inventory`나 `personal-profile` 자체가 아니다. 이들은 `my-skills` 위에서 배포되는 개별 skill이다.

한 줄 제품 정의:

> 내가 만든 Agent Skill을 한 곳에서 관리하고, 내가 사용하는 모든 AI agent에 안전하게 설치하고 동기화한다.

---

## 2. 배경과 문제

### 2.1 Skill이 host별로 분산된다

여러 AI agent를 함께 사용하면 skill이 각 host의 디렉터리에 따로 저장된다.

```text
Claude: ~/.claude/skills/
Codex:  ~/.agents/skills/
Hermes: ~/.hermes/skills/
```

한 host에서 만든 skill을 다른 host에서도 쓰려면 수동 복사와 수정이 필요하다. 시간이 지나면 같은 이름의 skill이 서로 다른 내용을 가지게 된다.

### 2.2 어느 파일이 원본인지 알기 어렵다

각 host가 자기 skill을 수정할 수 있으므로 다음 문제가 생긴다.

- 어떤 사본이 최신인지 판단하기 어렵다.
- 수정 사항을 다른 host에 반영하지 못한다.
- 실수로 생성물이나 설치된 사본만 수정할 수 있다.
- 기존 skill을 덮어쓰거나 잃어버릴 위험이 있다.

### 2.3 설치 경로와 지원 기능이 host마다 다르다

주요 host는 Agent Skills 표준을 공유하지만 discovery 경로, 추가 metadata, reload 방식, host 확장 기능은 서로 다르다.

따라서 모든 skill을 host별로 완전히 다시 작성할 필요는 없지만, 설치 대상과 호환성은 host별 contract로 관리해야 한다.

### 2.4 기존 도구는 목적이 더 넓거나 다르다

- gstack은 자체 workflow skill pack을 여러 host에 배포한다.
- Microsoft APM은 agent 설정 전반을 위한 범용 dependency manager다.
- `gh skill`은 GitHub 기반 skill 설치, 버전 고정, provenance에 강하다.
- AgentSync 계열은 rules 및 agent configuration 동기화에 초점을 둔다.
- TerminalSkills는 공개 skill catalog에 가깝다.

`my-skills`는 이들과 경쟁하는 범용 package manager가 아니다. 개인이 직접 만든 local skill collection을 최소한의 절차로 여러 host에 배포하는 데 집중한다.

---

## 3. 목표

### 3.1 핵심 목표

1. `skills/<name>/`을 canonical source로 관리한다.
2. Agent Skills 표준을 만족하는지 배포 전에 검증한다.
3. Claude Code, Codex, Hermes에 같은 skill bundle을 설치한다.
4. 설치 전에 변경 사항과 충돌을 보여준다.
5. `sync --check`로 canonical과 설치본 사이의 drift를 탐지한다.
6. `uninstall`은 `my-skills`가 설치한 파일만 제거한다.
7. macOS에서 먼저 완성하고 Windows 및 WSL2로 확장할 수 있는 구조를 만든다.

### 3.2 사용자 목표

사용자는 다음 작업을 짧고 예측 가능한 명령으로 수행할 수 있어야 한다.

```bash
my-skills new email-drafting
my-skills validate
my-skills install --all
my-skills sync
my-skills sync --check
my-skills status
my-skills uninstall --host hermes
```

### 3.3 성공 기준

MVP는 다음 조건을 모두 만족할 때 완료된다.

- 하나의 canonical skill이 세 host에서 발견된다.
- 세 host가 동일한 `SKILL.md`와 supporting files를 읽을 수 있다.
- canonical 수정 후 한 번의 sync로 모든 관리 대상 설치본이 갱신된다.
- unmanaged skill이나 사용자가 직접 만든 파일은 삭제하거나 덮어쓰지 않는다.
- 동일 상태에서 sync를 반복해도 추가 변경이 발생하지 않는다.
- clean temporary HOME에서 install, check, uninstall integration test가 통과한다.
- 실제 Claude Code, Codex, Hermes에서 대표 skill을 한 번씩 호출해 동작을 확인한다.

---

## 4. 비목표

MVP에서는 다음을 만들지 않는다.

- 공개 marketplace
- 원격 package dependency resolution
- transitive dependencies
- 조직 단위 policy engine
- MCP server, hook, plugin 전체 배포
- cloud synchronization service
- background daemon
- 자동 watch mode
- agent가 canonical source를 자동 수정하는 기능
- skill 품질을 자동으로 보장하는 범용 LLM evaluator
- `personal-profile`의 실제 개인정보 저장 체계
- 현재 컴퓨터의 전체 CLI inventory 수집 체계

이 기능들은 core install 및 sync 흐름이 검증된 이후 별도 단계에서 다룬다.

---

## 5. 제품 원칙

### 5.1 Agent Skills 표준이 canonical format이다

Canonical skill은 agentskills.io의 Agent Skills directory 구조를 따른다.

```text
skills/<skill-name>/
├── SKILL.md
├── scripts/       # optional
├── references/    # optional
├── assets/        # optional
└── agents/        # optional host metadata
```

필수 frontmatter:

```yaml
---
name: email-drafting
description: Drafts and revises email messages. Use when the user asks to write, reply to, shorten, or improve an email.
---
```

Canonical frontmatter는 기본적으로 표준 필드만 사용한다.

- `name`
- `description`
- `license`
- `compatibility`
- `metadata`
- `allowed-tools`는 host 지원 차이가 있으므로 경고 대상이다.

### 5.2 변환보다 pass-through를 우선한다

Claude Code, Codex, Hermes는 Agent Skills 표준을 지원한다. 따라서 MVP의 기본 배포는 canonical directory를 그대로 복사하는 방식이다.

```text
canonical skill
├── 표준 호환 host: 그대로 설치
└── host 차이 존재: 필요한 파일만 추가하거나 제한적으로 변환
```

모든 host를 위해 별도의 `outputs/<host>/`를 항상 생성하지 않는다. 변환이 필요하지 않은 skill까지 복제하면 불필요한 drift와 복잡성이 생긴다.

### 5.3 Host adapter는 예외 처리 계층이다

Host adapter는 다음 경우에만 사용한다.

- host별 추가 metadata가 필요하다.
- 표준 frontmatter 필드를 host가 지원하지 않는다.
- host별 경로나 tool 명칭을 skill 본문에서 피할 수 없다.
- 특정 host에서는 skill을 설치하면 안 된다.

가능하면 skill 본문은 host 중립적으로 작성한다.

나쁜 예:

```text
Run ~/.claude/skills/my-skill/scripts/run.py
```

좋은 예:

```text
Run scripts/run.py from the skill directory.
```

### 5.4 Canonical source와 machine state를 분리한다

Git으로 관리할 정보:

- canonical skills
- manifest
- target 기본값
- schema
- tests
- documentation

컴퓨터별 local state (설치 bookkeeping):

- 실제 설치 경로
- 설치된 host
- 설치 방식
- 설치 시각
- canonical content hash
- 설치본 content hash
- 백업 위치

Local state는 저장소에 commit하지 않는다.

위 install state와 별개로, skill이 다루는 **실제 데이터**(예: `personal-profile`의
memory, `cli-inventory`의 측정 결과)도 commit하지 않는다. 이 데이터는 여러 host가
공유해야 하므로 §15.4의 공유 데이터 루트에 둔다.

### 5.5 안전한 기본값을 사용한다

- 기본 설치 방식은 copy다.
- 개발 중에는 명시적인 `--mode link`를 제공할 수 있다.
- 충돌 시 기본 동작은 실패다.
- `--force` 없이 unmanaged 파일을 덮어쓰지 않는다.
- 삭제는 state에 기록된 managed artifact에만 허용한다.
- 변경 전 dry-run 결과를 생성할 수 있어야 한다.
- skill 인자 없이 실행한 `install` 또는 `sync`는 manifest에서 `enabled = true`인 skill만 대상으로 한다. 등록된 모든 skill을 무조건 동기화하지 않는다.
- `enabled = false`인 skill은 명령에서 빠지며, 전체를 대상으로 하려면 `--all`을 명시해야 한다.

---

## 6. 선행 프로젝트에서 가져올 것

### 6.1 gstack

가져올 패턴:

- 선언형 host config
- host registry
- host별 discovery 경로와 frontmatter 제한
- 모든 host를 대상으로 하는 parameterized test
- generation 및 install freshness 검사
- Unix symlink와 Windows copy fallback
- health check
- upgrade migration

가져오지 않을 것:

- gstack의 고정된 workflow skill set
- Claude-first template rewrite 구조 전체
- browser daemon 및 제품 workflow
- 초기 MVP에 불필요한 대규모 resolver system

### 6.2 Microsoft APM

가져올 패턴:

- manifest 기반 재현성
- lock 또는 state를 통한 content hash 기록
- 설치 전 security validation
- provenance와 audit 가능성

MVP에서 직접 사용하지 않는 이유:

- `my-skills`는 local canonical collection의 동기화가 우선이다.
- APM의 dependency resolution과 package ecosystem은 현재 범위보다 크다.
- Hermes까지 포함한 개인용 host 운영 방식은 별도 확인이 필요하다.

향후 선택지:

- `my-skills export --format apm`
- 외부 skill dependency 설치를 APM에 위임

### 6.3 GitHub CLI `gh skill`

가져올 패턴:

- source provenance
- tag 또는 commit pinning
- content-addressed change detection
- publish 전 validation

MVP에서 직접 사용하지 않는 이유:

- local working copy의 빠른 multi-host sync가 먼저다.
- 모든 skill을 GitHub release로 발행하도록 강제하지 않는다.

향후 선택지:

- `my-skills publish`를 `gh skill publish`에 연결
- 외부 skill 설치를 `gh skill install`에 위임

### 6.4 AgentSync 계열

가져올 패턴:

- `sync`
- `sync --check`
- drift report
- 향후 `sync --watch`

차이:

- rules 파일이 아니라 Agent Skill directory가 관리 단위다.

### 6.5 TerminalSkills

가져올 패턴:

- 표준 `SKILL.md` 중심의 단순한 skill catalog 구조
- skill 단위 설치

차이:

- 공개 catalog가 아니라 개인 canonical registry다.

---

## 7. 핵심 설계 결정

### 결정 1. MVP에는 상시 generator가 없다

모든 MVP host가 Agent Skills 표준을 지원하므로 canonical skill directory 자체가 배포 artifact다.

별도 build output은 다음 조건에서만 생성한다.

- host-specific overlay가 존재한다.
- canonical에 포함할 수 없는 host 전용 파일이 필요하다.
- 호환되지 않는 frontmatter를 제거해야 한다.

이 경우 임시 build directory를 사용한다.

```text
.my-skills/build/<host>/<skill-name>/
```

`.my-skills/`는 gitignore 대상이다.

### 결정 2. Release install은 copy, development install은 link다

`copy`:

- 기본값
- 설치본이 canonical 변경으로 즉시 바뀌지 않는다.
- sync 시 diff와 검증을 거친다.
- Windows에서도 예측 가능하다.

`link`:

- 명시적 개발 모드
- canonical 수정이 즉시 반영된다.
- symlink를 안정적으로 지원하는 환경에서만 사용한다.

### 결정 3. Host별 설치 경로는 선언형 registry로 관리한다

MVP user scope 기본값:

| Host | 기본 경로 | 비고 |
|------|-----------|------|
| Claude Code | `~/.claude/skills/` | 개인 skill 경로 |
| Codex | `~/.agents/skills/` | 공식 user scope |
| Hermes | `~/.hermes/skills/` | primary skill directory |

MVP에서는 host별 소유권과 uninstall을 명확하게 하기 위해 각 host의 기본 경로를 유지한다. 이후 여러 host가 같은 skill directory를 안정적으로 공유할 수 있음이 확인되면 shared target mode를 별도로 검토한다.

### 결정 4. 설치 state가 소유권의 기준이다

설치 디렉터리에 파일이 있다는 이유만으로 `my-skills`가 소유한다고 판단하지 않는다.

State에 기록된 항목만 update 또는 uninstall할 수 있다.

### 결정 5. Skill 내용은 기본적으로 host 중립적이어야 한다

Canonical skill은 특정 host의 slash command, 절대 경로, 전용 tool 이름에 의존하지 않는 것을 기본으로 한다.

Host 전용 동작이 필요한 경우 다음 중 하나를 사용한다.

1. `compatibility`에 제한을 명시한다.
2. manifest에서 지원 host를 제한한다.
3. host overlay를 추가한다.

---

## 8. 사용자 흐름

### 8.1 초기 설정

```bash
git clone <my-skills-repo>
cd my-skills
my-skills doctor
my-skills install --all --dry-run
my-skills install --all
```

결과:

- 지원 host 설치 여부 탐지
- 각 target path 확인
- 충돌 여부 확인
- 선택된 skill 설치
- 설치 state 기록

### 8.2 새 skill 생성

```bash
my-skills new email-drafting
```

생성 결과:

```text
skills/email-drafting/
└── SKILL.md
```

그다음:

```bash
$EDITOR skills/email-drafting/SKILL.md
my-skills validate email-drafting
my-skills sync --all
```

### 8.3 다른 agent에서 만든 skill 가져오기

예를 들어 Hermes가 `~/.hermes/skills/repo-analysis/`를 생성한 경우:

```bash
my-skills import ~/.hermes/skills/repo-analysis
```

Import 동작:

1. Agent Skills 표준 검증
2. 동일 이름 canonical skill 확인
3. 신규면 `skills/repo-analysis/`로 복사
4. 기존이면 diff 표시 후 명시적 선택 요구
5. canonical 등록 후 다른 host로 sync

Import는 MVP 후반 또는 v1.1 범위로 둔다. MVP에서는 수동 복사 후 `validate`로 대체할 수 있다.

### 8.4 Skill 수정 후 동기화

```bash
my-skills sync
```

동작:

1. canonical validation
2. manifest와 target 해석
3. 현재 설치본 hash 비교
4. managed install의 local modification 탐지
5. 변경 계획 출력
6. 안전하게 교체
7. 재검증
8. state 갱신

### 8.5 Drift 검사

```bash
my-skills sync --check
```

출력 예:

```text
email-drafting
  claude  FRESH
  codex   STALE    canonical changed
  hermes  MISSING
```

`--check`는 파일을 수정하지 않으며 drift가 있으면 non-zero로 종료한다.

### 8.6 제거

```bash
my-skills uninstall email-drafting --host hermes
```

제거 조건:

- state에 managed install로 기록되어 있어야 한다.
- 설치본이 사용자가 수정한 상태면 경고하고 기본적으로 중단한다.
- unmanaged sibling files는 보존한다.

---

## 9. CLI 명세

### 9.1 `init`

```bash
my-skills init
```

역할:

- `my-skills.toml` 생성
- `skills/` 생성
- `.gitignore`에 local state 경로 추가

### 9.2 `new`

```bash
my-skills new <name>
```

검증:

- lowercase alphanumeric 및 hyphen
- 64자 이하
- 선행, 후행, 연속 hyphen 금지
- 같은 이름의 canonical skill이 있으면 실패

### 9.3 `validate`

```bash
my-skills validate [skill]
```

검사:

- `SKILL.md` 존재
- YAML frontmatter 존재
- `name` 및 `description`
- directory와 `name` 일치
- description 1024자 이하
- supporting file reference 존재 여부
- absolute host path 누출
- 알려진 secret pattern
- hidden or bidirectional Unicode
- manifest의 host compatibility

### 9.4 `doctor`

```bash
my-skills doctor
```

출력:

- OS 및 shell
- 발견한 host executable
- target path
- target write permission
- symlink 지원 여부
- manifest 유효성
- local state 접근 가능 여부

### 9.5 `install`

```bash
my-skills install [skill] [--host <host>] [--all] [--mode copy|link] [--dry-run]
```

기본 동작:

- skill 미지정 시 manifest에서 `enabled = true`인 skill만 대상으로 한다. 등록된 모든 skill을 무조건 설치하지 않는다.
- host 미지정 시 manifest의 enabled target
- `--all`은 모든 enabled skill과 enabled target을 명시적으로 선택하는 shortcut
- mode 미지정 시 copy
- 충돌 시 실패

### 9.6 `sync`

```bash
my-skills sync [skill] [--host <host>] [--all] [--check]
```

기본 동작:

- skill 미지정 시 manifest에서 `enabled = true`인 skill만 대상으로 한다. 등록된 모든 skill을 무조건 동기화하지 않는다.
- `--all`은 모든 enabled skill과 enabled target을 명시적으로 선택하는 shortcut이다.

`install`과 차이:

- 기존 managed install을 기준으로 갱신한다.
- 사용자가 설치본을 직접 수정한 drift를 구분한다.

### 9.7 `status`

```bash
my-skills status
```

상태:

- `FRESH`
- `STALE`
- `DRIFTED`
- `MISSING`
- `CONFLICT`
- `UNMANAGED`
- `UNSUPPORTED`

### 9.8 `uninstall`

```bash
my-skills uninstall [skill] --host <host|all>
```

State에 기록된 managed install만 제거한다.

---

## 10. Manifest

MVP 구현은 Python 3.11+와 `uv` 기반 package를 기준으로 한다. Repository manifest는 Python 표준 라이브러리의 `tomllib`으로 읽을 수 있는 TOML을 사용한다.

Agent Skills frontmatter는 YAML이므로 자체 YAML parser를 만들지 않는다. Frontmatter parsing에는 검증된 YAML library를 사용하고, 가능하면 Agent Skills 공식 reference validator인 `skills-ref` 결과도 교차 검증한다.

`my-skills.toml` 예시:

```toml
schema_version = 1
skills_root = "skills"

[defaults]
install_mode = "copy"
collision = "error"
verify_after_install = true

[targets.claude]
enabled = true
scope = "user"
path = "~/.claude/skills"

[targets.codex]
enabled = true
scope = "user"
path = "~/.agents/skills"

[targets.hermes]
enabled = true
scope = "user"
path = "~/.hermes/skills"

[skills.shared-agent-operation]
enabled = true
hosts = ["claude", "codex", "hermes"]

[skills.cli-inventory]
enabled = true
hosts = ["claude", "codex", "hermes"]
```

Manifest 원칙:

- target 기본 경로는 코드에도 내장하되 manifest가 override할 수 있다.
- 실제 path는 실행 시 home directory 기준으로 resolve한다.
- host별 skill enable/disable을 지원한다.
- machine별 override는 commit하지 않는 `my-skills.local.toml`에서 처리한다.

우선순위:

```text
CLI option
→ my-skills.local.toml
→ my-skills.toml
→ built-in default
```

---

## 11. Local state

기본 state 경로:

```text
macOS/Linux:
  $XDG_STATE_HOME/my-skills/state.json
  fallback: ~/.local/state/my-skills/state.json

Windows:
  %LOCALAPPDATA%\my-skills\state.json
```

예시:

```json
{
  "schema_version": 1,
  "installs": [
    {
      "skill": "email-drafting",
      "host": "claude",
      "mode": "copy",
      "source": "/Users/example/git/my-skills/skills/email-drafting",
      "destination": "/Users/example/.claude/skills/email-drafting",
      "source_hash": "sha256:...",
      "installed_hash": "sha256:...",
      "installed_at": "2026-06-25T08:30:00Z"
    }
  ]
}
```

State write는 temporary file을 만든 뒤 atomic replace한다.

---

## 12. 설치 알고리즘

### 12.1 계획 단계

각 skill과 host 조합에 대해 다음을 계산한다.

```text
source
destination
source hash
destination existence
destination hash
managed ownership
compatibility
planned action
```

Planned action:

- `CREATE`
- `UPDATE`
- `NOOP`
- `REMOVE`
- `BLOCK_CONFLICT`
- `BLOCK_DRIFT`
- `SKIP_UNSUPPORTED`

### 12.2 Copy 설치

1. source validation
2. destination parent 생성
3. temporary sibling directory에 전체 복사
4. 복사본 validation
5. 기존 managed destination이 있으면 backup 또는 atomic rename
6. temporary directory를 destination으로 rename
7. destination hash 검증
8. state 갱신

실패 시 기존 destination을 보존하거나 복원한다.

### 12.3 Link 설치

1. platform symlink capability 확인
2. destination 충돌 확인
3. canonical skill directory를 가리키는 directory symlink 생성
4. link target 검증
5. state 기록

Windows에서는 Developer Mode와 권한 문제를 고려해 link 실패 시 자동 copy 전환 여부를 명시적으로 출력한다. 조용히 fallback하지 않는다.

### 12.4 Drift 처리

Canonical도 바뀌고 설치본도 직접 수정된 경우 자동 덮어쓰지 않는다.

```text
canonical changed + install unchanged → UPDATE
canonical unchanged + install changed → DRIFTED
canonical changed + install changed → CONFLICT
```

MVP에서는 conflict를 자동 merge하지 않는다.

---

## 13. Host contract

각 host는 동일한 contract를 구현한다.

```text
name
display_name
detect_commands
default_user_path
default_project_path
supports_symlink
reload_hint
frontmatter_policy
optional_metadata
```

개념 예시:

```python
HostConfig(
    name="codex",
    detect_commands=("codex",),
    default_user_path="~/.agents/skills",
    default_project_path=".agents/skills",
    supports_symlink=True,
    reload_hint="Codex detects changes automatically; restart if needed.",
)
```

Host별 조건문은 installer 곳곳에 흩어놓지 않고 registry를 통해 조회한다.

---

## 14. 저장소 구조

```text
my-skills/
├── README.md
├── pyproject.toml
├── uv.lock
├── my-skills.toml
├── .gitignore
│
├── docs/
│   ├── 2026-06-24-shared-agent-skills-setup-plan.md
│   └── 2026-06-25-cross-agent-skill-registry-plan.md
│
├── skills/
│   ├── cli-inventory/
│   │   ├── SKILL.md
│   │   ├── scripts/
│   │   └── references/
│   └── shared-agent-operation/
│       └── SKILL.md
│
├── src/
│   └── my_skills/
│       ├── cli.py
│       ├── config.py
│       ├── discovery.py
│       ├── validation.py
│       ├── planner.py
│       ├── installer.py
│       ├── state.py
│       ├── hashing.py
│       ├── security.py
│       └── hosts/
│           ├── base.py
│           ├── claude.py
│           ├── codex.py
│           └── hermes.py
│
├── tests/
│   ├── fixtures/
│   ├── test_validation.py
│   ├── test_planner.py
│   ├── test_installer.py
│   ├── test_state.py
│   ├── test_security.py
│   └── test_hosts.py
│
└── .my-skills/
    └── build/               # gitignored, host 변환이 필요할 때만 사용
```

초기 구현 중 파일 수가 과도해지면 `planner.py`와 `installer.py`, `hashing.py`와 `state.py`를 합쳐 시작할 수 있다. 위 구조는 책임 경계를 보여주는 목표 구조이지, Phase 1에서 모든 모듈을 미리 만들라는 의미가 아니다.

---

## 15. 보안과 개인정보

### 15.1 Skill은 실행 가능한 지시로 취급한다

Skill은 단순 문서가 아니다. Agent에게 명령 실행과 파일 접근을 지시할 수 있으므로 install 전에 검사해야 한다.

MVP validation:

- hidden bidirectional Unicode
- NUL byte
- symlink가 skill directory 밖을 가리키는지 여부
- secret 형태 문자열
- private key header
- 민감 파일명
- 절대 사용자 경로

검출 결과는 자동 수정하지 않고 실패 또는 경고로 보고한다.

### 15.2 개인정보 skill은 core와 분리한다

`personal-profile`은 v1 이후 추가할 수 있지만 다음 경계를 따른다.

```text
skills/personal-profile/        # canonical: 공개 가능한 지침과 schema (commit)
<shared data root>/personal-profile/   # 실제 private data (commit 안 함)
```

Canonical skill은 private data를 포함하지 않고 실제 데이터를 필요할 때 읽고 쓰는
방법만 정의한다. 실제 데이터가 놓이는 위치는 §15.4의 공유 데이터 루트를 따른다.

### 15.3 Machine inventory도 local data다

`cli-inventory`의 required tool 정책은 commit할 수 있지만 실제 hostname, 절대 path,
account, auth 상태는 machine-local data로 §15.4의 공유 데이터 루트에 저장한다.

### 15.4 공유 데이터 루트 (host 간 공유되는 실제 데이터)

데이터를 다루는 skill에는 두 종류의 non-canonical data가 있다.

1. **개발 중 repo-내부 임시 데이터** — repo 안의 gitignored `local/` (예:
   `local/cli-inventory/`). 단일 작업 디렉터리에서만 의미가 있다.
2. **여러 host가 공유해야 하는 실제 데이터** — `personal-profile`의 memory나
   누적되는 상태처럼, Claude / Codex / Hermes가 **같은 값**을 읽고 써야 하는 데이터.

문제: canonical skill은 `sync` 시 각 host 경로로 **복사**된다. 따라서 설치본
(`~/.claude/skills/<skill>/` 등) 옆의 상대경로 `local/`은 host마다 갈라지며, repo의
`local/`로도 이어지지 않는다. 즉 상대경로만으로는 host 간 공유가 성립하지 않는다.

해결: 머신당 하나인 **공유 데이터 루트**를 둔다. 모든 host의 설치본이 같은 절대
경로를 참조하므로 데이터가 자동으로 공유된다.

```text
$XDG_DATA_HOME/my-skills/<skill-name>/        # 기본
fallback: ~/.local/share/my-skills/<skill-name>/
Windows:  %LOCALAPPDATA%\my-skills\data\<skill-name>\
```

원칙:

- 이 공유 데이터 루트는 절대 commit하지 않는다 (machine-local).
- "skill 본문은 host 중립적이어야 한다"(§5.3)에 대한 **유일한 공식 예외**다. 이
  경로는 host가 아니라 `my-skills` 자체가 소유하는 머신 경로이므로 특정 host에
  종속되지 않는다.
- `my-skills`는 이 경로를 생성·노출하는 헬퍼(예: `my-skills data-path <skill>`)를
  제공해, skill 본문이 경로를 하드코딩하지 않고 일관되게 가리키도록 한다.
- 데이터를 저장할지, 다음 실행에서 재사용할지 새로 측정할지는 각 SKILL.md 지침이
  정한다. 공유 데이터 루트는 "어디에 두는가"만 보장한다.
- 민감 데이터가 없는 순수 지침 skill(`repo-analysis`, `shared-agent-operation`)은
  이 루트가 필요 없다. 데이터를 다루는 skill에만 적용되는 선택적 규칙이다.

---

## 16. 테스트 전략

### 16.1 Unit test

- skill name validation
- frontmatter parsing
- description limit
- manifest precedence
- path expansion
- content hash determinism
- state serialization
- drift classification
- collision policy
- security scan

### 16.2 Parameterized host test

모든 host config에 공통 test를 적용한다.

- 이름 중복 없음
- target path 유효
- source를 destination에 설치 가능
- optional metadata가 다른 host 설치를 깨뜨리지 않음
- install 후 expected `SKILL.md` 존재
- uninstall 후 managed artifact만 제거

새 host를 registry에 추가하면 기본 test suite에 자동 포함되어야 한다.

### 16.3 Integration test

Temporary HOME을 사용한다.

```text
canonical fixture
  → install all
  → installed content 검증
  → canonical 수정
  → sync --check 실패 검증
  → sync
  → fresh 상태 검증
  → 설치본 직접 수정
  → drift 검증
  → uninstall
  → unmanaged file 보존 검증
```

### 16.4 Cross-platform test

- macOS: 실제 개발 및 manual QA
- Linux: CI
- Windows: CI의 temporary directory 및 copy mode
- WSL2: Linux path와 Windows mount path를 구분한 smoke test

### 16.5 Real host manual QA

대표 skill 하나를 세 host에 설치하고 다음을 확인한다.

1. host가 skill을 목록에 표시한다.
2. explicit invocation이 가능하다.
3. description과 일치하는 자연어 요청에 skill이 활성화된다.
4. `references/`와 `scripts/`를 읽을 수 있다.
5. 수정 후 reload 또는 재시작 절차가 작동한다.

대표 skill은 private data가 없는 `shared-agent-operation` 또는 작은 `repo-analysis`를 우선한다. `cli-inventory`는 외부 CLI와 auth 상태 때문에 첫 end-to-end fixture로는 복잡하다.

---

## 17. 개발 단계

### Phase 0. 기준 확정

- [ ] 이 기획서를 구현 기준으로 승인
- [ ] 기존 문서는 역사적 배경 문서로 유지
- [ ] Agent Skills 표준 버전과 검증 규칙 기록
- [ ] Python 3.11+, `uv`, PyYAML 사용 확정

### Phase 1. Portable skill core

- [ ] `my-skills.toml`
- [ ] canonical `skills/`
- [ ] frontmatter parser
- [ ] Agent Skills validation
- [ ] host registry
- [ ] `doctor`
- [ ] `validate`

완료 증거:

- malformed fixture가 정확한 오류로 실패
- 세 host config가 공통 validation을 통과

### Phase 2. Safe install lifecycle

- [ ] install plan
- [ ] `--dry-run`
- [ ] copy install
- [ ] state 기록
- [ ] collision protection
- [ ] `status`
- [ ] `uninstall`

완료 증거:

- temporary HOME integration test
- unmanaged file 보존
- 반복 install이 NOOP

### Phase 3. Sync와 drift

- [ ] deterministic hashing
- [ ] `sync`
- [ ] `sync --check`
- [ ] drift 및 conflict 분류
- [ ] atomic update와 rollback

완료 증거:

- canonical 변경, install 변경, 양쪽 변경 test
- 실패한 update가 기존 설치본을 보존

### Phase 4. 실제 host 검증

- [ ] Claude Code
- [ ] Codex
- [ ] Hermes

완료 증거:

- 각 host의 list 또는 discovery 확인
- 대표 skill explicit invocation
- supporting file 접근 확인

### Phase 5. 최초 실제 skill

- [x] `shared-agent-operation`
- [x] `repo-analysis`
- [x] `cli-inventory`의 required-tool 정책
- [x] machine-local inventory 저장 위치 (repo-내부 `local/` 경계)

`personal-profile`은 privacy boundary와 실제 사용 시나리오가 확정된 뒤 추가한다.

### Phase 5.5. 공유 데이터 루트 (host 간 공유 데이터)

`personal-profile`처럼 여러 host가 같은 데이터를 공유해야 하는 skill의 전제 조건이다.
(§15.4 참고)

- [x] 공유 데이터 루트 경로 규칙 확정 (`$XDG_DATA_HOME/my-skills/<skill>/`,
      fallback `~/.local/share/my-skills/<skill>/`, Windows
      `%LOCALAPPDATA%\my-skills\data\<skill>\`) — `src/my_skills/data.py`
- [x] 경로를 생성·노출하는 헬퍼 `my-skills data-path <skill> [--create]`
      (manifest 불필요한 순수 path resolver, traversal 차단)
- [x] §5.3 host 중립성에 대한 공식 예외로 문서화 (§15.4)
- [x] `personal-profile`을 이 규칙의 첫 적용 사례로 구현 (memory 패턴: 저장 후 재사용)
      — `skills/personal-profile/` (SKILL.md + references/schema.md). canonical은
      지침·schema만, 실제 프로필은 공유 데이터 루트에 저장(commit 안 함)

### Phase 6. 확장

- [ ] `import`
- [ ] link development mode
- [ ] Windows 및 WSL2 실제 검증
- [ ] watch mode
- [ ] `gh skill` publish integration
- [ ] APM export 또는 dependency integration
- [ ] upgrade migration

---

## 18. 관측성과 오류 메시지

모든 mutation command는 다음 결과를 제공한다.

```text
planned
created
updated
unchanged
skipped
blocked
removed
```

오류 메시지는 다음 정보를 포함한다.

- skill
- host
- source
- destination
- 실패 원인
- 안전한 복구 방법

예:

```text
BLOCKED: email-drafting → hermes
Destination already exists and is not managed by my-skills:
  /Users/example/.hermes/skills/email-drafting

No files were changed.
Run `my-skills diff email-drafting --host hermes` to inspect the conflict.
```

---

## 19. 주요 위험과 대응

### 위험 1. 기존 ecosystem과 기능 중복

대응:

- local personal sync에 범위를 제한한다.
- remote dependencies는 APM 또는 `gh skill`과 통합한다.
- package manager 기능을 MVP에 넣지 않는다.

### 위험 2. Host 사양 변경

대응:

- host path와 capability를 registry에 격리한다.
- 공식 문서를 source로 기록한다.
- host contract test를 둔다.

### 위험 3. 설치된 skill을 host가 수정

Hermes처럼 agent가 skill을 직접 개선할 수 있는 host가 있다.

대응:

- install copy의 변경을 `DRIFTED`로 표시한다.
- 자동 덮어쓰지 않는다.
- 향후 `import` 또는 `adopt` 명령으로 canonical 반영을 지원한다.

### 위험 4. Copy와 link 의미 혼동

대응:

- 기본은 copy
- link는 명시적 개발 mode
- status에 mode 표시

### 위험 5. Host-specific extension이 portability를 깨뜨림

대응:

- canonical frontmatter portable subset 검사
- host compatibility warning
- overlay는 예외적으로만 허용

---

## 20. 최종 완료 정의

프로젝트의 첫 완성 기준은 다음 사용자 경험이다.

```bash
git clone <repo>
cd my-skills

my-skills doctor
my-skills validate
my-skills install --all
my-skills status
```

이후 사용자가 canonical skill 하나를 수정하고:

```bash
my-skills sync --check
my-skills sync
my-skills sync --check
```

를 실행했을 때 첫 check는 drift를 탐지하고, sync는 세 host를 갱신하며, 마지막 check는 성공해야 한다.

그리고 Claude Code, Codex, Hermes에서 동일한 대표 skill이 실제로 발견되고 호출되어야 한다.

이 observable behavior가 확인되기 전에는 프로젝트가 완성된 것으로 보지 않는다.

---

## 21. 참고 자료

2026-06-25 기준으로 확인한 자료:

- Agent Skills specification  
  https://agentskills.io/specification
- gstack host architecture  
  https://github.com/garrytan/gstack/blob/main/docs/ADDING_A_HOST.md
- Microsoft Agent Package Manager  
  https://github.com/microsoft/apm
- GitHub CLI skill management  
  https://github.blog/changelog/2026-04-16-manage-agent-skills-with-github-cli/
- Claude Code skills  
  https://code.claude.com/docs/en/skills
- Codex skills  
  https://developers.openai.com/codex/skills
- Hermes skills system  
  https://hermes-agent.nousresearch.com/docs/user-guide/features/skills
