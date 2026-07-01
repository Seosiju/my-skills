# 기본 스킬 seed + private registry 설계

> 작성일: 2026-07-01
> 상태: 설계 확정 (방향 A)
> 대상 저장소: `my-skills`
> 관련: `docs/2026-06-30-my-skills-open-source-roadmap.md` (Phase 3 agent UX 이후 보완)

## 1. 문제

오픈소스 배포 후 "사용자가 자기 스킬을 어떻게 관리하는가"가 초기 설계에 없었다.
현재 구조는 다음 두 목표를 **동시에** 달성하지 못한다.

1. `my-skills`를 설치하면 기본 스킬이 자동으로 바로 쓰인다.
2. 개인/회사 스킬은 각자의 private registry에서 관리한다.

원인은 런타임이 canonical root를 **하나만** 잡기 때문이다.

- `find_repo_root`는 `MY_SKILLS_ROOT` → cwd 상향탐색 → 캐시 순으로 단 하나의
  `my-skills.toml` + `skills/`를 찾는다. 레이어링이 없다.
  (`src/my_skills/cli_runtime.py:38`)
- `uv tool install git+...`는 `src/my_skills`(CLI)만 설치한다. 기본 스킬
  (`cli-inventory`, `personal-profile`, `my-skills`, `my-jira`)은 clone한 repo
  안에만 존재하고 wheel에는 들어가지 않는다. (`pyproject.toml`)
- `init-registry`는 `[skills.*]` 0개 + 빈 `skills/`를 만든다. 직후 `install`은
  아무것도 설치하지 않는다. (`src/my_skills/init_registry_commands.py:65`)

결과적으로 사용자가 느끼는 "스킬 로딩 환경이 둘로 갈린 것"은 의도된 설계가 아니라
single-root 모델이 강제한 **우발적 분열**이다.

- 공개 clone을 root로: 기본 스킬은 되나 개인 스킬을 공개 repo에 커밋해야 한다.
- private registry를 root로: 개인 스킬은 되나 기본 스킬이 사라진다.
- 둘을 `MY_SKILLS_ROOT`로 오갈 뿐 동시 사용은 원천적으로 불가능하다.

## 2. 결정: 방향 A — 기본 스킬을 registry에 seed

`init-registry`가 새 private registry를 만들 때 공개 기본 스킬을 그 안으로 **복사**한다.
이후 사용자는 자기 registry(single root) 하나만 쓴다.

기각한 대안:

- **B. CLI 패키지 동봉 + built-in 레이어 병합**: 개념적으로 더 깔끔하고 업스트림
  갱신이 자동으로 흐르지만, root 해석/병합 로직을 바꿔야 하고 그건 "기존 동작 파괴"
  위험이 실재하는 유일한 변경이다. 사용자가 default drift를 실제로 아파하기 전에는
  값어치가 안 난다. **장기 방향으로 열어둔다.**
- **C. 매니페스트 다중 registry**: 충돌/우선순위/write 대상 결정 등 복잡도가 지금
  단계에 과하다. 비범위.

A를 택한 이유:

- 런타임 root 해석을 건드리지 않으므로 기존 동작 파괴가 구조적으로 불가능하다.
- `init-registry` 직후 `install`이 실제로 무언가를 설치하게 되어 cold-start 절벽이
  사라진다.
- 사용자당 registry 1개로 "두 환경"이 자연스럽게 통합된다.
- 현재 버전 `0.1.0`, GitHub-first, PyPI 미배포, 고정 사용자 부재 — 지금이 저비용
  구간이다.

## 3. 설계 상세

### 3.1 기본 스킬을 패키지 seed 데이터로 포함

A라도 clone 없이(`uv tool install`만으로) 동작하려면 seed 원본이 패키지에 있어야 한다.
B와의 차이는 **사용 방식**이다: A는 init 때 한 번 복사하고 끝(single-root 유지),
B는 매 실행 병합(root 해석 변경). A의 패키징 부담은 seed 트리 하나뿐이다.

- 공개 안전한 기본 스킬을 wheel에 read-only seed로 포함한다.
- 저장 위치 후보: `src/my_skills/_defaults/skills/<name>/`. 빌드 시 repo `skills/`에서
  복제하거나(빌드 스텝) 이 트리를 직접 원천으로 둔다. **repo `skills/`를 단일
  원천으로 두고 빌드 시 복제**를 권장한다(원본 이중화 방지).
- `pyproject.toml`의 hatchling wheel 타겟에 seed 트리를 package data로 포함한다.

### 3.2 seed 대상과 enabled 기본값

기본 4개를 seed하되, 첫 install에서 자동 켜질지는 skill별로 다르다.

- `cli-inventory`, `personal-profile` → seed + `enabled = true` (config 없이 바로 동작).
- `my-skills` → seed + `enabled = true`. 에이전트가 CLI를 부리는 진입점이라 seed 중
  가장 중요하다. 단 현재 `skills/my-skills/SKILL.md` 본문이 옛 온보딩(clone +
  `uv run bootstrap`)을 전제하므로, seed 흐름(`init-registry --with-defaults`)에
  맞게 본문을 갱신한다.
- `my-jira` → seed + `enabled = false`. config가 있어야 동작하므로 첫 install에서
  자동 설치하면 미설정 스킬이 에러를 낸다. registry에는 "실제 config를 외부화한
  예시"로 존재시키되, 사용자가 config 채운 뒤 `my-skills enable my-jira`로 켠다.

seed 목록과 skill별 enabled 기본값은 코드 한 곳(`defaults.py` 상수)에서만 정의한다.

### 3.3 `init-registry --with-defaults`

- 플래그: `--with-defaults`(기본 ON) / `--no-defaults`(빈 registry 원할 때).
- 동작:
  1. 기존처럼 `my-skills.toml`, `skills/`, `.gitignore`, `README.md` 생성.
  2. seed 스킬을 새 registry `skills/<name>/`로 복사.
  3. 생성되는 `my-skills.toml`의 `[skills.<name>]`에 seed 스킬을 §3.2 enabled
     기본값대로 등록한다 (`hosts = ["claude", "codex", "hermes"]`).
- 결과: 직후 `my-skills install --dry-run`이 실제 설치 계획을 보여준다.

### 3.4 provenance (1차 필수 — capture-once, manifest 저장)

**seed 시점에만 얻는 정보다.** init 때 기록하지 않으면, 나중에 `update-defaults`가
생겨도 "이 스킬이 seed에서 왔나 / 사용자가 직접 썼나"를 영영 구분할 수 없다(그때
이미 존재하던 registry는 복구 불가). 그래서 소비자(update-defaults)가 후속이어도
**저장은 1차에** 한다.

저장 위치: **생성되는 `my-skills.toml`의 스킬별 블록.** InstallRecord가 이미 쓰는
어휘를 그대로 재사용한다(새 파일·새 어휘 없음):

```toml
[skills.cli-inventory]
enabled = true
hosts = ["claude", "codex", "hermes"]
source_type = "builtin-seed"     # 없으면 = 사용자 작성(local-authored)
source_revision = "0.1.0"        # seed 당시 CLI 버전 = 나중 diff 기준선
```

- 1차 범위 = 이 2필드 저장까지. 콘텐츠 해시 비교/업데이트 판정 같은 diff 메커닉은
  update-defaults에서 설계한다(그때가 검증 가능한 시점).
- 연속성(선택, 후속): install이 manifest의 `source_type`을 읽어 InstallRecord에
  승계하면 canonical→host까지 출처가 이어진다. 지금은 필수 아님.
- 주의: `InstallRecord`(state.py)의 provenance는 (skill, host) host 설치 단위라
  canonical seed 스킬 출처를 담을 수 없다. 그래서 install state가 아니라 manifest에 둔다.

### 3.5 문서/온보딩 통합 (단일 현관문)

지금 README는 "clone+bootstrap" 경로와 "private registry" 경로가 경쟁하듯 병렬로 있다.
신규 사용자 권장 경로를 **하나**로 정리한다.

```bash
uv tool install git+https://github.com/Seosiju/my-skills.git
my-skills init-registry            # 위치 프롬프트 + 기본 스킬 seed + git init
my-skills install
```

- **현관문 = `init-registry`.** 실사용자는 이것 하나로 registry 생성·seed·git init까지 끝낸다.
- **`bootstrap`은 기여자/유지보수자 전용으로 재분류한다.** bootstrap의 CLI 설치 스텝은
  `uv tool install --editable <clone>`(개발 설치)이라 clone을 가진 사람만 쓴다. seed가
  "clone해서 기본 스킬 받기" 용도를 없앴으므로, seed 이후 bootstrap의 유일한 청중은
  개발자다. **삭제하지 않고 온보딩 문서에서만 제외**한다.

### 3.6 registry 위치 입력 (설치 UX)

`init-registry`의 경로 인자를 필수 위치인자에서 선택으로 바꾼다.

- 기본 위치: `~/my-agent-skills` (OS 무관하게 `expanduser`로 렌더링:
  macOS/Linux `~/my-agent-skills`, Windows `C:\Users\<user>\my-agent-skills`).
- 경로 생략 + 대화형(TTY): 프롬프트 `Registry location [~/my-agent-skills]:`.
  빈 엔터 = 기본값, 절대/상대경로 입력 시 그 위치(상대는 cwd 기준).
- 경로 생략 + 비대화형(CI/파이프, `stdin.isatty()` false): 프롬프트를 걸지 않고
  기본 위치를 쓰고 생성 위치를 출력한다. 명시 경로 인자는 항상 그대로 동작한다.
- 대상 경로가 **이미 registry**(`my-skills.toml` 존재)인 경우: 자동 접미사(`(1)`)로
  중복 생성하지 않는다(registry는 remote에 push하는 버전관리 저장소라 중복이 사고를
  낸다). 대화형이면 "이미 registry가 있음 → 다른 경로 입력 or 기존 것 사용"으로
  되묻고, 비대화형이면 기존처럼 명확히 거부한다. 기존 registry에 나중에 나온 기본
  스킬을 추가하는 top-up은 init이 아니라 후속 `update-defaults`가 담당한다.

### 3.7 git init 자동화

registry는 결국 private remote로 push하는 git 저장소다. init 직후 커밋 가능한 상태로
넘긴다.

- `init-registry`가 `git init`을 실행한다(+ best-effort 첫 커밋).
- 가드: git 미설치면 크래시 없이 "건너뜀" 알림, 이미 git repo면 건너뜀, git user
  설정 없으면 첫 커밋은 실패해도 무시(`git init`까지는 보장).
- `--no-git`으로 opt-out(직접 git을 관리하려는 사용자, 또는 로컬 전용 사용자용).

**git은 제품 전체에서 optional이다.** CLI(`install`/`sync`/`audit`/`skills`)는 git을
요구하지 않는다. registry는 `my-skills.toml` + `skills/`를 가진 평범한 폴더면 충분하고,
git init은 편의(버전관리 + remote push 준비)일 뿐이다. 로컬 전용 사용자는 `--no-git`으로
순수 폴더 registry를 쓸 수 있으며, 이 경우 버전 히스토리·롤백·다중 PC 동기화·백업을
포기하는 트레이드오프는 사용자 책임이다. (host 사본 drift 감지는 git과 무관하게 동작한다.)

## 4. 비목표

- built-in 레이어 실시간 병합(B), 다중 registry(C).
- 업스트림 default 자동 동기화. (provenance만 남기고 pull은 후속)
- root 해석 로직 변경. **런타임은 그대로 둔다.**

## 5. 수용 기준

- clean 환경(임시 HOME/XDG, clone 없음)에서:
  - `uv tool install git+...` → `init-registry ~/tmp/reg` 후 `~/tmp/reg/skills/`에
    core defaults가 존재한다.
  - 생성된 `my-skills.toml`에 seed 스킬이 등록되어 있다.
  - `my-skills install --dry-run`이 빈손이 아니라 실제 계획을 낸다.
  - `my-skills install` 후 `~/.claude/skills/`에 스킬이 떨어지고 Claude 세션이 인식한다.
- `--no-defaults`는 기존처럼 빈 registry를 만든다(회귀 없음).
- `find_repo_root`/매니페스트 해석 로직은 변경되지 않는다(diff로 증명).
- 기존 clone+bootstrap 경로가 여전히 동작한다.
- wheel(`uv build`)에 seed 트리가 포함된다.

## 6. 테스트 계획 (콜드 E2E — "private repo 경로가 진짜 도는가")

임시 HOME/XDG로 신규 사용자를 흉내 낸다.

```bash
export HOME=$(mktemp -d); export XDG_CONFIG_HOME="$HOME/.config"
uv tool install --force git+https://github.com/Seosiju/my-skills.git
my-skills init-registry "$HOME/reg" && cd "$HOME/reg" && git init
my-skills skills            # seed 스킬이 보이는가
my-skills install --dry-run # 실제 계획이 나오는가
my-skills install
ls ~/.claude/skills         # 실제로 떨어졌는가
```

`--dry-run`에서 "설치할 게 없다"가 나오면 그 지점이 정확히 고칠 곳이다.
이 워크스루를 `tests/`에 픽스처 기반으로 고정한다(회귀 방지).

## 7. 실행 순서

상세 태스크(T1~T13 — 파일·수용기준·의존성·T9 본문 포함)는
`docs/2026-07-01-seed-implementation-spec.md`가 authoritative다. 여기 목록을
중복해서 두면 이중 관리로 드리프트나므로 두지 않는다.

착수 순서 요약: T10·T13(독립) → T1 → T2 → T3 → T4/T5/T6 → T7 → T8 → T9/T11/T12.

## 8. 결정 기록

확정 (2026-07-01):

- seed 기본값 = **ON** (`--no-defaults`로 opt-out). 이유: "설치하면 바로 동작"이 원 목표.
- `my-jira` = **seed 포함 + `enabled = false`**. 실제 config 외부화 예시로 두되 첫
  install에서 자동 설치는 안 함.
- `my-skills` 스킬 = **seed + `enabled = true`**. 에이전트가 CLI를 부리는 진입점.
  본문은 seed 온보딩 흐름으로 갱신 필요.
- seed 원천 = **옵션 1 (repo `skills/` 단일 원천 + 빌드 복제)** + editable fallback.
- registry 기본 위치 = **`~/my-agent-skills`** (경로 선택 입력 + TTY 프롬프트).
- 현관문 = **`init-registry`** 단일화. **`bootstrap`은 기여자/유지보수자 전용**으로
  재분류(삭제하지 않고 온보딩 문서에서만 제외). 이유: bootstrap의 CLI 설치가
  editable(개발 설치)이라 clone 보유자만 쓰고, seed가 clone 온보딩을 대체함.
- git init = **자동 실행** + best-effort 첫 커밋, `--no-git` opt-out, 가드(git 미설치/
  기존 repo/커밋 identity 없음).
- 기존 registry 경로에 재실행 = **자동 접미사 안 함.** 대화형 되물음 / 비대화형 거부.
  top-up은 후속 `update-defaults`.
- provenance = **1차 필수, manifest 저장(capture-once)**. seed 시점에만 얻는
  정보라 init 때 `my-skills.toml` 스킬 블록에 `source_type="builtin-seed"`,
  `source_revision`(기존 InstallRecord 어휘 재사용) 기록. install state가 아니라
  manifest에 두는 이유는 InstallRecord가 (skill,host) 단위이기 때문. diff 메커닉은
  update-defaults에서. §3.4.

후속 (이번 범위 밖):

- 윈도우 지원 검증. 코드는 cross-platform(`expanduser`)이나 윈도우 CI가 없고 host
  CLI의 윈도우 경로 규약이 미검증. 별도 "윈도우 검증" 항목으로 둔다.
- `my-skills update-defaults`(업스트림 seed 재-pull). provenance 필드가 토대.
- built-in 레이어 실시간 병합(방향 B)은 default drift를 사용자가 아파하기 시작하면 재검토.
- registry 원격 동기화(on-demand). `my-skills push` 명령 또는 `registry-sync` 스킬로
  "stage → diff → commit → push"를 사람이 부를 때 실행한다. **background watcher/데몬,
  변경 즉시 auto-push는 채택하지 않는다**(roadmap §4 비목표). auto-push는 실수로 커밋된
  민감정보나 반쯤 편집된 상태를 원격에 올릴 수 있으므로 push 전 **diff 확인(사람 검토)
  단계를 반드시 둔다**. 참고: audit gate는 축 1(install/share/import)에만 걸리고
  registry→원격 push(축 2)에는 관여하지 않으므로 "audit 우회"는 auto-push를 막는
  근거가 아니다(원하면 `push`가 pre-push audit를 courtesy로 돌릴 수는 있음).
