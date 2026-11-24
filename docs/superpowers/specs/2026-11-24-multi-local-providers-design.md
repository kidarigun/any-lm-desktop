# 멀티 로컬 LLM 공급자(LM Studio · Ollama · OpenAI 호환) — 설계

날짜: 2026-11-24 · 상태: 사용자 결정 반영 · 위치: `docs/superpowers/specs/2026-11-24-multi-local-providers-design.md`

## 1. 배경과 목표

Any LM Desktop은 처음에 LM Studio 전용으로 설계되었으나, 사용자는 같은 PC에서 Ollama나 OpenAI 호환 로컬 서버(llama.cpp, vLLM, LocalAI 등)도 선택해 쓰고자 한다.

### 사용자 결정
1. **로컬 전용 보안 유지**: `http://localhost`, `http://127.0.0.1`, `http://[::1]`만 허용한다. 외부/HTTPS 원격 서버는 허용하지 않는다.
2. **활성 공급자 선택 + 공급자별 설정 기억**: 연결 창에서 3개 공급자 중 하나를 활성화하되, 각 공급자의 서버 주소·기본 모델·API 토큰 보유 여부·모델별 설정(`modelPrefs`)을 따로 보관한다.
3. **지원 깊이**:
   - **LM Studio**: 기존 전용 기능 100% 유지(로드/언로드/컨텍스트 지정/`lms server start`/추론 토글).
   - **Ollama**: `/api/tags` + `/api/ps` + `/api/show`로 모델 목록·로드 상태·기능 배지(비전/도구/추론)·최대 컨텍스트를 읽고(임베딩 전용 모델 제외), `/api/generate`로 사전 로드(`options.num_ctx`) 및 언로드(`keep_alive: 0`), `ollama serve` 분리 실행으로 서버 시작을 지원한다. 채팅은 공통 `/v1/chat/completions`를 사용한다.
   - **OpenAI 호환**: `/v1/models` + `/v1/chat/completions`로 연동한다. 상시 서빙 모델로 간주해 로드/언로드/서버 시작을 비활성화하고, 사용자가 입력한 모델별 컨텍스트는 자동 요약 압축과 도구 출력 상한 계산에 사용한다.

## 2. 데이터 모델과 하위 호환

### 2.1 설정(`AppSettings` · `ProviderProfile`)
- `LlmProvider`: `lmstudio`(기본, `http://localhost:1234`) | `ollama`(`http://localhost:11434`) | `openai_compat`(`http://localhost:8080`).
- `AppSettings`에 `provider: LlmProvider`와 `providers: BTreeMap<LlmProvider, ProviderProfile>`를 추가한다.
- `ProviderProfile`은 `{ serverUrl, model, apiKeySet, modelPrefs }`를 담는다.
- 기존 최상위 필드(`serverUrl`, `model`, `apiKeySet`, `modelPrefs`)는 활성 공급자 프로필과 항상 동기화한다. 따라서:
  - 예전 `anylm_desktop_store.json`을 읽을 때 `provider` 기본값 `lmstudio`로 열리고 기존 값이 `providers.lmstudio`로 자동 이관된다.
  - 화면이나 기존 코드가 최상위 `modelPrefs`만 수정해 `saveSettings`를 호출해도 백엔드가 활성 프로필로 자동 동기화한다.

### 2.2 자격 증명(`secrets.rs`)
- Windows 자격 증명 관리자 서비스 `io.github.kidarigun.anylm-desktop` 아래에 공급자별 계정을 둔다:
  - `lmstudio`: `lm-studio-api-token` (기존 유지 + 레거시 `exaone-api-key` 읽기/삭제 호환)
  - `ollama`: `ollama-api-token`
  - `openai_compat`: `openai-compat-api-token`

## 3. 백엔드 공급자 어댑터(`src-tauri/src/providers/`)

- `providers/mod.rs`:
  - `ProviderCapabilities` (`supportsLoad`, `supportsUnload`, `supportsStartServer`, `supportsReasoningToggle`)
  - 공통 로컬 URL 검증(`validate_local_url`, `local_origin`), 공급자별 오류·서버 꺼짐 문구(`server_off_message`, `friendly_error`, `api_error_message`), 공통 디스패치(`list_models`, `plan_load`, `load_model`, `unload_instance`, `server_running`, `start_server`, `turn_model_info`, `chat_url`).
- `providers/ollama.rs`:
  - `/api/tags`, `/api/ps`, `/api/show` 파서 및 병렬 조회(`buffer_unordered(4)`).
  - `/api/generate` 로드/언로드.
  - `ollama serve` 백그라운드 분리 실행(`CREATE_NO_WINDOW | DETACHED_PROCESS`) 후 최대 15초 `/api/version` 폴링.
  - `turn_model_info`: 3초 제한 내에 `/api/ps` + 대상 모델 `/api/show` 조회, `/api/ps`에 없으면 `modelPrefs` 폴백.
- `providers/openai_compat.rs`:
  - 경로 접두어를 보존하되 끝의 `/v1`, `/chat/completions`, `/models`를 정규화하는 `urls()`.
  - `/v1/models`(`data` 또는 `models` 배열, `max_model_len` / `context_length` / `meta.n_ctx_train`) 파싱.
  - `plan_load`: 모델이 목록에 있으면 바로 `Ready`, 없으면 `Missing`(목록 자체가 비었더라도 사용자가 직접 모델 ID를 입력한 경우를 고려해 서버가 빈 목록을 반환하면 수동 입력값은 `Ready` 허용 가능 여부 검토 — 목록에 없으면 `Missing`이되 `ConnectionDialog`와 수동 입력 지원).
- `lmstudio.rs`: 기존 LM Studio 전용 구현과 테스트를 그대로 보존. `lib.rs`/`exaone.rs`/`harness.rs`가 `providers::*`를 통해 공급자별로 라우팅한다.
- 내부 Tauri 명령 이름(`lm_list_models`, `lm_plan_load`, `lm_load_model`, `lm_unload`, `lm_server_running`, `lm_start_server`)은 `AGENTS.md` 규칙에 따라 호환 유지한다.

## 4. 에이전트 루프 및 하니스

- `exaone.rs`:
  - `providers::chat_url(settings)`, `providers::friendly_error(settings.provider, &e)` 사용.
  - 스트림/재시도/인증/요약 오류 문구에 활성 공급자 이름을 반영(LM Studio 선택 시 기존 문구와 100% 동일).
  - `looks_like_context_overflow`에 Ollama·llama.cpp·vLLM의 컨텍스트 초과 문구 패턴 추가.
- `harness.rs`:
  - `ANYLM_PROVIDER` 환경 변수 추가(기본 `lmstudio`). `ANYLM_SERVER` 미지정 시 해당 공급자의 기본 로컬 URL 사용.

## 5. 프런트엔드 UI

- **사이드바 연결 버튼**: `{공급자 라벨} 연결` + 연결 상태 점.
- **`ConnectionDialog.tsx`**:
  - 상단에 `[LM Studio]` · `[Ollama]` · `[OpenAI 호환]` 전환 탭.
  - 탭을 전환해 `draft.provider !== savedProvider`인 동안은 이전 공급자 모델이 섞이지 않도록 모델 목록과 로드 버튼을 숨기고 `"[저장]을 누르면 {공급자} 서버에 연결하고 모델 목록을 불러옵니다."`를 표시.
  - 공급자 기능에 따라 `[서버 시작]`, `[로드]`, `[다시 로드]`, `[언로드]` 표시 여부 제어.
  - `[저장된 토큰 삭제]`는 현재 탭 공급자의 토큰만 삭제.
- **`ModelPicker.tsx`**: `supportsLoad === false`일 때 `로드됨`/`로드 안 됨` 그룹 구분 없이 단일 목록으로 표시.
- **오류 띠 `[서버 시작]`**: `supportsStartServer === true`인 공급자(LM Studio, Ollama)에서만 표시.

## 6. 완료 기준

1. Rust 단위 테스트(`cargo test --lib`): 설정 마이그레이션, 3종 공급자 URL 검증(외부/원격 차단), Ollama 파싱 및 모의 HTTP, OpenAI 호환 경로 정규화 및 모의 HTTP, 컨텍스트 초과 문구, 하니스 `ANYLM_PROVIDER` 모두 통과.
2. 프런트엔드 테스트(`npm test`): `providerMeta`, `modelText`, `sendFlow`, `noBrandText` 포함 전체 통과.
3. `npx tsc --noEmit`, `cd src-tauri && cargo check --all-targets`, `npm run build` 모두 경고/오류 없이 성공.
4. `AGENTS.md`, `docs/handoff/STATUS.md`, `docs/handoff/LOG.md`에 변경 사항과 수동 확인 목록 기록.
