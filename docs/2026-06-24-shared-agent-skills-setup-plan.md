# my-skills Setup Plan — 공통 Skill / 로컬 환경 / 다중 Agent·OS 지원

> **작성일:** 2026-06-24  
> **대상 repo:** `/Users/snu.sim/git/my-skills`  
> **목표:** kowausa가 자주 쓰는 공통 skill set을 하나의 repo에서 관리하고, `personal-profile`·`cli-inventory`처럼 필요한 context를 skill 구성요소로 포함해 macOS/Windows 및 Claude Code/Codex/Hermes에서 불러 쓸 수 있게 만든다.

---

## 0. 정정된 방향

이 repo의 핵심은 **개인정보를 숨기는 프로필 repo**가 아니라, 다음을 함께 관리하는 **공통 skill 운영 repo**다.

1. **공통 skill**  
   kowausa가 여러 agent에서 반복해서 쓰고 싶은 skill/workflow를 canonical source로 관리한다.

2. **로컬 환경/CLI inventory skill**  
   이것은 repo 전체의 목표가 아니라, 이 repo에서 만들 핵심 공통 skill 중 하나다. 이 skill은 현재 컴퓨터에 어떤 CLI가 설치되어 있고, 어디에 있으며, 어떤 용도로 쓰는지 파악하고 agent가 안전하게 활용하게 한다.  
   예: `gh`, `twg`, `gws`, `codex`, `claude`, `omx`, `lazycodex`, `codegraph` 등.

3. **`personal-profile` 공통 skill**  
   이것도 repo 전체의 목표가 아니라, 이 repo에서 만들 핵심 공통 skill 중 하나다. 이 skill은 단순한 작업 선호뿐 아니라, kowausa가 자주 언급해야 해서 귀찮은 기본 신상정보를 agent가 필요할 때 참고하게 한다.  
   예: 거주지, 출생연도, 자주 쓰는 이메일 주소 등.

4. **다중 환경 지원**  
   같은 skill repo를 현재 Mac뿐 아니라 다른 Mac, Windows PC, WSL2, Claude Code, Codex, Hermes에서도 사용할 수 있게 설계한다.

즉 목표는 다음에 가깝다.

```text
내가 자주 쓰는 skill과 나의 기본 context를 한 곳에 둔다
→ 각 컴퓨터와 agent가 자기 환경에 맞게 가져다 쓴다
→ 로컬 CLI/경로 차이는 environment inventory로 분리한다
```

---

## 1. 문제 정의

현재 kowausa는 Claude Code, Codex, Hermes를 함께 사용한다. 문제는 각 도구가 skill을 읽는 위치·형식·호출 방식이 다르고, 사용자 profile이나 로컬 CLI 환경 같은 공통 context도 agent별로 일관되게 배포되지 않는다는 점이다.

문제는 다음과 같다.

### 1.1 Skill이 agent별로 따로 논다

Claude Code에서 쓰는 skill과 Codex/Hermes에서 쓰는 skill이 서로 동기화되지 않는다.

예:

```text
Claude Code에는 어떤 workflow가 있음
Codex에는 없음
Hermes에는 비슷하지만 다른 버전이 있음
```

이러면 같은 작업을 하더라도 agent마다 품질과 전제가 달라진다.

### 1.2 로컬 CLI 지식이 agent마다 다르다

현재 컴퓨터에 어떤 CLI가 설치되어 있는지, 어떤 인증 상태인지, 어떤 경로로 실행해야 하는지 agent들이 일관되게 알지 못한다.

예:

```text
gh: GitHub CLI, 인증됨
twg: Jira/Atlassian CLI, ~/.local/bin/twg
gws: Google Workspace CLI, Homebrew 설치
codex: npx fallback 필요 가능
codegraph: 구조 분석용 CLI/MCP
```

이런 정보는 Claude/Codex/Hermes 모두에게 중요하다.

### 1.3 사용자 기본 정보 반복 입력이 귀찮다

kowausa가 자주 언급하는 기본 정보가 있다.

예:

- 거주지
- 출생연도
- 자주 쓰는 이메일 주소
- 자주 쓰는 계정/핸들
- 작업 스타일과 선호

이런 정보는 민감할 수 있지만, 사용자가 반복 입력을 줄이기 원한다면 저장할 수 있어야 한다. 다만 저장 위치와 공개 범위는 분리해야 한다.

### 1.4 컴퓨터가 바뀌면 다시 세팅해야 한다

현재 Mac에서 잘 동작하더라도, 다른 Mac이나 Windows PC에서 같은 skill을 쓰려면 다시 세팅해야 한다.

따라서 repo는 다음을 고려해야 한다.

```text
현재 Mac local inventory
다른 Mac에서의 bootstrap
Windows / WSL2에서의 bootstrap
agent별 generated output
환경별 missing CLI detection
```

---

## 2. 핵심 목표

MVP의 핵심 목표는 **공통 skill repo로서 동작하는 것**이다.

우선순위는 다음과 같다.

1. **공통 skill을 canonical source로 관리**
2. **`cli-inventory` 공통 skill을 만든다** — 이 skill이 현재 컴퓨터의 CLI/local environment를 scan·기록·활용하게 한다.
3. **Claude Code / Codex / Hermes용 output을 생성**
4. **macOS / Windows / WSL2 같은 환경 차이를 기록**
5. **`personal-profile` 공통 skill을 만든다** — 이 skill이 사용자 기본 profile 자료를 읽고, 민감도와 공유 범위에 맞게 활용하게 한다.

---

## 3. 참고 설계: gstack에서 가져올 것

`garrytan/gstack`은 다음 면에서 참고가 된다.

- 하나의 skill source를 여러 host용으로 생성한다.
- host별 경로와 용어 차이를 adapter로 처리한다.
- Windows/macOS/Linux 차이를 setup/test/path 수준에서 분기한다.
- generated skill과 source template의 freshness를 검사한다.
- local state는 repo 밖에 둔다.

이 repo는 gstack의 다음 패턴을 차용한다.

```text
canonical source
→ host adapter
→ platform adapter
→ generated output
→ local state / local inventory
```

단, gstack처럼 Claude-first rewrite에 과도하게 의존하지 않고, 처음부터 Claude/Codex/Hermes를 동등한 target으로 본다.

---

## 4. 설계 원칙

### 4.1 Skill이 중심이다

이 repo의 1차 목적은 profile repo가 아니라 **skill repo**다.

```text
canonical/skills = 사람이 관리하는 원본
outputs/*/skills = agent별 생성물
```

### 4.2 환경 정보는 profile과 분리한다

사용자 profile과 local machine inventory는 다르다.

```text
사용자 profile:
  kowausa에 대한 정보. 컴퓨터가 바뀌어도 유지될 수 있음.

machine inventory:
  특정 컴퓨터에 설치된 CLI, 경로, OS, shell, 인증 상태.
  컴퓨터마다 다름.
```

따라서 둘을 분리해야 한다.

### 4.3 개인정보는 저장 가능하지만 scope를 분명히 한다

kowausa가 원하면 거주지, 출생연도, 이메일 같은 기본 신상정보도 저장한다. 다만 다음을 구분한다.

```text
public-ish profile:
  agent가 답변에 참고해도 되는 기본 정보

private profile:
  로컬에서만 쓰거나 private repo에서만 관리할 정보

secrets:
  저장 금지. API key, token, password, recovery code 등
```

### 4.4 다른 컴퓨터를 고려한다

현재 Mac의 CLI inventory는 중요하지만, 그것만 전부가 아니다.

다른 Mac/Windows에서 setup할 때는 다음이 필요하다.

- 현재 환경 scan
- required/recommended CLI 목록과 비교
- missing CLI report
- 설치 가이드 생성
- OS별 path 차이 처리

### 4.5 generated output은 직접 수정하지 않는다

각 agent가 읽는 파일은 생성물이다.

```text
canonical/* 수정
→ generator 실행
→ outputs/* 생성
```

---

## 5. 제안 디렉토리 구조

```text
my-skills/
├── README.md
├── docs/
│   └── 2026-06-24-shared-agent-skills-setup-plan.md
│
├── canonical/
│   ├── skills/
│   │   ├── cli-inventory/
│   │   │   ├── SKILL.md
│   │   │   └── data/
│   │   │       ├── required-cli.yaml
│   │   │       └── current-machine.yaml
│   │   ├── shared-agent-operation/
│   │   │   └── SKILL.md
│   │   ├── repo-analysis/
│   │   │   └── SKILL.md
│   │   └── personal-profile/
│   │       ├── SKILL.md
│   │       └── data/
│   │           ├── profile.yaml
│   │           └── profile.private.example.yaml
│   │
│   └── workflows/
│       ├── research.md
│       ├── planning.md
│       └── repo-analysis.md
│
├── hosts/
│   ├── claude.yaml
│   ├── codex.yaml
│   └── hermes.yaml
│
├── platforms/
│   ├── macos.yaml
│   ├── windows.yaml
│   ├── wsl2.yaml
│   └── linux.yaml
│
├── generator/
│   └── build.py
│
├── outputs/
│   ├── claude/
│   ├── codex/
│   └── hermes/
│
├── local/
│   ├── README.md
│   └── .gitkeep
│
├── policy/
│   ├── sensitivity-policy.md
│   ├── secret-policy.md
│   └── write-authority.md
│
├── schemas/
│   ├── profile.schema.json
│   ├── cli-inventory.schema.json
│   └── skill.schema.json
│
└── tests/
    ├── test_generation.py
    ├── test_cli_inventory.py
    └── test_no_secrets.py
```

변경점:

- 기존 `generated/` 대신 `outputs/`를 사용한다. 이유: agent별 실제 output이라는 의미가 더 명확하다.
- `personal-profile`의 실제 프로필 자료는 `canonical/skills/personal-profile/data/` 안에 둔다. 즉 프로필은 별도 repo 목표가 아니라 skill의 구성요소다.
- `cli-inventory`의 CLI 목록과 현재 machine snapshot은 `canonical/skills/cli-inventory/data/` 안에 둔다. 즉 환경 inventory도 별도 목표가 아니라 skill의 구성요소다.
- `local/`은 gitignore 대상 local-only override 공간으로 둔다.

---

## 6. `personal-profile` Skill 설계

이 섹션의 핵심은 “프로필 문서 저장”이 아니라 **내 프로필이 담긴 공통 skill을 만드는 것**이다. Claude Code, Codex, Hermes가 모두 같은 `personal-profile` skill을 불러서 kowausa의 기본 context를 참고하게 한다.

### 6.1 `canonical/skills/personal-profile/data/profile.yaml`

`personal-profile` skill이 읽는 사용자 기본 정보 자료다.

예상 구조:

```yaml
schema_version: 1
user:
  preferred_name: kowausa
  preferred_language: ko

personal_context:
  birth_year: 1995
  location: Seoul
  emails:
    primary: null  # 실제 작성 여부는 사용자 승인 후

work_context:
  assumes_user_is_developer: false
  main_use_cases:
    - research
    - personal knowledge Q&A
    - PRD / planning / documentation
    - repo analysis

communication:
  be_direct: true
  avoid_scripted_roleplay: true
  prefer_korean: true
```

주의:

- 이메일 주소, 생년, 거주지 등은 사용자가 명시적으로 원하면 저장한다.
- API key, token, password는 절대 넣지 않는다.
- public/private 구분이 애매하면 `profile.private.yaml` 또는 local state로 뺀다.

### 6.2 `canonical/skills/personal-profile/data/profile.private.example.yaml`

실제 private profile의 예시 파일만 repo에 둔다.

```yaml
schema_version: 1
private_user_context:
  legal_name: null
  phone: null
  address: null
  private_emails: []
```

실제 private 값은 다음 중 하나에 둔다.

```text
local/profile.private.yaml
또는 private repo
또는 OS keychain/1Password
```

---

## 7. CLI / Local Environment 설계

MVP에서 가장 먼저 만들 핵심 skill이다. 즉 `CLI / Local Environment`는 repo 전체 방향이 아니라, 공통 skill 중 하나의 구체적인 주제다.

### 7.1 `canonical/skills/cli-inventory/data/required-cli.yaml`

kowausa가 여러 컴퓨터에서 공통으로 쓰고 싶은 CLI 목록이다.

예상 구조:

```yaml
schema_version: 1
required_tools:
  - name: git
    purpose: version control
    required: true
    platforms: [macos, windows, wsl2, linux]

  - name: gh
    purpose: GitHub CLI
    required: true
    auth_required: true
    platforms: [macos, windows, wsl2, linux]

  - name: claude
    purpose: Claude Code CLI
    required: false
    auth_required: true
    platforms: [macos, windows, wsl2]

  - name: codex
    purpose: OpenAI Codex CLI
    required: false
    auth_required: true
    fallback: npx --yes @openai/codex

  - name: twg
    purpose: Jira / Atlassian CLI
    required: false
    auth_required: true
    notes: Do not print credential values.

  - name: gws
    purpose: Google Workspace CLI
    required: false
    auth_required: true

  - name: codegraph
    purpose: code structure graph and MCP assistance
    required: false
    auth_required: false
```

### 7.2 `canonical/skills/cli-inventory/data/current-machine.yaml`

현재 컴퓨터에서 scan한 결과를 기록한다.

예상 구조:

```yaml
schema_version: 1
machine:
  hostname: snumac
  os: macos
  shell: zsh
  home: /Users/snu.sim

installed_tools:
  - name: gh
    detected: true
    path: /opt/homebrew/bin/gh
    version: null
    auth_status: authenticated

  - name: twg
    detected: true
    path: /Users/snu.sim/.local/bin/twg
    version: null
    auth_status: authenticated
```

이 파일은 현재 Mac의 snapshot이다. 다른 컴퓨터에서는 다시 scan해서 별도 파일을 만들 수 있다.

향후 확장:

```text
canonical/skills/cli-inventory/data/machines/snumac.yaml
canonical/skills/cli-inventory/data/machines/work-windows.yaml
canonical/skills/cli-inventory/data/machines/macbook-air.yaml
```

### 7.3 CLI scan script

나중에 `generator/scan_cli.py` 또는 `scripts/scan-cli.py`를 만든다.

역할:

1. required CLI 목록을 읽는다.
2. `command -v`로 설치 여부 확인한다.
3. 가능한 경우 version을 확인한다.
4. auth 상태는 위험하지 않은 명령으로만 확인한다.
5. 결과를 `current-machine.yaml`에 쓴다.
6. missing CLI report를 만든다.

---

## 8. Skill 설계

### 8.1 `canonical/skills/cli-inventory/SKILL.md`

목적:

- agent가 로컬 CLI를 사용할 때 어떤 도구가 있는지 확인하게 한다.
- 인증/secret 값을 출력하지 않도록 한다.
- OS별 경로 차이를 이해하게 한다.

내용:

- required CLI 목록 참조
- current-machine inventory 참조
- `command -v`, `--version` 등 안전한 확인 방법
- credential 값을 출력하지 말 것
- 다른 컴퓨터에서는 scan 먼저 수행할 것

### 8.2 `canonical/skills/shared-agent-operation/SKILL.md`

목적:

- Claude/Codex/Hermes가 공통으로 따라야 할 운영 방식 정의

내용:

- 한국어 선호
- 실행 결과를 검증하고 말하기
- 모호하면 작은 선택지 제시
- generated output 직접 수정 금지
- canonical 변경은 승인 기반

### 8.3 `canonical/skills/personal-profile/SKILL.md`

목적:

- 사용자 profile 자체를 담고 사용하는 공통 skill이다.
- agent가 사용자 기본 context를 확인할 위치와 사용 방식을 안내한다.

내용:

- `canonical/skills/personal-profile/data/profile.yaml` 읽기
- private profile은 로컬 파일이나 private source에서만 읽기
- 민감 정보는 응답에 불필요하게 노출하지 않기
- 이메일/생년/거주지 등은 사용자가 원하는 경우만 활용

### 8.4 `canonical/skills/repo-analysis/SKILL.md`

목적:

- repo 분석 방식의 공통화

내용:

- LazyCodex/OMO audit trail 선호
- preflight 후 분석 방향 선택
- CodeGraph 사용 가능 시 구조 분석에 활용
- 로그와 summary 저장

---

## 9. Host adapter 설계

### 9.1 Claude Code

```yaml
host: claude
output_dir: outputs/claude
entry_file: CLAUDE.md
skills_dir: skills
profile_format: markdown
skill_format: skill-md
```

예상 output:

```text
outputs/claude/
├── CLAUDE.md
└── skills/
    ├── cli-inventory/SKILL.md
    ├── shared-agent-operation/SKILL.md
    ├── personal-profile/SKILL.md
    └── repo-analysis/SKILL.md
```

### 9.2 Codex

```yaml
host: codex
output_dir: outputs/codex
entry_file: AGENTS.md
skills_dir: skills
profile_format: markdown
skill_format: skill-md
```

예상 output:

```text
outputs/codex/
├── AGENTS.md
└── skills/
    ├── cli-inventory/SKILL.md
    ├── shared-agent-operation/SKILL.md
    ├── personal-profile/SKILL.md
    └── repo-analysis/SKILL.md
```

### 9.3 Hermes

```yaml
host: hermes
output_dir: outputs/hermes
skills_dir: skills
profile_format: hermes-skill
skill_format: hermes-skill-md
```

예상 output:

```text
outputs/hermes/
└── skills/
    ├── cli-inventory/SKILL.md
    ├── shared-agent-operation/SKILL.md
    ├── personal-profile/SKILL.md
    └── repo-analysis/SKILL.md
```

---

## 10. Platform adapter 설계

### 10.1 macOS

```yaml
platform: macos
shell: zsh
path_style: posix
home_example: /Users/snu.sim
package_managers:
  - brew
  - npm
  - uv
notes:
  - Apple Silicon Homebrew path is usually /opt/homebrew/bin.
```

### 10.2 Windows

```yaml
platform: windows
shells:
  - powershell
  - git-bash
  - wsl2
path_style: mixed
notes:
  - Native Windows and WSL2 should be treated separately.
  - Symlink behavior differs from macOS/Linux.
  - Prefer copy or explicit install script when symlink is unreliable.
```

### 10.3 WSL2

```yaml
platform: wsl2
shell: bash
path_style: posix
notes:
  - Windows files and Linux files may have different performance/permission behavior.
  - Keep repo under WSL filesystem when doing development-heavy work.
```

---

## 11. Generator 설계

초기 generator는 `generator/build.py`로 작성한다.

역할:

1. canonical profile 읽기
2. canonical required CLI 읽기
3. current machine inventory 읽기
4. canonical skills 읽기
5. host config 읽기
6. host별 output 생성
7. provenance header 삽입

예상 provenance:

```markdown
<!--
AUTO-GENERATED FILE. DO NOT EDIT DIRECTLY.
Source repo: my-skills
Source files:
  - canonical/skills/personal-profile/data/profile.yaml
  - canonical/skills/cli-inventory/data/required-cli.yaml
  - canonical/skills/cli-inventory/SKILL.md
Generated for: codex
-->
```

---

## 12. Setup 단계

## 12-A. Skill 생성·수정·배포 루틴

이 repo의 핵심 운영 루틴은 **공통 skill의 생성과 수정이 모두 canonical source에서 시작되고, host별 output으로 재생성·설치·동기화된다**는 점이다.

### 12-A.1 Codex에서 새 공통 skill을 생성하는 루틴

예: Codex를 사용하다가 `email-drafting`이라는 공통 skill을 만들고 싶어진 경우.

```text
Codex session
→ canonical/skills/email-drafting/SKILL.md 작성
→ 필요하면 canonical/skills/email-drafting/data/ 추가
→ generator/build.py 실행
→ outputs/codex, outputs/claude, outputs/hermes 재생성
→ 테스트와 no-secret 검증
→ git commit / push
```

구체적 명령 예시는 다음과 같다.

```bash
mkdir -p canonical/skills/email-drafting/data
$EDITOR canonical/skills/email-drafting/SKILL.md
python3 generator/build.py
python3 tests/test_generation.py
python3 tests/test_no_secrets.py
git diff
git add canonical/skills/email-drafting outputs tests
git commit -m "feat: add email-drafting shared skill"
git push
```

중요 원칙:

- Codex 전용 파일을 먼저 만들지 않는다.
- 먼저 `canonical/skills/<skill-name>/SKILL.md`를 만든다.
- Codex/Claude/Hermes용 파일은 generator가 만든다.
- skill에 필요한 profile, CLI, 예시, template은 해당 skill의 `data/`에 둔다.

### 12-A.2 기존 공통 skill을 수정하는 루틴

예: `cli-inventory` skill을 개선하거나, `personal-profile` skill의 사용 규칙을 수정하는 경우.

```text
수정 요청 또는 발견
→ canonical/skills/<skill-name>/SKILL.md 또는 data/ 수정
→ generator/build.py 실행
→ outputs/* 차이 확인
→ 테스트와 no-secret 검증
→ git commit / push
→ 각 host에서 update/install
```

명령 예시는 다음과 같다.

```bash
$EDITOR canonical/skills/cli-inventory/SKILL.md
$EDITOR canonical/skills/cli-inventory/data/required-cli.yaml
python3 generator/build.py
python3 tests/test_generation.py
python3 tests/test_no_secrets.py
git diff
git add canonical/skills/cli-inventory outputs
git commit -m "fix: update cli-inventory shared skill"
git push
```

수정 시 주의:

- `outputs/`를 직접 고치지 않는다.
- agent가 사용 중 발견한 개선점은 canonical에 반영한다.
- 민감 정보가 들어갈 수 있는 수정은 저장 전 확인한다.
- secret은 canonical에도 outputs에도 들어가면 안 된다.
- 수정 후 모든 host output이 다시 생성되어야 한다.

### 12-A.3 다른 Claude Code / Codex / Hermes가 업데이트를 따라가는 루틴

다른 컴퓨터나 다른 host는 repo 업데이트를 다음 방식으로 따라간다.

```bash
cd ~/git/my-skills
git pull
python3 generator/build.py
python3 tests/test_generation.py
python3 installer/install.py --host <claude|codex|hermes> --mode copy
```

초기 MVP에서는 copy 기반 installer를 권장한다.

이유:

- macOS/Windows symlink 차이를 피할 수 있다.
- 잘못 생성된 output이 즉시 반영되는 위험을 줄인다.
- install 시점에 diff와 테스트를 확인할 수 있다.

나중에 안정화되면 symlink mode를 추가할 수 있다.

```bash
python3 installer/install.py --host all --mode symlink
```

### 12-A.4 upgrade skill

gstack의 `/gstack-upgrade`처럼, 이 repo도 자기 자신을 업데이트하는 공통 skill을 가질 수 있다.

예상 위치:

```text
canonical/skills/my-skills-upgrade/SKILL.md
```

역할:

1. `my-skills` repo 위치 찾기
2. `git pull`
3. generator 실행
4. 테스트 실행
5. host별 install 실행
6. 결과 보고

이 skill이 있으면 Claude/Codex/Hermes 어디서든 다음 요청이 가능해진다.

```text
my-skills 업데이트해줘
```

그리고 agent는 현재 host에 맞게 update/install 루틴을 수행한다.

---

### Phase 1 — repo skeleton

- [ ] `canonical/skills` 생성
- [ ] `canonical/skills/personal-profile/data` 생성
- [ ] `canonical/skills/cli-inventory/data` 생성
- [ ] `hosts` 생성
- [ ] `platforms` 생성
- [ ] `policy` 생성
- [ ] `generator` 생성
- [ ] `outputs` 생성
- [ ] `tests` 생성

### Phase 2 — `cli-inventory` skill 및 현재 Mac inventory

- [ ] required CLI 후보 목록 작성
- [ ] 현재 Mac에서 CLI 탐지
- [ ] path/version/auth status 기록
- [ ] secret 값이 들어가지 않았는지 확인
- [ ] missing/recommended CLI report 작성

### Phase 3 — canonical skill 작성

- [ ] `cli-inventory` skill 작성
- [ ] `shared-agent-operation` skill 작성
- [ ] `personal-profile` skill 작성
- [ ] `repo-analysis` skill 작성

### Phase 4 — host output 생성

- [ ] Claude output 생성
- [ ] Codex output 생성
- [ ] Hermes output 생성
- [ ] generated output 직접 수정 금지 header 삽입

### Phase 5 — 다른 컴퓨터 bootstrap 고려

- [ ] `required-cli.yaml` 기반 missing tool checker 작성
- [ ] macOS setup guide 작성
- [ ] Windows/WSL2 setup guide 작성
- [ ] host별 install guide 작성

---

## 13. 검증 기준

MVP 완료 조건:

- [ ] 공통 skill 원본이 `canonical/skills`에 있다.
- [ ] `cli-inventory` skill이 존재하고, 현재 Mac의 CLI inventory가 그 skill 내부 data로 기록되어 있다.
- [ ] Claude/Codex/Hermes output이 생성된다.
- [ ] `personal-profile` skill이 존재하고, 기본 신상정보와 작업 선호가 그 skill 내부 data로 포함되어 있다.
- [ ] secret은 저장되지 않는다.
- [ ] macOS/Windows/WSL2 차이를 기록할 구조가 있다.
- [ ] 다른 컴퓨터에서 missing CLI를 확인할 수 있는 방향이 있다.
- [ ] generated output에는 provenance가 있다.
- [ ] agent가 canonical을 수정하려면 승인 기반이라는 정책이 있다.

---

## 14. 적용 방식

초기에는 실제 agent 설정에 자동 연결하지 않는다.

추천 순서:

1. `outputs/`까지만 생성
2. 사람이 diff 확인
3. Claude/Codex/Hermes 각각에서 수동 로드 또는 symlink 여부 결정
4. 안전하다고 확인되면 install script 작성

이유:

- 기존 Claude/Codex/Hermes 설정을 덮어쓰면 위험하다.
- profile에는 개인 정보가 들어갈 수 있다.
- Windows/macOS 경로 차이가 있다.

---

## 15. 핵심 결론

수정된 방향은 다음이다.

```text
이 repo는 profile repo가 아니라 skill 공유 repo다.
다만 `personal-profile` skill과 `cli-inventory` skill은 각각 자기 내부 data를 함께 포함해야 제대로 동작한다.
현재 MVP의 핵심은 `cli-inventory`와 `personal-profile` 같은 공통 skill을 만들고, 각 skill 안에 CLI/local 환경 data와 사용자 profile data를 정리하는 것이다.
최종 목표는 macOS/Windows, Claude/Codex/Hermes 어디서든 같은 skill set을 불러 쓸 수 있게 하는 것이다.
```

따라서 setup은 다음 순서로 진행하는 것이 맞다.

```text
1. 공통 skill 구조 생성
2. `cli-inventory` skill 작성 및 현재 Mac inventory 자료 작성
3. `personal-profile` skill 작성 및 사용자 profile 자료 작성
4. host/platform adapter 작성
5. outputs 생성
6. 다른 컴퓨터 bootstrap 전략 추가
```

이 방향이 gstack의 장점을 가져오면서도, kowausa가 실제로 원하는 “어떤 환경에서도 내 skill을 불러 쓰는 구조”에 더 가깝다.
