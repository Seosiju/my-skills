# Skill Drive v1: Windows 실행 문서

> 대상: Windows PC에서 이 작업을 이어받는 에이전트와 사용자
> 기준: `dev` 브랜치 (이 문서가 들어간 커밋 이후)
> 관련 문서: 스펙 `docs/2026-11-23-skill-drive-rust-v1-spec.md` §7.1, §12 / 인계 문서 `docs/2026-11-23-skill-drive-v1-status-and-handoff.md` §7 H

이 문서 하나만 보고 Windows PC에서 진행할 수 있게 썼다. 순서대로 진행한다.
**[멈춤]** 표시가 있는 곳에서는 결과를 사용자에게 보여 주고 확인을 받은 뒤에 다음으로 넘어간다.

## 0. 목표와 범위

| 단계 | 하는 일 | 실제 환경을 바꾸는가 |
|---|---|---|
| 준비. 개발 환경 | Git, MSVC 빌드 도구, Rust 설치와 확인 | 소프트웨어 설치 (**[멈춤]** 사용자 확인 후) |
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
function Write-Utf8($Path, $Text) {
  # Resolve against the PowerShell location, not the process directory.
  $full = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Path)
  [IO.File]::WriteAllText($full, $Text, (New-Object Text.UTF8Encoding $false))
}
```

**터미널을 새로 열었을 때** (3.3에서 에이전트를 새 세션으로 확인하다 보면 생기기 쉽다): 위 함수와 아래 변수를 다시 설정한 뒤 이어서 진행한다.

```powershell
$T  = Join-Path $env:TEMP "skill drive probe"
$MS = (Resolve-Path "<tool repo>\target\release\my-skills.exe").Path
# 3단계 안이라면 격리 환경도 다시 설정한다
$env:MY_SKILLS_ROOT = "$T\registry"; $env:XDG_STATE_HOME = "$T\state"; $env:XDG_CONFIG_HOME = "$T\config"
```

---

## 준비. 개발 환경

이 작업에 꼭 필요한 것은 **Git for Windows**, **MSVC 빌드 도구**, **Rust(MSVC toolchain)** 세 가지다.
`dev`에는 Rust CLI의 소스와 테스트가 있다. 이 절차는 소스에서 빌드한 실행 파일을 사용한다.

### 이미 있는지 먼저 확인

```powershell
Get-Command git, cargo, rustup -ErrorAction SilentlyContinue | Format-Table Name, Source
winget --version              # winget이 있어야 아래 설치 명령을 쓸 수 있다
```

모두 있으면 아래 "설치 확인"으로 바로 간다.

### 설치 **[멈춤: 소프트웨어 설치는 사용자 확인 후]**

아래는 일반적으로 쓰이는 winget 패키지 ID와 설치 옵션이다. macOS에서 작성해서 이 PC에서 실행해 보지는 않았다. **설치가 실패하면 출력을 기록하고 멈춘다.**

```powershell
winget install --id Git.Git -e                       # Git for Windows (Git Bash 포함)

# MSVC 링커와 Windows SDK. 없으면 Rust 빌드가 링크 단계에서 실패한다.
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"

winget install --id Rustlang.Rustup -e
```

설치한 뒤에는 **새 PowerShell 창**을 연다. 그래야 PATH가 반영된다.

```powershell
rustup default stable-x86_64-pc-windows-msvc        # CI와 같은 stable을 쓴다
rustup component add rustfmt clippy
```

### 설치 확인

```powershell
git --version
rustup show active-toolchain         # stable-x86_64-pc-windows-msvc
cargo --version; rustc --version
cargo fmt --version; cargo clippy --version

# 링커까지 동작하는지 작은 프로그램으로 확인한다.
$Check = Join-Path $env:TEMP "rust-link-check"
Remove-Item -Recurse -Force $Check -ErrorAction SilentlyContinue
cargo new --bin --quiet $Check
cargo run --quiet --manifest-path "$Check\Cargo.toml"   # "Hello, world!"
Remove-Item -Recurse -Force $Check
```

`cargo run`이 `link.exe not found`나 `kernel32.lib`를 못 찾는다는 오류로 실패하면 MSVC 빌드 도구가 제대로 설치되지 않은 것이다. 기록하고 **[멈춤]**.

### Windows에서 코드를 고칠 때 (선택)

이 runbook에서는 코드를 고치지 않는다(5단계). 나중에 Windows에서 개발할 때는 다음 기준을 따른다.

CI와 같은 검사를 돌린다.

```powershell
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

`dev`에는 Python 테스트 모음이 없다.

---

## 1. 사전 점검 (읽기만)

```powershell
git --version
cargo --version; rustc --version          # Rust (MSVC toolchain)
rustup show active-toolchain               # x86_64-pc-windows-msvc 여야 함
Get-Command my-skills -All -ErrorAction SilentlyContinue | Format-Table Source
if (Get-Command my-skills -ErrorAction SilentlyContinue) { my-skills --version }   # 기존 Python 0.4.x가 있으면 버전이 나옴
Get-Command claude, codex, agy -ErrorAction SilentlyContinue | Format-Table Name, Source  # 세 에이전트 설치 여부
Test-Path "$env:USERPROFILE\.local\state\my-skills\state.json"
Test-Path "$env:USERPROFILE\.config\my-skills\root"
foreach ($d in ".claude\skills", ".agents\skills", ".gemini\config\skills") {
  "$d : " + (Test-Path "$env:USERPROFILE\$d")
}
```

판단 기준:

| 결과 | 의미 | 할 일 |
|---|---|---|
| `cargo`가 없음 | Rust가 없음 | "준비. 개발 환경"으로 돌아간다 |
| `rustup show`가 `-gnu` | 다른 toolchain | MSVC toolchain으로 바꿀지 **[멈춤]** |
| `my-skills`가 이미 있음 (0.4.x) | Python 판이 설치돼 있음 | 지우지 않는다. 2~3단계에서는 Rust binary를 **전체 경로로** 실행해서 충돌을 피한다 |
| `state.json`이 있음 | 이 PC에 기존 연결 기록이 있음 | 4단계에서 copy 기록을 convert할 수 있다. 지금은 기록만 해 둔다. 스펙 Q3(Windows state 위치) 판단에도 쓴다 |
| `claude`, `codex`, `agy` 중 없는 명령이 있음 | 그 에이전트가 설치되지 않음 | 3.3에서 그 에이전트는 확인할 수 없다. "junction을 못 찾음"과 구분해서 기록한다 |
| host 폴더에 레지스트리 스킬과 같은 이름의 폴더가 있는데 기록은 없음 | 손으로 복사했거나 다른 도구가 만든 것 | 4단계 dry-run에서 `BLOCK_CONFLICT`로 나온다. **[멈춤]** 처리 방법은 사용자가 정한다 |

결과는 5단계의 기록 양식에 적는다.

---

## 2. 빌드와 테스트

```powershell
cd <tool repo>                     # 없으면: git clone https://github.com/Seosiju/my-skills.git
git fetch origin; git switch dev; git pull
cargo test --all-targets           # 전체가 통과해야 한다
cargo test --lib drive::link::tests::windows -- --nocapture   # junction 테스트 4개가 실제로 실행되는지
cargo build --release
$MS = (Resolve-Path .\target\release\my-skills.exe).Path
git rev-parse --short HEAD         # 검증한 커밋. 5단계 결과 표에 적는다
& $MS --version                    # my-skills 1.0.0-alpha.N
```

- `cargo test --all-targets`가 실패하면 **[멈춤]** 실패 출력 전체를 기록한다.
- `link` 테스트 이름에 `windows::`가 보이지 않으면 Windows 코드가 실행되지 않은 것이다. 기록하고 **[멈춤]**.
- 이 단계에서는 `$MS`를 PATH에 넣지 않는다.

---

## 3. 안전한 확인 (실제 레지스트리와 기록은 건드리지 않음)

### 3.1 격리 환경 만들기

실제 연결 기록과 활성 레지스트리 설정을 건드리지 않도록 state와 config 위치를 임시 폴더로 돌린다.
임시 폴더 이름에 일부러 공백을 넣었다. 공백이 든 경로에서 link 판정과 hook이 동작하는지도 함께 확인하기 위해서다.
host 폴더는 실제 위치를 쓴다. 에이전트가 실제로 스킬을 발견하는지 확인하는 것이 목적이기 때문이다.

```powershell
$T = Join-Path $env:TEMP "skill drive probe"   # 공백이 든 경로로 시험한다 (스펙 §12 항목 4, hook 스크립트 인용 처리)
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
| agy (Antigravity CLI) | 먼저 **PowerShell에서** `agy -p '/skills' --output-format json`으로 목록을 본다(Git Bash에서는 `/skills`가 Windows 경로로 변환되므로 반드시 PowerShell에서 실행한다). 이 비대화형 조회는 모델을 호출하지 않으며, macOS에서 Antigravity CLI 1.2.16으로 검증됐다(`docs/release-checklist.md`). 이어서 새 세션에서 같은 질문으로 한 번 더 확인한다 |

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
post-checkout과 post-merge hook은 sync를 실행해 link를 만든다. 그래서 **이 단계에서는 host 경로를 임시 폴더로 돌린다.**
돌리지 않으면 실제 host 폴더에 시험용 link가 생기고, 3.6에서 임시 폴더를 지운 뒤 대상이 없는 junction으로 남는다.

```powershell
cd "$T\registry"
Write-Utf8 "$T\registry\my-skills.local.toml" @"
[targets.claude]
path = '$T\hosts\claude'

[targets.codex]
path = '$T\hosts\codex'

[targets.agy]
path = '$T\hosts\agy'
"@
& $MS doctor --no-update-check   # Hosts의 세 경로가 모두 $T\hosts\... 인지 확인. 아니면 [멈춤]
git init -q; git add -A; git -c user.name=probe -c user.email=probe@example.invalid commit -qm probe
& $MS hooks install
& $MS hooks status               # 네 hook 모두 installed
"T = $T"; "MS = $MS"             # 아래 Git Bash에 그대로 붙여 넣는다
```

Git Bash에서 같은 임시 레지스트리로 다음을 확인한다. hook은 Git Bash의 sh로 실행되기 때문이다.

```bash
T_WIN='<위에서 출력한 T 값>'
MS="$(cygpath -u '<위에서 출력한 MS 값>')"
cd "$(cygpath -u "$T_WIN")/registry"
echo "HOME=$HOME"                                   # 형식을 기록한다
command -v my-skills || echo "my-skills not on PATH" # 어느 판이 먼저 잡히는지 기록한다
export PATH="$(dirname "$MS"):$PATH"                # 이 Git Bash 세션에서만 Rust 판을 앞에 둔다
command -v my-skills                                # 이제 Rust 판 경로여야 한다
export MY_SKILLS_ROOT="$(cygpath -w "$PWD")"
export XDG_STATE_HOME="$T_WIN\\state" XDG_CONFIG_HOME="$T_WIN\\config"
"$MS" doctor --no-update-check | sed -n '/^Registry:/,/^Manifest/p'   # Registry, State, Data, Hosts 경로 형식

# (1) pre-commit: 깨진 스킬은 커밋되지 않아야 한다
printf -- '---\nname: Broken--Name\n---\n' > skills/skill-drive-probe/SKILL.md
git add -A && git -c user.name=probe -c user.email=probe@example.invalid commit -m broken
git log --oneline | head -1                          # 여전히 "probe" 커밋이어야 함
git checkout HEAD -- skills/skill-drive-probe/SKILL.md   # stage된 깨진 내용까지 되돌림

# (2) post-checkout과 post-merge: 새 스킬이 자동으로 연결돼야 한다
git switch -q -c side                                # post-checkout(브랜치 전환): sync 실행
mkdir -p skills/skill-drive-probe-two
printf -- '---\nname: skill-drive-probe-two\ndescription: Second temporary probe.\n---\n\nBody.\n' > skills/skill-drive-probe-two/SKILL.md
printf '\n[skills.skill-drive-probe-two]\nenabled = true\n' >> my-skills.toml
git add -A && git -c user.name=probe -c user.email=probe@example.invalid commit -qm "add second probe"
git switch -q -                                      # 원래 브랜치로: post-checkout
git merge -q --ff-only side                          # post-merge: sync 실행
ls -la "$(cygpath -u "$T_WIN")/hosts/claude"         # skill-drive-probe, skill-drive-probe-two가 보여야 함
```

PowerShell에서 결과를 한 번 더 확인한다.

```powershell
Get-Item "$T\hosts\claude\skill-drive-probe-two" | Select-Object FullName, LinkType, Target   # Junction
```

확인할 것:
- **pre-commit**: 커밋을 막았는지, 그리고 막은 이유가 `[BLOCKED] skill-drive-probe: name must be ...` 같은 검증 메시지인지.
  hook은 PATH의 `my-skills`를 먼저 찾는다. 위에서 PATH 앞에 Rust 판을 둔 것은 이 때문이다.
  처음 `command -v`에서 Python 판이 잡혔다면 그 사실을 기록한다. Python 판이 실행되면 `hooks` 명령이 없어 `invalid choice` 같은 오류로 막히는데, 이는 검증이 동작한 것이 아니다. 실제 사용에서는 4.1의 PATH 순서로 해결한다.
- **post-checkout과 post-merge**: 새 스킬이 임시 host 폴더에 junction으로 연결됐는지. 연결되지 않았다면 Git Bash 환경에서 sync가 실패한 것이다. 출력을 기록하고 **[멈춤]**.
- **경로 형식**: `HOME`이 `/c/Users/...` 형식일 때, `doctor`의 Registry, State, Data, Hosts가 모두 올바른 Windows 경로인지. 지금 코드는 `HOME`을 `USERPROFILE`보다 먼저 쓰기 때문에, 경로가 깨지면 이것이 원인이다. 기록하고 **[멈춤]** (코드 수정이 필요함).
- **공백 경로**: 위 과정 전체가 공백이 든 `$T`에서 문제없이 동작했는지.

### 3.6 정리

```powershell
cd ~
# 먼저 실제 host 폴더에 시험용 스킬이 남지 않았는지 확인한다 (3.4에서 해제했어야 함).
# 하나라도 True면 3.4를 다시 한 뒤 진행한다. 그대로 임시 폴더를 지우면 대상이 없는 junction이 남는다.
foreach ($d in ".claude\skills", ".agents\skills", ".gemini\config\skills") {
  "$d : " + (Test-Path "$env:USERPROFILE\$d\skill-drive-probe")
}
Remove-Item Env:MY_SKILLS_ROOT, Env:XDG_STATE_HOME, Env:XDG_CONFIG_HOME
Remove-Item -Recurse -Force (Join-Path $env:TEMP "skill drive probe")   # 임시 폴더만. 안의 junction은 모두 임시 폴더 안을 가리킨다
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
새 터미널에서는 0절의 `Write-Utf8` 함수를 다시 정의한다. 4단계부터는 `$MS` 대신 PATH의 `my-skills`를 쓴다.

### 4.2 레지스트리 준비

```powershell
# 이미 clone돼 있으면 git pull만. 없으면 사용자에게 원격 주소를 받아 clone한다.
git clone <registry remote> <registry>
cd <registry>
git status -sb                 # 기본 브랜치이고 깨끗해야 함
git config --get core.autocrlf                         # 값을 기록한다 (없으면 빈 줄)
Test-Path .gitattributes                               # 현재 레지스트리에는 없다
(git ls-files --eol | Select-String "w/crlf").Count    # 작업 폴더에서 CRLF로 바뀐 파일 수
```

줄바꿈은 스펙 §12 항목 5의 확인 대상이다. 지금 레지스트리에는 `.sh` 스크립트가 없고 Python 스크립트는 CRLF여도 대체로 동작하므로, 문제가 보이지 않으면 기록만 한다.

레지스트리의 `my-skills.toml`에는 지금 `install_mode = "copy"`가 명시돼 있다. 맥 전환(인계 문서 §7 G)에서 바뀔 예정이다.
**이 PC에서는 기기별 설정 파일로 기본은 link, `agy`만 copy로 지정한다.** Windows에서 Antigravity CLI(`agy`)는 junction으로 연결된 스킬을 발견하지 못하므로 `agy`만 `[targets.agy] install_mode = "copy"`로 두고 나머지 호스트는 link를 쓴다. 이 파일은 레지스트리 `.gitignore`에 있어서 다른 기기에 영향이 없으며, `convert --to link --host all`은 명시된 `[targets.agy] install_mode = "copy"`를 보고 `agy`를 자동으로 건너뛴다.

```powershell
Write-Utf8 (Join-Path (Get-Location) "my-skills.local.toml") @"
[defaults]
install_mode = "link"

[targets.agy]
install_mode = "copy"
"@
```

3.3에서 기본 경로와 다른 host 경로를 찾았다면, 그 경로도 이 파일의 `[targets.<host>]`에 `path`로 넣는다.

### 4.3 dry-run **[멈춤: 사용자 승인]**

`status`, `doctor`, `bootstrap`, `sync`는 현재 폴더에서 레지스트리를 찾을 수 있다.
활성 레지스트리 기록이 없으면 `status`나 `sync --dry-run`이 처음 찾은 위치를 기록할 수 있다
(`~\.config\my-skills\root`가 기본 위치). 어느 레지스트리를 볼지 명시하고 이 기록을
생성하지 않도록 미리보기 동안에는 `MY_SKILLS_ROOT`를 지정한다.

```powershell
$env:MY_SKILLS_ROOT = (Resolve-Path .).Path        # <registry> 안에서 실행
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

기존 copy 기록이 있으면 **convert를 먼저, 그 다음 bootstrap**을 실행한다.
convert는 이미 기록된 연결만 바꾼다. 기록이 일부만 있으면(예: 일부 host만, 또는 맥에서 새로 추가한 스킬이 없음) 나머지는 bootstrap이 연결한다.
순서를 반대로 하면 곧 link로 바꿀 copy를 bootstrap이 한 번 더 갱신하게 된다.

```powershell
Remove-Item Env:MY_SKILLS_ROOT                        # 4.3에서 임시로 지정한 것
my-skills set-root <registry>
my-skills convert --to link --host all --yes          # 4.3에서 기존 copy 전환을 승인한 경우에만, 먼저
my-skills bootstrap                                   # 항상: 남은 스킬과 host를 연결
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
| 검증한 커밋 | `git rev-parse --short HEAD` 값 (2단계에서 빌드한 커밋) |
| 개발 환경 | 새로 설치한 것, Rust 버전, MSVC 빌드 도구 버전 |
| `cargo test --all-targets` | 통과 N개 / 실패 N개 (Windows 링크 테스트 실행 여부) |
| 기존 Python 0.4.x, 기존 state.json (Q3 판단용) | 있음 / 없음, 있음 / 없음 |
| 세 에이전트 명령 설치 여부 | claude / codex / agy 각각 |
| Link capability | junction / symlink / copy fallback |
| Claude Code 발견 | 예 / 아니요 (방법, 스킬 폴더 경로) |
| Codex 발견 | 예 / 아니요 (방법, 스킬 폴더 경로) |
| agy 발견 | 예 / 아니요 (`/skills` 결과, 스킬 폴더 경로) |
| Git Bash `HOME` 형식과 doctor 경로 | 예: `/c/Users/...`, 경로 정상 / 깨짐 |
| pre-commit 차단 (Git Bash) | 예 / 아니요 (막은 이유가 검증 메시지인지) |
| post-checkout, post-merge가 link를 만듦 (Git Bash) | 예 / 아니요 |
| 처음 Git Bash에서 잡힌 my-skills | Rust 판 / Python 판 / 없음 |
| 공백이 든 경로에서 동작 | 정상 / 문제 |
| 줄바꿈 | core.autocrlf 값, CRLF 파일 수, 문제 여부 |
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
