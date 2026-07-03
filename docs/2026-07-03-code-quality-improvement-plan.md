# 코드 품질 개선 기획 — 스캔 이원화 해소 및 견고성 보강

> 작성일: 2026-07-03
> 상태: 기획 확정 v2 (구현 대기)
> 대상 저장소: `my-skills`
> 관련 문서: `docs/2026-06-30-my-skills-open-source-roadmap.md`,
> `docs/2026-07-01-default-skills-seed-design.md`

### 개정 이력

- **v2 (2026-07-03)** — 지침 자체 점검 결과 반영.
  - T1/T3의 "기존 동작 동일" 전제 폐기: audit 게이트는 정책 의존적
    (`--skip-audit`, `permissive`)이라 v1 T3(A안)대로 compose에서 보안 스캔을
    빼면 상시 방어선이 약해진다. §3 원칙 0(상시 방어선 불변식)을 신설하고
    T3를 고정 내부 정책 방식으로 교체, T1은 "의도적 강화"로 재정의.
  - T0(특성화 테스트) 신설. T4는 항목별 save를 기본안으로 반전(SIGKILL 내성).
    T6은 exit 0 유지로 변경(에이전트 계약 보호). DoD의 고정 테스트 개수 조건을
    커버리지 동등성 조건으로 대체.
- **v1 (2026-07-03)** — 최초 작성.

---

## 1. 요약

2026-07-03 기준 전체 코드베이스 분석(codegraph 기반) 결과를 바탕으로,
오픈소스 배포 품질을 끌어올리기 위한 수정 계획을 정의한다.

현재 상태는 양호하다: 계획(planner)/실행(installer)/상태(state) 분리가 명확하고,
원자적 쓰기·드리프트 6분류·audit 게이트가 동작하며, 테스트 214건이 전부 통과한다
(소스 4,045줄 / 테스트 3,082줄). 그러나 다음 구조적 결함이 남아 있다.

1. **보안 스캔이 두 벌** — `security.py`(구형)와 `audit/`(신형)가 같은 규칙을
   중복 보유하고, 설치 경로에서 둘 다 실행된다. 규칙 드리프트 위험.
2. **설치 부분 실패 시 상태 유실** — 플랜 실행 도중 예외가 나면 이미 디스크에
   쓰인 설치가 state.json에 기록되지 않아 다음 실행에서 `UNMANAGED`로 오판된다.
3. **state.json 전방 호환성 없음** — 미래 스키마의 필드가 있으면 `TypeError`로 즉사.
4. **카탈로그 단일 실패 전파** — 스킬 하나가 깨지면 `skills` 목록 전체가 실패.
5. **문서/메타데이터 불일치** — pyproject의 "Gemini CLI" 언급, README 스킬 표 누락 등.

이 문서는 각 문제의 근거 위치, 수정 설계, 테스트 기준, PR 분해를 정의한다.
기존 install/sync 사용자 가시 동작(출력 포맷, 종료 코드, 차단 규칙)은 원칙적으로
유지한다(회귀 금지). 단 T1이 명시한 "의도적 강화" 목록만 예외이며, 그 외의
변경은 T0 특성화 테스트가 회귀로 차단한다.

---

## 2. 현재 상황 진단

### 2.1 잘 되어 있는 것 (건드리지 않는다)

- **계층 분리**: `cli.py`(파서 선언) → `*_commands.py`(오케스트레이션·I/O) →
  도메인 모듈(순수 로직). 의존 방향 단방향, 순환 의존 없음.
- **계획/실행 분리**: `planner.plan_install`이 순수 `PlanItem` 값을 반환하고
  `install_commands._apply_plan_with_audit`가 실행. `--dry-run`/`--json`이
  실행 코드 재사용 없이 성립.
- **원자성**: `installer.copy_install`의 staging → backup → `os.replace` → 실패 시
  복원. `state.State.save`의 temp + `os.replace`. link 실패 시 copy로 조용히
  다운그레이드하지 않음.
- **확장점**: 호스트 추가는 `hosts/<name>.py` 선언 + 레지스트리 등록,
  audit 규칙 추가는 `Analyzer` Protocol 구현체 추가로 끝난다.
- **테스트 문화**: 문서 정합성 테스트(`test_readme_onboarding.py`,
  `test_public_hygiene.py`, `test_seed_packaging.py`)까지 존재.

### 2.2 문제 목록 (우선순위순)

| # | 문제 | 근거 위치 | 심각도 |
|---|------|-----------|--------|
| P1 | 보안 스캔 이원화 (규칙 중복 + 이중 스캔) | `src/my_skills/security.py` ↔ `src/my_skills/audit/static.py`, `src/my_skills/checks.py` | 높음 |
| P2 | 설치 부분 실패 시 state 유실 + raw traceback | `src/my_skills/install_commands.py:69` `_apply_plan_with_audit`, `:188` | 높음 |
| P3 | state.json 전방 호환성 없음 | `src/my_skills/state.py:55-65` `State.load` | 중간 |
| P4 | 카탈로그 단일 실패 전파 | `src/my_skills/catalog.py:136` `_description` | 중간 |
| P5 | 문서/메타데이터 불일치 | `pyproject.toml` description, `README.md` Included skills, `src/my_skills/validation.py:106` 주석 | 낮음 |
| P6 | 소소한 위생 문제 (보일러플레이트, 중복 IO, 정책 불일치) | `*_commands.py`, `src/my_skills/audit/dataflow.py:38`, `src/my_skills/sharing.py:286` | 낮음 |

### 2.3 문제별 상세

**P1 — 보안 스캔 이원화.**
`security.py`와 `audit/static.py`가 `_BIDI_CHARS`, `_PRIVATE_KEY_RE`,
`_AWS_KEY_RE`, `_SECRET_ASSIGN_RE`, `_ABS_USER_PATH_RE`, `TEXT_SUFFIXES`를
문자 그대로 두 벌 보유한다. `cmd_install`은 `_validate_selected`(→
`checks.compose_validation` → `security.scan_skill`)와 `_audit_selected`(→
`audit.gate.audit_skills`)를 연달아 호출하므로 같은 파일을 두 번 읽고 두 번
스캔한다. 한쪽 규칙만 갱신하면 조용히 어긋난다 — 보안 도구의 최악 실패 모드.

**P2 — 설치 부분 실패 시 state 유실.**
`_apply_plan_with_audit` 루프에서 `copy_install`/`link_install`이 예외를 던지면
(예: 권한 오류, 디스크 부족) 그 앞에서 성공한 항목들의 `state.put`이 함수 끝의
`state.save()`에 도달하지 못한다. 결과: 디스크에는 설치본이 있으나 state.json에
기록이 없어 다음 실행에서 `UNMANAGED` → `BLOCK_CONFLICT`로 오판. 또한
`cmd_install`/`cmd_sync`는 `ManifestError`만 잡으므로 `OSError`는 사용자에게
raw traceback으로 노출된다.

**P3 — state.json 전방 호환성 없음.**
`State.load`의 `InstallRecord(**raw)`는 미래 버전이 필드를 추가한 state.json을
읽는 순간 `TypeError`. `SCHEMA_VERSION`을 기록만 하고 검사하지 않는다.
구버전 CLI와 신버전 CLI가 같은 머신에 공존하는 순간(uv tool 업그레이드 도중 등)
바로 밟는 경로다.

**P4 — 카탈로그 단일 실패 전파.**
`catalog._description`이 스킬 하나의 SKILL.md 결손/프론트매터 오류를
`ManifestError`로 승격시켜 `my-skills skills` 전체가 실패한다. 목록 명령은
성격상 부분 성공이 맞다 — 깨진 스킬은 행 단위로 표시하고 나머지는 출력해야 한다.

**P5 — 문서/메타데이터 불일치.**

- `pyproject.toml` description이 "Gemini CLI and Hermes"를 언급하나 gemini는
  **애초에 지원 대상이 아니다**. 지원 호스트는 claude/codex/hermes 셋뿐이고
  (`src/my_skills/hosts/`), gemini 호스트는 계획에도 없다. description의 오기다.
  (참고: `skills/cli-inventory/scripts/scan_tools.py`의 `"gemini"` 라벨은
  cli-inventory 스킬이 머신에 설치된 gemini CLI를 탐지하는 것으로, my-skills의
  호스트 지원과 무관하다 — 건드리지 않는다.)
- `README.md`/`README.ko.md` "Included skills" 표는 3개(cli-inventory,
  personal-profile, my-skills)만 나열하나 실제 시드는 `my-jira`(disabled) 포함
  4개다 (`src/my_skills/defaults.py:12` `DEFAULT_SEED_SKILLS`).
- `validation.py:106` 주석은 "-> warning"이라 하나 실제로는 `errors`에 추가한다.

**P6 — 소소한 위생 문제.**

- 커맨드 핸들러 8곳 이상에서 `load_manifest → except ManifestError → print →
  return 2` 보일러플레이트 반복.
- `audit/dataflow.py`의 `_text_files`가 `analyzers._file_contexts`가 이미 읽은
  파일을 다시 읽는다(스킬당 이중 IO).
- `sharing._adopt_source_host`가 manifest audit 설정 대신 기본 `AuditPolicy()`로
  감사 메타데이터를 기록한다 — `cmd_share`의 게이트와 기록 정책이 어긋난다.

---

## 3. 수정 방향 (설계 원칙)

0. **상시 방어선 불변식.** 보안 스캔의 실행 여부는 사용자가 끌 수 없다.
   `--skip-audit`, `audit.enabled = false`, `profile = "permissive"`는 모두
   **audit 게이트의 차단 판단**에만 영향을 주며, `compose_validation`을 통한
   스캔 실행 자체는 항상 수행된다. 이 불변식을 깨는 리팩터링은 금지.
1. **audit 패키지를 유일한 규칙 정의처로.** `security.py`는 제거하고,
   구조 검증(`validation.py`)과 보안 스캔(audit)의 합성 지점인
   `checks.compose_validation`이 audit 분석기를 호출하게 한다. 규칙 정의는
   한 곳에만 존재한다. 실행 횟수 최소화는 목표가 아니다 — 원칙 0이 우선.
2. **부분 실패는 보고하되, 성공분은 반드시 기록.** 플랜 항목 실행을 항목 단위로
   격리하고, state 저장을 실패와 무관하게 보장한다.
3. **읽기는 관대하게, 쓰기는 엄격하게.** state.json의 미지 필드는 무시하고 읽되,
   스키마 메이저 불일치는 명시적 에러. 쓰기는 현행 스키마 그대로.
4. **목록 명령은 부분 성공.** 조회 계열(`skills`, `status`)은 스킬 하나의 오류로
   전체가 죽지 않고, 종료 코드도 바꾸지 않는다(`skills --json`은 에이전트 계약).
   쓰기 계열(install/sync)의 차단 규칙은 그대로 둔다.
5. **사용자 가시 동작 동결 — 특성화 테스트로 집행.** 출력·종료 코드 변경은
   T1의 의도적 강화 목록, P4의 invalid 행 추가, P2의 실패 항목 보고로 한정하며,
   그 외는 T0 특성화 테스트가 회귀로 잡는다. 종료 코드 규약(0 성공 /
   1 차단·드리프트 / 2 사용 오류)은 유지.

---

## 4. 구체적 수정 기획 (태스크 분해)

각 태스크는 독립적으로 집을 수 있는 단위이며 자체 테스트(RED → GREEN)를 포함한다.
의존성 순서를 지킨다.

### PR1 — 스캔 통합 (P1)

**T0. 특성화(golden) 테스트 — 리팩터링 전 필수 선행**

- 규칙별 픽스처 스킬(secret, bidi, abs-path, prompt-injection, 정상) ×
  명령(`validate`, `install --dry-run`, `import`, `share --plan`)의
  stdout·stderr·종료 코드를 그대로 고정하는 테스트를 T1 **착수 전에** 커밋한다.
- 이 테스트는 §3 원칙 5의 집행 장치다. T1의 "의도적 강화" 표에 있는 변경만
  이 테스트를 갱신할 수 있고, 그 외의 출력 변화는 회귀로 간주한다.
- 위치: `tests/test_characterization.py` (신규).

**T1. `compose_validation`을 audit 기반으로 전환 — 의도적 강화**

- `src/my_skills/checks.py`: `security.scan_skill` 호출을 제거하고
  audit 분석기(`audit.analyzers.run_audit`)를 **고정 내부 정책**으로 호출한다
  (정책 상수는 T3에서 정의). 시그니처
  `compose_validation(skill_dir) -> ValidationResult`는 유지.
- 매핑 규칙: `Severity.CRITICAL`/`HIGH` → `ValidationResult.errors`,
  `MEDIUM` 이하 → `warnings`.
- **결정: 정확한 동작 보존이 아니라 의도적 강화를 채택한다.** 규칙별 커스텀
  매핑으로 기존 severity를 재현하는 안은 기각 — 새 유지보수 표면을 만들면서
  P1의 단일화 목표를 다시 훼손한다. 다음 동작 변경을 의도된 것으로 선언하고
  CHANGELOG에 기재한다.

  | 변경 | 이전 | 이후 |
  |------|------|------|
  | abs-user-path (전 파일) | warning | error (audit HIGH) |
  | prompt-injection 등 audit 전용 규칙 | `validate` 미검출 | error로 표면화 |
  | network-sender 등 MEDIUM 규칙 | 미검출 | warning |

- **3중 중복 정리**: `src/my_skills/validation.py:23`의 `_ABS_PATH_RE` 본문
  검사(SKILL.md 본문 한정)는 audit abs-user-path 규칙이 전 파일을 error로
  커버하게 되므로 **삭제**하고 케이스를 audit 테스트로 이관한다.
- 테스트: `tests/test_security.py`의 규칙 케이스(bidi, private key, AWS key,
  secret assign, abs path, NUL, encoding)를 audit 규칙 ID 기준으로 이관하되
  **규칙별 케이스 수가 줄면 안 된다**. 회귀 게이트에 `tests/test_validation.py`,
  `tests/test_share.py`, `tests/test_import.py`, `tests/test_cli_install.py`를
  명시적으로 포함한다(`compose_validation` 호출자 전부: install/sync, import,
  share 후보 평가, validate).

**T2. `security.py` 제거**

- T1 완료 후 `src/my_skills/security.py` 삭제. 유일한 사용처는
  `checks.py`와 `tests/test_security.py`이므로(codegraph 확인 완료) 이관이
  끝나면 참조가 없다.
- `tests/test_security.py`는 audit 쪽 동등 테스트로 흡수 후 삭제하거나,
  `test_audit.py`로 케이스를 병합한다. 규칙별 케이스 수가 줄면 안 된다.
- 검증: `grep -rn "from .security\|import security" src/ tests/` 결과 0건.

**T3. 상시 방어선의 고정 정책화 (v1 "이중 스캔 제거"를 대체)**

- v1의 A안("install/sync 경로의 compose에서 보안 스캔 제거")은 **폐기**한다.
  audit 게이트는 `--skip-audit`·`audit.enabled = false`·`profile = "permissive"`
  로 무력화될 수 있으므로(`src/my_skills/audit/gate.py:32`의 skip 처리,
  `src/my_skills/audit/models.py:60`의 threshold None → blocked False),
  compose에서 스캔을 빼면 `install --skip-audit` + secret 스킬이 설치되는
  **보안 회귀**가 생긴다. 현재는 `_validate_selected`의 무조건 스캔이 이를 막고
  있다.
- 대신: `compose_validation`은 **고정 내부 정책**으로 항상 스캔한다.
  `AuditPolicy(enabled=True, disabled_rules=frozenset())` 상당의 상수를
  `checks.py`에 두고, manifest의 audit 설정과 `--skip-audit`은 게이트의
  차단 판단에만 적용한다(§3 원칙 0).
- install/sync에서 compose 스캔과 게이트 스캔이 두 번 도는 것은 **심층 방어로
  수용**한다. 스킬 디렉터리는 작아 비용이 무시할 수준이고, 제거하려다
  불변식을 깨는 쪽이 더 큰 위험이다. 스캔 결과 공유 최적화는 §7 범위 밖.
- 테스트(신규, §3 원칙 0의 고정 장치):
  1. `install --skip-audit` + secret 스킬 → 여전히 차단.
  2. manifest `profile = "permissive"` + secret 스킬 → install 차단
     (게이트는 통과하지만 compose가 막는다).
  3. manifest `audit.enabled = false` + secret 스킬 → 동일하게 차단.

### PR2 — 설치 견고성 (P2, P3)

**T4. 플랜 실행의 항목 단위 예외 격리 + state 보장 저장**

- `src/my_skills/install_commands.py` `_apply_plan_with_audit`:
  - `copy_install`/`link_install` 호출을 `try/except OSError`로 감싼다.
    실패 항목은 `FAILED: {skill} -> {host} ({exc})` 형태로 보고하고
    `blocked = True`로 집계, 루프는 계속 진행한다.
  - **항목 성공 직후 매번 `state.save()`를 호출한다(기본안).** save는
    temp+replace 원자 쓰기이고 state 파일이 작아 비용이 무시할 수준이며,
    try/finally 방식과 달리 SIGKILL·전원 차단에도 성공분이 보존된다. 호출자
    (`cmd_install`/`cmd_sync`)의 `try/finally` 저장은 이중 안전장치로 유지한다.
- `cmd_uninstall`도 동일하게 `uninstall()` 호출을 격리하고 removed 기록을
  항목별로 저장한다.
- 반환 규약: 실패 항목이 있으면 기존 차단과 동일하게 종료 코드 1.
- 테스트(신규, `tests/test_cli_install.py` 또는 `tests/test_installer.py`):
  1. 2개 스킬 플랜에서 두 번째 `copy_install`이 `OSError`를 던지도록
     monkeypatch → 첫 스킬의 InstallRecord가 state.json에 존재해야 한다(RED
     기준: 현행 코드는 유실).
  2. 실패 항목이 traceback이 아닌 한 줄 보고로 출력되고 종료 코드 1.

**T5. state.json 관대한 읽기 + 스키마 검사**

- `src/my_skills/state.py` `State.load`:
  - `data.get("schema_version", 1)`이 `SCHEMA_VERSION`보다 크면
    `StateError`(신규, `ValueError` 파생)를 던지고, 호출자는
    "state file was written by a newer my-skills; upgrade the CLI" 메시지와
    종료 코드 2로 처리한다.
  - `InstallRecord(**raw)`를 필드 화이트리스트 필터로 교체:
    `{k: v for k, v in raw.items() if k in _RECORD_FIELDS}`.
    `_RECORD_FIELDS`는 `dataclasses.fields(InstallRecord)`에서 파생.
  - 필수 필드(skill, host, mode, source, destination, source_hash,
    installed_hash, installed_at) 누락 시 해당 레코드만 건너뛰지 말고
    `StateError`로 명시 실패한다 — 조용한 레코드 유실은 UNMANAGED 오판을
    낳으므로 금지.
  - `StateError` 메시지에는 **state 파일 경로와 복구 조치**를 반드시 포함한다
    ("upgrade the CLI, or move `<path>` aside and re-run install"). 이 에러는
    읽기 전용 명령(`status`, `skills`)까지 막는데, 조용한 레코드 유실보다
    낫다는 의도된 선택임을 에러 문구와 테스트 주석에 명기한다.
- 테스트(신규, `tests/test_state.py`):
  1. 미지 필드가 섞인 레코드 로드 → 성공, 미지 필드 무시.
  2. `schema_version: 2` → `StateError`.
  3. 필수 필드 누락 → `StateError`.

### PR3 — 카탈로그 부분 성공 (P4)

**T6. `_description`의 행 단위 오류 격리**

- `src/my_skills/catalog.py`: `_description`이 `ManifestError`를 던지는 대신
  `_row_for_skill`에서 잡아 `CatalogRow`에 `invalid: <이유>` description과
  오류 플래그를 담는다. `CatalogRow`에 `error: str | None = None` 필드 추가.
- 테이블 출력: 깨진 스킬 행은 SKILL 열 뒤에 `(invalid)` 표기, JSON 출력은
  `"error"` 키 추가. 정상 스킬 행은 기존과 동일.
- `cmd_skills` 종료 코드: **깨진 스킬이 있어도 0을 유지한다.**
  `skills --json`은 에이전트 계약(`skills/my-skills/SKILL.md`)이자
  SECURITY.md·`docs/release-checklist.md`의 릴리스 위생 커맨드라, 비-0 종료가
  전체 실패로 오판될 수 있다. CI용 실패 신호는 이미 exit 1을 내는
  `validate`의 역할이며 목록 명령에 중복시키지 않는다(§3 원칙 4).
- 테스트(신규, `tests/test_status.py` 또는 catalog 테스트):
  SKILL.md 없는 스킬 + 정상 스킬 2개 등록 → 목록에 3행, 종료 코드 0,
  깨진 행에 `(invalid)` 표기, 정상 행 내용은 불변.

### PR4 — 문서·메타데이터 정합 + 위생 (P5, P6)

**T7. 메타데이터/문서 수정**

- `pyproject.toml` description에서 "Gemini CLI" 제거 →
  "…across Claude Code, Codex and Hermes." **(2026-07-03 완료)**
  gemini는 지원 대상이 아니므로 description에서 삭제하며, 어떤 문서에도
  gemini 지원을 새로 명시하지 않는다.
- `README.md`/`README.ko.md` "Included skills" 표에 `my-jira`
  (disabled 시드, Jira/Atlassian 부트스트랩) 행 추가.
- `src/my_skills/validation.py:106` 주석을 실제 동작(error)에 맞게 수정.
- 테스트: `tests/test_readme_onboarding.py`에 시드 스킬 목록
  (`DEFAULT_SEED_SKILLS`)과 README 표의 정합성 검사 추가 — 이후 시드가
  바뀌면 문서 누락이 테스트로 잡히게 한다.

**T8. 핸들러 보일러플레이트 축약 (선택)**

- `src/my_skills/cli_runtime.py`에 헬퍼 추가:

  ```python
  def with_manifest(fn):  # 데코레이터: ManifestError -> stderr + return 2
      ...
  ```

- 적용 대상: `install_commands.py`, `registry_commands.py`,
  `inspection_commands.py`, `audit_commands.py`의 `load_manifest_from_cwd`
  try/except 블록. 동작 변화 없음(출력 문자열 동일).
- 이 태스크는 회귀 위험 대비 이득이 작으므로 **선택**. 시간이 없으면 스킵.

**T9. audit 내부 위생 (선택)**

- `audit/dataflow.py` `_text_files`를 제거하고, `run_audit`이 만든
  `_file_contexts` 결과를 SKILL 스코프 분석기에 전달하도록
  `AuditContext`(SKILL 스코프)에 `files: tuple[tuple[str, str], ...]` 추가.
  스킬당 파일 읽기 1회로 축소.
- `sharing._adopt_source_host`의 `run_audit(canonical, policy=AuditPolicy())`를
  manifest 정책(`audit_policy_from_manifest`) 사용으로 교체 — 게이트와 기록의
  정책 일치.
- 테스트: 기존 `tests/test_audit.py`, `tests/test_share.py` 통과 유지 +
  share 시 기록되는 `audit_profile`이 manifest 설정을 반영하는 케이스 추가.

---

## 5. PR 분해와 실행 순서

```text
PR1 (T0→T1→T2→T3)  스캔 통합         — 특성화 테스트 선행, security.py 소멸,
                                        규칙 단일화 + 상시 방어선 고정
PR2 (T4, T5)       설치 견고성        — state 유실/전방 호환
PR3 (T6)           카탈로그 부분 성공
PR4 (T7, T8*, T9*) 문서 정합 + 위생   (*선택)
```

- PR1과 PR2는 서로 다른 파일을 만지므로 병행 가능하나, PR1이 `checks.py`와
  `install_commands.py`의 검증 흐름을 바꾸므로 **PR1 → PR2 순서를 권장**한다.
- PR3, PR4는 독립적. 언제든 끼워 넣을 수 있다.
- 각 PR의 게이트: `uv run pytest` 전건 통과 + `uv run my-skills doctor` +
  `uv run my-skills install --dry-run` 출력 육안 확인(사용자 가시 동작 동결
  검증). 릴리스 전 `docs/release-checklist.md` 절차 준수.

## 6. 완료 기준 (Definition of Done)

1. `src/my_skills/security.py`가 존재하지 않고, 보안 규칙은
   `src/my_skills/audit/` 아래에만 존재한다.
2. §3 원칙 0(상시 방어선 불변식)이 신규 테스트로 고정된다 —
   `--skip-audit`·`permissive`·`audit.enabled = false`에서도 secret 스킬
   설치가 차단된다.
3. 플랜 실행 도중 예외가 발생해도 성공한 항목의 InstallRecord가
   state.json에 남는다 (신규 테스트로 고정).
4. 미래 필드가 섞인 state.json을 구버전 규칙으로 읽어도 크래시하지 않고,
   상위 schema_version은 명시적 에러가 된다.
5. 깨진 스킬 1개가 있어도 `my-skills skills` 목록이 출력된다(해당 행은
   invalid 표기, 종료 코드 0 유지).
6. pyproject/README/시드 목록이 일치하고, 그 정합성이 테스트로 고정된다.
7. 전체 테스트가 통과하고, 이관된 보안 규칙마다 최소 1개 케이스가 유지되며,
   종료 코드 규약과 차단 규칙(DRIFTED/CONFLICT/UNMANAGED 차단)이 변하지
   않는다. (v1의 "214건" 고정 개수 조건은 test_security 이관으로 개수가
   변하므로 커버리지 동등성 조건으로 대체.)

## 7. 범위 밖 (이번에 하지 않는 것)

- gemini 호스트는 지원 대상이 아니다. 미래 작업으로도 다루지 않으며, 문서에
  gemini 지원을 명시하지 않는다(pyproject의 오기만 T7에서 제거).
- state.json 파일 락(동시 실행 보호) — 개인 도구 특성상 실위험 낮음. P2의
  보장 저장으로 피해 반경이 줄어들므로 관찰 후 결정.
- audit 규칙의 의미론적 강화(base64 우회 탐지 등) — 정규식 1차 방어선이라는
  현 포지셔닝 유지. 강화하려면 위협 모델 문서부터.
- `find_repo_root` 루트 캐시의 stale 알림 — UX 기획과 함께 다룰 것.
- compose ↔ audit 게이트 간 스캔 결과 공유 최적화(이중 실행 제거) — §3 원칙 0
  불변식을 깰 위험 대비 이득이 작다. 성능 문제가 실측되면 별도 기획.
