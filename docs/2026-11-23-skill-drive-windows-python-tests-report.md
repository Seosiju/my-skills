# Windows PC 점검 보고: 개발 환경과 Python 0.4.x 테스트

> 작성: 2026-11-23, Windows PC의 Claude Code 세션
> 대상: macOS 쪽 오케스트레이터. 이 문서를 보고 다음 지침을 정할 수 있게 썼다.
> 기준 커밋: `681a701` (`dev`)
> 관련 문서: runbook `docs/2026-11-23-skill-drive-windows-runbook.md` / 인계 문서 `docs/2026-11-23-skill-drive-v1-status-and-handoff.md` §6.5, §7 L

## 0. 요약

1. **Rust 개발 환경은 준비됐다.** runbook "준비" 절과 2단계의 테스트까지 통과했다. `cargo fmt --check`, `clippy -D warnings`, `cargo test --all-targets`(206개 통과)가 모두 통과했고, Windows junction 테스트 4개가 실기기에서 처음 실행돼 통과했다.
2. **Python 0.4.x 테스트(`uv run pytest`)는 Windows에서 돌리면 안 된다.**
   - 102개가 실패하고 213개가 통과하며, 1개는 끝나지 않는다.
   - **테스트가 실제 사용자 host 폴더에 파일을 썼다.** 시험용 스킬 8개가 실제 host 폴더에 생겼다.
   - 원인은 테스트의 격리 방식이다. Windows의 Python은 `HOME`을 무시한다.
3. 실제 레지스트리, 활성 레지스트리 기록(`root`), `state.json`은 바뀌지 않았다. 생긴 폴더는 사용자 승인을 받아 지웠고, 처음 상태로 돌아간 것을 확인했다.
4. runbook 3단계 이후(격리 확인, 실제 연결)는 **아직 하지 않았다.**

**오케스트레이터가 정할 것** (자세한 내용은 §6):
- D1. Python 테스트의 Windows 문제를 고칠지, §7 D(Python 제거)까지 기록만 하고 둘지
- D2. Rust 테스트에 같은 종류의 위험이 있는지 감사할지(§5)
- D3. runbook 3단계를 지금 이 PC에서 진행할지

---

## 1. 환경

| 항목 | 값 |
|---|---|
| OS | Windows 11 Home (10.0.26300) |
| Git | 2.56.1.windows.1, `core.autocrlf=true` |
| MSVC 빌드 도구 | Visual Studio Build Tools 2026 (VC Tools 포함). runbook은 2022판을 설치하라고 했지만 이미 있던 2026판으로 문제없이 링크됐다 |
| Rust | rustc/cargo 1.100.0, `stable-x86_64-pc-windows-msvc`, rustfmt 1.11.0, clippy 0.1.100 |
| winget | v1.29.380 (새로 설치한 것이 없어서 쓰지 않음) |
| uv, Python | uv 설치됨. 저장소 `.venv`는 CPython 3.15 |
| 에이전트 | `claude`, `codex`, `agy` 모두 PATH에 있음 |
| 기존 Python `my-skills` | 0.4.1 (`%USERPROFILE%\.local\bin\my-skills`) |
| 기존 연결 기록 | `%USERPROFILE%\.local\state\my-skills\state.json` 있음 (스펙 Q3 판단 자료) |
| 활성 레지스트리 기록 | `%USERPROFILE%\.config\my-skills\root` 있음, 기본 위치 `%USERPROFILE%\my-agent-skills`를 가리킴 |
| host 폴더 | 세 곳 모두 있음. 손으로 넣은 폴더와 다른 도구가 만든 symlink가 섞여 있다. 4단계 dry-run에서 `BLOCK_CONFLICT`가 나올 수 있다 |

참고: Rust를 설치하기 전에 시작한 터미널 세션은 PATH에 `%USERPROFILE%\.cargo\bin`이 없다. 사용자 PATH에는 들어 있으므로 새 터미널에서는 문제가 없다.

## 2. Rust 결과 (runbook 준비 절, 2단계)

| 검사 | 결과 |
|---|---|
| 링커 확인 (`cargo new` → `cargo run`) | `Hello, world!` |
| `cargo fmt --all --check` | 통과 |
| `cargo clippy --all-targets -- -D warnings` | 통과, 경고 없음 |
| `cargo test --all-targets` | 206개 통과, 0개 실패 |
| `cargo test --lib drive::link::tests::windows` | 4개 통과 (`create_link_prefers_junction`, `junction_removal_keeps_canonical`, `junction_target_reads_back`, `case_and_verbatim_prefix_are_ignored`) |

- `cargo test --all-targets` 실행 전후로 실제 host 폴더에는 변화가 없었다.
- 인계 문서의 "Rust 209개"와 3개 차이가 난다. `--all-targets`는 doctest를 실행하지 않으므로 그 차이로 보이지만, **확인하지는 않았다.**
- `cargo build --release`는 아직 실행하지 않았다.

## 3. 사고: Python 테스트가 실제 host 폴더에 썼다

### 3.1 무엇이 생겼나

`uv run pytest`를 그냥 실행했더니 아래 폴더가 실제 사용자 홈에 생겼다. 수정 시각은 모두 마지막 전체 실행 시각이었다.

| 위치 | 생긴 폴더 | 출처 |
|---|---|---|
| `%USERPROFILE%\.claude\skills` | `alpha`, `cli-inventory`, `my-skills` | `alpha`는 `tests/test_integration.py`의 시험용 스킬. 나머지 둘은 저장소 `skills/`의 seed와 내용이 같다 |
| `%USERPROFILE%\.agents\skills` | `cli-inventory`, `my-skills`, `personal-profile` | seed, 그리고 `tests/test_seed_e2e.py`의 legacy 시험용 스킬 |
| `%USERPROFILE%\.hermes\skills` | `cli-inventory`, `my-skills` | seed. **`.hermes` 폴더 자체가 새로 생겼다** |

- 실행 중이던 에이전트 세션의 스킬 목록에 `alpha`("Canonical skill for the integration test.")가 나타나서 알아챘다. host 폴더에 쓰면 실행 중인 에이전트에 바로 보인다는 뜻이다.
- 기존 폴더, `root`, `state.json`은 바뀌지 않았다. `root`와 `state.json`은 수정 시각이 11월 11일, 13일 그대로다. conftest가 `XDG_CONFIG_HOME`, `XDG_STATE_HOME`을 임시 폴더로 돌리기 때문이다.
- 실제 레지스트리(`%USERPROFILE%\my-agent-skills`)도 바뀌지 않았다. `git status`가 깨끗하고, 그날 바뀐 파일이 없으며, hook도 설치되지 않았다.
- 정리: 사용자 승인을 받아 위 8개 폴더와 `.hermes`를 지웠고, 처음 점검했을 때의 목록과 같은 것을 확인했다.

### 3.2 원인

테스트는 `HOME`만 임시 폴더로 바꾸는데, Windows의 Python은 `HOME`을 무시하고 `USERPROFILE`을 쓴다. 같은 `.venv`에서 직접 확인했다.

```text
>>> os.environ["HOME"] = r"C:\tmp\fakehome"
>>> os.path.expanduser("~"), pathlib.Path.home()
('%USERPROFILE% 값', '%USERPROFILE% 값')       # HOME이 반영되지 않음
```

- `HOME`을 바꾸는 테스트: `tests/test_integration.py`(`monkeypatch.setenv("HOME", ...)`), `tests/test_seed_e2e.py`(subprocess `env["HOME"]`), `tests/test_init_registry.py`(5곳), `tests/test_data.py`, `tests/test_state.py`
- `tests/` 어디에서도 `USERPROFILE`을 바꾸지 않는다.
- 제품 코드에서 `~`가 실제 홈으로 풀리는 곳:
  - `src/my_skills/config.py:82`: host 경로 `~/.claude/skills` 등. 이번에 쓰인 곳이다
  - `src/my_skills/init_registry_commands.py:128,130`: 기본 레지스트리 `~/my-agent-skills`
  - `src/my_skills/data.py:35,38`, `src/my_skills/cli_runtime.py:25`: XDG 변수가 없을 때의 기본값
- `tests/test_isolation.py::test_default_user_paths_are_isolated_under_pytest_tmp`가 바로 이것을 잡는 테스트이고, Windows에서 실패한다. CI의 Python 작업(`ci.yml`의 `test-build-smoke`)은 `ubuntu-latest`에서만 돌아서 지금까지 드러나지 않았다.

### 3.3 더 나쁠 수 있었던 경우

이 PC에는 기본 위치에 실제 레지스트리가 이미 있었다. 레지스트리가 없는 Windows PC라면 다음 일이 생길 수 있다. **실행해서 확인한 것은 아니고 코드로 추론한 것이다.**
- `init-registry`의 기본 위치를 쓰는 테스트가 실제 `%USERPROFILE%\my-agent-skills`에 레지스트리를 만들 수 있다.
- 그러면 다음에 `my-skills`를 실행할 때 그 레지스트리를 찾게 된다.

## 4. Python 테스트 실패 상세

다시 실행할 때는 사고가 반복되지 않도록 `USERPROFILE`과 `HOME`을 모두 임시 폴더로 돌렸고, 실행 전후로 실제 host 폴더 목록이 같은 것을 확인했다. 그 결과 시험용 폴더는 모두 임시 홈 안에 생겼다(`.claude`, `.agents`, `.hermes`, `my-agent-skills`). 끝나지 않는 테스트 1개는 뺐다.

```text
102 failed, 213 passed, 1 deselected
```

분류는 JUnit XML(`-o junit_logging=all`)의 테스트별 stderr를 기준으로 했다.

| 원인 | 건수 | 테스트 버그 / 제품 동작 |
|---|---|---|
| A. TOML 문자열 이스케이프 | 61건 + D 중 9건 (그로 인한 2차 실패로 보이는 3건 별도) | 테스트 버그 |
| B. symlink 권한 (`WinError 1314`) | 12건 | link 테스트 10건은 Python 0.4.x 제품의 한계 (Rust v1은 junction으로 해결). 나머지 2건은 시험용 symlink를 만들지 못한 테스트 환경 문제 |
| C. POSIX 경로 가정 | 9건 + D 중 2건 | 대부분 테스트 버그, 1종은 제품 출력 확인 필요 |
| D. 그 밖 (A·C·홈 격리로 설명됨) | 20건 | 아래 |
| E. 끝나지 않음 | 1건 | 홈 격리 문제가 원인 |

### A. TOML 문자열 이스케이프 (61건)

테스트가 `tmp_path`(Windows에서는 `C:\Users\...`)를 TOML 큰따옴표 문자열에 그대로 넣는다. 그러면 `\U`가 유니코드 이스케이프로 해석되어 manifest를 읽지 못한다.

```text
error: invalid TOML in ...\my-skills.toml: Invalid hex value (at line 5, column 13)
```

- 예: `tests/test_agy.py:12,18`의 `path = "{tmp_path / ...}"`
- CLI가 종료 코드 2로 끝나므로 `assert 2 == 0`, `assert 2 == 1`, 빈 stdout에 대한 `JSONDecodeError`로 나타난다.
- 해당 파일: `test_agy`(8), `test_audit`, `test_audit_static`, `test_characterization`, `test_cli_import`(13), `test_cli_install`(11), `test_import`(5), `test_link`, `test_manifest_edit`, `test_share`, `test_sync` 등
- 고치는 방법: TOML 작은따옴표 문자열(`'...'`)을 쓰거나, 경로를 `as_posix()`로 넣는다. runbook 3.5의 `my-skills.local.toml` 예시는 이미 작은따옴표를 쓴다.

### B. symlink 권한 (12건)

```text
OSError: cannot create a symlink at ... [WinError 1314] 클라이언트가 필요한 권한을 가지고 있지 않습니다
```

- `tests/test_link.py`(10), `tests/test_cli_import.py`의 symlink 원본 거부 테스트(2)
- Python 0.4.x의 link 모드는 `os.symlink`를 쓴다. 개발자 모드나 관리자 권한이 없으면 Windows가 막는다.
- 제품의 한계다. 0.4.x 사용자가 Windows에서 `--mode link`를 쓰면 같은 오류가 난다. Rust v1은 junction을 먼저 써서 이 문제를 피한다(`drive::link`).

### C. POSIX 경로 가정 (9건)

| 테스트 | 내용 | 판단 |
|---|---|---|
| `test_audit_static.py::test_static_audit_reports_invalid_utf8_regular_files`, `::test_audit_still_detects_nul_bytes_in_regular_files` | audit 결과의 상대 경로가 `scripts\bad...`로 나옴. 테스트는 `scripts/...`를 기대 | **제품 출력 확인 필요**: Windows에서 audit 보고와 hash 입력의 경로 구분자가 `\`로 바뀌면 macOS와 결과가 달라질 수 있다. Rust 판이 같은 경우 `/`를 쓰는지도 확인 필요 |
| `test_config.py::test_cli_override_beats_local`, `::test_agy_omitted_preserves_existing_default_selection` | `/tmp/...`를 `C:/tmp/...`로 풀거나 POSIX 경로를 비교 | 테스트 버그 |
| `test_data.py::test_data_root_posix_fallback`, `test_state.py::test_default_state_path_fallback` | POSIX 기본값을 기대하는데 Windows 분기가 먼저 걸림. 홈도 `HOME`이 아니라 `USERPROFILE`로 풀림 | 테스트 버그 (POSIX 전용) |
| `test_doctor.py::test_doctor_agy_detection_and_default_activation[False-*]` | 출력에 `.gemini/config/skills`가 있기를 기대. 실제로는 `\` 경로 | 테스트 버그 |
| `test_isolation.py::test_default_user_paths_are_isolated_under_pytest_tmp` | 기본 경로가 pytest 임시 폴더 아래가 아님 | **§3의 원인을 그대로 잡은 테스트** |

### D. 그 밖 (20건)

| 테스트 | 증상 | 원인 |
|---|---|---|
| `test_audit`(2), `test_characterization`(3), `test_cli_bootstrap`, `test_share`(2), `test_sync::test_sync_check_stale_nonzero_writes_nothing` | 종료 코드 2, 빈 stdout, `column 13` | A와 같음. capsys가 stderr를 가져가서 JUnit stderr에 남지 않았다 |
| `test_sync`의 conflict, drifted, update_failure (3) | 설치가 먼저 실패해서 `hosts\claude\alpha\SKILL.md` 없음 | A 때문에 생긴 2차 실패로 보임 (확인하지 않음) |
| `test_config.py::test_expand_path_is_absolute`, `::test_local_override_precedence` | `endswith('/.claude/skills')` | C (구분자) |
| `test_doctor.py::test_doctor_reports_registry_source_version_state_and_data` | `Data:` 줄이 기대와 다름 | C 또는 홈 격리. 확인하지 않음 |
| `test_doctor.py::test_doctor_agy_detection_and_default_activation[True-*]` | 종료 코드 1 | 확인하지 않음 |
| `test_cli_catalog.py::test_skills_json_filters_by_host_and_enabled_state` | JSON 일부 값이 다름 | 확인하지 않음 |
| `test_init_registry.py::test_init_registry_without_path_uses_default_location_when_not_tty` | 기본 위치가 실제 홈 기준 | 홈 격리 |
| `test_integration.py::test_install_then_repeat_is_noop`, `test_seed_e2e.py`(2) | 임시 홈에 설치 결과가 없음. 실제 홈(이번 재실행에서는 임시 `USERPROFILE`)에 설치됨 | 홈 격리. **§3.1 사고를 일으킨 테스트** |

### E. 끝나지 않는 테스트 (1건)

`tests/test_init_registry.py::test_init_registry_prompt_accepts_empty_default`

- 테스트는 `HOME`을 임시 폴더로 바꾸고, `isatty`를 True로, `input`이 항상 `""`를 돌려주도록 바꾼다.
- `src/my_skills/init_registry_commands.py:134-143`의 `_prompt_registry_target()`가 `~/my-agent-skills`를 실제 홈으로 푼다. 이 PC에서는 그 위치에 실제 레지스트리가 있어서 "already contains a my-skills registry"를 출력하고 다시 묻는다. `input`이 계속 `""`를 돌려주니 끝나지 않는다.
- 전체 실행이 49~53% 근처에서 멈추고 10~15분 뒤 시간 제한으로 끝난 이유가 이것이다.
- 제품 쪽 참고: 실제 사용자가 빈 줄을 반복해서 입력해도 같은 루프에 머문다. 의도한 동작이지만, 비대화형 환경에서 `input`이 EOF를 받으면 `EOFError`로 끝나는지는 확인하지 않았다.

## 5. Rust 쪽에 주는 의미

- 이번 Rust 테스트는 실제 홈을 건드리지 않았다. 인계 문서에 따르면 Rust 코드는 `HOME`을 `USERPROFILE`보다 먼저 읽으므로, 테스트가 `HOME`을 바꾸면 Windows에서도 격리된다.
- 반대로 runbook 3.5가 걱정하는 상황이 있다. Git Bash는 `HOME=/c/Users/...` 형식을 준다. 인계 문서 §7 L (4)에서 "Windows에서 `USERPROFILE`을 우선"하기로 바꾸면, **`HOME`만 바꾸는 Rust 테스트도 Python과 같은 방식으로 실제 홈에 쓰게 된다.** (4)를 결정할 때 테스트 격리(`USERPROFILE`도 함께 바꾸는 helper)를 같이 바꿔야 한다.
- C의 audit 경로 구분자 문제가 Rust 판에도 있는지는 확인하지 않았다. Windows CI의 Rust 테스트가 통과하므로 golden 비교 범위 안에서는 문제가 없지만, Windows에서 만든 audit 보고서나 hash를 macOS 결과와 비교한 적은 없다.

## 6. 결정할 것과 권장

| ID | 결정할 것 | 권장 |
|---|---|---|
| D1 | Python 테스트의 Windows 문제를 고칠지 | **기록만 하고 고치지 않는다.** §7 D에서 `dev`의 Python 코드를 지울 예정이고, 0.4.x는 Windows CI가 없었다. 단, 0.4.x의 다음 패치를 낸다면 `tests/conftest.py`에서 `USERPROFILE`도 임시 폴더로 바꾸는 한 줄만은 넣는 것이 좋다. 이 한 줄만으로 실제 홈에 쓰는 일이 사라진다 |
| D2 | Rust 테스트의 홈 격리를 감사할지 | §7 L (4)와 같이 다룬다. `HOME`과 `USERPROFILE`을 함께 바꾸는 공통 helper를 두고, Windows CI에서 "테스트 전후로 실제 host 폴더가 같은지" 검사하는 단계를 넣는 것을 권한다 |
| D3 | runbook 3단계를 이 PC에서 진행할지 | 진행해도 된다. 3단계는 Rust binary만 쓰고 격리 변수(`MY_SKILLS_ROOT`, `XDG_*`)를 쓴다. 단, host 폴더는 일부러 실제 위치를 쓰는 단계이므로 runbook 순서를 지킨다 |
| D4 | audit 경로 구분자 | Python과 Rust 모두에서, Windows에서 audit 보고와 hash 입력에 쓰는 상대 경로가 `/`로 통일되는지 확인하는 테스트를 추가할지 정한다 |

## 7. 다시 확인하는 방법 (실제 홈을 건드리지 않게)

**`uv run pytest`를 Windows에서 그냥 실행하지 않는다.** 꼭 돌려야 하면 Git Bash에서 다음처럼 홈을 임시 폴더로 돌린다.

```bash
FAKE="$(mktemp -d)"
( cd ~ && ls -A .claude/skills .agents/skills .gemini/config/skills .hermes 2>&1 ) > /tmp/before.txt
USERPROFILE="$(cygpath -w "$FAKE")" HOME="$FAKE" \
  uv run pytest -p no:cacheprovider -o junit_logging=all --junitxml=/tmp/junit.xml \
  --deselect tests/test_init_registry.py::test_init_registry_prompt_accepts_empty_default
( cd ~ && ls -A .claude/skills .agents/skills .gemini/config/skills .hermes 2>&1 ) | diff /tmp/before.txt - && echo "host folders unchanged"
```
