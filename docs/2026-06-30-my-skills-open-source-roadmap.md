# my-skills Open Source Roadmap

> 작성일: 2026-06-30
> 상태: 기획 초안
> 대상 저장소: `my-skills`
> 관련 Jira: `KAN-69` my-skills 오픈소스 배포

## 1. 요약

`my-skills`는 개인이 작성한 Agent Skill을 하나의 canonical source에서 관리하고
Claude Code, Codex, Hermes 같은 여러 agent host에 안전하게 설치/동기화하는 로컬
registry다.

오픈소스 배포의 목표는 범용 marketplace가 되는 것이 아니다. 먼저 "개인이 만든
skill collection을 안전하게 공개 repo로 관리하고, 다른 사람도 자신의 환경에서
설치해 쓸 수 있게 만드는 것"에 집중한다.

이번 기획은 Jira에는 큰 단위만 남기고, 실제 실행 기준은 이 문서와 후속 세부 문서에
둔다.

큰 작업 축은 세 가지다.

1. 공개 배포 전에 개인 데이터와 machine-local 설정을 canonical skill에서 분리한다.
2. `skillshare`에서 확인한 audit/governance 모델을 `my-skills`의 Python 구조에 맞게
   얇게 도입한다.
3. `gstack`, `OpenCode`, `CodeGraph`에서 배울 만한 host/context/agent UX 패턴을
   선별해서 장기 업그레이드 방향으로 둔다.

## 2. 제품 포지션

한 줄 정의:

> 내가 만든 Agent Skill을 한 곳에서 관리하고, 내가 사용하는 모든 AI agent에 안전하게
> 설치하고 동기화한다.

공개 대상:

- `my-skills` CLI와 core Python package
- 공개 가능한 bundled/example skills
- 설치, 동기화, validation, audit를 설명하는 문서와 테스트

공개 대상이 아닌 것:

- 개인 운영 skill 중 계정, 회사, 로컬 workflow가 강하게 묶인 것
- 실제 config, account id, site id, token, local path
- host별 설치본, state file, profile/memory data

이 포지션은 유지한다. `my-skills`가 풀려고 하는 문제는 "공개 skill marketplace"가
아니라 "내가 관리하는 skill의 원본, 설치본, drift, host 호환성을 잃지 않는 것"이다.

따라서 오픈소스 배포도 다음 원칙을 지킨다.

- `skills/<name>/`이 canonical source다.
- host directory는 설치 대상이며 build output처럼 취급한다.
- local state, account, token, machine path, 개인 memory는 repo 밖에 둔다.
- install/sync/share/import는 plan/apply를 분리하고, 위험을 먼저 보여준다.
- agent 대화 UX는 core CLI 위의 얇은 orchestration layer여야 한다.

## 3. 현재 상태

이미 갖춘 기반:

- `skills/` canonical directory
- `my-skills.toml` manifest
- Claude/Codex/Hermes target path
- copy/link install mode
- drift/stale/conflict/missing/unmanaged 상태 판정
- atomic copy install과 rollback
- `share`, `import`, `skills`, `status`, `sync`, `bootstrap`
- Agent Skills 구조 검증
- 기본 security scan
- README와 한국어 README

부족한 부분:

- 공개 repo에 포함되면 안 되는 개인/환경별 skill 데이터가 섞일 수 있다.
- GitHub Actions, release checklist, package publish 전략이 없다.
- security scan이 단순 regex 중심이고 severity, threshold, policy가 없다.
- install/sync/share/import write path에 audit gate가 명확히 결합되어 있지 않다.
- source provenance, last audit result, trust tier 같은 governance metadata가 없다.
- 경쟁/참고 도구에서 배울 수 있는 host adapter, context 관리, agent-facing UX가 아직
  제품 방향에 녹아 있지 않다.

## 4. 비목표

이번 오픈소스 준비 단계에서 하지 않을 것:

- 공개 skill marketplace
- remote package dependency resolver
- cloud synchronization service
- background daemon
- 조직 단위 policy server
- 모든 host별 skill format 변환기
- MCP server 필수화
- LLM 기반 자동 품질 평가기
- skill dependency graph와 transitive install

이 기능들은 core install/sync/audit 흐름이 공개 repo 기준으로 안정화된 뒤 별도
기획으로 다룬다.

## 5. Jira와 문서 운영 방식

Jira에는 큰 단위만 둔다.

- `KAN-70` `[정리] personal 데이터 분리`
- `KAN-71` `[출시] 공개 배포`
- `KAN-72` `[기능] Agent UX 완성`

세부 실행 항목은 Jira subtask로 과하게 쪼개지 않는다. 대신 `docs/`에 기획서를 두고,
작업은 문서의 단계와 체크리스트를 기준으로 진행한다.

문서 운영 규칙:

- 제품 방향 문서는 `docs/YYYY-MM-DD-<topic>-plan.md` 형식을 따른다.
- 이 단계에서는 별도 세부 문서로 쪼개지 않고, 이 roadmap을 단일 실행 기준으로 유지한다.
- 구현 PR은 관련 문서의 어느 섹션을 실행했는지 커밋/PR 설명에 적는다.
- 문서가 오래되면 새 문서가 이전 문서를 supersede한다고 명시한다.
- 공개 repo 기준으로 민감하거나 개인적인 내용은 문서에도 넣지 않는다.

Jira와 phase 매핑:

- Phase 0 -> `KAN-70` `[정리] personal 데이터 분리`
- Phase 1 + Phase 2 minimal audit -> `KAN-71` `[출시] 공개 배포`
- Phase 3 -> `KAN-72` `[기능] Agent UX 완성`
- Phase 4 -> post-open-source / future

## 6. 축 1: 공개 배포 전 정리

### 6.1 개인 데이터 분리

가장 먼저 정리할 항목이다. 공개 배포에서 가장 큰 위험은 기능 부족이 아니라 개인
계정, 경로, 로컬 설정, 사내/개인 workflow가 canonical skill에 섞이는 것이다.

해야 할 일:

- `my-jira`는 실제 사용 예시 skill로 유지한다.
- 단, `skills/my-jira/config.json` 같은 실제 계정/사이트 설정은 canonical repo에서
  제거한다.
- `config.example.json`만 남기고 실제 config는 data root나 local override로 이동한다.
- `my-skills.local.toml`, `local/`, XDG state/data root 사용법을 README에 분명히 적는다.
- canonical skill 본문에 `/Users/<name>`, `~/.claude`, `~/.agents`, `~/.hermes` 같은
  machine-specific path가 직접 들어가는지 audit한다.
- 개인 skill과 공개 가능한 bundled skill을 분리한다.

판단 기준:

- fresh clone에서 개인 계정 없이 `uv run pytest`가 통과해야 한다.
- 공개 skill은 실제 개인 계정이나 로컬 path 없이 읽혀야 한다.
- local setup은 예제 config와 명령으로 재현 가능해야 한다.

`my-jira` 유지 기준:

- 이름은 유지한다. 실제 사용 사례가 있는 예시 skill이라는 점이 제품 방향을 잘 보여준다.
- 공개 repo에는 실제 Jira site, cloud id, account id, email, board/project private context를
  남기지 않는다.
- `config.example.json`은 placeholder 값만 담는다.
- 실제 config 위치와 생성 방법을 skill 본문이나 README에 명확히 적는다.
- 나중에 사용자가 혼동하거나 개인/조직 smell이 강하다고 판단되면 `jira-example` split을
  별도 작업으로 검토한다.

### 6.2 공개 패키징과 설치 경로

초기 배포는 PyPI보다 GitHub-first가 현실적이다.

GitHub-first의 의미:

- 1차 배포 채널을 PyPI가 아니라 GitHub repository와 GitHub Releases로 둔다.
- 설치 경로는 `git clone`, `uv run`, `uv tool install git+https://...`를 우선 문서화한다.
- release tag, changelog, CI artifact를 GitHub에 모은다.
- PyPI publish는 public API와 release cadence가 안정된 뒤 별도 단계로 결정한다.

권장 순서:

1. GitHub Actions로 test, lint에 준하는 smoke check, wheel build를 검증한다.
2. README에 `uv tool install git+https://github.com/.../my-skills.git` 경로를 명확히 쓴다.
3. GitHub Releases에 tagged release와 changelog를 둔다.
4. 사용자 피드백을 받은 뒤 PyPI publish 여부를 결정한다.

필수 문서:

- install from source
- `uv tool install` based install
- bootstrap이 하는 일
- uninstall/recovery
- host path detection
- private data location
- troubleshooting

### 6.3 공개 repo hygiene

해야 할 일:

- `CONTRIBUTING.md`
- `SECURITY.md`
- `CHANGELOG.md`
- release checklist
- issue template 또는 최소 bug report template
- license와 README badge 정리
- `.gitignore`가 runtime/system artifact를 확실히 배제하는지 점검

최소 CI:

- Python version matrix는 처음엔 3.11 하나로 충분하다.
- `uv run pytest`
- `uv build`
- `my-skills validate` 또는 equivalent smoke command
- docs 링크/명령이 깨지지 않는지 최소 점검

## 7. 축 2: audit/governance 업그레이드

### 7.1 왜 audit가 핵심인가

Agent Skill은 문서처럼 보이지만 실제로는 agent에게 실행 지침을 주는 코드에 가깝다.
따라서 install/sync/share/import 전에 "이 skill을 신뢰해도 되는가"를 판단하는 gate가
필요하다.

현재 `src/my_skills/security.py`는 다음 정도를 본다.

- hidden/bidirectional Unicode
- private key header
- AWS key pattern
- secret-like assignment
- absolute user/home path leak
- NUL byte, invalid UTF-8

이것만으로는 부족하다. prompt injection, output suppression, credential exfiltration,
config poisoning, destructive command, suspicious fetch, hidden markdown comment 같은
agent-skill 특유의 위험을 놓칠 수 있다.

### 7.2 참고: skillshare에서 가져올 것

`runkids/skillshare` 분석에서 확인한 핵심 구조:

- file/skill/bundle analyzer scope
- static, dataflow, markdown, structure, integrity, tier, cross-skill analyzer
- YAML rule 기반 detection
- default/strict/permissive policy profile
- severity threshold
- install/update audit gate
- blocked audit sentinel
- install 실패 cleanup과 update rollback
- cross-skill risk synthesis

그대로 복제하지 않는다. `my-skills`는 Python CLI이고 목적이 더 작다. 다만 아래 개념은
가져온다.

- audit 결과를 structured model로 만든다.
- finding에는 severity, category, rule id, file, message가 있어야 한다.
- policy가 block threshold를 결정한다.
- write path는 audit gate를 통과해야 한다.
- `--skip-audit`과 `--force`는 명시적이고 로그/출력에 남아야 한다.
- bundle-level 분석은 나중 단계로 두되 모델에서 자리를 열어 둔다.

`--force`와 `--skip-audit`는 같은 의미가 아니다.

- `--force`: 이미 있는 파일 덮어쓰기, canonical과 source가 다른 상황, rollback 가능한
  overwrite 같은 write-conflict 결정을 명시한다.
- `--skip-audit`: audit 결과를 보지 않고 진행하는 보안 gate 우회를 명시한다.

둘을 합치면 사용자가 "덮어쓰기만 허용"하려고 한 상황에서 보안 gate까지 우회될 수 있다.
그래서 두 옵션은 분리한다.

기본 audit threshold는 처음엔 `CRITICAL`로 시작한다. `CRITICAL`은 private key, 명백한
credential exfiltration, 위험한 hidden instruction처럼 거의 확실히 막아야 하는 항목이다.
`HIGH`는 destructive command, suspicious network send, config poisoning처럼 위험하지만
false positive가 더 섞일 수 있는 항목이다. 기본값을 `HIGH`로 두면 초기 사용성이 거칠어질
수 있으므로, 기본은 `CRITICAL`, strict profile은 `HIGH`로 둔다.

### 7.3 Audit gate 적용 지점

Audit gate는 command별로 의미가 다르다. `share`/`import`는 canonical source로
들어오는 ingest path이고, `install`/`sync`는 canonical source를 host로 전파하는
propagation path다.

Ingest commands:

```text
share/import
  -> staging copy
  -> validate
  -> audit
  -> approve
  -> canonical write
```

Propagation commands:

```text
install/sync
  -> validate canonical source
  -> audit canonical source or check cached audit result
  -> plan
  -> approve
  -> host write
```

`share`와 `import`는 외부 또는 host-local skill이 canonical repo로 들어오는 경로이므로
가장 보수적으로 다룬다. canonical에 쓰기 전에 staging copy에서 validation과 audit를
끝내야 한다. `install`과 `sync`는 canonical source가 이미 신뢰 가능한 상태인지 확인하고,
dry-run에서는 write 없이 would-block 결과를 보여줘야 한다.

### 7.4 권장 구조

```text
src/my_skills/
  audit/
    __init__.py
    models.py          # Finding, Result, Severity, Category
    static.py          # prompt injection, hidden unicode, destructive commands
    structure.py       # path containment: skill 폴더 밖 ref, `..` 상대경로 탈출
    markdown.py        # hidden comments, external image/query exfil vector
    credentials.py     # credential source and secret-like patterns
    policy.py          # default/strict/permissive, threshold
    gate.py            # audit_gate_plan/apply wrappers
    rules.py           # later: YAML/JSON rule loading
    tiers.py           # later: command capability tiers
    dataflow.py        # later: simple source/sink detection
    cross_skill.py     # later: bundle-level risk synthesis
  security.py          # compatibility shim during migration
```

기존 `validation.py`는 structural validation을 유지한다. audit는 구조 검증이 아니라
신뢰/위험 판단을 맡는다.

`structure.py`의 path containment는 skill이 자기 디렉터리 밖을 가리키는 위험을 잡는다.
현재 `validation.py`는 본문의 절대경로(`/Users/...`)만 경고하고 `..` 상대경로 탈출은
통과시킨다. 정상 skill은 자기 폴더 안만 참조하므로 false positive가 거의 없는 싼 가드다.
category는 `traversal`, 기본 severity는 `HIGH`로 두되, `.ssh`/`.aws`/credentials 등
알려진 민감 경로를 지목하면 `CRITICAL`로 올린다.

### 7.5 1차 PR 범위

첫 PR은 작게 잡는다.

- `audit/models.py`
- `audit/static.py`
- `audit/policy.py`
- `audit/gate.py`
- builtin rule file
- 기존 `security.py` 호환 shim
- `my-skills audit [skill] --json`
- `install`, `sync`, `share`, `import`의 apply 전 audit gate
- `install/sync --dry-run`의 audit report와 would-block 표시
- CI에서 읽을 수 있는 JSON report shape
- 최소 provenance field 저장: `source_type`, `source_url`, `source_revision`,
  `last_audit_at`, `last_audit_result_hash`, `audit_profile`, `audit_threshold`
- default threshold는 `CRITICAL`
- strict threshold는 `HIGH`

이 단계에서 dataflow와 cross-skill은 만들지 않는다. 모델과 CLI surface만 장래 확장을
막지 않게 둔다.

### 7.6 2차 PR 범위

- prompt injection/output suppression 규칙 확장
- markdown hidden comment와 remote image/query exfil vector
- destructive command, dynamic exec, suspicious fetch 규칙
- external rule file: `.my-skills/audit-rules.yaml`
- project/user policy override는 `my-skills.toml [audit]` 유지
- audit result를 install state에 저장
- `skills --with-status`에 audit 상태 표시

### 7.7 3차 PR 범위

- analyzer scope: file, skill, bundle
- command capability tier: read-only, mutating, destructive, network, privilege,
  stealth, interpreter
- simple dataflow: credential source -> network sink
- cross-skill: credential reader x network sender 조합 감지
- trust tier: local-authored, imported-local, tracked-remote, external-unknown,
  organization-shared

## 8. 축 3: 참고 오픈소스에서 배울 점

### 8.1 gstack

가져올 것:

- canonical source와 generated/installed artifact 구분
- host adapter를 선언형으로 관리하는 방식
- freshness check
- repo 밖 private state
- host boundary policy
- decision UX로 위험과 선택지를 먼저 보여주는 방식

가져오지 않을 것:

- 거대한 generated workflow skill 구조
- Claude-first 문자열 rewrite
- agent가 instruction file을 직접 수정하고 커밋하는 workflow
- telemetry/browser/daemon 중심 구조
- schema 없는 JSONL 공유를 core로 삼는 방식

`my-skills`에는 host adapter가 필요하지만, 기본은 pass-through여야 한다. 대부분의 skill은
Agent Skills 표준 directory를 그대로 host에 복사하면 된다. adapter는 경로, metadata,
reload hint처럼 필요한 차이만 처리한다.

### 8.2 OpenCode

가져올 것:

- instruction/context를 단순 문서가 아니라 세션에 주입되는 관리 대상 컨텍스트로 보는
  관점
- 전역 지침과 프로젝트 지침 분리
- 긴 지침을 playbook, command, reference로 분리하는 방식
- session memory와 long-term knowledge를 분리하는 방식

가져오지 않을 것:

- 전체 durable session/event/epoch/replay 시스템
- provider별 복잡한 prompt replacement 체계
- 단일 사용자 local registry에 과한 mode system

`my-skills`에 필요한 것은 작은 agent-facing skill UX다. 예를 들어 `skills/my-skills`
본문은 CLI 명령의 안전한 사용법과 decision flow만 안내하고, 복잡한 로직은 Python CLI에
둔다.

### 8.3 CodeGraph

가져올 것:

- agent가 repo를 무작정 grep하지 않게 하는 structured context surface
- `status`, `explore`, `node`, `callers`처럼 목적별 명령을 작게 나누는 UX
- local index/state를 repo artifact와 분리하는 운영 모델

가져오지 않을 것:

- full code intelligence engine
- watcher/indexer/MCP server를 `my-skills` core에 넣는 것

`my-skills`에는 "skill registry를 agent가 안전하게 파악하는 JSON surface"가 더 중요하다.
예: `my-skills skills --json`, `my-skills status --json`, `my-skills audit --json`,
`my-skills share --plan --json`.

### 8.4 LazyCodex/OMO

가져올 것:

- agent-local execution discipline과 human-visible Jira를 분리하는 방식
- long task는 goal/ledger/checkpoint로 관리하고, Jira는 큰 단위 board로 유지하는 방식

가져오지 않을 것:

- Jira를 세부 task manager로 과하게 쓰는 방식
- 복잡한 agent loop state를 `my-skills` core에 넣는 것

## 9. 실행 순서

### Phase 0: 공개 배포 차단 요소 제거

목표: repo를 공개해도 개인 정보나 machine-local 설정이 노출되지 않는다.

작업:

- canonical skill에 들어간 실제 개인 config 식별
- 예제 config와 local config 분리
- README에 private data root 설명 보강
- `uv run pytest` 통과
- clean clone 기준 bootstrap dry-run 확인

완료 기준:

- 공개 repo에 실제 개인 계정/토큰/path가 없다.
- 개인 skill은 예제 또는 local-only로 분리되어 있다.

### Phase 1: release-ready hygiene

목표: 다른 사용자가 설치하고 검증할 수 있다.

작업:

- GitHub Actions 추가
- `uv build` 검증
- release checklist 작성
- `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`
- GitHub install path 문서화

완료 기준:

- fresh clone에서 README만 따라 해도 bootstrap dry-run과 tests를 실행할 수 있다.
- tag/release 절차가 문서화되어 있다.
- 임시 HOME 또는 clean machine에서 다음 smoke path가 통과한다.
  - `uv tool install git+https://github.com/<owner>/my-skills.git`
  - `my-skills bootstrap --dry-run`
  - `my-skills doctor`
  - `my-skills skills --json`
  - `my-skills install my-skills --host hermes --dry-run`

### Phase 2: minimal audit gate

목표: write path가 기본 audit policy를 통과해야 한다.

작업:

- `audit/` 패키지 1차 도입
- `my-skills audit` CLI: write 없이 독립 실행 가능한 report surface
- `install/sync --dry-run`에서 audit result와 would-block 표시
- ingest path(`share/import`)와 propagation path(`install/sync`) gate 분리
- supporting-file ref와 import ingest에 path containment 검사 추가. `..` 상대경로
  탈출과 skill 디렉터리 밖 참조를 차단한다.
- CI에서 사용할 수 있는 JSON output
- policy threshold
- 최소 provenance field 저장
- tests

완료 기준:

- 위험 finding이 threshold 이상이면 write가 block된다.
- `my-skills audit`는 write 없이 실행할 수 있다.
- `install/sync --dry-run`은 write 없이 audit would-block 여부를 보여준다.
- JSON output은 CI에서 파싱할 수 있다.
- `--skip-audit` 또는 `--force`는 명시적이고 출력에 남는다.
- 기존 validation과 audit 책임이 분리되어 있다.
- 스킬 본문/지원파일 ref가 `..`로 skill 디렉터리를 벗어나면 audit finding으로 잡힌다.
- import은 staging 단계에서 containment를 검사하고, 벗어나는 항목이 있으면 canonical
  write 전에 block된다.
- 기존 validation의 절대경로 경고와 audit의 `..` 차단이 역할 분리되어 중복되지 않는다.
- 정식 trust tier는 없더라도 source metadata와 last audit metadata는 저장된다.

### Phase 3: agent UX and governance

목표: agent 대화에서 안전한 plan/apply 흐름이 자연스럽다.

작업:

- `skills/my-skills` 안내를 audit/share/install decision flow 중심으로 정리
- `skills --with-status`에 audit 상태 표시
- source provenance와 last audit result를 agent-facing output에 표시
- trust tier 도입

완료 기준:

- agent가 `share`, `install`, `audit`를 임의 shell 조합 없이 표준 절차로 수행할 수 있다.
- 사용자는 적용 전에 risk와 선택지를 볼 수 있다.

### Phase 4: deeper audit

목표: agent skill 특유의 조합 위험을 볼 수 있다.

작업:

- markdown analyzer
- command tier
- dataflow
- cross-skill
- project/user rule override

완료 기준:

- credential reader x network sender 같은 bundle-level risk가 표시된다.
- strict profile에서 더 보수적으로 block할 수 있다.

## 10. 성공 기준

오픈소스 배포 준비 완료:

- 공개 repo에 개인 계정, token, machine-local path, local state가 없다.
- README만 보고 fresh clone 설치/검증이 가능하다.
- CI가 tests와 build를 검증한다.
- release checklist가 있다.
- bundled skill과 local-only skill의 경계가 명확하다.

audit/governance 1차 완료:

- `my-skills audit`가 사람이 읽는 출력과 JSON 출력을 제공한다.
- severity와 threshold가 있다.
- audit는 gate이기 전에 report surface로 독립 실행 가능하다.
- dry-run에서 audit would-block 여부를 볼 수 있다.
- install/sync/share/import는 audit gate를 통과해야 쓴다.
- share/import ingest gate와 install/sync propagation gate가 분리되어 있다.
- skill 디렉터리 밖을 가리키는 경로 탈출이 finding으로 보고된다.
- audit 실패 시 partial write가 남지 않는다.
- 최소 provenance와 last audit metadata가 저장된다.
- 기존 `security.py` 사용자/API는 migration 기간 동안 깨지지 않는다.

agent UX 완료:

- agent 대화에서 share/install 전에 plan과 risk를 보여줄 수 있다.
- `my-skills` skill 본문은 core CLI의 안전 절차를 안내한다.
- 복잡한 판단은 skill markdown이 아니라 CLI structured output에 둔다.

## 11. 남은 결정

확정된 결정:

- 배포는 GitHub-first로 시작한다. PyPI는 안정화 이후 별도 결정한다.
- `my-jira`는 예시 skill로 유지한다. 실제 config만 canonical repo 밖으로 분리한다.
- `--force`와 `--skip-audit`는 분리한다.
- default audit threshold는 `CRITICAL`, strict threshold는 `HIGH`로 시작한다.
- trust tier는 Phase 3에서 진행한다.
- audit override는 역할을 분리한다.
  - 1차는 `my-skills.toml [audit]`만 지원한다. profile, threshold, audit on/off 같은
    단순 정책 설정을 둔다.
  - `.my-skills/audit-rules.yaml`은 2차 이후에 추가한다. custom rule, rule disable,
    project-specific allowlist, severity override 같은 규칙 확장 전용으로 둔다.

권장 기본값:

- builtin rule file은 1차부터 둔다. 규칙을 코드에 박아두지 않고 데이터로 관리하기 위해서다.
- 1차 project/user override는 `my-skills.toml [audit]`의 profile/threshold 정도로 제한한다.
- 사용자가 직접 override하는 external rule file은 2차로 미룬다. rule schema, merge order,
  precedence, disable semantics가 정해지지 않은 상태에서 열면 정책 복잡도가 커진다.

## 12. 바로 다음 작업

가장 먼저 할 일은 `KAN-70`이다.

1. canonical skill에 포함된 private/config 파일을 전수 확인한다.
2. 공개 가능한 예제와 local-only 실제 설정을 분리한다.
3. README에 private data root와 local config 원칙을 추가한다.
4. tests를 통과시킨다.
5. 그 다음 `KAN-71`의 CI/release hygiene으로 넘어간다.

`KAN-72`의 Agent UX는 audit gate가 생긴 뒤에 진행하는 편이 좋다. UX가 먼저 좋아져도
core write path가 안전하지 않으면 agent가 더 빠르게 위험한 작업을 수행할 수 있기
때문이다.
