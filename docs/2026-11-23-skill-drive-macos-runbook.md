# Skill Drive v1: macOS 실행 문서 (맥 전환, 인계 문서 §7 G)

> 대상: 맥에서 이 작업을 이어받는 에이전트와 사용자
> 기준: `dev` 브랜치 (이 문서가 들어간 커밋 이후)
> 관련 문서: 스펙 `docs/2026-11-23-skill-drive-rust-v1-spec.md` §11(맥 체크리스트), §12(Windows 결과) /
> 인계 문서 `docs/2026-11-23-skill-drive-v1-status-and-handoff.md` §7 G / Windows 판 `docs/2026-11-23-skill-drive-windows-runbook.md`

## 실행 규칙 (먼저 읽는다)

- 순서대로 진행한다. **[멈춤]**에서는 결과를 사용자에게 보여 주고 확인을 받은 뒤에 넘어간다.
- 명령은 **블록 단위로, bash에서** 실행한다(zsh 사용자도 `bash`를 연다). 에이전트처럼 블록마다 새 셸을 쓰는 경우도 고려했다.
- **모든 블록은 첫 줄에서 공통 설정 파일을 불러온다**(`. "<GENV>" || exit 1`). 1.1이 그 파일을 만들고 경로를 출력한다. 3단계 블록은 3.1이 만드는 `<PENV>`를 불러온다.
- 블록 안의 확인은 실패하면 `exit 1`로 그 블록을 끝낸다. 그 뒤의 명령은 실행되지 않는다. `ABORT`나 `STOP`이 나오면 출력 전체를 기록하고 **[멈춤]**.
  사람이 대화형 창에 붙여 넣으면 `exit`가 창을 닫으므로, 사람은 블록을 파일로 저장해 `bash 파일`로 실행한다.
- 일부 명령은 실패하기 전에 일부 항목을 이미 적용했을 수 있다. 실패하면 상태를 확인하기 전에는 다시 실행하지 않는다.

**Windows에서 확인한 것 (스펙 §12):**
- Windows에서는 junction을 쓴다. Claude Code와 Codex는 junction 스킬을 발견하지만, **agy 1.5.14는 junction을 발견하지 못하고 일반 폴더만 발견한다.** Windows PC는 기기별 `my-skills.local.toml`에 `[defaults] install_mode = "link"`, `[targets.agy] install_mode = "copy"`를 쓴다.
- 맥은 symlink를 쓴다. agy가 symlink를 발견한다는 기록은 agy 1.2.16 때 것이다(`docs/release-checklist.md`). **지금 버전으로 3.3에서 다시 확인한다.**
- git hook은 PATH의 `my-skills`를 먼저 찾는다. Python 0.4.x가 Rust 판보다 앞에 있으면 hook이 잘못 동작한다.

**금지:** 원본(레지스트리의 `skills/`)을 직접 지우는 정리, `--force` 덮어쓰기, link 실패를 copy로 조용히 넘기는 처리(출력에 `[copy fallback: ...]`이 나오면 멈춘다. copy는 `[targets.<host>] install_mode = "copy"`처럼 설정으로만 쓴다), 5단계 밖의 레지스트리 커밋과 push, 공개 저장소(tool repo)에 개인 경로·계정·실제 스킬 이름·전역 안내 내용을 커밋하기.

| 단계 | 하는 일 | 실제 환경을 바꾸는가 |
|---|---|---|
| 1. 사전 점검·tool repo 갱신 | 도구, PATH, 레지스트리 위치와 상태, tool repo `dev` 갱신 | 레지스트리·host는 읽기만 (비공개 작업 폴더 생성, 1.3에서 tool repo만 `dev`로 갱신) |
| 2. 빌드와 테스트 | `dev` 빌드와 테스트 | tool repo만 |
| 3. 안전한 확인 | 임시 레지스트리로 symlink 발견, hook 확인 | 시험용 스킬 하나를 host 폴더에 연결했다가 지움 (**[멈춤]** 후) |
| 4. 이 맥의 전환 | 설치, 기기별 설정, copy를 link로, hook 설치 | **예** (계획 승인 + dry-run 승인 후) |
| 5. 공유 레지스트리 변경 | manifest 기본값, 안내 문서, 전역 안내 | **예** (diff 승인 후. push하면 Windows에도 간다) |
| 6. 두 기기 확인 | 맥의 push가 Windows hook으로 반영되는지 | Windows 쪽 작업 필요 |
| 7. 결과 기록 | 스펙 §11, 인계 문서 | tool repo만 |

---

## 1. 사전 점검 (1.1~1.2 읽기만, 1.3은 tool repo 갱신)

### 1.1 비공개 작업 폴더와 공통 설정

`TOOL`(tool repo)과 `REG`(개인 레지스트리)는 따옴표로 감싼 절대 경로로 채운다. `REG`를 아직 모르면 1.2에서 확인한 뒤 이 블록을 다시 실행한다. XDG state/config 경로와 백업 위치(`BACKUP`)도 `g.env`에 고정해 이후 셸에서도 같은 경로를 검증한다.

```bash
TOOL="$HOME/<tool repo 폴더>"; REG="$HOME/<registry 폴더>"
GDIR="$(mktemp -d)" && GDIR="$(cd "$GDIR" && pwd -P)" && chmod 700 "$GDIR" || { echo "ABORT: mktemp"; exit 1; }
CFG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"; STH_HOME="${XDG_STATE_HOME:-$HOME/.local/state}"   # 지금 값으로 고정한다
for p in "$CFG_HOME" "$STH_HOME"; do case "$p" in /*) ;; *) echo "ABORT: XDG path must be absolute: $p"; exit 1;; esac; done
STATE="$STH_HOME/my-skills/state.json"
ROOTCACHE="$CFG_HOME/my-skills/root"
BACKUP="$GDIR/state-backup"
{ printf 'TOOL=%q\nREG=%q\nGDIR=%q\nCFG_HOME=%q\nSTH_HOME=%q\nSTATE=%q\nROOTCACHE=%q\nBACKUP=%q\n' "$TOOL" "$REG" "$GDIR" "$CFG_HOME" "$STH_HOME" "$STATE" "$ROOTCACHE" "$BACKUP"; cat <<'EOF'
# 4단계부터 실제 명령은 항상 이것으로: 레지스트리, XDG config/state 위치, 설치한 binary를 모두 고정한다
ms() { MY_SKILLS_ROOT="$REG" XDG_CONFIG_HOME="$CFG_HOME" XDG_STATE_HOME="$STH_HOME" "${MSBIN:?run step 4.2 first}" "$@"; }
guard_dirs() {
  [ -d "$TOOL/.git" ] && [ -f "$TOOL/Cargo.toml" ] || { echo "ABORT: TOOL"; return 1; }
  [ -d "$REG/.git" ] && [ -f "$REG/my-skills.toml" ] || { echo "ABORT: REG"; return 1; }
  [ "$(cd "$REG" && pwd -P)" != "$(cd "$TOOL" && pwd -P)" ] || { echo "ABORT: REG is the tool repo"; return 1; }
}
EOF
} > "$GDIR/g.env" || exit 1
echo "GENV=$GDIR/g.env"                               # 이 경로를 기록한다. 이후 모든 블록이 불러온다
```

### 1.2 도구, PATH, 레지스트리 위치

tool repo 안에도 seed용 `my-skills.toml`이 있다. **tool repo 안에서 `doctor`를 실행하면 tool repo를 레지스트리로 잡을 수 있으므로** 저장소 밖에서 실행한다.

```bash
. "<GENV>" || exit 1
cd "$HOME" || exit 1
sw_vers; git --version; python3 --version
which -a cargo rustc; cargo --version; rustc --version
RU="$(command -v rustup || echo "$HOME/.cargo/bin/rustup")"; [ -x "$RU" ] && "$RU" toolchain list && "$RU" component list --installed --toolchain stable | grep -E '^(rustfmt|clippy)'   # rustup은 PATH에 없을 수 있다
which -a my-skills; my-skills --version                       # 기존 Python 0.4.x
command -v claude codex agy; claude --version; codex --version; agy --version   # agy 버전은 꼭 기록
echo "$PATH" | tr ':' '\n' | nl                               # cargo bin, Homebrew bin, Python my-skills 폴더의 순서
git config --global --get core.hooksPath; git config --system --get core.hooksPath
env -u MY_SKILLS_ROOT my-skills doctor --no-update-check | sed -n '/^Registry:/,/^Hosts:/p'   # Registry, State와 (source: ...)
echo "expected STATE=$STATE  ROOTCACHE=$ROOTCACHE"
cat "$ROOTCACHE" 2>/dev/null; ls -l "$STATE" 2>/dev/null
```

doctor의 Registry가 tool repo이거나 예상과 다르면 **[멈춤]**. 맞는 `REG`로 1.1을 다시 실행한다. doctor가 보고한 State 경로가 `$STATE`와 다르면 XDG 환경 변수를 확인한 뒤 1.1을 다시 실행한다.

```bash
. "<GENV>" || exit 1; guard_dirs || exit 1
git -C "$REG" remote -v                                       # 개인 레지스트리 원격인지 사용자에게 확인
git -C "$REG" status -sb; git -C "$REG" log --oneline -1
git -C "$REG" rev-parse --path-format=absolute --git-path hooks; ls "$(git -C "$REG" rev-parse --path-format=absolute --git-path hooks)" 2>/dev/null | grep -v '\.sample$'
ls "$REG/my-skills.local.toml" 2>/dev/null && cat "$REG/my-skills.local.toml"
for d in ~/.claude/skills ~/.agents/skills ~/.gemini/config/skills; do echo "[$d]"; ls -la "$d"; done
```

| 결과 | 할 일 |
|---|---|
| 레지스트리가 기본 브랜치가 아니거나 커밋하지 않은 변경이 있음 | **[멈춤]** 정리 방법은 사용자가 정한다 |
| 레지스트리 hook 폴더에 이미 hook이 있거나 전역 `core.hooksPath`가 설정됨 | 4.1~4.2의 pull 때 실행되지 않게 막고, 4.4 전에 hook 폴더 위치를 확정한다. 기록 |
| host 폴더에 레지스트리 스킬과 같은 이름의 폴더가 있는데 기록이 없음 | 4.3에서 `BLOCK_CONFLICT`가 된다. 미리 알린다 |
| host 폴더에 다른 도구가 만든 symlink가 있음 | 레지스트리에 없는 이름이면 건드리지 않는다. 기록만 |
| `cargo`가 Homebrew와 rustup 양쪽에 있음 | 2단계와 4.2는 rustup stable을 `rustup run stable cargo ...`로 쓴다(PATH는 바꾸지 않는다). stable에 `rustfmt`나 `clippy`가 없으면 사용자 확인 후 `rustup component add --toolchain stable rustfmt clippy`. 이유는 2단계. 쓴 toolchain과 버전을 기록 |
| rustup이 없고 cargo가 하나뿐 | 그 cargo의 버전이 CI(최신 stable)와 같은지 확인한다. 같으면 2단계 블록에서 rustup 확인과 `RUSTUP_BIN` 줄 대신 `echo 'rcargo() { cargo "$@"; }' >> "$GDIR/g.env"`를 쓴다. 버전이 낮으면 **[멈춤]** |

### 1.3 tool repo 갱신 (checkout 업데이트, 읽기 전용 아님)

전역 `core.hooksPath`가 `switch`·`pull` 때 실행되지 않도록 명령 범위에서 hook을 끈다.

```bash
. "<GENV>" || exit 1; guard_dirs || exit 1; cd "$TOOL" || exit 1
[ -z "$(git status --porcelain)" ] || { echo "ABORT: tool repo has local changes"; exit 1; }
git fetch origin && git -c core.hooksPath=/dev/null switch dev && git -c core.hooksPath=/dev/null pull --ff-only && git log --oneline -1 || exit 1
```

---

## 2. 빌드와 테스트

CI와 같은 최신 stable로 검사한다. Homebrew Rust와 rustup이 함께 있는 맥에서는 `cargo fmt`, `cargo clippy`를 그대로 쓰지 않는다.
- cargo는 하위 명령(`cargo-fmt`, `cargo-clippy`)을 PATH보다 먼저 `$CARGO_HOME/bin`에서 찾는다. 그곳에는 rustup proxy가 있으므로, Homebrew cargo를 PATH 앞에 두어도 rustup proxy가 실행되고, rustup toolchain에 구성요소가 없으면 실패한다.
- Homebrew Rust가 CI보다 오래된 버전이면 clippy가 CI에 없는 lint를 낸다(2026-11-24: Homebrew 1.96에서 1건, CI와 같은 1.100에서는 통과).

그래서 `rustup run stable cargo ...`로 toolchain을 명시하고(`rcargo`), PATH는 바꾸지 않는다.

```bash
. "<GENV>" || exit 1; guard_dirs || exit 1; cd "$TOOL" || exit 1
RUSTUP_BIN="$(command -v rustup || echo "$HOME/.cargo/bin/rustup")"   # PATH에 없어도 기본 위치를 본다
[ -x "$RUSTUP_BIN" ] || { echo "STOP: rustup not found (see the 1.2 table)"; exit 1; }
for c in rustfmt clippy; do
  "$RUSTUP_BIN" component list --installed --toolchain stable | grep -q "^$c" \
    || { echo "STOP: rustup stable has no $c (rustup component add --toolchain stable rustfmt clippy, after approval)"; exit 1; }
done
printf 'RUSTUP_BIN=%q\n' "$RUSTUP_BIN" >> "$GDIR/g.env"
echo 'rcargo() { "$RUSTUP_BIN" run stable cargo "$@"; }' >> "$GDIR/g.env"   # 4.2도 이것을 쓴다
. "$GDIR/g.env" || exit 1
rcargo --version; "$RUSTUP_BIN" run stable rustc --version                 # 기록한다
rcargo fmt --all --check && rcargo clippy --all-targets -- -D warnings && rcargo test --all-targets || { echo "STOP: checks"; exit 1; }
rcargo build --release || exit 1
TD="$(rcargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys;print(json.load(sys.stdin)["target_directory"])')" || exit 1
MS="$TD/release/my-skills"; "$MS" --version || exit 1                 # my-skills 1.0.0-alpha.N
printf 'MS=%q\nBUILT=%q\n' "$MS" "$(git rev-parse --short HEAD)" >> "$GDIR/g.env"
```

통과 개수를 기록한다(맥은 `cfg(unix)` 테스트가 더해져 Windows와 다르다). Rust 테스트는 사용자 경로를 임시 폴더로 돌린다. `dev`에는 Python 테스트 모음이 없다.

---

## 3. 안전한 확인

사람은 3단계를 **새 터미널 창**에서 하고 끝나면 닫는다. 에이전트는 블록마다 `<PENV>`를 불러온다.

### 3.1 격리 환경

```bash
. "<GENV>" || exit 1; [ -x "$MS" ] || { echo "ABORT: run step 2"; exit 1; }
BASE="$(mktemp -d)" || { echo "ABORT: mktemp"; exit 1; }; BASE="$(cd "$BASE" && pwd -P)"
case "$BASE" in /private/tmp/*|/private/var/folders/*|/tmp/*|/var/folders/*) ;; *) echo "ABORT: unexpected BASE $BASE"; exit 1;; esac
T="$BASE/skill drive probe"; case "$T" in *"'"*) echo "ABORT: apostrophe"; exit 1;; esac
mkdir -p "$T/registry/skills/skill-drive-probe" "$T/work" || exit 1
cat > "$T/registry/skills/skill-drive-probe/SKILL.md" <<'EOF'
---
name: skill-drive-probe
description: Temporary probe for the my-skills macOS check. Reply with the word PROBE-OK when asked about it.
---

This skill exists only to verify that the agent discovers skills linked by a symlink.
EOF
cat > "$T/registry/my-skills.toml" <<'EOF'
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
EOF
{ printf '. %q || return 1\nBASE=%q\nT=%q\n' "$GDIR/g.env" "$BASE" "$T"; cat <<'EOF'
export MY_SKILLS_ROOT="$T/registry" XDG_STATE_HOME="$T/state" XDG_CONFIG_HOME="$T/config" XDG_DATA_HOME="$T/data"
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_COMMON_DIR GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX GIT_CONFIG_PARAMETERS GIT_CONFIG_COUNT
HOSTS=("$HOME/.claude/skills" "$HOME/.agents/skills" "$HOME/.gemini/config/skills")
absent() { local ok=0 d n; for d in "${HOSTS[@]}"; do for n in "$@"; do
  if [ -e "$d/$n" ] || [ -L "$d/$n" ]; then echo "PRESENT: $d/$n"; ok=1; fi; done; done; return $ok; }
# owned-copy.intent: 만들기 전에 남기는 "만들 예정" 기록. owned-copy: mkdir이 성공한 뒤에만 생기는 "우리가 만들었음" 기록.
# 이 함수는 owned-copy가 있을 때만 지운다. intent만 남아 있으면 우리가 만들었는지 알 수 없으므로 멈추고 사람이 본다.
remove_owned_copy() {
  if [ -f "$T/owned-copy.intent" ] && [ ! -f "$T/owned-copy" ]; then echo "STOP: interrupted before ownership; inspect $(cat "$T/owned-copy.intent") by hand"; return 1; fi
  [ -f "$T/owned-copy" ] || return 0
  local c; c="$(cat "$T/owned-copy")" || return 1
  case "$c" in */skill-drive-probe-copy) ;; *) echo "ABORT: unexpected owned-copy path $c"; return 1 ;; esac
  if [ -L "$c" ]; then echo "ABORT: owned-copy path is a link: $c"; return 1; fi
  if [ -d "$c" ]; then
    if [ -e "$c/SKILL.md" ] || [ -L "$c/SKILL.md" ]; then
      [ -f "$c/SKILL.md" ] && [ ! -L "$c/SKILL.md" ] && grep -qF 'Temporary regular-folder copy for the my-skills macOS check.' "$c/SKILL.md" \
        || { echo "ABORT: probe file mismatch: $c/SKILL.md"; return 1; }
      rm -- "$c/SKILL.md" || return 1
    fi
    rm -f -- "$c/.SKILL.md.tmp"                         # 우리가 만든 폴더 안의 쓰다 만 임시 파일
    rm -f -- "$T/owned-copy.intent"
    rmdir -- "$c" || { echo "ABORT: $c is not empty"; return 1; }
  elif [ -e "$c" ]; then echo "ABORT: owned-copy path is not a directory: $c"; return 1
  fi
  rm -- "$T/owned-copy"
}
EOF
} > "$BASE/p.env" || exit 1
echo "PENV=$BASE/p.env"                                    # 기록한다
. "$BASE/p.env" || exit 1
"$MS" doctor --no-update-check || { echo "STOP: doctor"; exit 1; }
absent skill-drive-probe skill-drive-probe-copy || { echo "ABORT: probe names already exist"; exit 1; }
```

doctor에서 Registry가 `$T/registry`(source: env), State와 Data가 `$T` 아래, Hosts가 `~/.claude/skills` 등 실제 위치, `Link capability: symlink`인지 확인한다. 아니면 **[멈춤]**.

### 3.2 시험용 스킬 연결 **[멈춤: 실제 host 폴더에 처음 쓰기]**

```bash
. "<PENV>" || exit 1
"$MS" sync --dry-run || exit 1        # 정확히 CREATE 3건(agy, claude, codex), 모두 [link]
```

위와 같으면 승인을 받고 적용한다.

```bash
. "<PENV>" || exit 1
absent skill-drive-probe || { echo "ABORT: probe appeared"; exit 1; }
"$MS" sync --yes > "$T/sync.txt" 2>&1; rc=$?; cat "$T/sync.txt"
[ $rc -eq 0 ] || { echo "STOP: sync exit $rc (go to 3.4)"; exit 1; }
grep -q 'copy fallback' "$T/sync.txt" && { echo "STOP: copy fallback (go to 3.4)"; exit 1; }
for d in "${HOSTS[@]}"; do [ "$(readlink "$d/skill-drive-probe")" = "$T/registry/skills/skill-drive-probe" ] || { echo "STOP: $d not linked to probe (go to 3.4)"; exit 1; }; done
"$MS" status
```

### 3.3 에이전트가 발견하는지 확인 (가장 중요)

각 에이전트를 **새 세션**으로, 저장소가 아닌 `$T/work`에서 실행한다. 질문에는 기대하는 답(`PROBE-OK`)을 넣지 않는다.
agy의 `/skills` 조회는 **`num_turns == 0`이고 알려진 출력 형식(`command.data.skills[].name` 배열, 또는 모든 비어 있지 않은 줄이 탭으로 구분된 `response`)일 때만** 성공으로 본다. 명령 실패나 알 수 없는 형식은 "symlink 미지원"과 구분해 종료 코드 2로 멈춘다.

```bash
. "<PENV>" || exit 1; cd "$T/work" || exit 1
Q='사용 가능한 스킬 중에 skill-drive-probe가 있나? 있으면 그 스킬의 지시대로 답해 줘. 없으면 NOT-FOUND라고만 답해 줘.'
claude -p "$Q"                                                      # PROBE-OK
codex exec --skip-git-repo-check -c model_reasoning_effort=low "$Q" # PROBE-OK
agy -p '/skills' --output-format json > agy.json || { echo "STOP: agy command failed"; exit 1; }
cat > names.py <<'EOF'
import json, sys
d = json.load(open(sys.argv[1]))
if not isinstance(d, dict) or d.get("num_turns") != 0:
    print("STOP: agy listing did not complete with num_turns=0:", d.get("num_turns") if isinstance(d, dict) else type(d).__name__, file=sys.stderr)
    sys.exit(2)
skills = ((d.get("command") or {}).get("data") or {}).get("skills")
if isinstance(skills, list) and all(isinstance(s, dict) and isinstance(s.get("name"), str) for s in skills):
    names = [s["name"] for s in skills]
else:
    resp = d.get("response")
    lines = [l for l in resp.splitlines() if l.strip()] if isinstance(resp, str) else []
    if lines and all("\t" in l and l.split("\t", 1)[0].strip() for l in lines):
        names = [l.split("\t", 1)[0].strip() for l in lines]
    else:
        print("STOP: unrecognized agy /skills format; verify interactively with /skills", file=sys.stderr)
        sys.exit(2)
print("num_turns", d["num_turns"], "count", len(names))
for n in sys.argv[2:]:
    print(n, "listed" if n in names else "NOT listed")
EOF
python3 names.py agy.json skill-drive-probe || { echo "STOP: agy listing check failed; verify interactively with /skills"; exit 1; }
```

- Claude Code나 Codex가 `NOT-FOUND`라면, 그 에이전트 설정에서 사용자 스킬 폴더 위치를 확인한다. 위치가 다르면 3.4로 시험용 스킬을 먼저 해제하고, 임시 manifest의 `[targets.<host>]`에 `path = '...'`를 넣고, `HOSTS`도 같은 경로로 고친 뒤 3.2를 다시 한다.
- agy 명령이 실패하거나 `names.py`가 `STOP`으로 끝나면(`num_turns != 0` 또는 알 수 없는 형식), **copy 모드로 넘기지 말고** 설치된 agy 버전에서 대화형 `/skills`로 직접 확인한다.
- agy 조회가 정상 완료(`num_turns 0`, 알려진 형식)됐는데 `skill-drive-probe`가 `NOT listed`라면, 실제 적용 중인 agy 대상 폴더에 **일반 폴더 복사본**을 만들어 비교한다.

  ```bash
  . "<PENV>" || exit 1; cd "$T/work" || exit 1
  AGY_DIR="$("$MS" doctor --no-update-check --json | python3 -c 'import json,sys; print(next(h["path"] for h in json.load(sys.stdin)["hosts"] if h["name"]=="agy"))')" || { echo "ABORT: agy path"; exit 1; }
  [ -n "$AGY_DIR" ] && [ -d "$AGY_DIR" ] || { echo "ABORT: agy dir missing ($AGY_DIR)"; exit 1; }
  C="$AGY_DIR/skill-drive-probe-copy"
  absent skill-drive-probe-copy || exit 1
  [ ! -e "$C" ] && [ ! -L "$C" ] || { echo "ABORT: $C already exists"; exit 1; }
  printf '%s\n' "$C" > "$T/owned-copy.intent" || exit 1                  # "만들 예정"
  mkdir "$C" || { rm -f -- "$T/owned-copy.intent"; echo "STOP: mkdir failed; $C was not created by this probe, do not delete it"; exit 1; }
  mv -- "$T/owned-copy.intent" "$T/owned-copy" || { echo "STOP: could not record ownership (inspect by hand)"; exit 1; }   # "우리가 만들었음"
  printf -- '---\nname: skill-drive-probe-copy\ndescription: Temporary regular-folder copy for the my-skills macOS check.\n---\n\nRegular folder.\n' > "$C/.SKILL.md.tmp" \
    && mv -- "$C/.SKILL.md.tmp" "$C/SKILL.md" || { echo "STOP: write failed (run 3.4)"; exit 1; }
  rc=0
  agy -p '/skills' --output-format json > agy2.json && python3 names.py agy2.json skill-drive-probe-copy skill-drive-probe || rc=$?
  clean_rc=0; remove_owned_copy && echo "copy removed" || clean_rc=$?
  [ $rc -eq 0 ] || { echo "STOP: agy copy-control failed (exit $rc); verify interactively with /skills"; exit 1; }
  [ $clean_rc -eq 0 ] || { echo "STOP: copy-control cleanup failed"; exit 1; }
  ```

  복사본만 `listed`이고 symlink가 `NOT listed`일 때만 symlink 미지원으로 판단해 4단계에서 `[targets.agy] install_mode = "copy"`를 쓴다. 복사본도 안 나오면 경로나 캐시 문제다.
- 결과(에이전트별 발견 여부, agy 버전, 복사본 비교)를 기록하고 **[멈춤]**.

### 3.4 시험용 스킬 해제 (중간에 실패했을 때도 여기로)

```bash
. "<PENV>" || exit 1
fail=0
"$MS" uninstall skill-drive-probe --host all --yes || { echo "STOP: uninstall failed (exit $?)"; fail=1; }
if [ -f "$T/owned-copy" ] || [ -f "$T/owned-copy.intent" ]; then remove_owned_copy && echo "copy removed" || { echo "STOP: owned-copy cleanup failed"; fail=1; }; fi
absent skill-drive-probe skill-drive-probe-copy && echo "host folders clean" || { echo "STOP: entries left (record them; remove only verified probe links)"; fail=1; }
[ -f "$T/registry/skills/skill-drive-probe/SKILL.md" ] && echo "canonical kept" || { echo "STOP: canonical missing"; fail=1; }
[ $fail -eq 0 ] || exit 1
```

### 3.5 hook 확인

hook은 sync를 실행하므로 **host 경로를 임시 폴더로 돌리고, 시험용 저장소의 hook 폴더를 저장소 안의 새 빈 폴더로 고정한다.** 전역 `core.hooksPath`나 상위 Git 환경 변수(`GIT_COMMON_DIR` 등)를 물려받지 않게 먼저 비운다.

```bash
. "<PENV>" || exit 1; cd "$T/registry" || exit 1
cat > my-skills.local.toml <<EOF
[targets.claude]
path = '$T/hosts/claude'

[targets.codex]
path = '$T/hosts/codex'

[targets.agy]
path = '$T/hosts/agy'
EOF
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_COMMON_DIR GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX GIT_CONFIG_PARAMETERS GIT_CONFIG_COUNT
git init -q || exit 1
PHK="$(mktemp -d "$T/registry/.git/probe-hooks.XXXX")" || { echo "ABORT: probe hooks dir"; exit 1; }
git config core.hooksPath "$PHK" || exit 1
printf 'PHK=%q\n' "$PHK" >> "$BASE/p.env"
[ "$(git rev-parse --show-toplevel)" = "$T/registry" ] || { echo "ABORT: repo root"; exit 1; }
[ "$(git rev-parse --path-format=absolute --git-path hooks)" = "$PHK" ] || { echo "ABORT: hooks dir"; exit 1; }
[ -z "$(ls -A "$PHK")" ] || { echo "ABORT: probe hooks dir not empty"; exit 1; }
"$MS" doctor --no-update-check > "$T/doctor.txt" 2>&1 || { cat "$T/doctor.txt"; echo "STOP: doctor"; exit 1; }
cat "$T/doctor.txt"
"$MS" doctor --no-update-check --json > "$T/doctor.json" || { echo "STOP: doctor json"; exit 1; }
python3 - "$T/doctor.json" "$T" "$PHK" <<'EOF' || { echo "ABORT: isolation check failed"; exit 1; }
import json, os, sys
d = json.load(open(sys.argv[1]))
t, phk = sys.argv[2], sys.argv[3]
hosts = {h["name"]: h["path"] for h in d["hosts"]}
for h in ("claude", "codex", "agy"):
    assert os.path.realpath(hosts[h]) == os.path.realpath(os.path.join(t, "hosts", h)), (h, hosts[h])
assert os.path.realpath(d["state_path"]) == os.path.realpath(os.path.join(t, "state", "my-skills", "state.json")), d["state_path"]
assert os.path.realpath(d["data_root"]) == os.path.realpath(os.path.join(t, "data", "my-skills")), d["data_root"]
assert os.path.realpath(d["hooks"]["path"]) == os.path.realpath(phk), d["hooks"]
EOF
git add -A && git -c user.name=probe -c user.email=probe@example.invalid commit -qm probe || exit 1   # hook 설치 전 첫 커밋
```

```bash
. "<PENV>" || exit 1; cd "$T/registry" || exit 1
command -v my-skills                                               # 처음 잡히는 판을 기록 (Python 판일 수 있음)
printf 'export PATH=%q:"$PATH"\n' "$(dirname "$MS")" >> "$BASE/p.env"; . "$BASE/p.env" || exit 1
[ "$(sh -c 'command -v my-skills')" = "$MS" ] || { echo "ABORT: hooks would not find the Rust build"; exit 1; }
"$MS" hooks install --dry-run && "$MS" hooks install && "$MS" hooks status || exit 1   # Directory가 $PHK
```

pre-commit: 종료 코드가 0이 아니고, 이 스킬의 검증 메시지(`[BLOCKED] skill-drive-probe: ...`)가 있고, HEAD가 그대로여야 한다.

```bash
. "<PENV>" || exit 1; cd "$T/registry" || exit 1
H0="$(git rev-parse HEAD)"
printf -- '---\nname: Broken--Name\n---\n' > skills/skill-drive-probe/SKILL.md
git add -A; git -c user.name=probe -c user.email=probe@example.invalid commit -m broken > "$T/pre.txt" 2>&1; rc=$?; cat "$T/pre.txt"
git reset -q HEAD -- skills/skill-drive-probe/SKILL.md; git checkout -- skills/skill-drive-probe/SKILL.md
[ $rc -ne 0 ] && grep -q '\[BLOCKED\] skill-drive-probe:' "$T/pre.txt" && [ "$(git rev-parse HEAD)" = "$H0" ] && echo "pre-commit blocked (ok)" || { echo "STOP: pre-commit"; exit 1; }
```

post-checkout과 post-merge를 따로 확인한다. 각 단계에서 hook 출력에 `warning`이 없는지와 실제 링크 대상을 본다.

```bash
. "<PENV>" || exit 1; cd "$T/registry" || exit 1
L="$T/hosts/claude/skill-drive-probe-two"; WANT="$T/registry/skills/skill-drive-probe-two"
git switch -q -c side || exit 1
mkdir -p skills/skill-drive-probe-two
printf -- '---\nname: skill-drive-probe-two\ndescription: Second temporary probe.\n---\n\nBody.\n' > skills/skill-drive-probe-two/SKILL.md
printf '\n[skills.skill-drive-probe-two]\nenabled = true\n' >> my-skills.toml
git add -A && git -c user.name=probe -c user.email=probe@example.invalid commit -qm "add second probe" || exit 1
step() { local out; out="$(git "$@" 2>&1)" || { echo "$out"; echo "STOP: git $*"; exit 1; }; echo "$out"; echo "$out" | grep -qi 'warning' && { echo "STOP: hook warning"; exit 1; }; return 0; }
step switch -q -;    { [ -e "$L" ] || [ -L "$L" ]; } && { echo "STOP: present on base"; exit 1; }; echo "base: absent (ok)"
step switch -q side; [ "$(readlink "$L")" = "$WANT" ] || { echo "STOP: post-checkout"; exit 1; }; echo "post-checkout created (ok)"
step switch -q -;    { [ -e "$L" ] || [ -L "$L" ]; } && { echo "STOP: not pruned"; exit 1; }; echo "post-checkout pruned (ok)"
step merge -q --ff-only side; [ "$(readlink "$L")" = "$WANT" ] || { echo "STOP: post-merge"; exit 1; }; echo "post-merge created (ok)"
```

### 3.6 정리

```bash
. "<PENV>" || exit 1; cd "$HOME" || exit 1
[ ! -e "$T/owned-copy" ] && [ ! -e "$T/owned-copy.intent" ] || { echo "STOP: run 3.4 first (owned-copy record left)"; exit 1; }
absent skill-drive-probe skill-drive-probe-copy || { echo "STOP: run 3.4 first"; exit 1; }
case "$BASE" in /private/tmp/*|/private/var/folders/*|/tmp/*|/var/folders/*) rm -rf -- "$BASE" && echo "removed $BASE" || { echo "STOP: rm failed"; exit 1; };; *) echo "ABORT: not removing $BASE"; exit 1;; esac
```

실제 host 폴더에 시험용 이름이 남아 있으면 임시 폴더를 지우지 않는다(지우면 대상이 없는 symlink가 남는다). 해결하지 못한 **[멈춤]**이 있으면 4단계로 가지 않는다.

---

## 4. 이 맥의 전환

4단계의 실제 명령은 **`ms`(= `MY_SKILLS_ROOT="$REG" my-skills`)로만** 실행한다. 현재 폴더나 물려받은 환경 때문에 다른 레지스트리를 잡지 않게 하기 위해서다.

### 4.1 점검과 계획 **[멈춤: 계획 승인]**

```bash
. "<GENV>" || exit 1; guard_dirs || exit 1
BR="$(git -C "$REG" symbolic-ref --short HEAD)" || { echo "ABORT: detached HEAD"; exit 1; }
DEF="$(git -C "$REG" symbolic-ref --quiet --short refs/remotes/origin/HEAD 2>/dev/null | sed 's|^origin/||')"; DEF="${DEF:-main}"
[ "$BR" = "$DEF" ] || { echo "ABORT: branch $BR != default $DEF"; exit 1; }
[ -z "$(git -C "$REG" status --porcelain)" ] || { echo "ABORT: registry not clean"; exit 1; }
START="$(git -C "$REG" rev-parse HEAD)" || exit 1
git -C "$REG" fetch || exit 1
TARGET="$(git -C "$REG" rev-parse @{u})" || exit 1
echo "branch: $BR (default: $DEF)  START: $START  TARGET: $TARGET"
git -C "$REG" log --oneline "$START..$TARGET"
HK="$(git -C "$REG" rev-parse --path-format=absolute --git-path hooks)" || exit 1
OWN_HK="$(git -C "$REG" rev-parse --path-format=absolute --git-dir)/hooks" || exit 1
echo "hooks dir: $HK (registry-owned: $OWN_HK)"
[ "$HK" = "$OWN_HK" ] || echo "NOTE: effective hooks dir differs from registry-owned .git/hooks; decide before 4.4"
ls "$HK" 2>/dev/null | grep -v '\.sample$' || true
cat "$REG/my-skills.local.toml" 2>/dev/null || true
mkdir -p "$BACKUP" || { echo "ABORT: backup dir"; exit 1; }
if [ -e "$BACKUP/state.json.before" ] || [ -e "$BACKUP/none" ]; then
  echo "state backup already exists: $BACKUP"
elif [ -e "$STATE" ] || [ -L "$STATE" ]; then
  # 임시 파일에 복사한 뒤 성공했을 때만 이름을 바꾼다 (끊긴 복사가 백업으로 남지 않게)
  rm -f -- "$BACKUP/state.json.tmp"
  cp -p "$STATE" "$BACKUP/state.json.tmp" && mv -- "$BACKUP/state.json.tmp" "$BACKUP/state.json.before" \
    || { rm -f -- "$BACKUP/state.json.tmp"; echo "ABORT: state backup copy failed"; exit 1; }
else
  echo "no state file at $STATE" > "$BACKUP/none.tmp" && mv -- "$BACKUP/none.tmp" "$BACKUP/none" || { echo "ABORT: state backup marker failed"; exit 1; }
fi
echo "state backup: $BACKUP"; ls -la "$BACKUP"
```

사용자에게 다음 계획을 보여 주고 승인을 받는다.
1. `rcargo install --locked --root "$INSTALL_ROOT" --path "$TOOL"`. `INSTALL_ROOT`는 **my-skills 전용 폴더**로 정한다(예: `$HOME/.local/opt/my-skills`). `${CARGO_HOME:-$HOME/.cargo}`처럼 rustup proxy나 다른 binary가 있는 폴더, Python 판이 있는 폴더에는 설치하지 않는다. 정한 `INSTALL_ROOT`와 기존 `cargo install --list`, `CARGO_INSTALL_ROOT`/`CARGO_HOME`을 함께 보여 준다.
2. PATH: 셸 설정에 **`$INSTALL_ROOT/bin` 한 폴더만** Python 판 폴더보다 앞에 넣는다. Python 판 폴더를 PATH에 넣는 설정 줄보다 나중에 실행되는 곳에 `export PATH="<INSTALL_ROOT>/bin:$PATH"` 한 줄을 둔다. `~/.cargo/bin`을 PATH 앞에 넣지 않는다(그러면 다른 프로젝트의 cargo와 rustc까지 rustup 쪽으로 바뀐다). Python 판은 지우지 않는다. 고칠 셸 설정 파일과 추가할 줄을 보여 준다.
3. 레지스트리의 깨끗한 기본 브랜치(`$BR`, 시작 HEAD `$START`)를 위에서 본 `$TARGET` SHA로 빨리 감기. **기존 hook은 실행하지 않는다**(`-c core.hooksPath=/dev/null`). 또한 4.4에서 hook을 설치할 위치가 레지스트리 소유 `$OWN_HK`인지 확인한다(전역 `core.hooksPath`로 다른 폴더를 가리키면 레지스트리 로컬 `core.hooksPath` 고정 여부를 함께 승인받는다).
4. `$REG/my-skills.local.toml`(git에서 무시됨)에 넣을 내용과 `AGY_MODE`(`link` 또는 `copy`). 기존 내용은 보존하고 합친다.

   ```toml
   [defaults]
   install_mode = "link"
   ```

   3.3에서 정상 조회와 복사본 비교로 agy symlink 미지원을 확인했다면 `[targets.agy]`에 `install_mode = "copy"`도 넣고 `AGY_MODE=copy`로 기록한다(그 외는 `AGY_MODE=link`).
   이 맥 전용 기본값이 필요한 이유: 레지스트리 manifest는 5단계 전까지 `copy`다. 그 상태로 bootstrap하면 새 연결이 copy로 생기고, 나중에 기본값을 바꿔도 기존 연결은 바뀌지 않는다.
   manifest나 overlay에 `[targets.<host>] install_mode`가 이미 있으면 그 값이 기본값보다 우선한다. 보여 주고 확인한다.
5. 4.3 dry-run 결과를 보고 다시 승인을 받은 뒤 4.4를 실행한다

### 4.2 승인된 준비

```bash
. "<GENV>" || exit 1; guard_dirs || exit 1
INSTALL_ROOT="<4.1에서 승인한 my-skills 전용 폴더>"   # 예: $HOME/.local/opt/my-skills
case "$INSTALL_ROOT" in /*) ;; *) echo "ABORT: INSTALL_ROOT must be absolute"; exit 1;; esac
rcargo install --locked --root "$INSTALL_ROOT" --path "$TOOL" || exit 1
# PATH에 넣을 폴더에는 my-skills 하나만 있어야 한다 (rustup proxy 등이 함께 노출되지 않게)
[ "$(ls -A "$INSTALL_ROOT/bin")" = "my-skills" ] || { echo "STOP: $INSTALL_ROOT/bin has other files:"; ls -A "$INSTALL_ROOT/bin"; exit 1; }
MSBIN="$INSTALL_ROOT/bin/my-skills"; "$MSBIN" --version || exit 1
printf 'INSTALL_ROOT=%q\nMSBIN=%q\n' "$INSTALL_ROOT" "$MSBIN" >> "$GDIR/g.env"   # 이후 ms가 이 binary를 쓴다
```

**새 bash를 열어** 확인한다. `bash`와 `sh -c` 모두 위의 `$MSBIN`(같은 실제 파일과 버전)을 찾아야 한다. hook은 PATH로 `my-skills`를 찾기 때문이다. 실패하면 승인받은 PATH 변경을 한 뒤 **새 터미널에서** 다시 실행해 통과시킨다. 이미 실행 중인 터미널과 에이전트 세션은 예전 PATH를 쓰므로, 레지스트리에서 git을 실행할 세션은 새로 연다.

```bash
. "<GENV>" || exit 1
WANT_VER="$("$MSBIN" --version)" || exit 1
WANT_REAL="$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$MSBIN")" || exit 1
check_bin() {
  local label="$1" found="$2" real ver
  [ -n "$found" ] || { echo "STOP: my-skills not on PATH ($label)"; exit 1; }
  real="$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$found")" || exit 1
  ver="$("$found" --version)" || exit 1
  [ "$real" = "$WANT_REAL" ] && [ "$ver" = "$WANT_VER" ] || { echo "STOP: $label -> $found ($ver), want $MSBIN ($WANT_VER)"; exit 1; }
}
check_bin "bash" "$(command -v my-skills)"
check_bin "sh" "$(sh -c 'command -v my-skills')"
echo "PATH verified: $MSBIN ($WANT_VER)"
```

```bash
. "<GENV>" || exit 1; guard_dirs || exit 1
BR_WANT="<4.1에서 본 기본 브랜치>"; START="<4.1에서 본 START SHA>"; TARGET="<4.1에서 본 TARGET SHA>"
AGY_MODE="<link 또는 copy>"                         # 4.1에서 승인한 값
case "$AGY_MODE" in link|copy) ;; *) echo "ABORT: invalid AGY_MODE=$AGY_MODE"; exit 1;; esac
[ "$(git -C "$REG" symbolic-ref --short HEAD)" = "$BR_WANT" ] || { echo "ABORT: branch changed"; exit 1; }
[ "$(git -C "$REG" rev-parse HEAD)" = "$START" ] || { echo "ABORT: starting HEAD changed"; exit 1; }
[ -z "$(git -C "$REG" status --porcelain)" ] || { echo "ABORT: registry not clean"; exit 1; }
git -C "$REG" -c core.hooksPath=/dev/null merge --ff-only "$TARGET" || exit 1
[ "$(git -C "$REG" rev-parse HEAD)" = "$TARGET" ] || { echo "STOP: unexpected HEAD after merge"; exit 1; }
[ -z "$(git -C "$REG" status --porcelain)" ] || { echo "STOP: registry not clean"; exit 1; }
# 승인한 내용으로 $REG/my-skills.local.toml을 쓴다 (기존 내용 보존)
cat "$REG/my-skills.local.toml" || exit 1
[ -z "$(git -C "$REG" status --porcelain)" ] && echo "overlay ignored by git (ok)" || { echo "STOP: overlay not ignored"; exit 1; }
printf 'AGY_MODE=%q\n' "$AGY_MODE" >> "$GDIR/g.env"
```

4.1에서 hook 폴더가 레지스트리 소유 `.git/hooks`가 아니었고(전역 `core.hooksPath` 등) 레지스트리 로컬 고정을 승인받았다면, 여기서 고정한다. 그 밖의 경우에는 이 블록을 건너뛴다.

```bash
. "<GENV>" || exit 1; guard_dirs || exit 1
GITDIR="$(git -C "$REG" rev-parse --path-format=absolute --git-dir)" || exit 1; OWN_HK="$GITDIR/hooks"
[ -L "$OWN_HK" ] && { echo "ABORT: $OWN_HK is a symlink; decide with the user"; exit 1; }
mkdir -p "$OWN_HK" && git -C "$REG" config --local core.hooksPath "$OWN_HK" || exit 1
[ "$(git -C "$REG" rev-parse --path-format=absolute --git-path hooks)" = "$OWN_HK" ] && echo "registry hooks dir pinned: $OWN_HK" || { echo "STOP: pin failed"; exit 1; }
```

GUI git 클라이언트로 레지스트리를 다룬다면 그 클라이언트의 PATH도 Rust 판을 찾는지 기록한다.

### 4.3 dry-run **[멈춤: 사용자 승인]**

```bash
. "<GENV>" || exit 1; guard_dirs || exit 1; cd "$HOME" || exit 1
ms doctor --no-update-check || exit 1
ms doctor --no-update-check --json > "$GDIR/doctor.before.json" || exit 1
OWN_HK="$(git -C "$REG" rev-parse --path-format=absolute --git-dir)/hooks" || exit 1
python3 - "$GDIR/doctor.before.json" "$REG" "$STATE" "$OWN_HK" <<'EOF' || { echo "STOP: doctor path/hooks precondition failed"; exit 1; }
import json, os, sys
d = json.load(open(sys.argv[1]))
reg, state, own_hk = sys.argv[2:5]
assert os.path.realpath(d["registry"]["path"]) == os.path.realpath(reg), d["registry"]
assert os.path.realpath(d["state_path"]) == os.path.realpath(state), (d["state_path"], state)
assert d.get("hooks") and os.path.realpath(d["hooks"]["path"]) == os.path.realpath(own_hk), (d.get("hooks"), own_hk)
print("preconditions ok: state =", d["state_path"], "hooks =", d["hooks"]["path"])
EOF
ms status --json > "$GDIR/status.before.json" || exit 1
ms convert --to link --host all --dry-run || exit 1
if [ "${AGY_MODE:?set AGY_MODE in 4.2}" = "copy" ]; then
  ms convert --to copy --host agy --dry-run || exit 1
fi
ms sync --dry-run || exit 1
ms hooks install --dry-run || exit 1
```

- 실제 계획은 `convert --dry-run`과 `sync --dry-run`이다. `bootstrap --dry-run`은 요약일 뿐이고, 거기의 `then: my-skills hooks install` 줄은 bootstrap이 실제로 하는 일이 아니다(bootstrap은 hook을 설치하지 않는다).
- 모든 동작 종류를 센다: CONVERT, CREATE, UPDATE, REMOVE, NOOP, BLOCK 계열, `skipped host`.

| dry-run 결과 | 처리 |
|---|---|
| CONVERT, CREATE가 모두 `[link]`(`AGY_MODE=copy`라면 agy만 태그 없음), BLOCK 없음 | 승인을 받으면 4.4로 |
| UPDATE, REMOVE | 항목마다 이유를 보여 주고 승인을 받는다(REMOVE는 sync의 기본 해제 동작이다) |
| `CONFLICT`, `BLOCK_CONFLICT`, `BLOCK_DRIFT` | 종료 코드가 0이어도 **진행하지 않는다.** 처리 방법은 사용자가 정한다 |
| `AGY_MODE=copy`이고 agy에 기존 link 연결이 있음 | 위에서 출력한 `ms convert --to copy --host agy --dry-run` 결과를 함께 승인받는다(4.4의 `copy)` 분기에서 실행) |

### 4.4 전환

각 명령이 실패하거나 `copy fallback`이 나오면 그 자리에서 멈춘다. 적용 전에 백업 존재, `sh` PATH의 `my-skills`, 레지스트리 소유 hook 폴더를 다시 확인하고, `set-root` 직후에는 중립 폴더(`$GDIR`)에서 `MY_SKILLS_ROOT` 없이 캐시된 루트와 state 경로를 검증한다.

```bash
. "<GENV>" || exit 1; guard_dirs || exit 1; cd "$HOME" || exit 1
[ -f "$BACKUP/state.json.before" ] || [ -f "$BACKUP/none" ] || { echo "ABORT: run 4.1 backup first"; exit 1; }
SH_BIN="$(sh -c 'command -v my-skills')" || { echo "STOP: my-skills not on sh PATH"; exit 1; }
python3 - "$MSBIN" "$SH_BIN" <<'EOF' || { echo "STOP: sh PATH does not point to MSBIN"; exit 1; }
import os, subprocess, sys
want, got = sys.argv[1], sys.argv[2]
assert os.path.realpath(want) == os.path.realpath(got), (want, got)
assert subprocess.check_output([want, "--version"], text=True).strip() == subprocess.check_output([got, "--version"], text=True).strip()
EOF
HK="$(git -C "$REG" rev-parse --path-format=absolute --git-path hooks)" || exit 1
OWN_HK="$(git -C "$REG" rev-parse --path-format=absolute --git-dir)/hooks" || exit 1
GITDIR="$(git -C "$REG" rev-parse --path-format=absolute --git-dir)" || exit 1
python3 - "$HK" "$OWN_HK" "$GITDIR" <<'EOF' || { echo "STOP: hooks dir ($HK) is not the registry's own .git/hooks"; exit 1; }
import os, sys
hk, own, gitdir = sys.argv[1:]
# 실제 폴더여야 하고(공용 폴더로 가는 symlink가 아님), 레지스트리의 git 폴더 안에 있어야 한다
assert not os.path.islink(own), ("hooks dir is a symlink", own)
assert os.path.realpath(hk) == os.path.realpath(own), (hk, own)
assert os.path.realpath(own).startswith(os.path.realpath(gitdir) + os.sep), (own, gitdir)
EOF
run() { local out rc; out="$("$@" 2>&1)"; rc=$?; echo "$out"; [ $rc -eq 0 ] || { echo "STOP: $* (exit $rc)"; exit 1; }; echo "$out" | grep -q 'copy fallback' && { echo "STOP: copy fallback"; exit 1; }; return 0; }
run ms set-root "$REG"
python3 - "$GDIR" "$REG" "$STATE" "$ROOTCACHE" "$MSBIN" "$CFG_HOME" "$STH_HOME" <<'EOF' || { echo "STOP: root cache or state path mismatch"; exit 1; }
import json, os, subprocess, sys
gdir, reg, state, rootcache, msbin, cfg_home, sth_home = sys.argv[1:]
env = {k: v for k, v in os.environ.items() if k != "MY_SKILLS_ROOT"}
env.update(XDG_CONFIG_HOME=cfg_home, XDG_STATE_HOME=sth_home)   # ms와 같은 위치
d = json.loads(subprocess.check_output([msbin, "doctor", "--no-update-check", "--json"], cwd=gdir, env=env, text=True))
reg_real = os.path.realpath(reg)
assert d["registry"]["source"] == "cache", d["registry"]
assert os.path.realpath(d["registry"]["path"]) == reg_real, d["registry"]
assert os.path.realpath(d["state_path"]) == os.path.realpath(state), (d["state_path"], state)
assert os.path.realpath(open(rootcache).read().strip()) == reg_real, rootcache
print("verified cache:", d["registry"]["path"], "state:", d["state_path"])
EOF
run ms convert --to link --host all --yes            # AGY_MODE=copy라면 "skipped host agy"
case "${AGY_MODE:?set AGY_MODE in 4.2}" in
  link) ;;
  copy) run ms convert --to copy --host agy --yes ;;
  *) echo "ABORT: invalid AGY_MODE=$AGY_MODE"; exit 1 ;;
esac
run ms bootstrap                                      # 남은 연결. hook은 설치하지 않는다
run ms hooks install; run ms hooks status
run ms sync --check
ms status --json > "$GDIR/status.after.json" || exit 1
python3 - "$GDIR/status.after.json" <<'EOF'
import json, sys, collections
rows = json.load(open(sys.argv[1]))     # status --json은 항목 배열
c = collections.Counter((r["host"], r["status"], r["mode"], r["link_kind"], r["fallback_reason"]) for r in rows)
for k, v in sorted(c.items(), key=str): print(v, k)
EOF
```

- 기대: 연결해야 하는 항목은 모두 `FRESH`, mode `link`, link_kind `symlink`, fallback 없음(`AGY_MODE=copy`라면 agy는 `copy`). host를 제외한 스킬의 `UNSUPPORTED`와 비활성 스킬은 실패가 아니다.
- 비활성 host(hermes) 기록: 4.1의 백업(`$BACKUP/state.json.before`)과 지금의 `$STATE`에서 hermes 항목을 비교해 그대로인지 확인한다(일반 `status`에는 나오지 않는다).
- 문제가 있으면 멈추고, `$BACKUP` 백업을 근거로 사용자와 복구 방법을 정한다.

### 4.5 link가 바로 반영되는지 (스펙 §11) **[멈춤: 임시 수정 승인]**

추적 중인 일반 파일(`SKILL.md`)인지 먼저 확인하고, `doctor --json`이 보고하는 실제 host 경로로 Claude·Codex(및 `AGY_MODE=link`일 때 agy) 링크를 각각 검증한다. 실패하더라도 원본을 복구하고 복구 결과를 확인한 뒤 실패 코드를 반환한다.

```bash
. "<GENV>" || exit 1; guard_dirs || exit 1
S="<link로 연결된 스킬 하나>"; F="$REG/skills/$S/SKILL.md"; M="<!-- link check $(date +%s) -->"
git -C "$REG" ls-files --error-unmatch -- "skills/$S/SKILL.md" >/dev/null || { echo "ABORT: canonical file not tracked"; exit 1; }
[ -f "$F" ] && [ ! -L "$F" ] || { echo "ABORT: canonical file is not a regular file"; exit 1; }
git -C "$REG" diff --quiet -- "skills/$S/SKILL.md" && git -C "$REG" diff --cached --quiet -- "skills/$S/SKILL.md" || { echo "ABORT: file not clean"; exit 1; }
REQ_HOSTS=(claude codex); [ "${AGY_MODE:?set AGY_MODE in 4.2}" = "link" ] && REQ_HOSTS+=(agy)
ms doctor --no-update-check --json > "$GDIR/doctor.linkcheck.json" || exit 1
CANON_DIR="$(cd "$REG/skills/$S" && pwd -P)" || exit 1
LINKS=(); LINK_NAMES=()
for h in "${REQ_HOSTS[@]}"; do
  d="$(python3 -c 'import json,sys; print(next(x["path"] for x in json.load(open(sys.argv[1]))["hosts"] if x["name"]==sys.argv[2]))' "$GDIR/doctor.linkcheck.json" "$h")" || { echo "ABORT: host path lookup failed ($h)"; exit 1; }
  [ -L "$d/$S" ] || { echo "ABORT: $h missing symlink at $d/$S"; exit 1; }
  [ "$(cd "$d/$S" && pwd -P)" = "$CANON_DIR" ] || { echo "ABORT: $h ($d/$S) points elsewhere"; exit 1; }
  LINKS+=("$d/$S"); LINK_NAMES+=("$h")
done
H_BEFORE="$(git -C "$REG" hash-object "$F")" || exit 1
fail=0
if echo "$M" >> "$F"; then
  for i in "${!LINKS[@]}"; do
    l="${LINKS[$i]}"; h="${LINK_NAMES[$i]}"
    if [ "$(tail -1 "$l/SKILL.md")" = "$M" ]; then echo "reflected: $h ($l)"; else echo "NOT reflected: $h ($l)"; fail=1; fi
  done
else
  echo "STOP: append failed"; fail=1
fi
git -C "$REG" checkout -- "skills/$S/SKILL.md" || { echo "STOP: restore failed"; exit 1; }
[ "$(git -C "$REG" hash-object "$F")" = "$H_BEFORE" ] && git -C "$REG" diff --quiet -- "skills/$S/SKILL.md" && [ -z "$(git -C "$REG" status --porcelain)" ] || { echo "STOP: restore verification failed"; exit 1; }
echo "restored (verified)"
[ $fail -eq 0 ] || { echo "STOP: marker NOT reflected on one or more hosts"; exit 1; }
```

### 4.6 전환 뒤 발견 확인

```bash
. "<GENV>" || exit 1; S="<4.5의 스킬>"; W="$(mktemp -d "$GDIR/work.XXXX")" || exit 1
rc=0
(
  cd "$W" || exit 1
  Q="사용 가능한 스킬 중에 이름이 $S 인 스킬이 있나? 있으면 그 스킬 description의 첫 단어만 그대로 답하고, 없으면 NOT-FOUND라고만 답해."
  # 명령이 성공해도 답이 NOT-FOUND이면 실패로 본다
  out="$(claude -p "$Q")" || rc=1; echo "claude: $out"; case "$out" in *NOT-FOUND*|"") echo "STOP: claude did not find $S"; rc=1;; esac
  out="$(codex exec --skip-git-repo-check -c model_reasoning_effort=low "$Q")" || rc=1; echo "codex: $(printf '%s' "$out" | tail -1)"
  case "$out" in *NOT-FOUND*|"") echo "STOP: codex did not find $S"; rc=1;; esac
  {
    agy -p '/skills' --output-format json > agy.json && python3 - agy.json "$S" <<'EOF'
import json, sys
d = json.load(open(sys.argv[1]))
if not isinstance(d, dict) or d.get("num_turns") != 0:
    print("STOP: agy listing did not complete with num_turns=0:", d.get("num_turns") if isinstance(d, dict) else type(d).__name__, file=sys.stderr)
    sys.exit(2)
skills = ((d.get("command") or {}).get("data") or {}).get("skills")
if isinstance(skills, list) and all(isinstance(s, dict) and isinstance(s.get("name"), str) for s in skills):
    names = [s["name"] for s in skills]
else:
    resp = d.get("response")
    lines = [l for l in resp.splitlines() if l.strip()] if isinstance(resp, str) else []
    if lines and all("\t" in l and l.split("\t", 1)[0].strip() for l in lines):
        names = [l.split("\t", 1)[0].strip() for l in lines]
    else:
        print("STOP: unrecognized agy /skills format; verify interactively with /skills", file=sys.stderr)
        sys.exit(2)
listed = sys.argv[2] in names
print("num_turns", d["num_turns"], sys.argv[2], "listed" if listed else "NOT listed")
sys.exit(0 if listed else 1)        # 인식된 목록에 없으면 실패
EOF
  } || rc=1
  exit $rc
) || rc=$?
cd "$HOME" && rm -rf -- "$W" || { echo "STOP: cleanup $W failed"; exit 1; }
[ $rc -eq 0 ] || { echo "STOP: post-migration discovery failed (exit $rc)"; exit 1; }
```

---

## 5. 공유 레지스트리 변경 **[멈춤: diff 승인]**

이 단계의 커밋과 push는 **Windows PC에도 간다.** Windows PC는 기기별 설정이 우선이라 동작이 바뀌지 않고, pull할 때 hook이 sync를 실행한다.

1. manifest: `my-skills.toml`의 `[defaults] install_mode = "copy"`를 `"link"`로 바꾼다.
2. 레지스트리의 에이전트 안내 문서(인계 문서 §7 G): `skills/my-skills/SKILL.md`, `AGENTS.md`(동기화 단계, agy 복사 설치 문구, 그룹 비활성화 때 uninstall 안내), `instructions/global.md`, `README.md`.
   `skills/my-skills/SKILL.md`는 tool repo의 seed `skills/my-skills/SKILL.md`(`dev`)와 비교해 맞춘다. 고칠 곳 찾기:

   ```bash
   . "<GENV>" || exit 1; guard_dirs || exit 1; cd "$REG" || exit 1
   grep -rnE "uv tool|install_mode|copy|sync" AGENTS.md instructions README.md skills/my-skills
   ```

   Rust 판 명령으로 쓰고, host별 `install_mode`와 Windows의 agy copy 예외를 적는다.
3. 검사, 명시적 스테이징, 전체 diff·push 대상 승인:

   ```bash
   . "<GENV>" || exit 1; guard_dirs || exit 1; cd "$REG" || exit 1
   ms validate || exit 1
   FILES=(my-skills.toml skills/my-skills/SKILL.md AGENTS.md instructions/global.md README.md)  # 실제로 고친 파일만 명시
   git add -- "${FILES[@]}" || exit 1
   git diff --quiet || { echo "ABORT: unstaged changes remain"; exit 1; }
   [ -z "$(git ls-files --others --exclude-standard)" ] || { echo "ABORT: untracked files"; exit 1; }
   UP="$(git rev-parse --abbrev-ref @{u})" || exit 1; REMOTE="${UP%%/*}"; DEST="${UP#*/}"
   # 검토 기록: 커밋 직전에 이 값들이 그대로인지 다시 확인한다
   printf 'R_TREE=%q\nR_HEAD=%q\nR_BRANCH=%q\nR_REMOTE=%q\nR_DEST=%q\nR_PUSHURL=%q\n' "$(git write-tree)" "$(git rev-parse HEAD)" \
     "$(git symbolic-ref --short HEAD)" "$REMOTE" "$DEST" "$(git remote get-url --push --all "$REMOTE")" > "$GDIR/review.env" || exit 1
   [ "$(git remote get-url --push --all "$REMOTE" | wc -l)" -eq 1 ] || { echo "ABORT: $REMOTE has several push URLs; decide with the user"; exit 1; }
   cat "$GDIR/review.env"                             # push 대상(remote, push URL, 브랜치)을 사용자에게 보여 준다
   git diff --cached                                  # 스테이징된 diff 전체를 사용자에게 보여 주고 승인을 받는다
   ```

   승인 뒤 커밋과 push. 검토한 트리, HEAD, 브랜치, push 대상이 그대로일 때만 진행하고, push는 그 대상을 명시한다(pre-commit hook이 검사한다):

   ```bash
   . "<GENV>" || exit 1; guard_dirs || exit 1; cd "$REG" || exit 1; . "$GDIR/review.env" || exit 1
   [ "$(git write-tree)" = "$R_TREE" ] || { echo "ABORT: staged tree changed since review"; exit 1; }
   [ "$(git rev-parse HEAD)" = "$R_HEAD" ] && [ "$(git symbolic-ref --short HEAD)" = "$R_BRANCH" ] || { echo "ABORT: HEAD or branch changed"; exit 1; }
   [ "$(git remote get-url --push --all "$R_REMOTE")" = "$R_PUSHURL" ] || { echo "ABORT: push URLs changed"; exit 1; }
   git diff --quiet && [ -z "$(git ls-files --others --exclude-standard)" ] || { echo "ABORT: unreviewed changes"; exit 1; }
   git commit -m "<메시지>" || { echo "STOP: commit failed (pre-commit?)"; exit 1; }
   [ "$(git rev-parse HEAD^{tree})" = "$R_TREE" ] || { echo "STOP: committed tree differs from review"; exit 1; }
   git push "$R_REMOTE" "HEAD:refs/heads/$R_DEST" || { echo "STOP: push failed"; exit 1; }
   echo "pushed SHA: $(git rev-parse HEAD)"           # 이 SHA를 Windows PC에 알린다
   ```
4. 이 맥의 전역 안내에서 동기화 절차를 찾는다. 대상은 **각 에이전트의 최상위 안내 파일과, 그 파일이 `@경로`로 불러오는 파일만**이다.
   host 스킬 폴더(`~/.claude/skills`, `~/.agents/skills`, `~/.gemini/config/skills`)의 사본, 프로젝트 메모리, 세션 기록은 대상이 아니다. host 사본은 설치 결과물이라 직접 고치지 않는다.
   에이전트 설정 폴더 전체를 검색하면 결과의 대부분이 이런 파일이다.

   ```bash
   . "<GENV>" || exit 1
   CX="${CODEX_HOME:-$HOME/.codex}"
   python3 - ~/.claude/CLAUDE.md "$CX/AGENTS.md" "$CX/AGENTS.override.md" ~/.gemini/GEMINI.md <<'EOF'
   import os, re, sys
   queue = [p for p in sys.argv[1:] if os.path.isfile(p)]
   seen = set()
   while queue:
       p = os.path.realpath(queue.pop(0))
       if p in seen:
           continue
       seen.add(p)
       text = open(p, encoding="utf-8", errors="replace").read()
       for m in re.finditer(r"(?:^|\s)@(\S+\.md)", text):    # @경로 import (줄 맨 앞 또는 공백 뒤)
           q = os.path.expanduser(m.group(1))
           q = q if os.path.isabs(q) else os.path.join(os.path.dirname(p), q)
           if os.path.isfile(q):
               queue.append(q)
       print("MATCH" if "my-skills" in text else "-    ", p)
   EOF
   ```

   `MATCH`인 파일만 diff를 보여 주고 승인을 받아 고친다. **이 파일들은 레지스트리나 tool repo에 커밋하지 않는다.** Windows PC의 전역 안내는 Windows 쪽에서 따로 고친다.
5. 상태 재확인:

   ```bash
   . "<GENV>" || exit 1; guard_dirs || exit 1; cd "$HOME" || exit 1
   ms sync --check || exit 1
   ms status || exit 1
   ```
6. (선택) 6.2를 하려면 시험용 브랜치의 생성, push, 삭제도 **여기서 함께 승인**을 받는다.

---

## 6. 두 기기 확인 (Windows PC와 함께)

5단계의 push가 끝나면 Windows PC 쪽에 **push한 커밋 SHA**와 함께 알린다. Windows 쪽 작업은 거기서 일하는 에이전트가 한다.

### 6.1 post-merge (필수)

Windows에서:
- 레지스트리가 깨끗한 `main`인지, Git Bash에서 `sh -c 'command -v my-skills && my-skills --version'`이 Rust 판인지, 기기별 설정(`[defaults] link`, `[targets.agy] copy`)이 그대로인지 먼저 확인한다.
- 먼저 `git fetch origin`으로 원격만 받아서 `origin/main`의 SHA가 맥에서 알려 준 SHA와 정확히 같은지, 그리고 `git merge-base --is-ancestor HEAD <SHA>`로 fast-forward 관계인지 검증한다(더 새 리비전에 hook이 먼저 실행되지 않게 막는다).
- 검증이 통과한 뒤에만 `git merge --ff-only <SHA>`를 실행해 hook을 동작시키고, `HEAD`가 `<SHA>`인지와 post-merge hook의 sync 출력을 기록한다(hook이 실패해도 git은 성공으로 끝날 수 있으니 출력을 직접 본다).
- link 연결에는 바뀐 내용이 바로 보이고, agy copy는 `updated: ... -> agy`로 갱신돼야 한다. `my-skills status`가 모두 FRESH, `sync --check`가 0인지 확인한다.

### 6.2 새 스킬 (선택, 스펙 §11 마지막 항목)

공유 `main`과 양쪽 실제 host 폴더를 건드리지 않도록 **양쪽 모두 일회용 clone과 격리된 환경**에서 한다. 5.6에서 승인을 받은 경우에만 한다. 전역 `core.hooksPath`가 초기 checkout 때 실행되지 않도록 **양쪽 모두 첫 `clone`과 초기 브랜치 전환은 `git -c core.hooksPath=/dev/null ...`로 실행한다.**
- 보내는 쪽: 일회용 폴더에 `git -c core.hooksPath=/dev/null clone ...`한 뒤 고유한 이름의 브랜치(예: `probe/cross-device-<날짜>`)를 만들고, 시험용 스킬 하나를 manifest와 함께 커밋해 push한다. **브랜치 생성, commit, push 모두** `git -c core.hooksPath=/dev/null ...`로 실행한다(또는 clone 직후 clone 안의 새 빈 폴더로 `core.hooksPath`를 고정하고 확인한 뒤 진행한다). 실제 레지스트리 checkout은 쓰지 않는다.
- 받는 쪽: 임시 폴더에 `git -c core.hooksPath=/dev/null clone ...`한다. 3.5처럼 **모든 활성 host 경로와 state, config, data 위치**를 임시 폴더로 돌리고(Windows는 `LOCALAPPDATA` 포함) `doctor --json`으로 격리를 확인한 뒤, clone 안에 새 빈 hook 폴더를 만들고 로컬 `git config core.hooksPath <clone 안의 빈 폴더>`로 고정한 다음 `hooks install`과 `hooks status`를 확인한다. 그 뒤 `git fetch origin && git merge --ff-only origin/<브랜치>` → post-merge가 임시 host 폴더에 연결하는지 확인 → 임시 폴더를 지운다.
- 보내는 쪽: 원격 브랜치를 지우고, 양쪽 실제 host 폴더에 시험용 스킬이 없는지 확인한다.
- 맥 체크리스트(스펙 §11)를 채우려면 **Windows에서 보내고 맥에서 받는** 방향으로 한다.
- 하지 않았다면 스펙 §11의 그 항목을 열어 두고, 7단계에 "보류"와 G 완료에 미치는 영향을 적는다.

---

## 7. 결과 기록

스펙 §11 체크리스트를 채우고 그 아래에 다음 표를 넣는다. **공개 저장소다.** 사용자 이름, 실제 스킬 이름, 전체 경로, 전역 안내 내용, 기기의 실제 연결·기록 수, 전역 안내 파일의 줄 번호는 쓰지 않는다. 경로는 `~/...` 형태로 적는다.

```markdown
**맥 실기기 (YYYY-MM-DD, macOS 버전, Rust 버전):**

| 항목 | 결과 |
|---|---|
| 검증한 커밋 | 2단계에서 빌드한 커밋 |
| cargo test | 통과 N개 / 실패 N개 |
| 기존 Python 0.4.x, 기존 state.json | 있음/없음, 있음/없음 (기록의 연결 방식) |
| Rust toolchain (2단계) | rustup stable / Homebrew, 버전 |
| 세 에이전트 버전 | claude / codex / agy (agy 버전 꼭 적기) |
| Link capability | symlink |
| Claude Code, Codex, agy 발견 (3.3) | 각각 예/아니요. agy가 아니요면 복사본 비교 결과와 copy 지정 여부 |
| pre-commit, post-checkout, post-merge (3.5) | 각각 예/아니요 |
| 처음 잡힌 my-skills, hook이 찾은 my-skills | Python 판/Rust 판, PATH 변경 여부 |
| 공백이 든 경로 | 정상/문제 |
| 전환 (4단계) | convert와 새 연결 진행 여부(개수는 적지 않음), 건너뛴 host, BLOCK 여부, hermes 기록 보존 여부 |
| link 바로 반영 (4.5), 전환 뒤 발견 (4.6) | host별 예/아니요 |
| 공유 레지스트리 변경 (5단계) | manifest 기본값, 고친 문서(레지스트리 안 파일 이름만), 전역 안내 수정 여부 |
| 두 기기 확인 (6단계) | 6.1 결과, 6.2 진행 또는 보류 |
| 기타 | 발견한 문제 |
```

인계 문서의 진행 기록에 한 줄을 넣는다. §7 G는 **필수 항목(3.3, 3.5, 4단계, 5단계, 6.1)이 모두 통과했을 때만** 완료로 바꾼다. 6.2를 보류했다면 그렇게 적는다.

```bash
. "<GENV>" || exit 1; guard_dirs || exit 1; cd "$TOOL" || exit 1
git -c core.hooksPath=/dev/null switch dev && git -c core.hooksPath=/dev/null pull --ff-only || exit 1
git add docs/2026-11-23-skill-drive-rust-v1-spec.md docs/2026-11-23-skill-drive-v1-status-and-handoff.md
git diff --cached                                  # 개인 정보가 없는지 사람이 다시 본다
```

```bash
. "<GENV>" || exit 1; guard_dirs || exit 1; cd "$TOOL" || exit 1
cargo test --test public_hygiene && git commit -m "docs: record macOS device verification" && git push || { echo "STOP: publish failed (GDIR kept: $GDIR)"; exit 1; }
```

위의 테스트·커밋·push가 모두 성공했고 `$BACKUP` 백업이 더 이상 필요 없을 때만 별도 블록으로 비공개 작업 폴더를 지운다.

```bash
. "<GENV>" || exit 1; cd "$HOME" || exit 1
case "$GDIR" in /private/tmp/*|/private/var/folders/*|/tmp/*|/var/folders/*) rm -rf -- "$GDIR" && echo "removed $GDIR" || { echo "STOP: rm failed"; exit 1; };; *) echo "ABORT: not removing $GDIR"; exit 1;; esac
```

코드 문제를 찾으면 고치지 말고 기록한다. 수정은 별도 작업으로 진행한다.
