# 멀티 로컬 LLM 공급자 구현 계획

설계: `docs/superpowers/specs/2026-11-24-multi-local-providers-design.md`

- [x] **단계 1: 설계·계획·결정 문서 작성**
  - `docs/superpowers/specs/2026-11-24-multi-local-providers-design.md`
  - `docs/superpowers/plans/2026-11-24-multi-local-providers.md`
  - `docs/decisions/0012-multi-local-providers.md`
- [x] **단계 2: Rust 설정 및 자격 증명 확장 (`store.rs`, `secrets.rs`)**
  - `LlmProvider`, `ProviderProfile`, `AppSettings` 동기화 및 하위 호환 테스트
  - 공급자별 Windows 자격 증명 관리자 계정 분리(`load/store/delete_provider_api_key`)
- [x] **단계 3: Rust 공급자 어댑터 모듈 구현 (`src-tauri/src/providers/`)**
  - `providers/mod.rs`: `ProviderCapabilities`, 로컬 URL 보안 검증, 공통 디스패치
  - `providers/ollama.rs`: `/api/tags`, `/api/ps`, `/api/show`, `/api/generate`, `ollama serve` 분리 실행
  - `providers/openai_compat.rs`: URL 정규화, `/v1/models` 파싱, 즉시 Ready `plan_load`(빈 목록+수동 ID 포함), `turn_model_info`
- [x] **단계 4: 에이전트 루프·명령·하니스 연동 (`exaone.rs`, `lib.rs`, `harness.rs`)**
  - 공급자 라우팅 및 오류/재시도 문구 일반화, 오버플로우 패턴 확장
  - `save_settings`/`clear_api_key`/`load_api_key_into` 공급자별 프로필 및 토큰 동기화
  - `harness.rs` `ANYLM_PROVIDER` 지원
- [x] **단계 5: 프런트엔드 타입·유틸·UI 구현 (`src/types.ts`, `src/api.ts`, `src/lmstudio/`, `src/App.tsx`)**
  - `providerMeta.ts` + 단위 테스트, `modelText.ts` + `sendFlow.ts` 확장
  - `ConnectionDialog.tsx` 3종 공급자 전환 탭·기능별 UI, `ModelPicker.tsx`, `App.tsx` 연동
- [x] **단계 6: 모의 HTTP 테스트 및 전체 검증**
  - Ollama / OpenAI 호환 모의 HTTP 통합 테스트
  - `npm test`, `npx tsc --noEmit`, `cargo test --lib`, `cargo check --all-targets`, `npm run build` 통과
- [x] **단계 7: Claude Code / Codex 인계 문서 갱신 및 커밋**
  - `AGENTS.md`, `docs/handoff/STATUS.md`, `docs/handoff/LOG.md` 갱신
  - 커밋 (`Agent: antigravity`)
