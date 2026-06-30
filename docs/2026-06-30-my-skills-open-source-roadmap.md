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
3. `gstack`, `OpenCode`, `CodeGraph`, `LazyCodex/OMO`에서 배울 만한 host/context/agent
   UX 패턴을 선별해서 장기 업그레이드 방향으로 둔다.

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
- 개인 데이터/머신 설정 분리와 누출 회귀 테스트(`tests/test_public_hygiene.py`)
- README와 한국어 README

부족한 부분:

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
- 모든 host별 skill format 변환기(임의 포맷 범용 변환). 단, "중립 원천 → 알려진 host
  렌더링"(§8.1 장기 방향)은 별개이며 전제조건 충족 시 추진한다.
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

- 제품 방향 문서는 `docs/YYYY-MM-DD-<topic>-{plan,roadmap}.md` 형식을 따른다.
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
- audit는 평평한 함수 더미가 아니라 plugin analyzer 파이프라인으로 둔다. 검사 추가는
  analyzer 등록만으로 되고, runner·gate 코드는 수정하지 않는다. (skillshare의 정갈한
  구조의 핵심이 이 Analyzer protocol + scope 분리다.)

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
  -> audit canonical source or check cached audit result(캐싱은 2차)
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
    analyzers.py       # Analyzer protocol(ID/scope/analyze), Scope(file/skill), 등록 registry
    static.py          # prompt injection, hidden unicode, destructive commands
    structure.py       # path containment: skill 폴더 밖 ref, `..` 상대경로 탈출
    markdown.py        # 2차: hidden comments, external image/query exfil vector
    credentials.py     # 2차: credential source and secret-like patterns
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

`analyzers.py`는 검사 전략을 plugin으로 묶는 seam이다. 각 analyzer는 `id`, `scope`,
`analyze(ctx)`만 구현하고, runner가 scope(file/skill)에 따라 호출 시점을 정한다.
검사 추가는 analyzer를 registry에 등록하는 것으로 끝나며 gate/runner는 손대지 않는다.
1차는 file/skill scope와 static/structure만 얹고, bundle scope·dataflow·tier·cross-skill은
Phase 4에서 같은 protocol 위에 추가한다. analyzer가 둘뿐인 1차에 추상화를 두는 것은,
Phase 4 확장이 확정적이고 나중에 retrofit하는 비용이 더 크기 때문이다.

### 7.5 1차 PR 범위

첫 PR은 작게 잡는다.

- `audit/models.py`
- `audit/analyzers.py` — Analyzer protocol + scope(file/skill) + registry
- `audit/static.py`
- `audit/structure.py` — path containment(`..` 탈출, skill 폴더 밖 ref)
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

- analyzer scope: bundle (file/skill scope와 Analyzer protocol은 Phase 2에서 도입)
- command capability tier: read-only, mutating, destructive, network, privilege,
  stealth, interpreter
- simple dataflow: credential source -> network sink
- cross-skill: credential reader x network sender 조합 감지
- trust tier: local-authored, imported-local, tracked-remote, external-unknown,
  organization-shared

### 7.8 PR 차수와 Phase 매핑

§7.5~7.7의 "1·2·3차 PR"은 audit 기능을 내용으로 묶은 단위다. 실제 실행 순서는 §9
Phase가 authoritative이며, 충돌 시 Phase 배치를 따른다. 대략의 대응:

- 1차 PR ≈ Phase 2 (models/analyzers/static/structure/policy/gate, write 전 gate 연결)
- 2차 PR: 규칙 확장·external rule override ≈ Phase 4 / `skills --with-status`·audit state 저장 ≈ Phase 3
- 3차 PR: trust tier ≈ Phase 3 / command tier·dataflow·cross-skill·bundle scope ≈ Phase 4

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

장기 방향 — 중립 원천 + host 렌더링:

skill의 본질은 로직·방법론이지 글자 그대로의 전사가 아니다. 따라서 최종 방향은 원천
스킬을 host-중립(중립 placeholder 토큰 사용)으로 한 벌만 두고, 설치 시 host별로
렌더링하는 것이다. gstack식 Claude-first 문자열 치환은 쓰지 않는다. 전제조건이 충족되면
별도 phase로 연다.

이때 host 차이는 둘로 나뉜다. (a) 형식 제약(frontmatter keepFields, description 길이
등)은 장기 방향에서도 검증으로 유지한다 — 조용히 잘라내지 않고 막아준다. (b) 콘텐츠
참조(host-특정 경로·도구명)만 중립 placeholder 렌더링 대상으로 한다. 즉 "변환을 잘한다"는
사실상 (b) 렌더링으로 좁혀지며, 그래서 작고 안전하다.

전제조건:

1. 중립 skill 표현 / placeholder 토큰 규칙 정의(Phase 3 host-중립 검증에서 축적).
2. drift 재정의: "설치본 == canonical"에서 "설치본 == 재생성(canonical, host)"으로.
3. Phase 3 검증에서 모은 실제 host 제약 데이터로 렌더링 범위를 (b) 콘텐츠 참조로 한정.

A(현재 검증)는 B(장기 렌더링)의 우회로가 아니라 토대다. 둘 다 "skill은 host-중립
콘텐츠"라는 같은 core를 전제하므로, Phase 3에서 중립성을 authoring 규칙으로 세워두면
나중 렌더링 레이어는 그 위에 작게 얹힌다.

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

### Phase 0: 공개 배포 차단 요소 제거 (완료 — 커밋 d021624, 6c28f4f)

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

확인 기록(2026-07-01, 현재 브랜치 `codex/my-skills-open-source-roadmap`):

- Phase 0 완료 커밋 `d021624`, `6c28f4f`가 현재 `HEAD`에 포함되어 있음을 확인했다.
- `skills/my-jira/config.json`은 tracked file이 아니고, placeholder
  `skills/my-jira/config.example.json`만 tracked file이다.
- README에는 `my-skills data-path`, `my-skills.local.toml`, `local/`을 통한
  machine-local/private data 분리 방법이 문서화되어 있다.
- `tests/test_public_hygiene.py`, 전체 `uv run pytest`, clean clone + 임시
  HOME/XDG 환경의 `uv run pytest`, `my-skills bootstrap --dry-run`이 통과했다.

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

확인 기록(2026-07-01, 현재 브랜치 `codex/my-skills-open-source-roadmap`):

- GitHub Actions CI, bug report template, `CONTRIBUTING.md`, `SECURITY.md`,
  `CHANGELOG.md`, `docs/release-checklist.md`를 추가했다.
- README와 한국어 README에 GitHub-first install, source checkout smoke,
  release hygiene 명령을 문서화했다.
- CI의 `actions/checkout@v7`, `astral-sh/setup-uv@v8.2.0` 태그가 upstream에
  존재함을 확인했다.
- `uv run pytest`, `uv build`, CI YAML parse, clean HOME/XDG source smoke,
  임시 tool/home 환경의 `uv tool install git+file://...` smoke가 통과했다.
- `uv build` 산출물은 검증 후 repo 밖 상태로 정리했고 commit 대상에 포함하지 않는다.

### Phase 2: minimal audit gate

목표: write path가 기본 audit policy를 통과해야 한다.

작업:

- `audit/` 패키지 1차 도입
- Analyzer protocol + scope(file/skill) + 최소 registry 도입. static/structure 검사를
  이 protocol 위에 얹는다. (bundle scope·dataflow·tier·cross-skill은 Phase 4)
- audit는 write 전 staging에서 수행한다. write-then-scan-rollback은 채택하지 않는다.
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
- 새 검사를 추가할 때 gate/runner 수정 없이 analyzer 등록만으로 동작한다(테스트로 증명).
- analyzer enable/disable이 코드 분기가 아니라 정책 데이터로 제어된다.
- audit는 write 전 staging에서 수행되며, 검사 미통과 콘텐츠는 canonical/host 최종 위치에
  기록되지 않는다.
- audit 스캔이 에러로 실패하면 write가 fail-closed로 차단되고, `--skip-audit`으로만
  우회된다.
- 정식 trust tier는 없더라도 source metadata와 last audit metadata는 저장된다.

### Phase 3: agent UX and governance

목표: agent 대화에서 안전한 plan/apply 흐름이 자연스럽다.

작업:

- `skills/my-skills` 안내를 audit/share/install decision flow 중심으로 정리
- `skills --with-status`에 audit 상태 표시
- source provenance와 last audit result를 agent-facing output에 표시
- trust tier 도입
- host adapter에 선언형 host 제약(frontmatter keepFields, description 길이 등)을 추가하고,
  install/sync 시 위반을 검증으로 차단·경고한다(콘텐츠 변환 없음).
- host-중립 authoring 규칙(중립 placeholder 토큰, 절대 host 경로 금지)을 validation에
  정의한다. 이는 장기 host 렌더링(§8.1)의 토대다.

완료 기준:

- agent가 `share`, `install`, `audit`를 임의 shell 조합 없이 표준 절차로 수행할 수 있다.
- 사용자는 적용 전에 risk와 선택지를 볼 수 있다.
- host별 frontmatter 제약을 위반하는 skill은 해당 host install 전에 차단·경고된다.
- skill 콘텐츠는 host별로 변환되지 않으며, 설치본은 canonical과 hash가 일치한다.
- host-중립 authoring 규칙이 validation으로 강제된다(절대 host 경로/비중립 참조 검출).

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
- audit는 write 전 staging에서 수행되어, 검사 미통과 콘텐츠가 최종 위치에 닿지 않는다.
- audit 스캔이 에러로 실패하면 write가 fail-closed로 차단된다(`--skip-audit`으로만 우회).
- skill 디렉터리 밖을 가리키는 경로 탈출이 finding으로 보고된다.
- audit는 analyzer 단위로 분리돼 있고, 검사 추가가 등록만으로 되며 enable/disable이
  정책 데이터로 제어된다.
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
- gate는 write-before로 간다. audit는 staging 단계에서 수행하고, skillshare식
  write-then-scan-rollback은 채택하지 않는다. 이유: my-skills는 이미 atomic staging이
  있어, 검사 미통과 콘텐츠가 최종 canonical/host에 일시적으로도 닿지 않게 할 수 있다.
- scan 에러는 fail-closed. audit 스캔이 에러로 끝나면 ingest·propagation 모두 write를
  차단한다. 우회는 명시적 `--skip-audit`으로만. 이유: 보안 gate의 안전 기본값은
  fail-closed. (스캐너 버그로 전체 설치가 막힐 위험은 `--skip-audit`으로 완화.)
- threshold-only로 간다. block 판정은 severity 최댓값 ≥ threshold 기준만 쓴다.
  skillshare의 RiskScore(가중합)·RiskLabel 집계와 finding dedupe mode는 1차 비범위다.
  위험을 한눈에 보여줄 필요가 생기면 Phase 3 UX에서 RiskLabel만 선택적으로 검토한다.
- host 차이 대응은 단계적으로 간다.
  - 지금(Phase 3): 변환하지 않는다. adapter는 host 제약을 선언만 하고, install/sync는
    위반 시 차단·경고한다(검증). 설치본 == canonical(hash 일치)을 유지한다.
  - 장기 방향: 원천을 host-중립으로 두고 host별로 렌더링한다(§8.1 장기 방향). 형식
    제약(a)은 그때도 검증으로 유지하고, 콘텐츠 참조(b)만 중립 placeholder 렌더링 대상으로
    한다. Claude 경로 문자열 치환식 rewrite는 채택하지 않는다.
- host boundaryInstruction(cross-host prompt-injection 경계 지침)은 도입하지 않는다.
  my-skills는 cross-model invocation을 오케스트레이션하지 않고 skill을 host-중립으로
  유지하므로 필요성이 낮다. cross-host invocation을 지원하게 되면 재검토한다.
- 설치본 provenance 헤더 주입은 하지 않는다. drift 감지가 이미 실수 편집을 잡고, 헤더
  주입은 content hash와 충돌한다. 설치 위치에 출처 노출이 필요해지면 SKILL.md 본문이
  아니라 별도 sidecar(hash 비대상)로 둔다. 장기 방향에서 재생성 기반 drift가 도입되면
  스탬핑 필요성 자체가 줄어든다.
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

`KAN-70`(개인 데이터 분리)은 완료되었다 — 개인 config 제거, `config.example.json`
전환, `tests/test_public_hygiene.py` 누출 회귀 테스트, README private data 문서화.

다음은 `KAN-71`의 CI/release hygiene이다.

1. GitHub Actions(test/build/hygiene) 추가
2. `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`
3. GitHub install path 문서화
4. release checklist

`KAN-72`의 Agent UX는 audit gate가 생긴 뒤에 진행하는 편이 좋다. UX가 먼저 좋아져도
core write path가 안전하지 않으면 agent가 더 빠르게 위험한 작업을 수행할 수 있기
때문이다.
