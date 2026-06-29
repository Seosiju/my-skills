# my-skills Skill Catalog and Sharing UX Plan

> 작성일: 2026-06-29  
> 상태: 제안안  
> 대상 저장소: `my-skills`  
> 관련 문서: `docs/2026-06-25-cross-agent-skill-registry-plan.md`

---

## 1. 요약

현재 `my-skills`는 canonical skill directory, manifest, install state,
drift detection을 안전하게 갖추고 있다. 부족한 부분은 사용자가 agent 대화 안에서
그 흐름을 발견하고 선택하기 어렵다는 점이다.

이번 개선의 목표는 세 가지다.

1. `my-skills skills`로 이 repo가 보유한 skill catalog를 바로 볼 수 있게 한다.
2. Claude Code, Codex, Hermes 대화 안에서 host-local skill을 `my-skills`로
   가져오거나 canonical skill을 현재 host에 설치할 수 있게 한다.
3. import/install 전에 risk list를 보여주고, 사용자가 enable 여부를 선택하게 한다.

핵심 설계 원칙은 기존 기획서와 같다. `skills/<name>/`이 원본이고, host directory는
설치 대상이며, local state는 소유권과 drift 판단에만 쓰인다. Agent 대화 UX는 이
원칙 위에 얇게 올라가는 orchestration layer여야 한다.

---

## 2. 문제

### 2.1 Catalog discovery가 약하다

현재 사용자는 `status`를 통해 skill과 host 상태를 볼 수 있지만, 이 명령은 설치
상태 중심이다. "내가 이 repo에 어떤 skill을 보유하고 있는가"를 빠르게 보는
catalog 명령이 없다.

### 2.2 Import와 install이 agent 대화의 자연스러운 흐름이 아니다

현재 가능한 흐름은 다음과 같다.

```bash
uv run my-skills import ~/.claude/skills/foo
uv run my-skills install foo --host hermes
```

기능은 맞지만, 사용자가 Claude Code, Codex, Hermes 대화 중에 "이 skill 공유해줘"라고
말했을 때 agent가 안전하게 진행하기 위한 표준 절차가 없다. 각 agent가 임의로 shell
명령과 manifest edit을 조합하면 품질과 안전성이 흔들린다.

### 2.3 Risk가 선택 전에 충분히 드러나지 않는다

`validate`와 security scan은 있지만, import/install 의사결정 화면으로 묶여 있지
않다. 사용자는 다음 정보를 한 번에 봐야 한다.

- 이 skill이 어디서 왔는가
- canonical에 같은 이름이 있는가
- secret, absolute path, host-specific instruction, script 같은 위험이 있는가
- 가져온 뒤 기본 sync 대상에 포함할 것인가
- 어느 host에 배포할 것인가

---

## 3. 비목표

- 공개 marketplace를 만들지 않는다.
- remote upload service를 만들지 않는다.
- dependency resolver나 package manager를 만들지 않는다.
- agent가 canonical source를 몰래 수정하거나 자동 overwrite하지 않는다.
- host별 skill format을 대규모로 변환하지 않는다.
- MCP server를 필수로 만들지 않는다. 필요하면 나중에 optional surface로 둔다.

"공유"는 이 문서에서 local host skill을 canonical `my-skills` repo로 승격하거나,
canonical skill을 다른 host에 설치하는 것을 뜻한다. 원격 publish는 별도 단계다.

---

## 4. 설계 원칙

### 4.1 Core CLI가 source of truth다

Agent slash command는 판단과 대화 UX를 맡고, 실제 file write는 `my-skills` CLI가
맡는다. 이렇게 해야 Claude Code, Codex, Hermes가 같은 안전 모델을 공유한다.

### 4.2 Plan/apply를 분리한다

대화형 흐름은 먼저 risk plan을 만든 뒤 사용자 선택을 받아 적용한다.

```text
scan -> plan -> user decision -> apply -> validate -> sync/status
```

이 패턴은 기존 `install --dry-run`, `sync --check`, drift classification과 맞다.

### 4.3 `hosts`는 compatibility, `enabled`는 default selection이다

현재 manifest의 의미를 유지한다.

- `hosts = [...]`: 이 skill이 지원하는 host 목록
- `enabled = true|false`: bare `install` / `sync` 대상에 포함할지

Per-host enable 개념을 성급히 새로 만들지 않는다. 특정 host에 설치하지 않으려면
`hosts`를 좁히거나 명령에서 `--host`를 명시한다.

### 4.4 gstack에서 가져올 것은 routing과 decision UX다

가져올 것:

- slash command 또는 skill routing으로 agent 대화에서 진입하는 방식
- decision brief를 통해 위험과 선택지를 먼저 보여주는 방식
- host별 discovery path를 registry로 관리하는 방식
- 큰 workflow logic을 skill 본문에 넣지 않고 CLI/script로 내리는 방식

가져오지 않을 것:

- gstack의 거대한 generated workflow skill 구조
- 특정 host 중심의 template rewrite
- background daemon, telemetry, browser workflow

---

## 5. User Experience

### 5.1 Terminal catalog

```bash
my-skills skills
```

기본 출력은 사람이 읽기 쉬운 compact table이다.

```text
Skill                    Enabled  Hosts                 Summary
repo-analysis            yes      claude,codex,hermes   Surveys an unfamiliar code repository...
shared-agent-operation   yes      claude,codex,hermes   Baseline operating conventions...
cli-inventory            yes      claude,codex,hermes   Declares required CLI tools...
personal-profile         yes      claude,codex,hermes   Memory-like skill...
```

옵션:

```bash
my-skills skills --host claude
my-skills skills --enabled
my-skills skills --disabled
my-skills skills --with-status
my-skills skills --json
```

`--with-status`는 기존 `status`의 drift classification을 join해서 보여준다.
`skills`는 catalog 중심, `status`는 install/debug 중심으로 역할을 분리한다.

### 5.2 Agent 대화에서 가져오기

사용자:

```text
/my-skills share this skill
```

또는 host가 slash command를 지원하지 않으면:

```text
my-skills에서 이 skill 공유해줘
```

Agent 동작:

1. 현재 host를 식별한다.
2. source skill 후보를 찾는다.
3. `my-skills share --from <host> --plan --json`을 실행한다.
4. host에서 발견한 skill list와 각 후보의 risk list를 사용자에게 보여준다.
5. 사용자가 공유할 skill과 enable 여부를 선택하면 apply 명령을 실행한다.
6. 기본 대상은 모든 enabled target host다. 사용자가 특정 host만 원하면 좁힐 수 있다.
7. `validate`, `skills --with-status`, 필요 시 `sync --check`로 검증한다.

Decision brief 예:

```text
가져올 후보: office-hours
Source: ~/.claude/skills/gstack/office-hours

Risk:
- BLOCKER: frontmatter name과 directory name이 다를 수 있음
- WARN: allowed-tools는 host별 지원 차이가 있음
- WARN: absolute ~/.claude path가 본문에 포함됨
- INFO: scripts/가 있어 실행 가능한 파일을 포함함

선택:
A) import하고 enabled=true로 등록
B) import하지만 enabled=false로 등록
C) diff를 먼저 확인
D) skip
```

### 5.3 Agent 대화에서 받아오기

사용자:

```text
/my-skills install repo-analysis here
```

Agent 동작:

1. 현재 host를 식별한다.
2. `my-skills install repo-analysis --host <host> --dry-run --json`을 실행한다.
3. create/update/noop/block plan을 보여준다.
4. 사용자가 승인하면 apply한다.
5. host reload hint를 출력한다.

이 흐름은 기존 `install`을 감싸는 UX일 뿐이다. Core semantics는 바꾸지 않는다.

---

## 6. CLI 명세 제안

### 6.1 `my-skills skills`

```bash
my-skills skills [--host <host>] [--enabled|--disabled] [--with-status] [--json]
```

역할:

- canonical `skills/`와 manifest를 기준으로 목록을 출력한다.
- `SKILL.md` frontmatter에서 description을 읽어 summary를 만든다.
- `--with-status`일 때만 install state와 hash 비교를 수행한다.

출력 JSON shape:

```json
{
  "skills": [
    {
      "name": "repo-analysis",
      "enabled": true,
      "hosts": ["claude", "codex", "hermes"],
      "description": "Surveys an unfamiliar code repository...",
      "path": "skills/repo-analysis",
      "status": {
        "claude": "FRESH",
        "codex": "MISSING",
        "hermes": "MISSING"
      }
    }
  ]
}
```

### 6.2 `my-skills share`

```bash
my-skills share --from <host> [skill] --plan [--json]
my-skills share --from <host> <skill> --enable
my-skills share --from <host> <skill> --disable
```

역할:

- host skill directory를 scan한다.
- canonical에 없는 skill, unmanaged skill, drifted skill 후보를 찾는다.
- import 전에 validation/security/collision 결과를 risk plan으로 보여준다.
- apply 시 canonical `skills/<name>/`에 복사하고 manifest 등록까지 처리한다.

`share`는 기존 `import`를 대체하지 않는다. `import <path>`는 low-level primitive로
남기고, `share --from <host>`가 host-aware workflow를 제공한다.

### 6.3 `my-skills enable` / `disable`

```bash
my-skills enable <skill>
my-skills disable <skill>
```

역할:

- manifest의 `[skills.<name>].enabled`를 수정한다.
- skill compatibility인 `hosts`는 수정하지 않는다.
- 처음에는 단순 append/update만 지원하고, 복잡한 TOML formatting 보존은 범위 밖으로 둔다.

이 명령은 agent 대화에서 risk decision 이후 선택을 반영하기 위한 작은 primitive다.

### 6.4 JSON plan output

Agent integration을 위해 write 명령에는 가능한 한 `--plan --json` 또는
`--dry-run --json`을 제공한다.

권장 shape:

```json
{
  "action": "share",
  "source_host": "claude",
  "candidates": [
    {
      "name": "office-hours",
      "source": "/Users/.../.claude/skills/gstack/office-hours",
      "canonical_exists": false,
      "recommended_default": "import_enabled_all_hosts",
      "risks": [
        {
          "severity": "warning",
          "code": "host_absolute_path",
          "message": "body contains ~/.claude references"
        }
      ],
      "choices": ["import_enabled_all_hosts", "import_disabled", "show_diff", "skip"]
    }
  ]
}
```

Severity:

- `blocker`: apply must not proceed without explicit force or source edit
- `warning`: apply can proceed after user confirmation
- `info`: context only

---

## 7. Agent Skill Surface

### 7.1 Add a canonical management skill

Add a host-neutral skill:

```text
skills/my-skills/
└── SKILL.md
```

Purpose:

- route user requests like "list my skills", "share this skill", "install this here"
- run the CLI primitives
- render risk decisions in the current agent conversation
- avoid each host inventing its own flow

Example frontmatter:

```yaml
---
name: my-skills
description: Manage the user's personal cross-agent skill registry. Use when listing available skills, importing a host-local skill into my-skills, installing a shared skill into the current agent, or checking skill drift.
---
```

### 7.2 Slash command strategy

Preferred:

- Hosts that expose Agent Skills as slash commands should let users run `/my-skills`.

Fallback:

- Hosts that do not expose slash commands use natural language trigger or their native
  skill invocation syntax.

Do not bake slash syntax into the core CLI. Slash command support is host UI behavior.

### 7.3 Host detection

The management skill should not guess from path strings if the host already exposes
environment or executable context. Detection order:

1. Explicit user instruction: "from Claude", "install into Hermes"
2. Known host environment variables, if any are documented
3. Current agent executable/session metadata if available
4. Ask the user to choose host

The CLI should still accept `--from` and `--host` explicitly, because deterministic
commands are easier to test.

---

## 8. Risk Model

Risk list should be built from existing validators first.

Existing checks to reuse:

- `validate_skill`
- `scan_skill`
- `status_of`
- `plan_install`
- `hash_directory`

Additional share-specific risks:

| Code | Severity | Meaning |
|------|----------|---------|
| `canonical_name_collision` | blocker | canonical skill exists with different content |
| `invalid_frontmatter` | blocker | source is not a valid Agent Skill |
| `secret_like_content` | blocker | private key, token, AWS key, or secret assignment detected |
| `host_absolute_path` | warning | source mentions `~/.claude`, `~/.agents`, `~/.hermes`, `/Users/...`, or `/home/...` |
| `host_specific_tools` | warning | body or frontmatter names host-specific tools or `allowed-tools` |
| `executable_scripts` | warning | source includes `scripts/` or executable files |
| `large_assets` | warning | source contains large binary assets |
| `description_weak` | info | description is valid but not useful for trigger routing |

Default recommendation:

- blocker exists: do not import
- warnings only: show the warning list and ask before import; default to enabled only after explicit user choice
- no warnings: import enabled and target all enabled hosts by default

---

## 9. Implementation Plan

### Phase 1: Read-only catalog

- Add `cmd_skills`.
- Parse frontmatter descriptions.
- Add tests for table output and JSON output.
- Keep `status` unchanged.

Success:

```bash
uv run my-skills skills
uv run my-skills skills --json
uv run my-skills skills --with-status
```

### Phase 2: Plan output for agents

- Add `--json` to `install --dry-run`.
- Add `share --from <host> --plan --json`.
- Reuse validators and status planner.
- No manifest writes yet.

Success:

```bash
uv run my-skills share --from claude --plan --json
```

returns deterministic candidate/risk JSON.

### Phase 3: Manifest enable primitives

- Add `enable` and `disable`.
- Support new skill registration after `share`.
- Preserve simple TOML structure; avoid a new dependency unless formatting becomes a blocker.

Success:

```bash
uv run my-skills enable repo-analysis
uv run my-skills disable repo-analysis
```

updates `[skills.repo-analysis].enabled`.

### Phase 4: Agent management skill

- Add `skills/my-skills/SKILL.md`.
- Install it into Claude, Codex, Hermes through the existing pipeline.
- The skill should only orchestrate CLI commands and user decisions.

Success:

- In Claude Code: `/my-skills` can list catalog and guide import/install.
- In Codex/Hermes: equivalent native skill invocation works, even if slash syntax differs.

### Phase 5: Apply share workflow

- Add `share --from <host> <skill> --enable|--disable`.
- Import source skill into canonical.
- Register manifest entry.
- Run validation before and after.
- Never overwrite different canonical content without explicit force.

Success:

```bash
uv run my-skills share --from claude repo-analysis --enable
uv run my-skills sync repo-analysis
```

works in a temporary HOME integration test.

---

## 10. Testing Strategy

Unit tests:

- `skills` lists manifest skills and descriptions.
- `skills --host claude` filters compatibility.
- `skills --with-status` includes status without changing state.
- `share --plan` reports missing, unmanaged, invalid, collision, and warning cases.
- `enable` and `disable` update manifest entries.

Integration tests:

- Temporary HOME with fake Claude/Codex/Hermes skill directories.
- Import a host-local skill, register it disabled, enable it, sync it to another host.
- Collision with existing canonical skill blocks.
- Secret-like source blocks.
- Repeated share/sync is idempotent.

Manual QA gate:

- Install `my-skills` management skill into at least Claude Code and one other host.
- From the agent conversation, ask to list skills.
- From the agent conversation, ask to share a host-local test skill.
- Confirm risk list appears before write.
- Confirm user can choose disabled import.
- Confirm `my-skills skills --with-status` reflects the result.

---

## 11. 결정 사항

1. `share --from <host>`의 기본 UX
   - 결정: agent 대화에서는 먼저 host의 skill list를 보여주고 사용자가 선택하게 한다.
   - CLI 기준으로는 `share --from <host> --plan`이 전체 후보 scan을 담당하고, 실제 apply는 선택된 skill 이름을 명시한다.
   - 이유: read-only 탐색은 넓게 허용하되, file write는 명시적 선택 뒤에만 수행한다.

2. 공유 기본값
   - 결정: 공통으로 쓰는 skill이 많지 않을 것으로 보고, 전체 공유가 쉬운 방향을 기본값으로 둔다.
   - warning이 없고 사용자가 import를 선택하면 `enabled = true`와 모든 enabled target host를 기본값으로 한다.
   - warning이 있으면 risk list를 먼저 보여주고, 사용자가 명시적으로 선택한 경우에만 enabled import를 허용한다.

3. host-specific slash command shim
   - 결정: 보류한다.
   - 먼저 canonical `my-skills` management skill 하나로 시작하고, 특정 host에서 ergonomics가 나쁠 때만 shim을 추가한다.

4. 명령어 이름
   - 결정: `share`로 통일한다.
   - `promote`는 내부 의미를 설명할 때만 쓰고, 사용자-facing 명령어 이름으로는 사용하지 않는다.
   - 이유: 사용자는 "이 skill 공유해줘"라고 말할 가능성이 높고, 별도 명칭 논의로 얻는 이득이 작다.

---

## 12. 권장 다음 단계

Phase 1인 `my-skills skills`를 먼저 만든다.

이 작업은 risk가 낮고 read-only이며, 일상적인 사용성을 바로 개선한다. 또한 이후
agent conversation flow가 재사용할 catalog model을 만든다. 그 다음에는 manifest를
수정하는 workflow를 추가하기 전에 JSON plan output을 먼저 추가한다.
