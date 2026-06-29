# my-skills

[English](README.md) | **한국어**

개인용 크로스 에이전트 **Agent Skill 레지스트리**. 스킬을 한 번 작성해 하나의
정규(canonical) 위치에 보관하고, Claude Code·Codex·Hermes 전반에 설치·동기화·공유합니다.

정규 포맷은 [Agent Skills](https://agentskills.io/specification) 표준입니다.
각 스킬은 `skills/<name>/` 아래의 디렉터리이며, YAML frontmatter(`name`,
`description`)를 가진 `SKILL.md`를 포함합니다.

## 현재 상태

**Phase 7 — 카탈로그 및 공유 UX.** 레지스트리는 이제 이식 가능한 스킬 코어,
안전한 install/status/uninstall 라이프사이클, 드리프트를 인지하는 `sync`,
공유 데이터 루트, `import`, 개발용 `--mode link`, `skills` 카탈로그(기본적으로
호스트별 설치 상태를 인라인으로 표시), 그리고 호스트-로컬 스킬을 정규 `skills/`로
승격하는 `share` 워크플로를 모두 포함합니다.

watch 모드, 마켓플레이스 스타일의 publish/export, 업그레이드 마이그레이션,
실제 Windows/WSL2 검증 같은 항목은 향후 작업으로 `docs/`에 남아 있습니다.

### 사용 가능한 스킬

| 스킬 | 하는 일 |
|------|---------|
| `repo-analysis` | 낯선 저장소에서 방향을 잡기 위한 호스트 중립 루틴(목적, 구조, 빌드/테스트 명령, 엔트리 포인트). |
| `cli-inventory` | 워크플로가 요구하는 CLI 도구를 선언하고 `scripts/check_tools.py`로 PATH 가용성을 확인합니다. 요구 도구 *정책*은 커밋되지만, 머신별 실제 결과는 머신-로컬로 유지됩니다. |
| `shared-agent-operation` | AI 코딩 에이전트 전반에서 공유하는 기본적이고 호스트 중립적인 운영 관례. |
| `personal-profile` | 메모리형 스킬: 사용자에 대한 지속적인 사실(정체성, 선호)을 기억하고 에이전트 전반에 적용합니다. 정규 본체에는 지침과 스키마만 두고, 프로필 데이터는 [공유 데이터 루트](#공유-데이터-루트)에 저장되며 절대 커밋되지 않습니다. |
| `my-skills` | 카탈로그, share, install, sync, enable, disable 워크플로를 CLI로 안내하는 에이전트용 관리 스킬. |

**머신-로컬 경계.** 정규 스킬은 머신별 데이터(호스트명, 절대 경로, 계정,
인증/버전)를 절대 저장하지 않습니다. 그런 데이터는 git이 무시하는 `local/`
디렉터리(예: `local/cli-inventory/`)에 위치합니다 —
`skills/cli-inventory/references/required-tools.md` 참고.

## 요구 사항

- Python 3.11+
- [uv](https://docs.astral.sh/uv/)

## 명령어

```bash
# 모든 정규 스킬(또는 하나)을 표준에 대해 검증한다.
uv run my-skills validate
uv run my-skills validate shared-agent-operation

# 환경, 감지된 호스트, 대상 경로, 매니페스트 상태를 보고한다.
uv run my-skills doctor

# 등록된 정규 스킬을 호스트별 설치 상태와 함께 나열한다.
# 에이전트/UI 용도로는 --json을 추가한다.
uv run my-skills skills
uv run my-skills skills --json

# 아무것도 쓰지 않고 설치 계획을 미리 본 뒤 설치한다.
uv run my-skills install --dry-run
uv run my-skills install --dry-run --json
uv run my-skills install            # 활성 스킬 -> 활성 대상 (복사)
uv run my-skills install email-drafting --host claude

# 스킬별·호스트별 설치 상태를 표시한다.
uv run my-skills status

# 변경 없이 드리프트를 감지한다 (FRESH가 아니면 비정상 종료코드).
uv run my-skills sync --check

# 정규 편집을 관리되는 설치본으로 전파한다 (누락 생성, 오래된 항목 갱신).
uv run my-skills sync

# 관리되는 설치본을 제거한다 (state에 기록된 대상만).
uv run my-skills uninstall email-drafting --host claude

# 스킬의 공유 머신-로컬 데이터 디렉터리를 출력한다 (--create로 생성).
uv run my-skills data-path personal-profile
uv run my-skills data-path personal-profile --create

# 개발용 설치: 호스트 복사본을 정규에 심볼릭 링크한다 (편집이 즉시 반영).
uv run my-skills install repo-analysis --host claude --mode link

# 외부 스킬 디렉터리를 정규 skills/로 가져온다 (--force로 덮어쓰기).
uv run my-skills import ~/.hermes/skills/repo-analysis

# 정규 my-skills로 공유하기 전에 호스트-로컬 스킬을 검토한다.
uv run my-skills share --from claude --plan --json

# 호스트-로컬 스킬 하나를 공유하고 매니페스트에 등록한 뒤 동기화한다.
uv run my-skills share --from claude repo-analysis --enable
uv run my-skills share --from claude repo-analysis --disable
uv run my-skills sync repo-analysis

# 기본 install/sync 선택을 위한 매니페스트 참여 여부를 토글한다.
uv run my-skills enable repo-analysis
uv run my-skills disable repo-analysis
```

`validate`, `install`, `sync`(및 `--check`)는 오류/차단/드리프트 시 비정상
종료코드를 반환하므로 CI에서 그대로 사용할 수 있습니다.

### 공유 데이터 루트

정규 스킬은 `sync` 시 각 호스트로 *복사*되므로, 복사본 기준의 `local/`
디렉터리는 호스트마다 갈라집니다. 실제 머신-로컬 데이터를 가진 스킬(예:
`personal-profile` 메모리)은 대신 모든 호스트가 공유하는 단일 머신 레벨
데이터 루트를 읽고 씁니다:

```text
$XDG_DATA_HOME/my-skills/<skill>/        # POSIX (대체 ~/.local/share/...)
%LOCALAPPDATA%\my-skills\data\<skill>\   # Windows
```

`my-skills data-path <skill>`이 그 경로를 해석하므로 `SKILL.md`가 경로를
하드코딩할 필요가 없습니다. 데이터 루트는 머신-로컬이며 절대 커밋되지 않습니다 —
스킬 호스트 중립성에 대한 유일하게 허용된 예외입니다(경로가 어느 호스트가 아니라
`my-skills`에 속하기 때문). 순수 지침 스킬(`repo-analysis`,
`shared-agent-operation`)에는 필요하지 않습니다.

### 개발 모드 (`--mode link`)

`install --mode link`는 호스트 복사본을 정규 스킬에 대한 **디렉터리
심볼릭 링크**로 만들어, `sync` 없이도 편집이 즉시 반영됩니다. 링크된 설치본은
항상 `FRESH`로 보고됩니다. 링크가 교체되면 `DRIFTED`로, 삭제되면 재설치가
다시 링크합니다(복사하지 않음). `uninstall`은 심볼릭 링크만 제거하며, 링크가
가리키는 정규 소스는 절대 삭제되지 않습니다. OS가 심볼릭 링크를 만들 수 없으면
조용히 복사로 떨어지지 않고 명시적 메시지와 함께 실패합니다. 복사 모드가
기본값입니다.

### 기존 스킬 가져오기

`import <path>`는 어떤 호스트에서 작성한 스킬을 정규 `skills/`로 가져옵니다.
소스를 검증(표준 + 보안 스캔)한 뒤 frontmatter `name` 아래로 복사합니다.
동일한 스킬은 무동작이며, *다른* 기존 스킬은 `--force`를 넘기지 않는 한
건드리지 않습니다. import는 `skills/`에만 쓰므로, 이후 `my-skills.toml`에
`[skills.<name>]`을 추가하고 `sync`를 실행합니다.

### 호스트-로컬 스킬 공유

`share --from <host> --plan --json`은 호스트 스킬 디렉터리를 스캔해 에이전트
대화용으로 결정적인 후보/위험 JSON을 내보냅니다. 각 후보는 소스 경로, 정규
상태, 콘텐츠 해시, 검증/보안 위험, 가능한 선택지, 권장 기본값을 보고합니다.

계획을 검토한 뒤 선택한 스킬 하나를 적용합니다:

```bash
uv run my-skills share --from claude repo-analysis --enable
uv run my-skills share --from claude repo-analysis --disable
```

`share`는 소스를 검증하고, 정규 `skills/`로 복사하며, 활성 대상 전체에 대해
`[skills.<name>]`을 등록하고, 정규 복사본을 검증하며, 소스 호스트의 기존
복사본을 로컬 state로 흡수해 `sync <skill>`이 그 소스 호스트를 비관리 충돌로
취급하지 않고 계속 진행하도록 합니다. 다른 정규 복사본은 `--force`를 명시적으로
넘기지 않는 한 절대 덮어쓰이지 않습니다.

### 에이전트 관리 스킬

정규 `my-skills` 스킬은 다른 스킬과 동일하게 설치됩니다:

```bash
uv run my-skills install my-skills --host all
```

Claude Code에서는 호스트가 스킬을 슬래시 명령으로 노출할 때 `/my-skills`로
호출할 수 있습니다. Codex와 Hermes에서는 호스트의 기본 스킬 호출 방식을
사용합니다. 이 스킬은 파일을 직접 수정하지 않고, 에이전트가 `my-skills` 명령을
실행하고 사용자에게 plan/risk 출력을 보여주며 사용자가 선택한 항목만 적용하도록
안내합니다.

### 드리프트 상태

`status`와 `sync --check`는 각 (스킬, 호스트)를 분류합니다:

| 상태 | 의미 |
|------|------|
| `FRESH` | 설치 복사본이 정규본 및 기록된 state와 일치 |
| `STALE` | 정규본이 변경됨; `sync`가 설치본을 갱신 |
| `DRIFTED` | 설치 복사본이 로컬에서 편집됨; `sync`가 덮어쓰지 않음 |
| `CONFLICT` | 정규본과 설치 복사본이 모두 변경됨 — 자동 병합 불가 |
| `MISSING` | 등록되었지만 설치되지 않음 |
| `UNMANAGED` | my-skills가 설치하지 않은 복사본이 존재 |
| `UNSUPPORTED` | 호스트가 스킬의 `hosts` 목록에 없음 |

`sync`는 안전한 경우만 씁니다(누락 생성, 오래된 항목 갱신). `DRIFTED`,
`CONFLICT`, `UNMANAGED`는 보고되며 비정상 종료코드로 차단됩니다.

### 안전 모델

- **기본은 복사.** 설치는 정규 디렉터리를 복사하며, 다음 `install` 또는
  `sync` 전까지 설치 복사본은 바뀌지 않습니다. `--mode link`는 명시적 개발
  모드이며, 심볼릭 링크 생성이 실패하면 조용히 복사로 떨어지지 않고 오류로
  처리합니다.
- **충돌 = 차단.** 대상이 이미 존재하고 my-skills가 관리하는 것으로 기록되어
  있지 *않으면* 설치가 차단되고 아무것도 덮어쓰지 않습니다.
- **드리프트 보호.** 설치 복사본이 로컬에서 수정되었으면 install과 uninstall은
  덮어쓰기를 거부하고 대신 보고합니다.
- **관리 대상만 uninstall.** `uninstall`은 로컬 state 파일에 기록된 대상만
  제거하며, 비관리 형제 파일은 절대 건드리지 않습니다.
- **원자적 쓰기.** 설치는 임시 디렉터리에 준비한 뒤 제자리로 교체합니다.
  실패 시 이전 복사본을 복원합니다. state도 원자적으로 기록됩니다.

설치 state는 머신-로컬(`$XDG_STATE_HOME` 또는 `~/.local/state/my-skills/`
아래)이며 절대 커밋되지 않습니다.

## 구조

```text
my-skills.toml            # 매니페스트: 대상 + 스킬 + 기본값
skills/<name>/SKILL.md    # 정규 스킬
src/my_skills/            # 패키지 (config, frontmatter, validation, security, hosts, cli)
tests/                    # 단위 + 픽스처 기반 테스트
```

매니페스트의 `enabled` 플래그가 기본 선택을 제어합니다. 인자 없는
`install` / `sync`는 `enabled = true`인 스킬만 대상으로 하며, `--all`을 넘기면
등록된 모든 스킬을 대상으로 합니다(계획의 5.5 / 9.5 / 9.6 절 참고).

## 테스트

```bash
uv run pytest
```
