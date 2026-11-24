# 0012 — 멀티 로컬 LLM 공급자 지원 (LM Studio · Ollama · OpenAI 호환)

- 앱의 로컬 전용 보안 원칙(`http://localhost`, `http://127.0.0.1`, `http://[::1]`만 허용)을 유지한 채 `lmstudio`(기본 `1234`), `ollama`(기본 `11434`), `openai_compat`(기본 `8080`, llama.cpp·vLLM·LocalAI 등) 3개 공급자를 선택해 쓸 수 있게 한다.
- `AppSettings`에 `provider`와 `providers`(공급자별 `ProviderProfile`: `serverUrl`, `model`, `apiKeySet`, `modelPrefs`)를 추가하고, 기존 최상위 필드(`serverUrl`, `model`, `apiKeySet`, `modelPrefs`)는 활성 공급자 프로필과 항상 동기화한다. 덕분에 예전 저장 파일과 최상위 필드만 바꾸는 부분 저장 호출(`modelPrefs` 등)이 손실 없이 호환된다.
- API 토큰은 `secrets.rs`에서 공급자별 Windows 자격 증명 계정(`lm-studio-api-token`, `ollama-api-token`, `openai-compat-api-token`)에 분리 보관한다.
- Ollama는 모델 목록·로드 상태·기능 배지·로드/언로드에 `/api/tags`, `/api/ps`, `/api/show`, `/api/generate`를 사용하고, 실제 대화·도구 호출·스트리밍·추론 끄기는 공통 `/v1/chat/completions` 경로를 사용해 스트리밍/도구 파서를 이원화하지 않는다.
- `ollama serve`는 데몬이 끝나지 않고 파이프를 붙잡으므로 `lmstudio::run_quiet` 대신 `CREATE_NO_WINDOW | DETACHED_PROCESS`로 분리 실행한 뒤 `/api/version`을 최대 15초 확인한다.
- 내부 Tauri 명령 이름(`lm_list_models`, `lm_plan_load`, `lm_load_model`, `lm_unload`, `lm_server_running`, `lm_start_server`)과 `lmstudio.rs`의 기존 구현·테스트는 유지하고 `src-tauri/src/providers/` 어댑터를 통해 활성 공급자로 분기한다.
