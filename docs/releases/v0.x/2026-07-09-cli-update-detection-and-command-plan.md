# CLI 업데이트 감지 및 명시적 업데이트 명령 기획서

> 작성일: 2026-07-09
> 상태: 구현 대기 (Codex 작업용)
> 대상 저장소: github.com/Seosiju/my-skills
> 관련 문서: docs/2026-07-04-registry-root-reliability-and-docs-plan.md, docs/release-checklist.md

## 0. 용어와 전제

이 문서는 아래 용어만 사용한다.

| 용어 | 의미 |
|---|---|
| **tool repo** | 이 저장소. CLI 소스, seed 스킬, 문서, 테스트가 있는 공개 저장소 |
| **installed CLI** | `uv tool install` 등으로 설치되어 `PATH`에서 실행되는 `my-skills` 명령 |
| **main** | 개발 브랜치. 코드가 merge되어도 그 자체가 안정 릴리스는 아니다 |
| **release tag** | 배포 기준점. `v0.2.0`, `v0.3.0` 같은 SemVer tag |
| **latest stable release** | remote tag 중 `vMAJOR.MINOR.PATCH` 형식의 가장 높은 SemVer tag |
| **stable channel** | 기본 업데이트 채널. latest stable release tag로 설치한다 |
| **main channel** | 개발용 업데이트 채널. `origin/main` HEAD로 설치한다 |

**중요한 제품 계약:** `main`에 merge/push된 코드는 저장소 최신 코드일 뿐이고,
사용자 컴퓨터의 installed CLI는 자동으로 바뀌지 않는다. 업데이트는 release tag를
기준으로 감지하고, 사용자가 명시적으로 `my-skills update`를 실행할 때만 적용한다.

## 1. 문제 정의

현재 사용자는 아래 흐름을 수동으로 반복해야 한다.

```bash
my-skills --version
uv tool list | grep my-skills
git ls-remote --tags --refs https://github.com/Seosiju/my-skills.git 'v*'
uv tool install --force git+https://github.com/Seosiju/my-skills.git@v0.2.0
```

이 구조는 두 가지 혼란을 만든다.

1. **main에 반영된 코드와 설치된 실행본을 혼동한다.**
   구현이 merge되어도 installed CLI는 이전 tag/commit을 계속 실행할 수 있다.
2. **업데이트 가능 여부를 사용자가 매번 직접 확인해야 한다.**
   `doctor`가 CLI/registry 상태는 보여주지만, 설치본이 최신 릴리스인지 알려주지
   않는다.

사용자가 원하는 동작은 "자동 적용"이 아니라 다음 두 가지다.

- 업데이트 가능 여부는 CLI가 감지한다.
- 실제 업데이트는 명시적 명령어로만 수행한다.

## 2. 진단 — 현재 확인된 사실

### F1. 현재 릴리스 기준점은 `v0.2.0`이다

2026-07-09 현재 remote release tag는 `v0.2.0` 하나다.

```text
6486c87252d56a0a752d77a670b4abd4764edf9e refs/tags/v0.2.0
```

`pyproject.toml`과 `src/my_skills/__init__.py`의 패키지 버전도 `0.2.0`이다.

### F2. installed CLI는 버전만 출력하고 업데이트 상태는 모른다

현재 `my-skills --version`은 다음처럼 패키지 버전만 출력한다.

```text
my-skills 0.2.0
```

이 값만으로는 최신 release tag 존재 여부, 설치 요청 ref, 설치 commit이 최신인지
판단할 수 없다.

### F3. `doctor`는 진단 표면이지만 update check가 없다

`src/my_skills/inspection_commands.py`의 `cmd_doctor`는 CLI 버전, OS, Python,
registry, state/data path, hosts, manifest 상태를 출력한다. 사용자가 "내 설치본이
최신인가?"를 확인할 자연스러운 위치는 `doctor`이지만 현재 update 상태 줄은 없다.

### F4. CLI는 argparse 기반이고 새 subcommand 추가가 단순하다

`src/my_skills/cli.py`의 `build_parser()`가 모든 subcommand를 정의한다.
`update`도 기존 패턴처럼 `cmd_update`를 import하고 `p_update.set_defaults(...)`로
연결하면 된다.

### F5. uv는 Git URL tool install과 재설치를 지원한다

uv 공식 문서 기준으로 `uv tool install git+https://github.com/httpie/cli` 같은 Git
source 설치가 가능하며, version/source 제약을 바꾸려면 tool을 다시 install하는
방식이 권장된다. 따라서 self-update 구현은 `uv tool install --force
git+https://github.com/Seosiju/my-skills.git@<ref>`를 실행하는 방식으로 잡는다.

## 3. 목표 / 비목표

**목표**

1. `my-skills doctor`가 installed CLI의 업데이트 가능 여부를 보여준다.
2. `my-skills update`가 latest stable release tag로 CLI를 명시적으로 업데이트한다.
3. `my-skills update --channel main`으로 개발 브랜치 설치도 가능하게 한다.
4. 네트워크 실패, git 부재, uv 부재는 명확한 메시지로 처리하고 registry 진단을
   망가뜨리지 않는다.
5. update 기능 추가 릴리스는 `0.3.0`/`v0.3.0`으로 배포한다.

**비목표**

- 백그라운드 자동 업데이트
- 명령 실행 중 몰래 self-update 적용
- PyPI 배포 전환
- registry 스킬 동기화(`install`/`sync`)와 CLI 업데이트 통합
- GitHub Releases API 의존. 기본 구현은 Git remote tag 조회로 충분하다.

## 4. UX 계약

### 4.1 `doctor` 출력

`doctor`는 기존 exit code 계약을 유지한다. 업데이트 확인 실패는 진단 정보일 뿐,
manifest가 유효하다면 exit code를 실패로 바꾸지 않는다.

최신 상태:

```text
my-skills 0.3.0
Update:  up to date (stable v0.3.0)
```

업데이트 가능:

```text
my-skills 0.2.0
Update:  available v0.3.0 (run 'my-skills update')
```

main channel 설치본:

```text
my-skills 0.3.0
Update:  installed from main; stable v0.3.0 is current
```

확인 불가:

```text
my-skills 0.2.0
Update:  not checked (git not found)
```

네트워크가 느리거나 막힌 환경을 위해 `doctor --no-update-check`를 제공한다.

```bash
my-skills doctor --no-update-check
```

### 4.2 `update` 명령

기본 업데이트:

```bash
my-skills update
```

동작:

1. remote에서 latest stable release tag를 찾는다.
2. 현재 installed CLI와 비교한다.
3. 이미 최신이면 아무것도 쓰지 않고 0으로 종료한다.
4. 업데이트가 필요하면 `uv tool install --force
   git+https://github.com/Seosiju/my-skills.git@vX.Y.Z`를 실행한다.
5. 완료 후 새 실행본의 `my-skills --version`을 실행해 기대 버전과 맞는지 확인한다.

예상 출력:

```text
Current: my-skills 0.2.0
Latest:  v0.3.0
Updating via uv tool install --force git+https://github.com/Seosiju/my-skills.git@v0.3.0
Updated: my-skills 0.3.0
```

개발 브랜치 업데이트:

```bash
my-skills update --channel main
```

예상 출력:

```text
Current: my-skills 0.3.0
Target:  main (a1b2c3d)
Updating via uv tool install --force git+https://github.com/Seosiju/my-skills.git@main
Updated: my-skills 0.3.0 from main
```

쓰기 없는 확인:

```bash
my-skills update --check
```

Exit code:

| 상황 | exit code |
|---|---:|
| 최신 상태 | 0 |
| 업데이트 가능 | 1 |
| latest ref 확인 불가 | 2 |

쓰기 없는 미리보기:

```bash
my-skills update --dry-run
```

`--dry-run`은 target ref와 실행할 uv 명령만 출력하고 설치하지 않는다. 업데이트 가능
여부 자체는 실패가 아니므로 exit code 0을 반환한다.

## 5. 설계 결정

### D1. 기본 기준은 latest stable release tag

`main`은 개발 브랜치이고 언제든 문서/테스트/미완성 변경이 들어올 수 있다.
일반 사용자의 `my-skills update`는 항상 가장 높은 SemVer tag를 대상으로 한다.

SemVer tag 판별:

- 허용: `v0.2.0`, `v0.3.0`, `v1.0.0`
- 제외: `0.3.0`, `v0.3`, `v0.3.0-rc.1`, `test-tag`

초기 구현은 prerelease를 제외한다. 필요하면 별도 `--channel prerelease`로 확장한다.

### D2. `doctor`는 update check 실패를 soft failure로 다룬다

`doctor`의 주 목적은 로컬 환경과 registry 상태 진단이다. 네트워크가 없다는 이유로
`doctor` 전체가 실패하면 오히려 진단 명령의 신뢰성이 떨어진다.

따라서:

- update check 성공: `Update:` 줄에 상태 출력
- update check 실패: `Update: not checked (...)` 출력
- manifest 진단 exit code는 기존 로직 유지

### D3. 새 의존성을 추가하지 않는다

SemVer 비교는 정규식과 tuple 비교로 충분하다.

```python
SEMVER_TAG_RE = re.compile(r"^refs/tags/v(\d+)\.(\d+)\.(\d+)$")
```

`packaging` 같은 dependency는 추가하지 않는다.

### D4. GitHub API 대신 `git ls-remote`를 사용한다

이유:

- 인증 토큰이 필요 없다.
- rate limit과 JSON 파싱을 피한다.
- tag와 branch ref 조회에 충분하다.
- 사용자가 이미 Git URL 설치 흐름을 쓰고 있어 `git` 의존성이 자연스럽다.

`git`이 없으면 update check는 `not checked (git not found)`로 끝내고,
`update` 명령은 exit code 2로 실패한다.

### D5. 적용은 `uv tool install --force`로 한다

`uv tool upgrade my-skills`는 기존 설치 source/constraint를 존중할 수 있어,
`v0.2.0`으로 설치된 tool을 `v0.3.0`으로 옮기는 의도가 불명확해질 수 있다.
명확한 target ref를 지정하려면 재설치가 낫다.

사용할 명령:

```bash
uv tool install --force git+https://github.com/Seosiju/my-skills.git@v0.3.0
```

main channel:

```bash
uv tool install --force git+https://github.com/Seosiju/my-skills.git@main
```

## 6. 구현 방안

### 6.1 새 모듈: `src/my_skills/update_commands.py`

신규 모듈을 추가해 update 관련 로직을 `inspection_commands.py`에서 분리한다.

권장 구조:

```python
from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass
from importlib import metadata

REPO_URL = "https://github.com/Seosiju/my-skills.git"
SEMVER_TAG_RE = re.compile(r"^refs/tags/v(\d+)\.(\d+)\.(\d+)$")
DEFAULT_TIMEOUT_SECONDS = 3


@dataclass(frozen=True)
class InstallInfo:
    version: str
    source_url: str | None
    requested_revision: str | None
    commit_id: str | None
    executable: str | None


@dataclass(frozen=True)
class RemoteRef:
    name: str
    version: tuple[int, int, int] | None
    commitish: str


@dataclass(frozen=True)
class UpdateStatus:
    current: InstallInfo
    channel: str
    latest: RemoteRef | None
    state: str
    detail: str
```

필수 함수:

| 함수 | 역할 |
|---|---|
| `read_install_info()` | `importlib.metadata`와 `direct_url.json`에서 현재 설치 정보를 읽는다 |
| `latest_stable_ref()` | `git ls-remote --tags --refs <repo> 'v*'` 결과에서 가장 높은 SemVer tag를 고른다 |
| `latest_main_ref()` | `git ls-remote <repo> refs/heads/main`으로 main HEAD를 읽는다 |
| `check_update(channel="stable")` | 현재 설치 정보와 remote ref를 비교해 `UpdateStatus`를 반환한다 |
| `format_update_status(status)` | `doctor`에 넣을 한 줄 메시지를 만든다 |
| `cmd_update(args)` | argparse command handler |

### 6.2 `direct_url.json` 처리

PEP 610 metadata가 있으면 설치 source를 읽는다.

예상 shape:

```json
{
  "url": "https://github.com/Seosiju/my-skills.git",
  "vcs_info": {
    "vcs": "git",
    "requested_revision": "v0.2.0",
    "commit_id": "a5b1c30592d34f6731b8f3b23dfc2211bbe563dd"
  }
}
```

처리 규칙:

- 파일이 없으면 `source_url`, `requested_revision`, `commit_id`는 `None`.
- JSON이 깨져 있으면 update check 실패가 아니라 "source unknown"으로 다룬다.
- installed CLI가 PyPI나 local path에서 온 경우에도 stable tag 기준 업데이트 가능
  여부는 version 비교로 판단한다.

### 6.3 비교 규칙

stable channel:

| 현재 상태 | latest stable | 결과 |
|---|---|---|
| current version < latest version | `v0.3.0` | `available` |
| current version == latest version | `v0.3.0` | `current` |
| current version > latest version | `v0.3.0` | `ahead` |
| current version 파싱 불가 | `v0.3.0` | `unknown-current` |

main channel:

| 현재 requested revision / commit | remote main | 결과 |
|---|---|---|
| commit_id == main commit | same | `current` |
| commit_id != main commit | newer main | `available` |
| commit_id 없음 | main commit 있음 | `unknown-current` |

주의: stable channel의 기본 판단은 version 비교다. release tag 생성 시 반드시
패키지 버전을 올린다는 릴리스 규칙으로 이 비교를 신뢰한다.

### 6.4 `doctor` 연결

`src/my_skills/inspection_commands.py`:

1. `cmd_doctor` 시작부에서 버전 출력 직후 update status를 출력한다.
2. `args.no_update_check`가 true면 `Update:  skipped`를 출력하거나 줄 자체를 생략한다.
3. `check_update()`는 timeout을 짧게 잡고 예외를 `UpdateStatus(state="not-checked")`로
   변환한다.

권장 출력 위치:

```text
my-skills 0.2.0
Update:  available v0.3.0 (run 'my-skills update')
OS:      macOS-...
```

### 6.5 CLI parser 연결

`src/my_skills/cli.py`:

```python
from .update_commands import cmd_update

p_doctor.add_argument(
    "--no-update-check",
    action="store_true",
    help="Skip checking the latest released CLI version",
)

p_update = sub.add_parser("update", help="Update the installed my-skills CLI")
p_update.add_argument(
    "--channel",
    choices=("stable", "main"),
    default="stable",
    help="Update target (default: stable release tag)",
)
p_update.add_argument(
    "--check",
    action="store_true",
    help="Only report whether an update is available",
)
p_update.add_argument(
    "--dry-run",
    action="store_true",
    help="Show the update command without changing the installed CLI",
)
p_update.set_defaults(func=cmd_update)
```

### 6.6 update command verification after install

`cmd_update`는 `uv tool install --force ...` 성공만 믿지 말고 설치 후 관찰 가능한
검증을 수행한다.

stable channel:

```bash
my-skills --version
```

출력의 버전이 target tag와 일치해야 한다.

main channel:

- `my-skills --version`은 main의 package version을 출력한다.
- commit까지 확인하려면 새 installed CLI의 `direct_url.json`을 다시 읽어
  `commit_id`가 remote main commit과 같은지 확인한다.
- commit 확인이 불가능하면 경고를 출력하되, `uv tool install`과 version command가
  성공했으면 exit 0으로 둔다.

## 7. 테스트 계획

네트워크와 실제 `uv tool install`은 테스트에서 실행하지 않는다. `subprocess.run`,
`shutil.which`, metadata reader를 함수 인자로 주입하거나 monkeypatch한다.

### 7.1 새 테스트 파일

권장 파일:

- `tests/test_update_commands.py`
- 기존 doctor 출력 변경은 `tests/test_doctor.py`에 최소 추가

### 7.2 단위 테스트 목록

1. **SemVer tag 선택**
   - 입력: `v0.2.0`, `v0.10.0`, `v0.3.1`, `not-a-release`
   - 기대: `v0.10.0`

2. **prerelease 제외**
   - 입력: `v0.3.0-rc.1`, `v0.2.0`
   - 기대: `v0.2.0`

3. **stable update available**
   - current `0.2.0`, latest `v0.3.0`
   - 기대: state `available`, doctor line에 `run 'my-skills update'`

4. **stable current**
   - current `0.3.0`, latest `v0.3.0`
   - 기대: state `current`

5. **ahead of release**
   - current `0.4.0`, latest `v0.3.0`
   - 기대: state `ahead`, update 명령은 no-op

6. **git missing**
   - `shutil.which("git") -> None`
   - `doctor`: `Update: not checked (git not found)`, exit code 기존 doctor 기준
   - `update`: exit code 2

7. **uv missing**
   - `shutil.which("uv") -> None`
   - `update`: exit code 2, 설치 시도 없음

8. **`update --check` exit code**
   - 최신: 0
   - 업데이트 가능: 1
   - 확인 불가: 2

9. **`update --dry-run`**
   - target ref와 uv 명령 출력
   - subprocess install 호출 없음
   - exit code 0

10. **stable update install command**
    - latest `v0.3.0`
    - 기대 호출:
      `uv tool install --force git+https://github.com/Seosiju/my-skills.git@v0.3.0`

11. **main channel install command**
    - remote main commit `abc123`
    - 기대 호출:
      `uv tool install --force git+https://github.com/Seosiju/my-skills.git@main`

12. **doctor does not fail on update check failure**
    - update check가 timeout/exception을 던져도 manifest valid이면 doctor exit 0

13. **`doctor --no-update-check`**
    - remote 조회 함수 호출 없음

14. **broken `direct_url.json` tolerated**
    - JSON parse 실패
    - current version만으로 stable 비교 가능

15. **post-install verification failure**
    - `uv tool install`은 0이지만 `my-skills --version`이 기대 버전과 다름
    - exit code 1, stderr에 검증 실패 메시지

### 7.3 통합 검증

구현 완료 후 최소 실행:

```bash
uv run pytest tests/test_update_commands.py tests/test_doctor.py -q
uv run pytest -q
python -m compileall -q src tests
uv build
git diff --check
```

수동 QA:

```bash
uv run my-skills doctor --no-update-check
uv run my-skills update --check
uv run my-skills update --dry-run
uv run my-skills update --channel main --dry-run
```

주의: `uv run my-skills update`를 실제 실행하면 현재 개발 환경의 installed CLI를
바꿀 수 있다. 실제 self-update QA는 release tag를 찍은 뒤 installed CLI 대상으로
별도 단계에서 수행한다.

## 8. 문서 업데이트 범위

구현 PR에는 아래 문서 변경을 포함한다.

### 8.1 `README.md` / `README.ko.md`

Quick start 또는 Everyday commands 아래에 "Update the CLI" 섹션 추가.

영문 예시:

````markdown
## Updating the CLI

`my-skills doctor` reports when a newer stable release is available. Updates are
never applied automatically.

```bash
my-skills doctor
my-skills update
```

By default, `update` installs the latest `vMAJOR.MINOR.PATCH` release tag. For
development builds:

```bash
my-skills update --channel main
```
````

한국어 예시:

````markdown
## CLI 업데이트

`my-skills doctor`는 더 최신 안정 릴리스가 있으면 알려줍니다. 업데이트는 자동으로
적용되지 않습니다.

```bash
my-skills doctor
my-skills update
```

기본값은 최신 `vMAJOR.MINOR.PATCH` 릴리스 tag입니다. 개발 브랜치를 따라가려면:

```bash
my-skills update --channel main
```
````

### 8.2 `CHANGELOG.md`

`## [Unreleased]` 아래:

```markdown
### Added

- `doctor` now reports whether the installed CLI is behind the latest stable
  release tag.
- `update` command for explicitly updating the installed CLI to the latest
  stable release, with an opt-in `--channel main` development channel.
```

release commit에서 `0.3.0` 섹션으로 이동한다.

### 8.3 `docs/release-checklist.md`

release checklist에 아래 항목을 추가한다.

```markdown
- After tagging, verify the released CLI can detect and install the tag:
  `my-skills update --check` and `my-skills update --dry-run`.
```

## 9. 릴리스 계획

이 기능은 새 CLI 사용자 기능이므로 patch가 아니라 minor release다.

릴리스 버전:

```text
0.2.0 -> 0.3.0
tag: v0.3.0
```

순서:

1. update 기능 구현 PR merge.
2. `pyproject.toml`, `src/my_skills/__init__.py`, `uv.lock`의 버전을 `0.3.0`으로 올림.
3. `CHANGELOG.md`의 Unreleased 항목을 `## [0.3.0] - YYYY-MM-DD` 아래로 이동.
4. 전체 테스트와 build 실행.
5. release commit 생성.
6. annotated tag 생성:

```bash
git tag -a v0.3.0 -m "Release v0.3.0"
```

7. main과 tag push.
8. 기존 설치본에서 확인:

```bash
my-skills doctor
my-skills update --check
my-skills update
my-skills --version
```

기대 결과:

```text
my-skills 0.3.0
```

## 10. 실패 모드와 메시지

| 상황 | `doctor` | `update` |
|---|---|---|
| offline | `Update: not checked (network unavailable)` | exit 2, "could not reach release tags" |
| git 없음 | `Update: not checked (git not found)` | exit 2 |
| uv 없음 | 영향 없음 | exit 2, "uv not found; install uv first" |
| SemVer tag 없음 | `Update: not checked (no stable release tags found)` | exit 2 |
| 이미 최신 | `Update: up to date (...)` | "Already up to date", exit 0 |
| 설치 실패 | 영향 없음 | uv stderr 요약 출력, exit 1 |
| 설치 후 버전 불일치 | 영향 없음 | "updated command did not report expected version", exit 1 |

stderr/stdout 원칙:

- `doctor`의 사람이 읽는 진단은 기존처럼 stdout.
- `update`의 정상 진행 메시지는 stdout.
- 실패 이유와 subprocess stderr 요약은 stderr.
- JSON 출력 옵션은 이번 범위에 추가하지 않는다.

## 11. 구현 순서

권장 commit 단위:

1. **update status core**
   - `update_commands.py`에 install info, remote ref 조회, SemVer 비교 추가
   - 단위 테스트 추가

2. **doctor integration**
   - `doctor` update line 추가
   - `doctor --no-update-check` 추가
   - 기존 doctor 테스트 보강

3. **update command**
   - `my-skills update`, `--check`, `--dry-run`, `--channel main` 추가
   - subprocess install과 post-install verification 테스트 추가

4. **docs**
   - README/README.ko/CHANGELOG/release checklist 업데이트

5. **release**
   - 구현 merge 후 별도 release commit으로 `0.3.0` bump 및 `v0.3.0` tag

## 12. 완료 기준

- `my-skills doctor`가 업데이트 상태를 보여준다.
- `my-skills doctor --no-update-check`가 remote 조회 없이 동작한다.
- `my-skills update --check`가 업데이트 가능 여부를 exit code로 표현한다.
- `my-skills update --dry-run`이 실제 설치 없이 target uv 명령을 보여준다.
- `my-skills update`가 stable tag로 설치하고 설치 후 버전을 검증한다.
- `my-skills update --channel main --dry-run`이 main target을 보여준다.
- 네트워크/git/uv 실패가 traceback 없이 설명 가능한 메시지로 끝난다.
- 전체 테스트, compileall, build, diff check가 통과한다.
- 기능 release는 `0.3.0`/`v0.3.0`으로 배포된다.
