---
name: my-jira
description: >-
  junseon의 Jira/Atlassian 작업 부트스트랩(setup) 스킬. 호출되면 config.json의
  검증값(사이트·계정·프로젝트)을 로드하고 읽기 헬스체크 후 KAN 현재 상태를 띄워
  "Jira 준비 완료" 상태로 만든다. 이후 이슈 조회/추가/수정/삭제/전환을 재파악 없이
  바로 수행. "jira", "내 이슈", "KAN", "칸반 추가/수정/삭제" 류 요청에서 사용.
  상세 명령 문법은 상위 `twg` 스킬과 `twg help`로 확인.
---

# my-jira — Jira 작업 부트스트랩 (setup)

이 스킬은 **setup 절차**다. 호출되면 아래 순서를 실행해 현재 대화를 "Jira 준비됨"
상태로 만든다. 영속 데이터는 모두 `config.json`에 있다. SKILL.md는 절차만 담는다.

데이터(사이트/계정/프로젝트 등)가 필요하면 **항상 `config.json`을 먼저 읽는다.**
값이 틀리면(사이트 변경, 프로젝트 추가 등) `config.json`을 갱신한다.

## 부트스트랩 절차 (이 순서대로 실행)

### 1단계 — config 로드
이 스킬 폴더의 `config.json`(SKILL.md와 같은 디렉토리)을 읽는다.
- 파일이 **있으면** 값을 즉시 사용 (재파악 0).
- 파일이 **없으면** 초기 셋업: 사이트/계정/프로젝트를 확인해 새로 만든다
  (사이트는 `~/.config/twg/auth.conf`의 `site=`/`cloud-id=`, 계정은 아래 헬스체크,
  프로젝트는 `twg jira workitem query`로 키 수집).

### 2단계 — 읽기 헬스체크
```bash
twg user search "" --email "<config.account.email>" --limit 1
```
- 출력에 `Matched by: jira-rest`가 보이면 통과 → `config.json`의 `read_verified`를
  오늘 날짜로 갱신.
- 실패하면 중단하고 재인증 안내: `twg auth login` (자세한 건 `twg help auth`).

### 3단계 — KAN 현재 상태 스냅샷
`query`는 버전 내성 패턴 필수 ("조회 규칙" 2번 — `| jq` 직접 파이프 금지, 엔벨로프 대응):
```bash
out="${TMPDIR:-/tmp}/twg-out.json"
twg jira workitem query \
  --jql "project = <config.project> AND statusCategory != Done ORDER BY priority DESC, updated DESC" \
  --first 20 --output json > "$out" 2>/dev/null
jq -e . "$out" >/dev/null 2>&1 || out=$(sed -n 's/^[[:space:]]*stdout:[[:space:]]*"\(.*\)".*/\1/p' "$out" | head -1)
jq -r '.data.issues[] | "\(.key)  [\(.status.name)]  \(.summary)"' "$out"
```
열린 이슈 목록을 사용자에게 간단히 보여준다.

### 4단계 — 준비 완료 선언
"Jira 준비 완료 (site=<site>, project=<project>). 조회/추가/수정/삭제/전환 가능."
- 단 `config.write_verified == false`이면: **쓰기(생성/수정/삭제/전환)는 아직 실제
  검증 전**이며 첫 쓰기 요청 시 처음 시도된다는 점을 한 줄로 덧붙인다.

## 검증된 환경 값 (요약 — 단일 진실원은 config.json)

| 항목 | 값 |
|---|---|
| 바이너리 | PATH의 `twg` (보통 `~/.local/bin/twg`). skill 폴더의 `./twg` **아님** |
| 사이트 / cloud-id | `hermesforjs` / `e0ffe317-5557-43b4-a551-466ff24275a2` |
| 계정 | junseon / idealinnov39@gmail.com / `712020:076771c9-2a42-4120-877f-2b10b68c08bd` |
| 프로젝트 | `KAN` (단일) |
| 상태값 | `해야 할 일`, `진행 중`, `완료` |
| 쓰기 검증 | 검증됨 (`write_verified: "2026-06-29"`) |
| twg 버전 | **1.0.18+ 권장** (이하 버전은 업데이트 배너가 stdout 오염 → jq 파손). 각 호스트에서 `twg update`로 최신 유지 = 이식성 앵커 |

## 조회 규칙 (read 견고화 — 항상 지킬 것)

1. **절대 `... --output json | jq` 직접 파이프 금지.** twg가 배너·진행표시(spinner)를
   stdout으로 흘리면 스트림이 오염/절단돼 깨진다. → JSON은 **파일로 받은 뒤** jq.
2. **`workitem query`는 환경 내성 패턴 필수.** twg는 **에이전트 환경**(`AI_AGENT`/
   `CLAUDECODE` 등 env 감지)에서 query를 large-output로 보고 stdout에 **YAML 엔벨로프**
   (실제 JSON은 `output_files.stdout` 임시파일)를 뱉는다. 일부 버전·환경(예: 1.0.1, env 제거)
   에선 inline JSON. 둘 다 견디는 표준 패턴 (호스트·버전·환경 무관 동일 결과):
   ```bash
   out="${TMPDIR:-/tmp}/twg-out.json"
   twg jira workitem query --jql '<제한 JQL>' --first 50 --output json > "$out" 2>/dev/null
   # 엔벨로프면 실제 JSON 파일로 교체 (inline JSON이면 그대로 둠)
   jq -e . "$out" >/dev/null 2>&1 || out=$(sed -n 's/^[[:space:]]*stdout:[[:space:]]*"\(.*\)".*/\1/p' "$out" | head -1)
   jq -r '.data.issues[] | "\(.key)  [\(.status.name)]  \(.summary)"' "$out"
   ```
3. **`workitem get`은 inline JSON** (`--output json` → `.data[0]`). 엔벨로프 처리 불필요.
4. **JQL은 항상 제한 포함** (`project = KAN`, `assignee = currentUser()` 등). 무제한 거부됨.
5. **응답 구조**: `query` → `.data.issues[]`, `get` → `.data[0]`.

## 자주 쓰는 명령

```bash
out="${TMPDIR:-/tmp}/twg-out.json"

# query 헬퍼 (버전 내성): jira_list '<제한 JQL>' [page]
jira_list() {
  local f="${TMPDIR:-/tmp}/twg-out.json"
  twg jira workitem query --jql "$1" --first "${2:-50}" --output json > "$f" 2>/dev/null
  jq -e . "$f" >/dev/null 2>&1 || f=$(sed -n 's/^[[:space:]]*stdout:[[:space:]]*"\(.*\)".*/\1/p' "$f" | head -1)
  jq -r '.data.issues[] | "\(.key)  [\(.status.name)]  \(.summary)"' "$f"
}

# 내 이슈
jira_list "assignee = currentUser() ORDER BY updated DESC" 20

# KAN 열린 이슈
jira_list 'project = KAN AND statusCategory != Done ORDER BY priority DESC, updated DESC' 50

# 단일 이슈 (inline JSON, .data[0])
twg jira workitem get KAN-25 --output json > "$out" 2>/dev/null
jq -r '.data[0] | "\(.key)  \(.summary)  [\(.status.name)]"' "$out"
```

## 쓰기 작업 (생성 / 수정 / 삭제 / 전환)

`write_verified`가 날짜로 설정돼 있으면 쓰기 권한이 검증된 상태다(현재 검증됨).
값이 `false`인 새 환경에서는 첫 쓰기가 곧 검증이며, 성공 시 `config.json`의
`write_verified`를 검증 날짜로 갱신할 것.

- 추가: `twg jira workitem create ...` — 필드 메타데이터는
  `twg jira workitem field create-metadata`로 먼저 확인, `customfield_*` ID 사용.
- 수정: `twg jira workitem update KAN-XX ...`
- 전환: `twg jira workitem transitions KAN-XX`로 가능한 전환 확인 후
  `twg jira workitem transition KAN-XX ...`.
- 삭제: `twg jira workitem delete KAN-XX` — **파괴적**. 현재 상태를 먼저 보여주고
  확인받은 뒤 실행 (사용자가 즉시 실행을 명시하지 않은 한).
- 본문(설명/코멘트)은 명령의 format 플래그에 맞게. Jira 위키 마크업(`h2.`, `*bold*`)
  금지, HTML 모드면 실제 HTML 태그.

## 함정 (직접 밟은 것들)

1. **배너 오염 (가장 자주 깨짐)**: 구버전 twg가 업데이트 배너/spinner를 stdout에 흘려
   `... --output json | jq`가 `jq: Unfinished string at EOF`로 깨진다. 간헐적(배너는
   주기적으로만 뜸)이라 "됐다 안 됐다" 한다. → **파일로 받은 뒤 jq** ("조회 규칙" 1번)
   + twg 1.0.18+ 유지.
2. 페이지 크기 = `--first` (`--limit` 아님 → `unknown option` 에러).
3. 무제한 JQL 거부됨 → 항상 제한 조건 포함 (`project = KAN`, `assignee = currentUser()` 등).
4. 출력 모드(**에이전트 환경 의존, 직접 확인함**): twg는 에이전트 환경(`AI_AGENT`/
   `CLAUDECODE` 등 env 감지)에서 `workitem query --output json`을 **YAML 엔벨로프**
   (실제 JSON은 `output_files.stdout` 임시파일)로 낸다(엔벨로프에 `agent_output:` 블록 포함).
   env 제거 시 직접 JSON 확인됨. 단 `workitem get --output json`은 (env 무관) inline JSON.
   → "조회 규칙" 2번 환경 내성 패턴으로 통일하면 호스트·버전·환경 무관 동일 결과.
5. 정체성 조회는 `user search --email`. `resolve --query "me"`는 실패.
6. 응답 구조: `workitem query`는 `.data.issues[]`, `workitem get`은 `.data[0]`.
7. 상태값은 한글. JQL에 `status = "완료"` 또는 언어 독립 `statusCategory`(Done/In Progress/To Do).

## 더 깊은 작업

- 명령 문법 불확실 → `twg help <terms>`, `twg help describe "<path>"`.
- 롤업/리포트·컨텍스트·PR·운영 헬스 → 상위 `twg-*` 워크플로 스킬.
