# import 매니페스트 자동 등록 기획서

> 작성일: 2026-07-07 (2026-07-08 교차 검토 반영)
> 상태: 구현 대기 (Codex 작업용) — 독립 세션 교차 검토 통과
> 대상 저장소: github.com/Seosiju/my-skills
> 관련 문서: docs/2026-07-04-registry-root-reliability-and-docs-plan.md (용어 정의 §0을 그대로 따른다)

## 1. 문제 정의

`my-skills import <dir>`는 스킬 파일을 registry의 `skills/`로 복사하지만
**매니페스트(`my-skills.toml`)에는 등록하지 않는다.** 그 결과 새 스킬을 만드는
표준 흐름이 명령어만으로 완결되지 않는다:

```
$ my-skills import /tmp/hello-test
imported: hello-test -> /Users/example/git/my-agent-skills/skills/hello-test
Next: add [skills.hello-test] to my-skills.toml, then `my-skills sync`.

$ my-skills enable hello-test
error: unknown skill: hello-test        ← import 직후인데 실패

$ my-skills skills
(hello-test 없음)                        ← 매니페스트 기준 목록이라 안 보임

$ my-skills validate hello-test
[OK] hello-test                          ← 파일은 정상. 등록만 빠짐
```

사용자 관점에서 `import → enable → install`은 자연스러운 순서인데, 중간에
"TOML을 손으로 편집하라"는 안내가 끼어든다. CLI의 다른 모든 쓰기 경로(share,
enable/disable, install/sync)는 명령어로 완결되므로 import만 예외인 것은
설계 불일치다. 2026-07-07 사용자 머신에서 실제 온보딩 중 이 순서 그대로
막혔다 (위 출력은 실측).

## 2. 진단 — 검증된 사실 (2026-07-07, 전부 실측/코드 확인)

### F1. `cmd_import`는 복사·검증만 하고 등록하지 않는다
`src/my_skills/registry_commands.py:108-165` (`cmd_import`):
validate → audit gate → `shutil.copytree(source, target_dir)` 후
수동 편집 안내를 출력하고 끝난다 (`registry_commands.py:164`):

```python
print(f"Next: add [skills.{name}] to my-skills.toml, then `my-skills sync`.")
```

### F2. `enable`은 등록된 섹션이 없으면 실패한다 (신규 등록 불가)
`enable`/`disable` → `set_skill_enabled` (`src/my_skills/manifest_edit.py:20-26`)는
`[skills.<name>]` 섹션이 없으면 `ManifestEditError("unknown skill: ...")`를
던진다. 즉 enable은 기존 항목의 토글 전용이며 신규 등록 수단이 아니다.

### F3. 등록 함수는 이미 존재하고 share가 쓰고 있다
`register_skill(manifest_path, skill, *, enabled, hosts)`
(`src/my_skills/manifest_edit.py:29-52`):

- 섹션이 없으면 `[skills.<name>]` + `enabled` + `hosts`를 append
- 섹션이 있으면 `enabled`/`hosts` 키만 갱신 (멱등)
- 스킬 이름 유효성 검증 포함 (`_validate_skill_name`)

share 흐름이 유일한 호출자다 (`src/my_skills/sharing.py:85`):

```python
hosts = tuple(name for name, target in manifest.targets.items() if target.enabled)
register_skill(manifest_path, candidate.name, enabled=enabled, hosts=hosts)
```

**즉 이번 작업은 새 기능이 아니라 기존 함수를 import 경로에 연결하는 것이다.**

### F4. `source_type`은 필수 키가 아니다
`register_skill`은 `source_type`/`source_revision`을 쓰지 않는다. 이 키들은
seed 생성 시에만 기록되고 (`init_registry_commands.py:208`,
`"builtin-seed"`), 로드 시 기본값 `""`로 관대하게 읽힌다
(`config.py:154`). share로 등록된 스킬도 이 키 없이 정상 동작한다.
따라서 import 등록 시에도 두 키는 생략해도 된다 (선택: F4-보강 §5 참고).

### F5. "up to date" 조기 반환도 등록을 건너뛴다
`cmd_import`는 canonical 디렉터리가 이미 동일하면
`up to date: ... already matches` 출력 후 즉시 `return 0`
(`registry_commands.py:149-152`). 이때도 매니페스트 미등록 상태면 등록이
영영 안 된다. 실측: 사용자 머신에서 1차 import(잘못된 cwd) 후 재시도 시
"up to date"만 나오고 enable은 계속 실패했다.

### F6. share와 import의 UX 계약 차이
share는 `--enable`/`--disable` 중 정확히 하나를 요구한다
(`registry_commands.py:51-53`). import에는 대응 플래그가 없다.

### F7. `cmd_import`는 이미 파싱된 매니페스트 객체를 갖고 있다
`load_manifest`가 돌려주는 `Manifest.skills`는 `dict[str, Skill]`이고
각 `Skill`에 `enabled`/`hosts`가 있다 (`config.py:62-79`). 따라서
등록 여부·기존 상태 판별에 새 헬퍼가 필요 없다 —
`name in manifest.skills`로 충분하다.

**주의 (local 오버레이):** `_resolve_skills`(`config.py:142-157`)는
`my-skills.local.toml`을 병합한 값을 돌려준다. 병합된 `enabled`를 읽어
`register_skill`로 되쓰면 로컬 오버레이 값이 메인 `my-skills.toml`에
구워지는 부작용이 생긴다. 그러므로 "기존 상태 보존"은 값을 읽어
되쓰는 방식이 아니라 **재등록 자체를 건너뛰는** 방식이어야 한다 (§5.1).

### F8. 독립 재현 (2026-07-08, Codex 교차 검토)
별도 AI 세션(Codex)이 임시 registry에서 `import → enable → skills →
import 재시도`를 독립 실행해 동일하게 재현했다: import는 파일만 복사,
`enable`은 `unknown skill` 실패, `skills` 목록 미노출, 재-import는
"up to date"만 출력하고 매니페스트는 계속 미등록 (F5 확인).

## 3. 목표 / 비목표

**목표**

1. `my-skills import <dir>` 한 번으로 파일 복사 + 매니페스트 등록이 끝난다.
2. `import` 직후의 `enable`/`disable`/`skills`/`install`이 항상 동작한다.
3. 기존 등록 항목의 `enabled` 상태를 import가 마음대로 뒤집지 않는다 (멱등).

**비목표**

- share 흐름 변경 (이미 올바르게 동작)
- 매니페스트 스키마 변경 (`source_type` 필수화 등)
- `skills`/`status`가 미등록 canonical 디렉터리를 표시하는 기능 (§7 후속 과제)

## 4. 설계 결정: 등록 시 enabled 기본값

| 안 | 동작 | 판단 |
|---|---|---|
| A. 기본 `enabled = false` + `--enable` 플래그 | 등록은 항상, 켜는 것은 명시적 | **채택** |
| B. share처럼 `--enable`/`--disable` 중 하나 강제 | 일관성은 높으나 기존 `import <dir>` 호출이 전부 에러로 깨짐 | 기각 (하위호환) |
| C. 기본 `enabled = true` | 편하지만 "쓰기 게이트는 명시적 승인" 원칙(audit/–yes 계열)과 어긋남 | 기각 |

A안 근거: `import`는 이미 인자 없이 동작하는 공개 명령이므로 기본 동작을
깨지 않고, "등록되어 있으나 꺼짐"은 안전한 기본 상태다. 켜는 순간은
`--enable` 또는 후속 `my-skills enable`로 사용자가 명시한다.

## 5. 구현 방안

### 5.1 `cmd_import` 수정 (`src/my_skills/registry_commands.py`)

1. copytree 성공 후 등록 추가:

```python
hosts = tuple(name for name, target in manifest.targets.items() if target.enabled)
register_skill(root / "my-skills.toml", name, enabled=args.enable, hosts=hosts)
```

   share(`sharing.py:84-85`)와 동일한 hosts 계산을 사용한다.

2. **기존 항목 보호 (멱등성) — 확정 (2026-07-08 교차 검토 반영):**
   대상 섹션이 이미 존재하면 `register_skill`이 `enabled`/`hosts`를
   인자값으로 덮어쓴다(F3). 켜져 있는 스킬을 재-import했을 때 `--enable`
   없이 꺼지는 사고를 막아야 한다. 새 헬퍼를 만들지 않고, `cmd_import`가
   이미 갖고 있는 `manifest` 객체로 분기한다 (F7):

```python
if name not in manifest.skills:
    hosts = tuple(t for t, target in manifest.targets.items() if target.enabled)
    register_skill(root / "my-skills.toml", name, enabled=args.enable, hosts=hosts)
elif args.enable and not manifest.skills[name].enabled:
    set_skill_enabled(root / "my-skills.toml", name, True)
```

   - **미등록 → 등록** (신규 경로).
   - **기존 등록 → 재등록 생략.** 값을 읽어 되쓰지 않는 이유: F7의 local
     오버레이 병합값이 메인 TOML에 구워지는 부작용과 `hosts` 덮어쓰기를
     동시에 차단한다. `--enable`이 붙은 경우에만 `set_skill_enabled`로
     enabled 키 하나만 갱신한다 (hosts 불변).
   - 기각된 대안: `manifest_edit.py`에 `is_skill_registered` 헬퍼 신설,
     `register_skill(preserve_enabled=...)` 파라미터 추가 — 둘 다 이미
     로드된 매니페스트로 해결되는 일에 API 표면을 늘린다.

3. **F5 수정:** "up to date" 조기 반환 경로에서도 등록 보장. 파일이 동일해도
   매니페스트에 없으면 등록하고 `registered: <name> (disabled)`를 출력한다.

4. 마지막 안내 메시지 교체:

```
imported: hello-test -> <target_dir>
registered: hello-test (disabled)
Next: `my-skills enable hello-test`, then `my-skills install hello-test --host <host>`.
```

   `--enable`을 쓴 경우:

```
registered: hello-test (enabled)
Next: `my-skills install hello-test --host <host>`.
```

### 5.2 CLI 플래그 (`src/my_skills/cli.py`)

`import` 서브파서에 추가:

```python
parser.add_argument("--enable", action="store_true",
                    help="Register the skill as enabled (default: registered but disabled)")
```

### 5.3 F4-보강 — 범위 제외 확정 (2026-07-08)

등록 블록에 `source_type = "import"`를 기록하는 안은 **이번 범위에서
뺀다** (교차 검토 합의). 사유: 현재 스키마상 필수 키가 아니고(F4),
provenance 표시가 manifest 필드만으로 완결되는 구조도 아니어서
(`inspection_commands.py:210-214`는 state 레코드를 함께 본다) 이 작업에
섞으면 범위가 커진다. 필요해지면 share의 provenance와 묶어 별도 작업으로.

## 6. 테스트 계획

기존 import/share 테스트 파일 옆에 추가 (tests/ 내 대응 모듈 관례를 따른다).

1. **신규 import 등록:** import 후 `my-skills.toml`에
   `[skills.<name>]`, `enabled = false`, `hosts = [...enabled targets]` 존재.
2. **`--enable`:** `enabled = true`로 등록.
3. **enable 연쇄:** import 직후 `cmd_enable`이 성공한다 (회귀 방지 — 이번
   버그의 재현 시나리오).
4. **멱등/보호:** `enabled = true`로 등록된 스킬을 `--enable` 없이
   재-import해도 `enabled = true` 및 기존 `hosts` 유지 (재등록 생략 경로).
4b. **등록된 disabled 스킬 + `--enable`:** enabled만 true로 바뀌고
   hosts는 불변 (`set_skill_enabled` 경로).
4c. **local 오버레이 미오염 (F7):** `my-skills.local.toml`로 enabled가
   오버라이드된 스킬을 재-import해도 메인 `my-skills.toml`의 해당 섹션이
   바이트 단위로 불변.
5. **up-to-date 경로 등록 (F5):** canonical과 동일한 소스를 import했고
   매니페스트에 미등록이면, 등록이 수행된다.
6. **audit/validation 차단 시 미등록:** 기존 차단 경로에서 매니페스트가
   변경되지 않는다.
7. **skills 목록 노출:** import 후 `cmd_skills` 출력에 스킬이 나타난다.

## 7. 문서·후속 과제

- **skills/my-skills/SKILL.md** (canonical 스킬 설명서): "새 스킬 만들기"
  절을 추가 — 임시 디렉터리에 SKILL.md 작성 → `import`(자동 등록) →
  `enable` → `install --host <host>` → 수정 시 canonical 편집 후 `sync`.
  현재 설명서는 share/install/sync만 다루고 신규 생성 흐름이 없다.
- **README.md / README.ko.md**: import 예시의 "Next: TOML 수동 편집" 서술이
  있으면 새 동작으로 갱신.
- **CHANGELOG.md**: `import`가 매니페스트 등록까지 수행함을 기록 (동작 변경).
- (후속, 이번 범위 밖) `skills`/`status`가 `skills/`에 있으나 미등록인
  디렉터리를 `unregistered`로 표시하면 이런 상태가 조기에 드러난다.

## 8. 런북 — 이미 미등록 상태에 빠진 registry 복구 (수동)

구현 전이거나 구버전 CLI 사용자를 위한 즉시 복구 절차. registry의
`my-skills.toml`에 블록을 직접 추가한다 (예: hello-test):

```toml
[skills.hello-test]
enabled = true
hosts = ["claude", "codex", "hermes"]
```

이후 `my-skills skills`로 노출 확인, `my-skills install hello-test --host
<host>`로 배포. `source_type`/`source_revision`은 생략 가능하다 (F4).

## 9. 수용 기준

- [ ] `import` 성공 시 매니페스트에 스킬이 등록된다 (기본 disabled).
- [ ] `import --enable` 시 enabled로 등록된다.
- [ ] `import` 직후 `enable`/`disable`/`skills`가 해당 스킬에 대해 동작한다.
- [ ] 이미 등록·활성화된 스킬 재-import가 enabled 상태를 바꾸지 않는다.
- [ ] "up to date" 경로에서도 미등록이면 등록된다.
- [ ] audit/validation 차단 시 매니페스트는 변경되지 않는다.
- [ ] "Next: add [skills.…] to my-skills.toml" 수동 안내 문구가 제거된다.
- [ ] 재-import가 기존 항목의 `hosts`를 바꾸지 않고, local 오버레이 값을
      메인 TOML에 되쓰지 않는다.
- [ ] 전체 테스트 통과, CHANGELOG 갱신.

## 10. 검토 이력

- **2026-07-07** 초안 작성 (Claude, 사용자 온보딩 중 실측 기반).
- **2026-07-08** 독립 교차 검토 (Codex, 별도 세션): 임시 registry에서 버그
  독립 재현(F8), 수정 방향 승인. 반영된 변경 — §5.1 기존 항목 보호를
  "매니페스트 객체 기반 재등록 생략"으로 확정(헬퍼 신설안 기각),
  §5.3 `source_type` 보강 범위 제외 확정. 검토 과정에서 F7(local 오버레이
  되쓰기 부작용)이 추가 식별되어 테스트 4b/4c로 반영.
