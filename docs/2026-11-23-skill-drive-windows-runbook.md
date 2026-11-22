# Skill Drive v1: Windows 실행 문서

> 대상: Windows PC에서 이 작업을 이어받는 에이전트와 사용자
> 기준: `dev` 브랜치 (이 문서가 들어간 커밋 이후)
> 관련 문서: 스펙 `docs/2026-11-23-skill-drive-rust-v1-spec.md` §7.1, §12 / 인계 문서 `docs/2026-11-23-skill-drive-v1-status-and-handoff.md` §7 H

이 문서 하나만 보고 Windows PC에서 진행할 수 있게 썼다. 순서대로 진행한다.
**[멈춤]** 표시가 있는 곳에서는 결과를 사용자에게 보여 주고 확인을 받은 뒤에 다음으로 넘어간다.

## 0. 목표와 범위

| 단계 | 하는 일 | 실제 환경을 바꾸는가 |
|---|---|---|
| 1. 사전 점검 | 도구, 기존 Python `my-skills`, 기존 연결 기록 확인 | 아니요 (읽기만) |
| 2. 빌드와 테스트 | `dev`를 받아 빌드하고 테스트 실행 | 아니요 (저장소 안에서만) |
| 3. 안전한 확인 | 임시 레지스트리와 시험용 스킬로 junction 발견, 경로, hook 확인 | 시험용 스킬 하나를 host 폴더에 연결했다가 지움 |
| 4. 실제 연결 | 사용자 레지스트리를 이 PC의 세 에이전트에 link로 연결 | **예** (dry-run 후 승인) |
| 5. 결과 기록 | 스펙 §12에 결과를 적고 `dev`에 push | 저장소만 |

**금지:**
- 원본(레지스트리의 `skills/`)을 직접 지우거나 고치는 정리 작업
- 관리자 권한으로만 통과하는 우회
- link 실패를 copy 대체로 조용히 넘기는 처리 (출력에 `[copy fallback]`이 나오면 기록하고 멈춘다)
- `--force`를 쓰는 덮어쓰기
- 레지스트리 저장소에 커밋하거나 push하기 (이 문서의 범위가 아님)

명령은 PowerShell 기준이다. Git Bash가 필요한 단계는 따로 표시했다.
파일은 BOM 없는 UTF-8로 쓴다. Windows PowerShell 5.1의 `Set-Content -Encoding utf8`는 BOM을 붙이므로 쓰지 않고, 아래의 `Write-Utf8` 함수를 쓴다.

```powershell
function Write-Utf8($Path, $Text) { [IO.File]::WriteAllText($Path, $Text, (New-Object Text.UTF8Encoding $false)) }
```

---

## 1. 사전 점검 (읽기만)

```powershell
git --version
cargo --version; rustc --version          # Rust (MSVC toolchain)
rustup show active-toolchain               # x86_64-pc-windows-msvc 여야 함
Get-Command my-skills -All -ErrorAction SilentlyContinue | Format-Table Source
my-skills --version 2>$null                # 기존 Python 0.4.x가 있으면 버전이 나옴
Test-Path "$env:USERPROFILE\.local\state\my-skills\state.json"
Test-Path "$env:USERPROFILE\.config\my-skills\root"
foreach ($d in ".claude\skills", ".agents\skills", ".gemini\config\skills") {
  "$d : " + (Test-Path "$env:USERPROFILE\$d")
}
```

판단 기준:

| 결과 | 의미 | 할 일 |
|---|---|---|
| `cargo`가 없음 | Rust가 없음 | rustup으로 설치. MSVC 빌드 도구("Desktop development with C++")가 필요하다. 설치는 **[멈춤]** 사용자 확인 후 |
| `rustup show`가 `-gnu` | 다른 toolchain | MSVC toolchain으로 바꿀지 **[멈춤]** |
| `my-skills`가 이미 있음 (0.4.x) | Python 판이 설치돼 있음 | 지우지 않는다. 2~3단계에서는 Rust binary를 **전체 경로로** 실행해서 충돌을 피한다 |
| `state.json`이 있음 | 이 PC에 기존 연결 기록이 있음 | 4단계에서 copy 기록을 convert할 수 있다. 지금은 기록만 해 둔다 |
| host 폴더에 레지스트리 스킬과 같은 이름의 폴더가 있는데 기록은 없음 | 손으로 복사했거나 다른 도구가 만든 것 | 4단계 dry-run에서 `BLOCK_CONFLICT`로 나온다. **[멈춤]** 처리 방법은 사용자가 정한다 |

결과는 5단계의 기록 양식에 적는다.

---

## 2. 빌드와 테스트

```powershell
cd <tool repo>                     # 없으면: git clone https://github.com/Seosiju/my-skills.git
git fetch origin; git switch dev; git pull
cargo test                         # 전체가 통과해야 한다
cargo test --lib drive::link::tests::windows -- --nocapture   # junction 테스트 4개가 실제로 실행되는지
cargo build --release
$MS = (Resolve-Path .\target\release\my-skills.exe).Path
& $MS --version                    # my-skills 1.0.0-alpha.N
```

- `cargo test`가 실패하면 **[멈춤]** 실패 출력 전체를 기록한다.
- `link` 테스트 이름에 `windows::`가 보이지 않으면 Windows 코드가 실행되지 않은 것이다. 기록하고 **[멈춤]**.
- 이 단계에서는 `$MS`를 PATH에 넣지 않는다.

---

## 3. 안전한 확인 (실제 레지스트리와 기록은 건드리지 않음)

### 3.1 격리 환경 만들기

실제 연결 기록과 활성 레지스트리 설정을 건드리지 않도록 state와 config 위치를 임시 폴더로 돌린다.
host 폴더는 실제 위치를 쓴다. 에이전트가 실제로 스킬을 발견하는지 확인하는 것이 목적이기 때문이다.

```powershell
$T = Join-Path $env:TEMP "skill-drive-probe"
Remove-Item -Recurse -Force $T -ErrorAction SilentlyContinue   # 이 임시 폴더만 지운다
New-Item -ItemType Directory "$T\registry\skills\skill-drive-probe" | Out-Null
Write-Utf8 "$T\registry\skills\skill-drive-probe\SKILL.md" @"
---
name: skill-drive-probe
description: Temporary probe for the my-skills Windows check. Reply with the word PROBE-OK when asked about it.
---

This skill exists only to verify that the agent discovers skills linked by a Windows junction.
"@
Write-Utf8 "$T\registry\my-skills.toml" @"
schema_version = 1
skills_root = "skills"

[defaults]
install_mode = "link"

[targets.claude]
enabled = true

[targets.codex]
enabled = true

[targets.agy]
enabled = true

[targets.hermes]
enabled = false

[skills.skill-drive-probe]
enabled = true
"@

$env:MY_SKILLS_ROOT  = "$T\registry"
$env:XDG_STATE_HOME  = "$T\state"
$env:XDG_CONFIG_HOME = "$T\config"
& $MS doctor --no-update-check
"T = $T"; "MS = $MS"          # 두 값을 3.5에서 Git Bash에 그대로 붙여 넣는다
```

`doctor`에서 확인할 것:
- `Link capability: junction`. 다른 값이 나오면 기록하고 **[멈춤]**.
- Hosts의 경로가 모두 `C:\Users\<이름>\...` 형태의 올바른 Windows 경로인지. `/c/...`나 `\`와 `/`가 섞인 경로가 보이면 기록하고 **[멈춤]**.

### 3.2 시험용 스킬 연결

시험용 스킬 하나만 실제 host 폴더 세 곳에 junction으로 연결한다. 같은 이름의 폴더가 이미 있으면 my-skills가 막는다.

```powershell
& $MS sync --dry-run          # CREATE 3건 (claude, codex, agy), mode link
& $MS sync --yes              # created: ... (link: junction) 세 줄
& $MS status                  # 세 host 모두 FRESH
foreach ($d in ".claude\skills", ".agents\skills", ".gemini\config\skills") {
  Get-Item "$env:USERPROFILE\$d\skill-drive-probe" | Select-Object FullName, LinkType, Target
}
```

`LinkType`이 `Junction`이어야 한다.

### 3.3 에이전트가 발견하는지 확인 (가장 중요)

각 에이전트를 **새 세션**으로 시작해서 확인한다. 이미 열려 있던 세션은 스킬 목록을 다시 읽지 않을 수 있다.

| 에이전트 | 확인 방법 |
|---|---|
| Claude Code | 새 세션에서 "사용 가능한 스킬 중에 skill-drive-probe가 있나? 있으면 그 스킬의 지시대로 답해 줘"라고 묻는다. `PROBE-OK`가 나오면 발견한 것이다 |
| Codex | 같은 방법으로 묻는다 |
| agy (Antigravity CLI) | 새 세션에서 `/skills`로 목록을 확인한다. 같은 질문으로도 한 번 더 확인한다 |

어떤 에이전트가 발견하지 못하면 다음을 차례로 확인하고 결과를 적는다. 이것이 스펙 Q1의 경로 실측이다.
1. 그 에이전트의 문서나 설정에서 사용자 스킬 폴더 위치를 찾는다. 위치가 다르면 임시 manifest의 `[targets.<host>]`에 `path = '...'`를 넣고 3.2를 다시 한다.
2. 위치가 같은데도 못 찾으면 junction을 따라가지 못하는 것일 수 있다. 시험용 스킬을 그 host 폴더에 직접 복사해 보고, 발견되는지 비교한다. 복사본은 확인한 뒤 직접 지운다.
3. 결과에 따라 junction, symlink(개발자 모드), copy 중 무엇이 동작하는지 기록하고 **[멈춤]**.

### 3.4 시험용 스킬 해제

```powershell
& $MS uninstall skill-drive-probe --host all --yes   # removed 세 줄
foreach ($d in ".claude\skills", ".agents\skills", ".gemini\config\skills") {
  "$d : " + (Test-Path "$env:USERPROFILE\$d\skill-drive-probe")   # 모두 False
}
Test-Path "$T\registry\skills\skill-drive-probe\SKILL.md"           # True: 원본은 그대로
```

### 3.5 hook과 Git Bash 확인

임시 레지스트리를 git 저장소로 만들고 hook을 시험한다. 실제 레지스트리에는 하지 않는다.

```powershell
cd "$T\registry"
git init -q; git add -A; git -c user.name=probe -c user.email=probe@example.invalid commit -qm probe
& $MS hooks install
& $MS hooks status            # 네 hook 모두 installed, "my-skills on PATH" 결과 확인
```

Git Bash에서 같은 임시 레지스트리로 다음을 확인한다. hook은 Git Bash의 sh로 실행되기 때문이다.

```bash
T_WIN='<3.1에서 출력한 T 값>'             # 예: C:\Users\<이름>\AppData\Local\Temp\skill-drive-probe
MS="$(cygpath -u '<3.1에서 출력한 MS 값>')"   # 3단계에서는 Rust binary가 PATH에 없다
cd "$(cygpath -u "$T_WIN")/registry"
echo "HOME=$HOME"                       # 형식을 기록한다
export MY_SKILLS_ROOT="$(cygpath -w "$PWD")"
export XDG_STATE_HOME="$T_WIN\\state" XDG_CONFIG_HOME="$T_WIN\\config"
printf -- '---\nname: Broken--Name\n---\n' > skills/skill-drive-probe/SKILL.md
git add -A && git -c user.name=probe -c user.email=probe@example.invalid commit -m broken
# 기대: [BLOCKED] skill-drive-probe: ... 이 출력되고 커밋이 만들어지지 않음
git log --oneline | head -1            # 여전히 "probe" 커밋이어야 함
git checkout HEAD -- skills/skill-drive-probe/SKILL.md   # stage된 깨진 내용까지 되돌림
"$MS" doctor --no-update-check 2>/dev/null | sed -n '/Hosts:/,/Manifest/p'   # 경로 형식 확인
```

확인할 것:
- pre-commit이 커밋을 막았는지. **막은 이유가 `[BLOCKED] skill-drive-probe: name must be ...` 같은 검증 메시지여야 한다.**
  hook은 PATH의 `my-skills`를 먼저 찾는다. 1단계에서 Python 판이 PATH에 있었다면 hook이 Python 판을 실행한다. Python 판에는 `hooks` 명령이 없어서 `invalid choice` 같은 오류로 커밋이 막힌다. 그렇게 막혔다면 검증이 동작한 것이 아니다. 기록하고, 4.1에서 PATH 순서를 정한 뒤 다시 확인한다
- hook이 `my-skills`를 찾았는지. PATH에 없으면 hook 안에 기록된 전체 경로를 쓴다
- **`HOME`이 `/c/Users/...` 형식일 때** Git Bash에서 `my-skills doctor`를 실행해 host 경로가 올바른 Windows 경로로 나오는지. 지금 코드는 `HOME`을 `USERPROFILE`보다 먼저 쓴다. 경로가 깨지면 이것이 원인이다. 기록하고 **[멈춤]** (코드 수정이 필요함)

### 3.6 정리

```powershell
cd ~
Remove-Item Env:MY_SKILLS_ROOT, Env:XDG_STATE_HOME, Env:XDG_CONFIG_HOME
Remove-Item -Recurse -Force (Join-Path $env:TEMP "skill-drive-probe")   # 임시 폴더만
```

3단계에서 하나라도 **[멈춤]**이 나왔다면 4단계로 가지 않는다.

---

## 4. 실제 연결 (dry-run 후 승인)

### 4.1 binary를 쓸 수 있게 두기 **[멈춤: PATH 변경]**

```powershell
$Bin = Join-Path $env:LOCALAPPDATA "Programs\my-skills\bin"
New-Item -ItemType Directory -Force $Bin | Out-Null
Copy-Item $MS "$Bin\my-skills.exe"
```

사용자 PATH에서 `$Bin`이 Python 판 `my-skills`보다 **앞에** 오도록 할지 사용자에게 확인한다. Python 판은 지우지 않는다.
바꾼 뒤 새 PowerShell과 새 Git Bash에서 `my-skills --version`이 Rust 판(1.0.0-alpha.N)을 가리키는지 확인한다.

### 4.2 레지스트리 준비

```powershell
# 이미 clone돼 있으면 git pull만. 없으면 사용자에게 원격 주소를 받아 clone한다.
git clone <registry remote> <registry>
cd <registry>
git status -sb                 # 기본 브랜치이고 깨끗해야 함
```

레지스트리의 `my-skills.toml`에는 지금 `install_mode = "copy"`가 명시돼 있다. 맥 전환(인계 문서 §7 G)에서 바뀔 예정이다.
**이 PC에서는 기기별 설정 파일로 link를 지정한다.** 이 파일은 레지스트리 `.gitignore`에 있어서 다른 기기에 영향이 없다.

```powershell
Write-Utf8 (Join-Path (Get-Location) "my-skills.local.toml") @"
[defaults]
install_mode = "link"
"@
```

3.3에서 기본 경로와 다른 host 경로를 찾았다면, 그 경로도 이 파일의 `[targets.<host>]`에 `path`로 넣는다.

### 4.3 dry-run **[멈춤: 사용자 승인]**

```powershell
my-skills set-root <registry> --dry-run
my-skills doctor --no-update-check
my-skills status
my-skills bootstrap --dry-run
my-skills sync --dry-run
```

사용자에게 보여 줄 것:
- `doctor`의 레지스트리 위치, Link capability, 연결 집계
- `sync --dry-run`의 동작 목록. CREATE는 새 link이고, BLOCK은 막힌 항목이다. 항목별로 개수를 센다

경우별 처리:

| dry-run 결과 | 처리 |
|---|---|
| 모두 CREATE (link) | 승인을 받으면 4.4로 |
| 1단계에서 본 기존 copy 기록이 있음 | `my-skills convert --to link --host all --dry-run` 결과도 함께 보여 준다. 승인을 받으면 4.4에서 convert도 실행 |
| `BLOCK_CONFLICT` (같은 이름의 폴더가 있는데 기록이 없음) | **진행하지 않는다.** 해당 폴더를 어떻게 할지 사용자가 정한다 |
| `[copy fallback]` 또는 `BLOCK_DRIFT` | 진행하지 않고 기록한다 |

### 4.4 연결

```powershell
my-skills set-root <registry>
my-skills bootstrap                                   # 기존 copy가 없을 때
my-skills convert --to link --host all --yes          # 4.3에서 승인한 경우에만
my-skills status                                      # 활성 host 모두 FRESH
my-skills sync --check                                # 종료 코드 0
my-skills hooks install
my-skills hooks status
```

마지막으로 3.3과 같은 방법으로 세 에이전트의 새 세션에서 실제 스킬 하나가 보이는지 확인한다.

---

## 5. 결과 기록

스펙 `docs/2026-11-23-skill-drive-rust-v1-spec.md` §12 "Windows 확인 결과"의 "실기기" 아래에 다음 표를 채워 넣는다.
**공개 저장소다.** 사용자 이름, 실제 스킬 이름, 전체 경로는 쓰지 않는다. 경로는 `%USERPROFILE%\...` 형태로 적는다.

```markdown
**실기기 (YYYY-MM-DD, Windows 버전, Rust 버전):**

| 항목 | 결과 |
|---|---|
| cargo test | 통과 N개 / 실패 N개 (windows 링크 테스트 실행 여부) |
| Link capability | junction / symlink / copy fallback |
| Claude Code 발견 | 예 / 아니요 (방법, 스킬 폴더 경로) |
| Codex 발견 | 예 / 아니요 (방법, 스킬 폴더 경로) |
| agy 발견 | 예 / 아니요 (`/skills` 결과, 스킬 폴더 경로) |
| Git Bash `HOME` 형식과 doctor 경로 | 예: `/c/Users/...`, 경로 정상 / 깨짐 |
| pre-commit 차단 (Git Bash) | 예 / 아니요 |
| hook이 my-skills를 찾음 | PATH / 기록된 전체 경로 / 실패 |
| 실제 연결 (4단계) | 진행함 / 보류 (이유). 진행했다면 FRESH 개수, convert 개수 |
| 기타 | 발견한 문제 |
```

그리고 스펙 §7.1 표의 Windows 경로에 붙은 "(실기기 확인 필요)"를 실측한 값으로 바꾼다.

```powershell
cd <tool repo>
git switch dev; git pull
git add docs/2026-11-23-skill-drive-rust-v1-spec.md
git commit -m "docs(spec): record Windows device verification"
git push
```

코드 문제(예: 3.5의 `HOME` 경로 문제)는 고치지 말고 기록만 한다. 수정은 맥 쪽 작업과 맞춰 별도로 진행한다.
