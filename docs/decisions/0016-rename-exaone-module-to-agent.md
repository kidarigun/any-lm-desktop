# 0016 — 핵심 에이전트 모듈(`exaone.rs`)과 스트림 심볼을 `agent.rs` / `anylm-stream`으로 변경

사용자 지시에 따라 원본 프로젝트명을 따르고 있던 핵심 에이전트 루프 모듈 및 스트림 이벤트 식별자를 Any LM Desktop 프로젝트에 맞는 이름으로 변경한다.

## 변경 내역

- Rust 모듈 파일: `src-tauri/src/exaone.rs` → `src-tauri/src/agent.rs` (`pub mod agent`)
- 메인 턴 함수: `run_exaone_turn` → `run_agent_turn`
- 스트림 Tauri 이벤트: `"exaone-stream"` → `"anylm-stream"` (`agent::STREAM_EVENT`)
- 프런트엔드 리스너: `BackendAPI.onExaoneStream` → `BackendAPI.onAgentStream`
- 브랜드 검사 테스트(`src/ui/noBrandText.test.ts`): `onExaoneStream|exaone-stream` 예외 제거

## 유지 항목 (호환성 및 데이터 소스 보호)

- 기존 사용자의 저장 파일·자격 증명·런타임 호환용 레거시 상수(`store.rs` `LEGACY_STORE_NAME`, `secrets.rs` `LEGACY_ACCOUNT`, `runtime.rs` `LEGACY_COMPLETE_MARK`)와 기본 카탈로그 URL(`catalog.rs` `DEFAULT_CATALOG_URL`, 결정 0014)은 그대로 유지한다.
- 향후 원본(`upstream`)에서 커밋을 cherry-pick할 때 `src-tauri/src/exaone.rs`를 수정하는 패치가 있으면 `src-tauri/src/agent.rs`로 매핑해 반영한다.
