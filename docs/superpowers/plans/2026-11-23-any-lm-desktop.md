# Any LM Desktop 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Claude Code 이외의 도구(Codex, Antigravity)는 이 줄을 무시하고 `AGENTS.md`의 인계 규칙에 따라 한 작업씩 진행한다.

**Goal:** K-EXAONE Desktop 0.4.4 소스를 이력째 복제해, LM Studio 로컬 서버 전용 작업 앱 "Any LM Desktop"을 만들고, Claude Code · Codex · Antigravity가 번갈아 개발할 수 있는 인계 체계를 갖춘다.

**Architecture:** 원본(Tauri v2 + React/TS + Rust) 구조와 내부 이름을 유지한 채, 충돌 요소(앱 ID·자격 증명·포트 등)만 바꾸고 K-EXAONE 전용 요소를 걷어 낸다. LM Studio 연동은 새 모듈 `lmstudio.rs`(서버 주소·모델 목록·로드)와 `context_budget.rs`(컨텍스트 계산)에 모으고, 기존 에이전트 루프(`exaone.rs`)는 그 값을 받아 쓰도록 최소한만 고친다.

**Tech Stack:** Tauri 2, Rust(reqwest 0.12, serde_json, tokio), React + TypeScript + Vite, Vitest, LM Studio 0.4.28 REST API(`/v1/*` OpenAI 호환, `/api/v1/*` 고유).

**Spec:** `docs/superpowers/specs/2026-11-23-any-lm-desktop-design.md` (작업 1에서 이 위치로 옮긴다. 옮기기 전 원본 위치: `C:\DOCS\Local LLM\docs\specs\2026-11-23-any-lm-desktop-design.md`)

## Global Constraints

- 저장소: `C:\DOCS\any-lm-desktop`. 원본 `C:\DOCS\EXAONE`와 그 GitHub 저장소에는 아무것도 쓰지 않는다(커밋·푸시·파일 수정 금지).
- 앱 이름 `Any LM Desktop`, identifier `io.github.kidarigun.anylm-desktop`, 버전 `0.1.0`.
- 내부 파일·함수·이벤트 이름(`exaone.rs`, `run_exaone_turn`, `onExaoneStream`, `"exaone-stream"`)은 바꾸지 않는다. 사용자에게 보이는 문자열과 스펙 6.2 표의 충돌 요소만 바꾼다.
- 화면 문구는 한국어. 사용자에게 보이는 곳에 "EXAONE"/"K-EXAONE"/"LG"를 쓰지 않는다(NOTICE·README의 출처 설명만 예외).
- LM Studio 서버 주소는 `http://localhost`, `http://127.0.0.1`, `http://[::1]`만 허용. 기본값 `http://localhost:1234`.
- 기본 원하는 컨텍스트 `32768`. 자동 요약 압축 기준 `0.70`(로드된 컨텍스트 대비). 글자→토큰 추정 `1토큰 = 1.5자`(`chars()` 기준).
- 모델 로드 요청 시간 제한 5분. 응답 헤더 대기 180초, 스트림 무응답 900초(원본 유지).
- 앱은 사용자가 [내리고 로드]·[다시 로드]·[언로드]를 누를 때만 모델을 내린다.
- 매 작업 끝: `npm test` · `npx tsc --noEmit` · `cd src-tauri && cargo test --lib` 모두 통과 → 계획 체크박스 · `docs/handoff/STATUS.md` 갱신 → 한 커밋. 커밋 메시지 끝에 `Agent: <claude-code|codex|antigravity>` 줄.

## Review Focus

1. LM Studio 서버가 꺼져 있거나 답변 도중 꺼짐 → 몇 초 안에 "LM Studio 서버가 꺼져 있습니다" 안내와 [서버 시작] 버튼, 무한 대기 없음. (작업 3 `connection_refused_is_friendly`)
2. 대화에 저장된 모델을 LM Studio에서 지움 → JIT 로드 실패 대신 "모델을 찾을 수 없습니다. 다른 모델을 고르세요". (작업 4 `plan_load_missing_model`)
3. 로드된 컨텍스트를 알 수 없음(목록 실패, JIT) → 예산 계산은 8192로 가정하고 계속. (작업 8 `unknown_context_falls_back_to_8192`)
4. 대화가 짧아 압축할 게 없는데 70%를 넘음(첨부가 큰 첫 메시지) → 자동 압축을 조용히 건너뛰고 보낸다(오류 없음). (작업 9 `auto_compact_skips_when_nothing_to_cover`)
5. 한국어 위주 대화의 토큰 추정 → 바이트가 아니라 글자 수로 센다. (작업 8 `estimate_counts_chars_not_bytes`)

---

### Task 1: 저장소 만들기와 인계 체계

**Files:**
- Create: `C:\DOCS\any-lm-desktop\` (git clone), `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, `docs/handoff/STATUS.md`, `docs/handoff/LOG.md`, `docs/handoff/START.md`, `docs/decisions/0001-fork-with-history.md`
- Move in: 스펙 → `docs/superpowers/specs/2026-11-23-any-lm-desktop-design.md`, 이 계획 → `docs/superpowers/plans/2026-11-23-any-lm-desktop.md`
- Delete: `docs/superpowers/plans/2026-11-13-exaone-desktop-plan.md`, `docs/superpowers/specs/2026-11-13-exaone-desktop-design.md`(원본 문서. 이력에는 남는다), `.claude/launch.json`의 원본 전용 항목은 이름만 바꾼다

**Interfaces:**
- Produces: 이후 모든 작업이 따르는 `AGENTS.md` 규칙과 `STATUS.md` 형식.

- [x] **Step 1: 복제와 브랜치**

```powershell
git clone C:\DOCS\EXAONE C:\DOCS\any-lm-desktop
cd C:\DOCS\any-lm-desktop
git checkout -B main origin/feature/agent-loop-and-document-canvas
git log -1 --format=%h   # e885b02 이어야 한다
```

- [x] **Step 2: 원격 정리**

```powershell
git remote remove origin
git remote add upstream https://github.com/kidarigun/k-exaone-desktop.git
git remote set-url --push upstream DISABLED
git branch -D feature/agent-loop-and-document-canvas 2>$null
git remote -v   # upstream fetch = github URL, push = DISABLED
```

- [x] **Step 3: 기준 상태 확인 (아무것도 고치기 전)**

Run: `npm install` → `npm test` → `npx tsc --noEmit` → `cd src-tauri; cargo test --lib`
Expected: 모두 통과. 실패하면 고치지 말고 실패 목록을 `docs/handoff/STATUS.md`의 "기준 상태"에 적는다(이후 작업이 새로 깬 것과 구분하려고).

- [x] **Step 4: 인계 파일 작성**

`AGENTS.md`에 들어갈 절(이 순서): ① 프로젝트 한 단락(스펙 1절 요약) ② 폴더 지도(`src-tauri/src/*.rs`, `src/*` 하위 폴더 한 줄씩) ③ 빌드·테스트 명령(Global Constraints 마지막 줄) ④ 코딩 규칙(Global Constraints 2~5줄을 그대로) ⑤ **인계 규칙**: 스펙 6.4 "인계 규칙" 1~7을 그대로 옮긴다 ⑥ 원본 수정 가져오기 절차(`git fetch upstream`, `git cherry-pick <hash>`, 충돌 기록은 LOG에).
`CLAUDE.md` 내용은 정확히 한 줄 `@AGENTS.md`. `GEMINI.md`는 `이 저장소의 작업 지침은 AGENTS.md에 있습니다. 먼저 AGENTS.md를 읽고 그대로 따르세요.` 한 줄.
`STATUS.md` 절: `## 지금 목표` · `## 진행 중` (계획 파일 경로 + 작업 번호 + 단계) · `## 마지막 확인` (명령과 결과, 날짜) · `## 다음 할 일` (파일·함수 수준으로 구체적으로) · `## 결정 필요` · `## 기준 상태` · `## 마지막 작업자` (도구, 모델, 시각).
`START.md`: 도구마다 붙여 넣을 시작 문구 — "AGENTS.md와 docs/handoff/STATUS.md를 읽고, STATUS의 '다음 할 일'부터 이어서 진행해. 시작 전에 git status와 git log -5로 상태가 STATUS와 맞는지 확인해." — 와 마무리 문구 — "지금 단계를 마무리할 수 있으면 마무리하고, 아니면 wip 커밋을 하고 STATUS의 '다음 할 일'에 정확한 위치를 적은 뒤 LOG에 한 줄 남겨." 
`0001-fork-with-history.md`: 스펙 4절 표와 선택 이유.

- [x] **Step 5: 스펙·계획 옮기고 원본 설계 문서 삭제, 커밋**

```powershell
git add -A
git commit -m "chore: fork K-EXAONE Desktop 0.4.4 as Any LM Desktop and add handoff docs" -m "Agent: claude-code"
```

- [ ] **Step 6: GitHub 비공개 저장소 — 사용자에게 먼저 확인**

사용자에게 "`kidarigun/any-lm-desktop` 비공개 저장소를 만들고 main을 올릴까요?"를 묻고, 승인하면:

```powershell
gh repo create kidarigun/any-lm-desktop --private --source . --remote origin --push
```

Expected: `git remote -v`에 origin(새 저장소)과 upstream(push DISABLED)이 보인다.

---

### Task 2: 정체성 분리와 업데이트 제거

**Files:**
- Modify: `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `package.json`, `package-lock.json`(이름·버전만), `index.html`(title), `src-tauri/src/main.rs`(lib 이름), `src-tauri/src/lib.rs`, `src-tauri/src/secrets.rs`, `src-tauri/src/single_instance.rs`, `src-tauri/src/html_preview.rs`, `src-tauri/src/mcp.rs`(자격 증명 서비스명 쓰는 곳), `src/App.tsx`(localStorage 키), 그 밖에 `exaone_`로 시작하는 localStorage 키를 쓰는 `src/**`
- Delete: `src/update/UpdatePanel.tsx`, `scripts/make_latest_json.py`, `tauri-plugin-updater` 의존성, `lib.rs`의 `app_version`을 뺀 업데이트 명령(`check_update`, `install_update`, `find_update`)과 `migrate_old_data_dir`
- Test: `src-tauri/src/lib.rs` 테스트 모듈, `src-tauri/src/single_instance.rs` 테스트 모듈

**Interfaces:**
- Produces: `secrets::SERVICE = "io.github.kidarigun.anylm-desktop"`, `single_instance::PORT = 45789`, 미리보기 호스트 `anylm-preview.localhost`, 대화상자 환경 변수 `ANYLM_DLG_INIT|NAME|FILTER`, localStorage 접두어 `anylm_`, 기본 작업 폴더 `문서\AnyLM`.

- [x] **Step 1: 실패하는 테스트**

`lib.rs` tests에 추가:

```rust
#[test]
fn identity_is_any_lm_desktop() {
    let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    assert_eq!(conf["productName"], "Any LM Desktop");
    assert_eq!(conf["identifier"], "io.github.kidarigun.anylm-desktop");
    assert_eq!(conf["version"], "0.1.0");
    assert!(conf["plugins"].get("updater").is_none());
    assert_eq!(conf["bundle"]["createUpdaterArtifacts"], serde_json::Value::Null);
    let text = include_str!("../tauri.conf.json");
    assert!(!text.contains("EXAONE") && !text.contains("exaone"));
}
```

`single_instance.rs` tests에 `assert_eq!(PORT, 45789); assert_eq!(HELLO, b"ANYLM-DESKTOP-SHOW\n");`. 기존 미리보기 주소 테스트(`lib.rs`의 `exaone-preview.localhost` 단언)는 `anylm-preview.localhost`로 고친다.

- [x] **Step 2: 실패 확인** — `cd src-tauri; cargo test --lib identity_is_any_lm_desktop` → FAIL

- [x] **Step 3: 스펙 6.2 표대로 바꾼다.** 설치 파일 문구: `shortDescription` "LM Studio 로컬 모델용 작업 앱", `copyright` "© 2026 김기군", `longDescription` "LM Studio의 로컬 모델로 문서를 쓰고, 프로젝트 파일을 다루고, Skill과 MCP로 기능을 넓히는 데스크톱 앱입니다.". CSP `frame-src`도 새 호스트로. 업데이트 관련 화면(App.tsx의 UpdatePanel 사용처, 설정의 "새 버전 확인" 토글)과 `checkUpdates` 설정을 지운다. `migrate_old_data_dir` 호출과 함수를 지운다. 기본 작업 폴더 `.join("AnyLM")`, 임시 폴더 이름 `any-lm-desktop-data`.

- [x] **Step 4: 통과 확인** — 세 검증 명령 모두 PASS. 추가로 `git grep -n -i "exaone" -- src-tauri/tauri.conf.json package.json src-tauri/Cargo.toml index.html` 결과 없음.

- [x] **Step 5: 동시 실행 확인** — 설치된 원본 앱을 켠 채 `npm run tauri dev`. Expected: 두 창이 모두 뜨고 새 앱 데이터가 `%APPDATA%\io.github.kidarigun.anylm-desktop`에 생긴다. 원본 데이터 폴더의 수정 시각이 바뀌지 않는다.

- [x] **Step 6: 커밋** — `chore: separate app identity from K-EXAONE Desktop and drop updater`

---

### Task 3: 설정을 LM Studio 연결로 바꾸기

**Files:**
- Create: `src-tauri/src/lmstudio.rs` (lib.rs에 `pub mod lmstudio;`)
- Modify: `src-tauri/src/store.rs` (`AppSettings`), `src-tauri/src/exaone.rs` (`validate_base_url` 사용처, API 키 필수 검사 두 곳, `bearer_auth` 세 곳, 요청 URL), `src-tauri/src/lib.rs` (`save_settings`, `clear_api_key`), `src/types.ts`, `src/api.ts`(기본 설정, 웹 미리보기용 가짜 db)
- Test: `lmstudio.rs` 테스트 모듈, `store.rs` 테스트 모듈

**Interfaces:**
- Produces:
  - `store::AppSettings { api_key: String /* LM Studio 토큰, 선택 */, api_key_set: bool, server_url: String, model: String /* 기본 모델 key, 빈 값 허용 */, writing_style: String, auto_compact: bool /* 기본 true */, model_prefs: BTreeMap<String, ModelPref> }` — `base_url`, `reasoning_effort`, `api_key_saved_at`, `check_updates` 삭제.
  - `store::ModelPref { context_length: Option<u32>, reasoning: Option<String> /* "on" | "off" */ }` (camelCase 직렬화)
  - `lmstudio::ServerUrls { chat: String, models: String, load: String, unload: String }`
  - `lmstudio::server_urls(server_url: &str) -> Result<ServerUrls, String>` — `/v1/chat/completions`, `/api/v1/models`, `/api/v1/models/load`, `/api/v1/models/unload`를 붙인다. 끝의 `/`는 무시.
  - `lmstudio::authorize(rb: reqwest::RequestBuilder, token: &str) -> reqwest::RequestBuilder` — 토큰이 비면 헤더 없음.
  - `lmstudio::friendly_error(err: &reqwest::Error) -> String` — 연결 거부·연결 시간 초과면 `"LM Studio 서버가 꺼져 있습니다. LM Studio에서 서버를 켜거나 [서버 시작]을 눌러 주세요."`
  - TS `AppSettings { apiKey, apiKeySet?, serverUrl, model, writingStyle, autoCompact, modelPrefs: Record<string, { contextLength?: number; reasoning?: "on" | "off" }> }`

- [x] **Step 1: 실패하는 테스트**

```rust
#[test]
fn server_urls_accept_only_local_http() {
    let u = server_urls("http://localhost:1234/").unwrap();
    assert_eq!(u.chat, "http://localhost:1234/v1/chat/completions");
    assert_eq!(u.models, "http://localhost:1234/api/v1/models");
    assert_eq!(u.load, "http://localhost:1234/api/v1/models/load");
    assert!(server_urls("http://127.0.0.1:1234").is_ok());
    assert!(server_urls("http://[::1]:1234").is_ok());
    assert!(server_urls("http://192.168.0.5:1234").is_err());
    assert!(server_urls("https://localhost.evil.com").is_err());
    assert!(server_urls("http://user:pw@localhost:1234").is_err());
}

#[test]
fn empty_token_sends_no_auth_header() {
    let c = reqwest::Client::new();
    let r = authorize(c.get("http://localhost:1234"), "").build().unwrap();
    assert!(r.headers().get("authorization").is_none());
    let r = authorize(c.get("http://localhost:1234"), "tok").build().unwrap();
    assert_eq!(r.headers()["authorization"], "Bearer tok");
}

#[tokio::test]
async fn connection_refused_is_friendly() {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port(); // 바로 닫혀 비어 있는 포트
    let err = reqwest::get(format!("http://127.0.0.1:{port}")).await.unwrap_err();
    assert!(friendly_error(&err).contains("LM Studio 서버가 꺼져 있습니다"));
}
```

`store.rs`: `default_settings_point_to_lm_studio` — `AppSettings::default()`의 `server_url == "http://localhost:1234"`, `model == ""`, `auto_compact == true`, `model_prefs.is_empty()`. 기존 `api_key_is_never_serialized` 테스트는 새 필드로 고친다.

- [x] **Step 2: 실패 확인** — `cargo test --lib lmstudio::` → FAIL(모듈 없음)

- [x] **Step 3: 구현.** `exaone.rs`에서 `settings.base_url` → `lmstudio::server_urls(&settings.server_url)?.chat`, `.bearer_auth(...)` → `lmstudio::authorize(..., &settings.api_key)`, "API 키가 설정되지 않았습니다" 검사 두 곳(`run_exaone_turn`, `summarize_for_compaction`) 삭제, 요청 전송 오류는 `friendly_error`로. `validate_base_url`과 그 테스트는 지우고 `server_urls`로 대신한다. `reasoning_effort`를 본문에 넣던 코드는 이 작업에서 지운다(작업 7에서 대체). 설정 저장 형식이 바뀌므로 `store.rs`의 불러오기는 모르는 필드를 무시하고 없는 필드는 기본값으로 채운다(`#[serde(default)]`).

- [x] **Step 4: 통과 확인** — 세 검증 명령 PASS. TS 쪽 컴파일 오류는 설정 화면에서 지운 필드를 쓰는 곳만 임시로 정리한다(화면 재구성은 작업 5).

- [x] **Step 5: 커밋** — `feat: connect settings to LM Studio server URL with optional token`

---

### Task 4: 모델 목록 · 로드 · 서버 상태

**Files:**
- Modify: `src-tauri/src/lmstudio.rs`, `src-tauri/src/lib.rs`(명령 등록), `src/api.ts`, `src/types.ts`
- Create: `src-tauri/src/testdata/lmstudio_models.json` (2026-11-23에 `GET /api/v1/models`로 받은 실제 응답: qwen3.5-9b(로드됨, 32768), qwen3.6-35b-a3b(미로드), nomic 임베딩)
- Test: `lmstudio.rs` 테스트 모듈

**Interfaces:**
- Consumes: 작업 3의 `server_urls`, `authorize`, `friendly_error`, `AppSettings`.
- Produces:
  - `LmModel { key, display_name, params: Option<String>, quantization: Option<String>, size_bytes: u64, max_context_length: u32, loaded: Vec<LoadedInstance>, vision: bool, tool_use: bool, reasoning_options: Vec<String> }`, `LoadedInstance { id: String, context_length: u32 }` (camelCase 직렬화)
  - `parse_models(v: &serde_json::Value) -> Vec<LmModel>` — `type == "llm"`만.
  - `enum LoadPlan { Ready { instance_id: String, context_length: u32 }, SmallerLoaded { instance_id: String, loaded: u32, wanted: u32 }, NotLoaded { others: Vec<String> /* 로드된 다른 LLM의 instance id */ }, Missing }` — `#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]` (TS에서 `{ kind: "smallerLoaded", instanceId, loaded, wanted }` 꼴)
  - `plan_load(models: &[LmModel], key: &str, wanted: u32) -> LoadPlan`
  - `wanted_context(settings: &AppSettings, model: &LmModel) -> u32` — `model_prefs[key].context_length` 또는 32768, `max_context_length`를 넘지 않게.
  - `async list_models(settings) -> Result<Vec<LmModel>, String>`, `async load_model(settings, key: &str, context_length: u32) -> Result<LoadedInstance, String>` (시간 제한 5분), `async unload_instance(settings, instance_id: &str) -> Result<(), String>`, `async server_running(settings) -> bool`, `lms_path() -> Option<PathBuf>` (`%USERPROFILE%\.lmstudio\bin\lms.exe`), `start_server() -> Result<(), String>`(`lms server start`, 창 없이)
  - Tauri 명령: `lm_list_models`, `lm_plan_load { key }`, `lm_load_model { key, contextLength }`, `lm_unload { instanceId }`, `lm_server_running`, `lm_start_server`
  - TS `BackendAPI.listModels()`, `planLoad(key)`, `loadModel(key, contextLength)`, `unloadModel(instanceId)`, `serverRunning()`, `startServer()`; 타입 `LmModel`, `LoadedInstance`, `LoadPlan`

- [ ] **Step 1: load/unload 요청 형식 확인.** https://lmstudio.ai/docs/developer/rest 에서 `POST /api/v1/models/load`·`unload` 본문 형식을 확인하고, 9B로 실측한다(`context_length` 지정 로드 → `GET /api/v1/models`에 그 값이 보이는지). 같은 요청이 LM Studio 화면에 저장해 둔 모델별 GPU 설정(전문가 가중치 CPU 보내기)을 덮어쓰는지도 35B로 확인한다. 결과를 `docs/decisions/0002-lmstudio-load-api.md`에 남긴다.

- [ ] **Step 2: 실패하는 테스트**

```rust
fn fixture() -> Vec<LmModel> { parse_models(&serde_json::from_str(include_str!("testdata/lmstudio_models.json")).unwrap()) }

#[test]
fn parses_only_llms_with_capabilities() {
    let m = fixture();
    assert_eq!(m.len(), 2);
    let q = m.iter().find(|x| x.key == "qwen3.5-9b").unwrap();
    assert_eq!(q.display_name, "Qwen3.5 9B");
    assert_eq!(q.max_context_length, 262144);
    assert_eq!(q.loaded[0].context_length, 32768);
    assert!(q.vision && q.tool_use);
    assert_eq!(q.reasoning_options, vec!["off", "on"]);
}

#[test]
fn plan_load_cases() {
    let m = fixture();
    assert!(matches!(plan_load(&m, "qwen3.5-9b", 32768), LoadPlan::Ready { context_length: 32768, .. }));
    assert!(matches!(plan_load(&m, "qwen3.5-9b", 16384), LoadPlan::Ready { context_length: 32768, .. })); // 이상이면 그대로
    assert!(matches!(plan_load(&m, "qwen3.5-9b", 65536), LoadPlan::SmallerLoaded { loaded: 32768, wanted: 65536, .. }));
    match plan_load(&m, "qwen3.6-35b-a3b", 32768) { LoadPlan::NotLoaded { others } => assert_eq!(others, vec!["qwen3.5-9b"]), p => panic!("{p:?}") }
}

#[test]
fn plan_load_missing_model() {
    assert!(matches!(plan_load(&fixture(), "deleted-model", 32768), LoadPlan::Missing));
}

#[test]
fn wanted_context_respects_prefs_and_max() { /* pref 없음 → 32768, pref 8192 → 8192, pref 999999 → 262144 */ }
```

- [ ] **Step 3: 실패 확인** — `cargo test --lib lmstudio::` → FAIL

- [ ] **Step 4: 구현** — 위 Interfaces 그대로. `start_server`는 `CREATE_NO_WINDOW`로 실행(원본 `runtime.rs`의 방식 참고). `lms_path`가 없으면 `"LM Studio 명령줄 도구를 찾지 못했습니다. LM Studio를 한 번 실행한 뒤 다시 시도해 주세요."`.

- [ ] **Step 5: 통과 확인** — 세 검증 명령 PASS. 실측: 앱 개발 실행에서 개발자 도구 콘솔로 `await window.__TAURI__.core.invoke("lm_list_models")` → 두 모델.

- [ ] **Step 6: 커밋** — `feat: list, plan and load LM Studio models`

---

### Task 5: "LM Studio 연결" 설정 창

**Files:**
- Create: `src/lmstudio/ConnectionDialog.tsx`, `src/lmstudio/modelText.ts`, `src/lmstudio/modelText.test.ts`
- Modify: `src/App.tsx` (원본 "Modal 1: API & Model Settings" 자리를 `ConnectionDialog`로 교체, 사이드바 하단 [API 설정] 버튼 → [LM Studio 연결] + 연결 상태 점)

**Interfaces:**
- Consumes: 작업 4의 `BackendAPI.listModels/serverRunning/startServer/loadModel/unloadModel`, 작업 3의 `AppSettings.modelPrefs`.
- Produces: `describeModel(m: LmModel): string`, `loadedLabel(m: LmModel): string`, `capabilityBadges(m: LmModel): ("vision" | "tools" | "reasoning")[]`; `<ConnectionDialog settings onSave onClose />`.

- [ ] **Step 1: 실패하는 테스트** (`modelText.test.ts`)

```ts
it("describes a model in one line", () => {
  expect(describeModel(qwen9b)).toBe("9B · Q4_K_M · 6.1GB");
});
it("labels load state", () => {
  expect(loadedLabel(qwen9b)).toBe("로드됨 · 32K");
  expect(loadedLabel(qwen35)).toBe("로드 안 됨");
});
it("lists capability badges", () => {
  expect(capabilityBadges(qwen9b)).toEqual(["vision", "tools", "reasoning"]);
});
```

(`qwen9b`, `qwen35`은 작업 4 테스트 데이터와 같은 값의 TS 객체. 크기는 `size_bytes / 1024^3`을 소수 첫째 자리로.)

- [ ] **Step 2: 실패 확인** — `npx vitest run src/lmstudio` → FAIL

- [ ] **Step 3: 구현.** 창 구성(위에서 아래): 서버 주소 입력 + 연결 상태(초록 "연결됨"/빨강 "꺼져 있음" + [서버 시작]) → 선택 사항 "API 토큰"(LM Studio에서 인증을 켠 경우만, 원본처럼 저장 후 화면에 돌려주지 않음) → 모델 목록(행마다 표시 이름, `describeModel`, `loadedLabel`, 기능 배지, "기본 모델" 라디오, 컨텍스트 입력(기본 32768, 최대 `maxContextLength`), [로드]/[언로드]) → "자동 요약 압축" 토글(기본 켬) → 문체 선택(원본 유지). 원본 디자인 언어(Tailwind 클래스)를 따른다.

- [ ] **Step 4: 통과 확인** — 세 검증 명령 PASS. `npm run tauri dev`에서 창을 열어 두 모델이 보이고, 9B 행이 "로드됨 · 32K"인지 확인.

- [ ] **Step 5: 커밋** — `feat: add LM Studio connection dialog`

---

### Task 6: 대화별 모델 선택

**Files:**
- Create: `src/lmstudio/ModelPicker.tsx`, `src/lmstudio/LoadChoiceDialog.tsx`
- Modify: `src-tauri/src/store.rs` (`Conversation.model_key`, `ChatMessage.model_key`), `src-tauri/src/lib.rs` (`create_conversation`, `send_chat_message`, `edit_and_resend_message`, `resolve_tool_approval`, 새 명령 `set_conversation_model`), `src-tauri/src/exaone.rs` (`TurnRequest.model_key`, 요청 본문 `"model"`), `src/types.ts`, `src/api.ts`, `src/App.tsx`
- Test: `lib.rs` 테스트 모듈, `src/lmstudio/sendFlow.test.ts`
- Create: `src/lmstudio/sendFlow.ts`

**Interfaces:**
- Consumes: 작업 4 `LoadPlan`, `planLoad`, `loadModel`, `unloadModel`.
- Produces:
  - `Conversation.model_key: Option<String>`, `ChatMessage.model_key: Option<String>` (serde default, 없으면 생략)
  - `fn conversation_model(settings: &AppSettings, conv: &Conversation) -> Result<String, String>` — `conv.model_key` → `settings.model` 순, 둘 다 비면 `"사용할 모델을 고르세요. [LM Studio 연결]에서 기본 모델을 정하거나 대화 위의 모델 선택에서 고를 수 있습니다."`
  - `TurnRequest.model_key: &str` — 요청 본문 `"model"`과 새 어시스턴트 메시지의 `model_key`에 쓴다.
  - 명령 `set_conversation_model { conversationId, modelKey } -> Conversation`
  - TS `nextSendStep(plan: LoadPlan): "send" | "load" | "askUnload" | "send-with-reload-hint" | "missing"`

- [ ] **Step 1: 실패하는 테스트**

```rust
#[test]
fn conversation_model_prefers_conversation_then_default() {
    let mut s = AppSettings::default(); s.model = "qwen3.5-9b".into();
    let mut c = Conversation::default();
    assert_eq!(conversation_model(&s, &c).unwrap(), "qwen3.5-9b");
    c.model_key = Some("qwen3.6-35b-a3b".into());
    assert_eq!(conversation_model(&s, &c).unwrap(), "qwen3.6-35b-a3b");
    s.model.clear(); c.model_key = None;
    assert!(conversation_model(&s, &c).unwrap_err().contains("사용할 모델을 고르세요"));
}
```

```ts
it("maps load plans to the next step", () => {
  expect(nextSendStep({ kind: "ready", instanceId: "a", contextLength: 32768 })).toBe("send");
  expect(nextSendStep({ kind: "notLoaded", others: [] })).toBe("load");
  expect(nextSendStep({ kind: "notLoaded", others: ["qwen3.5-9b"] })).toBe("askUnload");
  expect(nextSendStep({ kind: "smallerLoaded", instanceId: "a", loaded: 8192, wanted: 32768 })).toBe("send-with-reload-hint");
  expect(nextSendStep({ kind: "missing" })).toBe("missing");
});
```

- [ ] **Step 2: 실패 확인** — `cargo test --lib conversation_model` · `npx vitest run src/lmstudio/sendFlow` → FAIL

- [ ] **Step 3: 구현.** 원본의 추론 강도 선택기 자리(App.tsx `REASONING_EFFORTS` 렌더링 부분)에 `ModelPicker`(로드된 모델이 위, 이름 옆에 `loadedLabel`)를 둔다. 보내기 전 흐름(App.tsx의 `sendChatMessage` 호출 직전): `planLoad` → `nextSendStep`에 따라 바로 보내기 / "{모델} 로드 중…" 표시 후 `loadModel` / `LoadChoiceDialog`("지금 로드된 {X}를 내리고 {Y}를 로드할까요?" [내리고 로드] [그대로 로드 시도] [취소]) / 대화 위 안내 "이 모델은 {8K} 컨텍스트로 로드돼 있습니다 · [{32K}로 다시 로드]" 후 보내기([다시 로드] = 그 인스턴스 `unloadModel` → `loadModel`. 같은 모델의 두 번째 인스턴스는 만들지 않는다) / "모델을 찾을 수 없습니다. 다른 모델을 고르세요." 로드 실패 오류 옆에도 [내리고 로드]. 프로젝트·코드 화면에서 `toolUse == false` 모델을 고르면 "이 모델은 도구 사용 학습이 안 돼 있어 파일 작업이 불안정할 수 있습니다" 경고(막지 않음). 답변 아래 통계 줄에 모델 표시 이름.

- [ ] **Step 4: 통과 확인** — 세 검증 명령 PASS. 실측: 한 대화에서 9B로 한 번, 35B로 바꿔 한 번 질문 → [내리고 로드] 후 35B가 답하고 두 답변 아래 모델 이름이 다르다.

- [ ] **Step 5: 커밋** — `feat: choose LM Studio model per conversation`

---

### Task 7: 추론 켜기/끄기

**Files:**
- Modify: `src-tauri/src/lmstudio.rs`, `src-tauri/src/exaone.rs`(요청 본문, `preserve_thinking`/`keep_thinking` 제거), `src/lmstudio/ModelPicker.tsx` 옆 토글, `src/App.tsx`
- Create: `docs/decisions/0003-reasoning-toggle.md`
- Test: `lmstudio.rs` 테스트 모듈

**Interfaces:**
- Consumes: 작업 4 `LmModel.reasoning_options`, 작업 3 `ModelPref.reasoning`.
- Produces: `lmstudio::reasoning_fields(options: &[String], pref: Option<&str>) -> serde_json::Map<String, Value>`; `const REASONING_OFF_FIELDS`(결정 0003의 값).

- [ ] **Step 1: 끄는 방법 실측.** 9B에 같은 질문을 후보 요청(`"reasoning_effort": "none"`, `"reasoning": {"effort": "none"}`, `"chat_template_kwargs": {"enable_thinking": false}`, LM Studio 문서에 있는 방식)으로 보내 `usage.completion_tokens_details.reasoning_tokens`가 0이 되는 것을 찾는다. 35B에서도 같은지 확인. 결과·근거를 결정 0003에 남긴다.

- [ ] **Step 2: 실패하는 테스트**

```rust
#[test]
fn reasoning_fields_follow_model_and_pref() {
    let on_off = vec!["off".to_string(), "on".to_string()];
    assert!(reasoning_fields(&[], Some("off")).is_empty());          // 추론 없는 모델: 아무것도 안 보냄
    assert!(reasoning_fields(&on_off, None).is_empty());             // 기본값: 모델 기본에 맡김
    assert!(reasoning_fields(&on_off, Some("on")).is_empty());
    assert_eq!(serde_json::Value::Object(reasoning_fields(&on_off, Some("off"))), REASONING_OFF_FIELDS.clone());
}
```

(`REASONING_OFF_FIELDS`는 `LazyLock<Value>`로 결정 0003의 값을 담는다.)

- [ ] **Step 3: 실패 확인 → 구현 → 통과 확인.** 대화 상단 "생각하기" 토글은 선택한 모델의 `reasoningOptions`가 있을 때만 보이고, 바꾸면 `modelPrefs[key].reasoning`에 저장한다. 이전 답변의 추론은 다음 요청에 다시 보내지 않는다(`build_api_messages_with_compaction`의 `keep_thinking` 인자는 항상 false로 넘기거나 인자를 없앤다 — 원본 cherry-pick을 고려해 인자는 두고 false를 넘긴다).

- [ ] **Step 4: 커밋** — `feat: per-model reasoning toggle for LM Studio`

---

### Task 8: 컨텍스트 예산 (줄이기 기준 · 도구 출력 상한 · 초과 감지)

**Files:**
- Create: `src-tauri/src/context_budget.rs` (lib.rs에 `pub mod context_budget;`)
- Modify: `src-tauri/src/exaone.rs` (`COMPACT_TRIGGER_CHARS` 사용처, `tool_result_for_model`, `looks_like_context_overflow`, `TurnRequest.context_length`), `src-tauri/src/lib.rs` (턴 시작 시 `context_length` 구하기)
- Test: `context_budget.rs`, `exaone.rs` 테스트 모듈

**Interfaces:**
- Consumes: 작업 4 `list_models`, `LmModel.loaded`.
- Produces:
  - `pub const FALLBACK_CONTEXT: u32 = 8192; pub const AUTO_COMPACT_RATIO: f64 = 0.70; pub const CHARS_PER_TOKEN: f64 = 1.5;`
  - `estimate_tokens(text_chars: usize) -> u64` (올림)
  - `usage_ratio(last_prompt_tokens: Option<u64>, extra_chars: usize, context_length: u32) -> f64` — 마지막 `prompt_tokens`(없으면 0) + `estimate_tokens(extra_chars)`를 컨텍스트로 나눔
  - `trim_trigger_chars(context_length: u32) -> usize` = `context_length × 0.70 × 1.5`
  - `tool_output_cap(context_length: u32) -> usize` = `min(40_000, max(4_000, context_length / 2))`
  - `resolve_context(loaded: Option<u32>) -> u32` — 없으면 `FALLBACK_CONTEXT`
  - `TurnRequest.context_length: u32`

- [ ] **Step 1: LM Studio 초과 오류 문구 수집.** 9B를 `context_length` 4096으로 로드하고 6,000토큰 넘는 메시지를 보내 응답 상태 코드와 본문을 받아 적는다. 실측 뒤 32768로 다시 로드.

- [ ] **Step 2: 실패하는 테스트**

```rust
#[test] fn estimate_counts_chars_not_bytes() { assert_eq!(estimate_tokens("가나다라마바".chars().count()), 4); }
#[test] fn unknown_context_falls_back_to_8192() { assert_eq!(resolve_context(None), 8192); assert_eq!(resolve_context(Some(32768)), 32768); }
#[test] fn ratio_uses_last_prompt_tokens_plus_new_text() { assert!((usage_ratio(Some(20_000), 3_000, 32768) - (22_000.0 / 32768.0)).abs() < 1e-9); }
#[test] fn caps_scale_with_context() {
    assert_eq!(trim_trigger_chars(32768), 34_406);
    assert_eq!(tool_output_cap(32768), 16_384);
    assert_eq!(tool_output_cap(4096), 4_000);
    assert_eq!(tool_output_cap(262144), 40_000);
}
```

`exaone.rs`: `detects_lm_studio_overflow` — Step 1에서 받은 실제 오류 문자열로 `looks_like_context_overflow`가 true. 기존 `detects_context_overflow_errors`의 오류 문구 접두어("EXAONE API 응답 오류")는 작업 10에서 바뀌는 문구에 맞춘다.

- [ ] **Step 3: 실패 확인 → 구현.** 1단계 줄이기 조건 `total_chars(...) > COMPACT_TRIGGER_CHARS` → `> context_budget::trim_trigger_chars(req.context_length)`. `tool_result_for_model`의 상한 → `tool_output_cap(req.context_length)`(함수에 인자 추가). `READ_PAGE_CHARS` 같은 상수 기반 페이지 크기는 그대로 둔다. `lib.rs`의 턴 시작부(세 곳: 보내기, 다시 보내기, 승인 후 이어가기)에서 `list_models` → 해당 모델 `loaded[0].context_length` → `resolve_context`.

- [ ] **Step 4: 통과 확인 → 커밋** — `feat: size context trimming and tool output to the loaded LM Studio context`

---

### Task 9: 자동 요약 압축

**Files:**
- Modify: `src-tauri/src/lib.rs` (`compact_conversation` 본문을 `fn`으로 분리, `send_chat_message`에서 호출), `src/api.ts` (`onAutoCompaction`), `src/App.tsx` (진행 표시)
- Test: `lib.rs` 테스트 모듈

**Interfaces:**
- Consumes: 작업 8 `usage_ratio`, `AUTO_COMPACT_RATIO`, 작업 3 `AppSettings.auto_compact`, 원본 `exaone::summarize_for_compaction`, `exaone::build_local_compaction_summary`.
- Produces:
  - `fn compaction_cut(conv: &Conversation, keep_recent: usize) -> Option<usize>` — 원본 `compact_conversation`의 `covered` 계산(직전 user 경계 맞춤)을 꺼낸 것. 0이면 `None`.
  - `fn should_auto_compact(settings: &AppSettings, conv: &Conversation, new_user_chars: usize, context_length: u32) -> bool` — `auto_compact` 켜짐 && 마지막 어시스턴트 메시지 `stats.prompt_tokens` 기반 `usage_ratio >= 0.70` && `compaction_cut(conv, 2).is_some()`
  - 이벤트 `"anylm-auto-compaction"` 페이로드 `{ conversationId, phase: "started" | "done" | "fallback" }`
  - TS `BackendAPI.onAutoCompaction(cb)`

- [ ] **Step 1: 실패하는 테스트**

```rust
#[test]
fn auto_compact_triggers_at_70_percent() {
    let conv = conv_with(10, /* last assistant prompt_tokens */ 23_000); // 메시지 10개
    assert!(should_auto_compact(&AppSettings::default(), &conv, 0, 32768));   // 23000/32768 = 0.70...
    assert!(!should_auto_compact(&AppSettings::default(), &conv_with(10, 20_000), 0, 32768));
}
#[test]
fn auto_compact_respects_setting() { let mut s = AppSettings::default(); s.auto_compact = false; assert!(!should_auto_compact(&s, &conv_with(10, 30_000), 0, 32768)); }
#[test]
fn auto_compact_skips_when_nothing_to_cover() { assert!(!should_auto_compact(&AppSettings::default(), &conv_with(1, 30_000), 0, 32768)); }
#[test]
fn compaction_cut_matches_original_boundary_rule() { /* user,assistant,user,assistant,user,assistant + keep 2 → Some(4); 메시지 2개 + keep 2 → None */ }
```

(`conv_with(n, tokens)`: user/assistant가 번갈아 나오는 메시지 n개, 마지막 어시스턴트에 `stats.prompt_tokens = tokens`인 테스트용 함수.)

- [ ] **Step 2: 실패 확인 → 구현.** `send_chat_message`에서 사용자 메시지를 기록에 넣기 **전에** `should_auto_compact(..., user_content 글자 수, context_length)`를 보고, 참이면 이벤트 `started` → 원본과 같은 방식으로 AI 요약(`keep_recent` 2, focus 없음) → 실패하면 `build_local_compaction_summary`로 대신하고 `fallback`, 성공하면 `done` → 저장된 `conv.compaction`으로 턴을 진행한다. 다시 보내기·승인 후 이어가기(한 턴 안)에서는 호출하지 않는다. 화면: `started`이면 대화 맨 아래 "컨텍스트가 70% 찼습니다. 이전 대화를 요약하는 중…", 끝나면 원본 압축 카드가 보인다.

- [ ] **Step 3: 통과 확인.** 세 검증 명령 PASS. 실측: 9B를 8192로 로드하고 긴 답변이 나오는 질문을 몇 번 → 압축 카드가 생기고 대화가 이어진다. 실측 뒤 32768로 되돌린다.

- [ ] **Step 4: 커밋** — `feat: auto summary compaction at 70% of loaded context`

---

### Task 10: K-EXAONE 전용 요소와 문구 걷어 내기

**Files:**
- Modify: `src/App.tsx` (`RateBadge`, `LowBudgetNotice`, `keyAgeDays`/`KEY_VALID_DAYS`, `REASONING_EFFORTS`, `rateInfo` 상태와 구독, 헤더 "K-EXAONE" 표시와 비공식 안내 툴팁, 압축 창 "EXAONE이 … (1회 호출)", 프로젝트 지침 자리표시 "EXAONE이 참고할…", 통계 툴팁), `src/api.ts` (`getRateInfo`), `src/types.ts` (`RateInfo`), `src-tauri/src/exaone.rs` (`RateInfo`, `parse_rate_info`, `remember_rate`, `set_rate_file`, `last_rate_info`, `rate_limit_headers`, 하루 한도·키 만료 오류 문구, `result_code` 형식 오류 처리, `base_system_prompt`의 정체성 두 곳), `src-tauri/src/lib.rs` (`rate_info` 명령, `set_rate_file` 호출), 그 밖의 `src/**` 사용자 문구
- Test: `exaone.rs` 테스트 모듈, `src/ui/noBrandText.test.ts`(새 파일)

**Interfaces:**
- Consumes: 작업 6 `TurnRequest.model_key`, 모델 표시 이름(턴 시작 시 `list_models`에서).
- Produces: `base_system_prompt(project: Option<&Project>, model_display_name: &str) -> String`; 통계 줄 "입력 n · 출력 n · 추론 n토큰 · 초당 n.n토큰 · {모델}".

- [ ] **Step 1: 실패하는 테스트**

```rust
#[test]
fn system_prompt_names_app_and_model() {
    let p = base_system_prompt(None, "Qwen3.5 9B");
    assert!(p.contains("Any LM Desktop") && p.contains("Qwen3.5 9B"));
    assert!(!p.contains("EXAONE") && !p.contains("LG"));
}
```

```ts
// noBrandText.test.ts — 화면 소스에 브랜드 문구가 남지 않았는지
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
const walk = (d: string): string[] => readdirSync(d).flatMap((n) => (statSync(join(d, n)).isDirectory() ? walk(join(d, n)) : [join(d, n)]));
it("has no K-EXAONE branding in UI sources", () => {
  const files = walk("src").filter((f) => /\.(tsx|ts)$/.test(f) && !f.endsWith(".test.ts"));
  const hits = files.filter((f) => /K-EXAONE|EXAONE|LG AI/.test(readFileSync(f, "utf8").replace(/onExaoneStream|exaone-stream/g, "")));
  expect(hits).toEqual([]);
});
```

- [ ] **Step 2: 실패 확인 → 구현.** 시스템 프롬프트 첫 문장: `"당신은 Any LM Desktop의 어시스턴트입니다. 지금 답하는 모델은 {model}입니다."` — 그 뒤 작업 지침은 원본 그대로. 오류 접두어 "EXAONE API 응답 오류" → "LM Studio 응답 오류". 401/403 → "LM Studio가 인증을 요구합니다. [LM Studio 연결]에서 API 토큰을 입력해 주세요.". 초당 토큰 = `completionTokens / (elapsedMs / 1000)`.

- [ ] **Step 3: 통과 확인** — 세 검증 명령 PASS, `git grep -n -i "exaone" -- src ':!*.test.ts'` 결과가 `onExaoneStream`/`exaone-stream`/import 경로뿐.

- [ ] **Step 4: 커밋** — `refactor: remove K-EXAONE-only UI, rate limits and branding`

---

### Task 11: harness를 LM Studio로, 문서 정리

**Files:**
- Modify: `src-tauri/src/harness.rs` (자격 증명 대신 `ANYLM_SERVER`(기본 `http://localhost:1234`)·`ANYLM_MODEL`(필수)·`ANYLM_CONTEXT`(기본 32768) 환경 변수, `EXA_MAX_CALLS`/`EXA_DAY_RESERVE`와 한도 처리 제거, 출력 폴더 변수 `ANYLM_OUT`), `harness/make_scenarios.py`·`harness/report.py`(문구), `README.md`, `CHANGELOG.md`, `NOTICE`, `THIRD_PARTY_NOTICES.md`(`tauri-plugin-updater` 항목 제거 — `scripts/third_party_notices.py`로 다시 생성)
- Test: `harness.rs` 테스트 모듈

**Interfaces:**
- Produces: `harness::config_from_env(get: impl Fn(&str) -> Option<String>) -> Result<HarnessConfig, String>`, `HarnessConfig { server_url: String, model: String, context_length: u32, out_dir: PathBuf }`

- [ ] **Step 1: 실패하는 테스트** — `config_requires_model`(ANYLM_MODEL 없으면 Err에 "ANYLM_MODEL"), `config_defaults`(server_url·context 기본값).

- [ ] **Step 2: 실패 확인 → 구현 → 통과 확인.** README: 무엇인지, 필요한 것(Windows, LM Studio 0.4 이상, 서버 켜기), 개발 명령, harness 사용법, 데이터 위치(`%APPDATA%\io.github.kidarigun.anylm-desktop`), "K-EXAONE Desktop 0.4.4(Apache-2.0)에서 갈라져 나온 비공개 프로젝트". CHANGELOG `## 0.1.0 — 미출시`에 작업 2~10 요약. NOTICE에 출처와 상표 안내.

- [ ] **Step 3: 실측** — `$env:ANYLM_MODEL="qwen3.5-9b"; cd src-tauri; cargo test --lib harness::live -- --ignored --nocapture` → 시나리오가 끝까지 돈다. 결과 요약을 STATUS "마지막 확인"에.

- [ ] **Step 4: 커밋** — `chore: point harness at LM Studio and rewrite docs`

---

### Task 12: 완료 기준 확인과 인계 연습

**Files:**
- Modify: `docs/handoff/STATUS.md`, `docs/handoff/LOG.md`

- [ ] **Step 1: 스펙 7절 완료 기준 1~8을 하나씩 실측**하고, 기준마다 결과(통과/실패, 근거)를 STATUS "마지막 확인"에 적는다. 실패한 기준은 새 작업으로 이 계획 끝에 추가한다.

- [ ] **Step 2: 인계 연습(완료 기준 9).** 사용자에게 다른 도구(Codex 또는 Antigravity)에서 `docs/handoff/START.md`의 시작 문구로 세션을 열어 달라고 요청한다. 그 도구가 STATUS만 보고 남은 일(또는 작은 연습 작업: README 오탈자 수정 같은 것)을 이어서 커밋하는지, 커밋에 `Agent:` 줄이 붙는지, STATUS·LOG를 갱신하는지 확인한다. 어긋난 점은 `AGENTS.md`에 규칙으로 보탠다.

- [ ] **Step 3: 최종 검증과 커밋** — 세 검증 명령 PASS → `docs: record acceptance results and handoff drill`. 사용자 승인을 받은 뒤 `git push origin main`.
