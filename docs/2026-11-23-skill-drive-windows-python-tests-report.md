# Windows PC 점검 보고: 개발 환경과 테스트 격리

> 작성: 2026-11-23, Windows PC의 Claude Code 세션. 개정: 같은 날 Codex 독립 검토(`aa65f37` 대상)를 반영
> 대상: macOS 쪽 오케스트레이터. 이 문서를 보고 다음 지침을 정할 수 있게 썼다.
> 기준 커밋: `681a701` (`dev`)
> 관련 문서: runbook `docs/2026-11-23-skill-drive-windows-runbook.md` / 인계 문서 `docs/2026-11-23-skill-drive-v1-status-and-handoff.md` §6.5, §7 L

**근거 표시.** 각 주장에 다음 중 하나를 붙였다.
- **[확정]**: 남아 있는 원자료(JUnit XML, 분류 로그, 전후 목록 파일)나 소스로 확인한 것
- **[관찰]**: 작성자가 당시 터미널에서 본 것. 로그 파일은 남기지 않았다
- **[추론]**: 코드를 읽고 판단한 것
- **[미확인]**: 근거가 부족한 것

## 0. 요약

1. **Rust 개발 환경은 준비됐다.** [관찰] runbook "준비" 절과 2단계의 테스트까지 통과했다.
   - `cargo fmt --check`, `clippy -D warnings`, `cargo test --all-targets`(206개 통과), Windows junction 테스트 4개가 모두 통과했다.
   - `cargo build --release`와 에이전트의 junction 발견 확인(3단계)은 아직 남았다.
2. **이 체크아웃(`681a701`)의 Python 0.4.x 테스트를 Windows에서 격리 없이 실행하면 실제 사용자 위치에 쓴다.**
   - [관찰] 시험용 스킬 8개가 실제 host 폴더에 생겼다.
   - [확정] `USERPROFILE`과 `HOME`을 임시 폴더로 돌린 재실행에서는 315건 중 102건이 실패하고 213건이 통과했다(CPython 3.15.2). 끝나지 않는 1건은 의도적으로 뺐다.
   - [확정] 그 재실행으로도 데이터 경로(`LOCALAPPDATA`)는 격리되지 않았다(§3.2).
3. **Rust 테스트 1건도 실제 `%LOCALAPPDATA%`에 썼다.** [관찰] `%LOCALAPPDATA%\my-skills\data\alpha`(빈 폴더)가 생겼다. [추론] `cli_connect.rs`의 `data-path` 테스트가 만든 것이다(§5).
4. [관찰] 확인한 범위(§3.1)에서는 실제 레지스트리, `root`, `state.json`에 변화가 없었다.
   - host 폴더에 생긴 시험용 폴더는 사용자 승인을 받아 지웠다.
   - `%LOCALAPPDATA%\my-skills\data\alpha`는 아직 지우지 않았다.
5. runbook 3단계 이후(격리 확인, 실제 연결)는 **아직 하지 않았다.**

**오케스트레이터가 정할 것** (자세한 내용은 §6):
- D1. Python 테스트의 Windows 문제를 고칠지, §7 D(Python 제거)까지 기록만 하고 둘지
- D2. Rust 테스트 환경 격리를 통일할지
- D3. runbook 3단계를 지금 이 PC에서 진행할지
- D4. Windows에서 audit 보고서의 경로 표기를 통일할지

---

## 1. 환경

| 항목 | 값 |
|---|---|
| OS | Windows 11 Home (10.0.26300) |
| Git | 2.56.1.windows.1, `core.autocrlf=true` |
| MSVC 빌드 도구 | Visual Studio Build Tools 2026 (VC Tools 포함). runbook은 2022판을 설치하라고 했지만, 이미 있던 2026판으로 링크가 됐다 |
| Rust | rustc/cargo 1.100.0, `stable-x86_64-pc-windows-msvc`, rustfmt 1.11.0, clippy 0.1.100 |
| winget | v1.29.380. 새로 설치한 것이 없어서 쓰지 않았다 |
| uv, Python | uv 설치됨. 저장소 `.venv`는 CPython 3.15.2 (Ubuntu CI는 3.11) |
| 에이전트 | `claude`, `codex`, `agy` 모두 PATH에 있음 |
| 기존 Python `my-skills` | 0.4.1 (`%USERPROFILE%\.local\bin\my-skills`) |
| 기존 연결 기록 | `%USERPROFILE%\.local\state\my-skills\state.json` 있음 (스펙 Q3 판단 자료) |
| 활성 레지스트리 기록 | `%USERPROFILE%\.config\my-skills\root` 있음. 기본 위치 `%USERPROFILE%\my-agent-skills`를 가리킨다 |
| host 폴더 | 세 곳 모두 있다. 손으로 넣은 폴더와 다른 도구가 만든 symlink가 섞여 있어서, 4단계 dry-run에서 `BLOCK_CONFLICT`가 나올 수 있다 |

참고: Rust를 설치하기 전에 시작한 터미널 세션은 PATH에 `%USERPROFILE%\.cargo\bin`이 없다. 사용자 PATH에는 들어 있으므로 새 터미널에서는 문제가 없다.

## 2. Rust 결과 (runbook 준비 절, 2단계)

모두 [관찰]이다. 출력은 당시 터미널에서 확인했고 파일로 남기지 않았다.

| 검사 | 결과 |
|---|---|
| 링커 확인 (`cargo new` → `cargo run`) | `Hello, world!` |
| `cargo fmt --all --check` | 통과 |
| `cargo clippy --all-targets -- -D warnings` | 통과, 경고 없음 |
| `cargo test` / `cargo test --all-targets` | 둘 다 206개 통과, 0개 실패 |
| `cargo test --lib drive::link::tests::windows` | 4개 통과 (`create_link_prefers_junction`, `junction_removal_keeps_canonical`, `junction_target_reads_back`, `case_and_verbatim_prefix_are_ignored`) |

- [관찰] `cargo test --all-targets` 실행 전후로 실제 host 폴더 세 곳의 이름 목록은 같았다. host 폴더 밖은 그때 확인하지 않았다. 나중에 `%LOCALAPPDATA%`에 쓴 것을 찾았다(§5).
- 인계 문서의 "Rust 209개"와의 차이:
  - [관찰] 첫 `cargo test` 출력에서 doctest는 0개였다. 그러니 doctest 때문은 아니다.
  - [추론] Windows에서는 `cfg(unix)` 통합 테스트 4개(`audit_parity` 1, `hash_golden` 2, `hooks_git` 1)가 빠진다. link 테스트는 unix 3개 대신 windows 4개가 들어간다. 그러면 206 − 4 + 3 + 4 = 209가 된다.
  - [미확인] macOS의 실제 테스트 목록과 비교하지 않았으므로 원인은 확정하지 않았다.
- `cargo build --release`는 아직 실행하지 않았다.

## 3. 사고: 테스트가 실제 사용자 위치에 썼다

### 3.1 host 폴더에 생긴 것과 확인한 범위

[관찰] 격리 없이 `uv run pytest`를 여러 번 실행했다. 일부는 시간 제한으로 중단됐다. 그 뒤 host 폴더 이름 목록에 아래 폴더가 있었고, 폴더 수정 시각이 모두 같았다. 어느 실행이 만들었는지는 구분하지 못했다.

| 위치 | 생긴 폴더 | 출처 |
|---|---|---|
| `%USERPROFILE%\.claude\skills` | `alpha`, `cli-inventory`, `my-skills` | `alpha`는 `tests/test_integration.py`의 시험용 스킬이다. 나머지 둘은 저장소 `skills/`의 seed와 내용이 같았다(`diff -rq`) |
| `%USERPROFILE%\.agents\skills` | `cli-inventory`, `my-skills`, `personal-profile` | seed와 `tests/test_seed_e2e.py`의 legacy 시험용 스킬 |
| `%USERPROFILE%\.hermes\skills` | `cli-inventory`, `my-skills` | seed. `.hermes` 폴더 자체가 새로 생겼다 |

- [관찰] 실행 중이던 에이전트 세션의 스킬 목록에 `alpha`("Canonical skill for the integration test.")가 나타나서 알아챘다. host 폴더에 쓰면 실행 중인 에이전트에도 바로 보인다.

확인한 범위 [관찰]:
- host 폴더 세 곳과 `.hermes`의 최상위 이름 목록(`ls -A`). 기존 항목의 내용과 메타데이터는 비교하지 않았다.
- `root`, `state.json`의 수정 시각. 11월 11일, 13일 그대로였다. 내용은 비교하지 않았다. [추론] conftest가 `XDG_CONFIG_HOME`과 `XDG_STATE_HOME`을 임시 폴더로 돌리므로 쓰이지 않았을 것이다.
- 실제 레지스트리:
  - `git status`가 깨끗했다.
  - `.git` 밖에서 그날 10:00 이후 바뀐 파일이 없었다(`find -newermt`).
  - `.git/hooks`에 sample이 아닌 hook이 없었다.
  - `.git` 폴더 자체의 수정 시각은 10:00 이후였다. 직전에 실행한 `git status` 때문이라고 판단했지만 확인하지는 않았다.
- 정리한 뒤: 위 이름 목록이 처음 점검했을 때 목록과 같았고 `.hermes`가 없었다.
- `%LOCALAPPDATA%\my-skills`: 나중에 사용자 승인을 받아 읽기만 했다(§5).

확인하지 않은 범위 [미확인]: 그 밖의 홈 아래 위치.

### 3.2 원인 (두 가지)

**원인 1. 홈 (`HOME`)**
- [확정] 같은 `.venv`의 CPython 3.15.2를 별도 프로세스로 실행해 확인했다. `HOME`을 바꿔도 `os.path.expanduser("~")`와 `Path.home()`은 `USERPROFILE`을 돌려준다.
- 테스트는 `HOME`만 바꾼다:
  - `tests/test_integration.py`: `monkeypatch.setenv("HOME", ...)`
  - `tests/test_seed_e2e.py`: subprocess `env["HOME"]`
  - `tests/test_init_registry.py`(5곳), `tests/test_data.py`, `tests/test_state.py`
- `tests/` 어디에서도 `USERPROFILE`을 바꾸지 않는다.
- host 폴더에 쓰인 것은 이 때문이다. [추론] host 경로 `~/.claude/skills` 등은 `src/my_skills/config.py:82`의 `expanduser()`를 거친다.
- 이 밖에도 `~`가 쓰이는 곳이 있다: `init_registry_commands.py:128,130`(기본 레지스트리 `~/my-agent-skills`), `cli_runtime.py:25`(XDG가 없을 때의 기본값).

**원인 2. 데이터 (`LOCALAPPDATA`)**
- [확정: 소스] `src/my_skills/data.py:34-37`의 Windows 분기는 `XDG_DATA_HOME`보다 먼저 `LOCALAPPDATA`를 읽는다. conftest는 `XDG_DATA_HOME`만 바꾼다.
- [확정: JUnit] `USERPROFILE`과 `HOME`을 돌린 재실행에서 `tests/test_isolation.py::test_default_user_paths_are_isolated_under_pytest_tmp`는 9줄(state)과 10줄(root) 검사를 통과했다. 11줄 `data_root()`에서 실패했고, 그때 값은 실제 `%LOCALAPPDATA%\my-skills\data`였다.
- [확정] CI의 Python 작업(`ci.yml`의 `test-build-smoke`)은 `ubuntu-latest`, Python 3.11에서만 돈다. 그래서 두 원인 모두 드러나지 않았다.

### 3.3 더 나쁠 수 있었던 경우 [추론, 실행하지 않음]

이 PC에는 기본 위치에 실제 레지스트리가 있었다. 레지스트리가 없는 Windows PC라면 다음 일이 생길 수 있다.
- `init-registry`의 기본 위치를 쓰는 테스트가 실제 `%USERPROFILE%\my-agent-skills`에 레지스트리를 만들 수 있다.
- 그러면 다음에 `my-skills`를 실행할 때 그 레지스트리를 찾게 된다.

## 4. Python 테스트 실패 상세

[확정] 재실행 조건과 결과:
- `USERPROFILE`과 `HOME`을 임시 폴더로 돌렸다. `LOCALAPPDATA`는 돌리지 않았다.
- 실행 전후 host 폴더 이름 목록이 같았다(`b3.txt`, `a3.txt`).
- 수치는 Windows, CPython 3.15.2 기준이다.
- 결과: JUnit 기준 315건 중 102건 실패, 213건 통과, 오류 0건

```text
102 failed, 213 passed, 1 deselected
```

`1 deselected`: 앞선 실행에서 끝나지 않은 `test_init_registry_prompt_accepts_empty_default`(§4 E)를 **의도적으로 `--deselect`한 것**이다. 실패 수에는 들어가지 않는다.

### 4.1 로그 분류 (서로 겹치지 않음)

[확정] 분류 방법:
- JUnit XML(`-o junit_logging=all`)에서 테스트별 실패 메시지, traceback, stderr를 읽었다.
- 규칙을 A → B → C 순서로 적용해 처음 맞는 하나로 분류했다. 어디에도 맞지 않으면 D로 넣었다.
- 규칙: A는 `invalid TOML`이 있는 것, B는 `1314`가 있는 것, C는 POSIX 경로 패턴(`/tmp`, `'scripts/`, `.gemini/config/skills`, `.local/share|state`, `parents>`)이 있는 것이다.

| 분류 | 건수 |
|---|---|
| A. TOML 문자열 이스케이프 | 61 |
| B. symlink 권한 (`WinError 1314`) | 12 |
| C. POSIX 경로 가정 | 9 |
| D. 그 밖 | 20 |
| 합계 | 102 |

### 4.2 분류별 내용

**A. TOML 문자열 이스케이프 (61건)** [확정]
- 테스트가 `tmp_path`(Windows에서는 `C:\Users\...`)를 TOML 큰따옴표 문자열에 그대로 넣는다. 그러면 `\U`가 유니코드 이스케이프로 해석되어 manifest를 읽지 못한다.

  ```text
  error: invalid TOML in ...\my-skills.toml: Invalid hex value (at line 5, column 13)
  ```

- 예: `tests/test_agy.py:12,18`의 `path = "{tmp_path / ...}"`
- CLI가 종료 코드 2로 끝나므로 `assert 2 == 0`, `assert 2 == 1`로 나타난다.
- `test_doctor.py::test_doctor_agy_detection_and_default_activation[True-True]`와 `[True-False]`도 여기에 들어 있다.
- 고치는 방법 [추론]: TOML 작은따옴표 문자열(`'...'`)을 쓰거나, 경로를 `as_posix()`로 넣는다. runbook 3.5의 `my-skills.local.toml` 예시는 이미 작은따옴표를 쓴다.

**B. symlink 권한 (12건)** [확정]

```text
OSError: cannot create a symlink at ... [WinError 1314]
```

- `tests/test_link.py` 10건: Python 0.4.x의 link 모드는 `os.symlink`를 쓴다. 개발자 모드나 관리자 권한이 없으면 Windows가 막는다. 0.4.x 사용자가 Windows에서 `--mode link`를 써도 같은 오류가 날 것이다 [추론]. Rust v1은 junction을 먼저 써서 이 문제를 피한다(`drive::link`).
- `tests/test_cli_import.py` 2건: 시험용 symlink를 만들지 못한 테스트 환경 문제다.

**C. POSIX 경로 가정 (9건)** [확정: 분류]

| 테스트 | 내용 |
|---|---|
| `test_audit_static.py::test_static_audit_reports_invalid_utf8_regular_files`, `::test_audit_still_detects_nul_bytes_in_regular_files` | audit 결과의 상대 경로가 `scripts\bad...`로 나온다. 테스트는 `scripts/...`를 기대한다. §4.4 참고 |
| `test_config.py::test_cli_override_beats_local`, `::test_agy_omitted_preserves_existing_default_selection` | `/tmp/...`가 `C:/tmp/...`로 풀리거나, POSIX 경로를 비교한다 |
| `test_data.py::test_data_root_posix_fallback`, `test_state.py::test_default_state_path_fallback` | POSIX 기본값을 기대하는데 Windows 분기가 먼저 걸린다 |
| `test_doctor.py::test_doctor_agy_detection_and_default_activation[False-True]`, `[False-False]` | 출력에 `.gemini/config/skills`가 있기를 기대하는데, 실제로는 `\` 경로가 나온다 |
| `test_isolation.py::test_default_user_paths_are_isolated_under_pytest_tmp` | 실패 지점은 `data_root()`(`LOCALAPPDATA`)다. §3.2의 원인 2 |

**D. 그 밖 (20건)**: 증상만 [확정]이고, 원인 판단은 §4.3에 둔다.

| 테스트 | 증상 |
|---|---|
| `test_audit`(2), `test_characterization`(3), `test_cli_bootstrap`(1), `test_share`(2), `test_sync::test_sync_check_stale_nonzero_writes_nothing` | 종료 코드 2, 빈 stdout에 대한 `JSONDecodeError`, 또는 `column 13` |
| `test_sync`의 conflict, drifted, update_failure (3) | `hosts\claude\alpha\SKILL.md` 없음 |
| `test_config.py::test_expand_path_is_absolute`, `::test_local_override_precedence` | `endswith('/.claude/skills')` 실패 |
| `test_doctor.py::test_doctor_reports_registry_source_version_state_and_data` | `Data:` 줄이 기대와 다름 |
| `test_cli_catalog.py::test_skills_json_filters_by_host_and_enabled_state` | JSON 일부 값이 다름 |
| `test_init_registry.py::test_init_registry_without_path_uses_default_location_when_not_tty` | 기본 위치가 임시 `HOME` 기준이 아님 |
| `test_integration.py::test_install_then_repeat_is_noop`, `test_seed_e2e.py`(2) | 임시 `HOME`에 설치 결과가 없음 |

### 4.3 D의 원인 재판정 (추정)

모두 [추론]이며 분류 표의 건수는 바꾸지 않는다.

| 대상 | 판단 |
|---|---|
| 종료 코드 2, 빈 stdout, `column 13` (9건) | A와 같은 TOML 오류로 보인다. capsys가 stderr를 가져가서 JUnit stderr에 남지 않은 것으로 보인다 |
| `test_sync`의 3건 | A 때문에 앞 단계 설치가 실패한 2차 실패로 보인다 |
| `test_config`의 2건 | C와 같은 구분자 문제 |
| `test_init_registry`, `test_integration`, `test_seed_e2e` (4건) | 원인 1(홈). 재실행에서는 임시 `USERPROFILE` 쪽에 설치됐다. `test_integration`과 `test_seed_e2e`의 시험용 스킬 이름은 §3.1에서 실제 host 폴더에 생긴 것과 같다 |
| `test_doctor`의 `Data:` 줄, `test_cli_catalog` | [미확인] |

### 4.4 audit 출력과 해시 입력은 다르다

- **해시 입력** [확정: 소스]: 두 판 모두 이미 `/`로 정규화한다.
  - Python: `src/my_skills/hashing.py:41,44`의 `as_posix()`
  - Rust: `crates/my-skills/src/drive/hash.rs:74-78`에서 `MAIN_SEPARATOR`를 `/`로 바꾼다
  - [추론] 기기 사이의 hash 비교에는 영향이 없다.
- **audit 출력**:
  - [확정] Python은 Windows에서 `scripts\...`로 출력한다(C의 `test_audit_static` 2건).
  - [추론] Rust는 `crates/my-skills/src/check/audit/analyzers.rs:33`이 `relative.to_string_lossy()`를 쓰므로 Windows에서 `\`로 나올 수 있다.
  - [미확인] Rust의 실기기 결과는 없다.

### 4.5 E. 끝나지 않는 테스트 (1건)

`tests/test_init_registry.py::test_init_registry_prompt_accepts_empty_default`

- [관찰] 이 테스트가 시작된 뒤 출력이 멈췄고, 실행은 시간 제한(4분, 15분)으로 끝났다. `-v` 실행에서 마지막으로 출력된 테스트가 이것이었다.
- [확정: 소스] 테스트는 `HOME`을 임시 폴더로 바꾸고, `isatty`를 True로 바꾸고, `input`이 항상 `""`를 돌려주게 한다. `_prompt_registry_target()`(`init_registry_commands.py:134-143`)은 대상에 `my-skills.toml`이 있으면 메시지를 출력하고 다시 묻는다.
- [추론] `~`가 `HOME`을 무시하고 실제 홈으로 풀린다. 이 PC의 활성 레지스트리 기록이 기본 위치를 가리키므로, 그 위치에 `my-skills.toml`이 있어서 루프가 끝나지 않은 것으로 보인다. 그 파일이 있는지는 직접 확인하지 않았다.

## 5. Rust 쪽에 주는 의미

**실제 `%LOCALAPPDATA%`에 쓴 테스트**
- [관찰] 사용자 승인을 받아 읽기만 했다. `%LOCALAPPDATA%\my-skills`에는 다음 두 빈 폴더만 있었고, 둘 다 2026-11-23 09:58:22에 만들어졌다.

  ```text
  my-skills\data
  my-skills\data\alpha
  ```

- [추론] 만든 것은 `crates/my-skills/tests/cli_connect.rs::data_path_uses_xdg_data_home_and_rejects_bad_names`다.
  - 이 테스트는 `data-path alpha --create`를 실행한다.
  - 이 테스트 환경(`cli_connect.rs:70-74`)은 `HOME`과 `XDG_*`만 바꾼다.
  - Windows의 `data_root()`(`src/platform/paths.rs:41-45`)는 `LOCALAPPDATA`를 읽는다.
  - 위치 검사는 `#[cfg(not(windows))]`로 Windows에서 건너뛴다.
- 시각 근거 [관찰]: 저장소 `.venv`는 첫 `uv run pytest` 때 만들어졌다(`pyvenv.cfg` 09:58:49). 그 직전에 실행한 것은 첫 `cargo test`뿐이었다. Python `test_data.py`는 `data_root()`를 직접 부를 뿐 `--create`로 `alpha`를 만들지 않는다.
- 이 폴더는 아직 지우지 않았다.
- [추론] Windows CI에서도 같은 테스트가 러너의 `LOCALAPPDATA`에 쓴다.

**Rust 테스트 환경의 격리 수준** [확정: 소스]

| 파일 | 지정하는 변수 |
|---|---|
| `support_g.rs`, `support_h.rs`, `support_i.rs` | `HOME`, `USERPROFILE`, `LOCALAPPDATA`, `XDG_*` 모두 |
| `cli_connect.rs:70-74`, `dry_run.rs:52-58` | `HOME`과 `XDG_*`만. `USERPROFILE`과 `LOCALAPPDATA`는 없음 |
| `manifest_state_data.rs` | `USERPROFILE` 없음 |

**`HOME` 우선순위와의 관계** [추론]
- Rust `home_dir()`(`paths.rs:10-19`)는 `HOME`을 `USERPROFILE`보다 먼저 읽는다. 그래서 지금은 `HOME`만 바꾸는 테스트도 홈 경로만큼은 격리된다.
- 인계 문서 §7 L (4)에서 Windows에서 `USERPROFILE`을 먼저 쓰기로 바꾸면, `cli_connect.rs`와 `dry_run.rs`는 Python과 같은 방식으로 실제 홈에 쓰게 된다. (4)를 결정할 때 테스트 환경도 함께 바꿔야 한다.

## 6. 결정할 것과 권장

| ID | 결정할 것 | 권장 |
|---|---|---|
| D1 | Python 테스트의 Windows 문제를 고칠지 | **기록만 하고 고치지 않는 것을 권한다.** §7 D에서 `dev`의 Python 코드를 지울 예정이고, 0.4.x에는 Windows CI가 없었다. 0.4.x 패치를 낸다면 conftest에서 최소한 `USERPROFILE`과 `LOCALAPPDATA`를 함께 임시 폴더로 돌린다. [근거 부족] 그것만으로 실제 위치에 쓰는 일이 모두 사라지는지는 실행해서 확인하지 않았다. 예를 들어 subprocess를 쓰는 `test_seed_e2e.py`는 env를 따로 만든다 |
| D2 | Rust 테스트 환경 격리를 통일할지 | 모든 CLI 테스트 환경을 support_g/h/i 수준(`HOME`, `USERPROFILE`, `LOCALAPPDATA`, `XDG_*`)으로 통일한다. `data_path_uses_xdg_data_home_and_rejects_bad_names`에 Windows 위치 검사를 넣는다. Windows CI에 "테스트 전후로 사용자 위치(host 폴더, `%LOCALAPPDATA%\my-skills`)가 같은지" 비교하는 단계를 넣는다. §7 L (4)와 함께 다룬다 |
| D3 | runbook 3단계를 이 PC에서 진행할지 | 진행해도 된다. 3단계는 Rust binary를 쓰고 격리 변수(`MY_SKILLS_ROOT`, `XDG_*`)를 쓴다. 단, host 폴더는 일부러 실제 위치를 쓰는 단계이므로 runbook 순서를 지킨다. `%LOCALAPPDATA%\my-skills\data\alpha`를 먼저 지울지도 함께 정한다 |
| D4 | Windows에서 audit 보고서의 경로 표기 | Python과 Rust의 audit 보고서 파일 경로를 `/`로 통일할지, 회귀 테스트를 추가할지 정한다. 해시 입력은 두 판 모두 이미 `/`이므로 이 결정의 대상이 아니다 |

## 7. 다시 확인하는 방법 (사용자 위치를 건드리지 않게)

**`uv run pytest`를 Windows에서 그냥 실행하지 않는다.** 꼭 돌려야 하면 Git Bash에서 다음처럼 실행한다.

> **코드 검토 기반 수정안이다. 이 형태 그대로 실행해서 검증하지는 않았다.** 처음 실행할 때 전후 비교 결과와 pytest 종료 코드를 함께 기록한다.

```bash
W="$(mktemp -d)"; mkdir -p "$W/home" "$W/localappdata"
# 실제 사용자 위치의 이름 목록. HOME을 바꾸기 전, 바깥 셸에서 실행한다.
snap() { for d in .claude/skills .agents/skills .gemini/config/skills .hermes \
                  AppData/Local/my-skills; do echo "[$d]"; ls -AR "$HOME/$d" 2>&1; done; }
snap > "$W/before.txt"
USERPROFILE="$(cygpath -w "$W/home")" HOME="$W/home" LOCALAPPDATA="$(cygpath -w "$W/localappdata")" \
  uv run pytest -p no:cacheprovider -o junit_logging=all --junitxml="$(cygpath -w "$W/junit.xml")" \
  --deselect tests/test_init_registry.py::test_init_registry_prompt_accepts_empty_default \
  > "$W/pytest.txt" 2>&1
echo $? > "$W/pytest-exit.txt"
snap > "$W/after.txt"
diff "$W/before.txt" "$W/after.txt" && echo "user locations unchanged"
echo "pytest exit: $(cat "$W/pytest-exit.txt")   logs: $W"
```
