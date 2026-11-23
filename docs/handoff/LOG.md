# 작업 기록 (LOG)

맨 아래에 추가만 한다. 형식: `날짜 | 도구/모델 | 한 일 | 커밋`

- 2026-11-23 | claude-code | 원본 K-EXAONE Desktop 0.4.4(e885b02)를 복제해 저장소를 만들고 인계 문서를 작성함(작업 1) | c15fab8
- 2026-11-23 | claude-code | 정체성 분리(이름·identifier·버전·포트·자격 증명 서비스명 등)와 업데이트 기능 제거(작업 2). 원본과 동시 실행 확인 | (작업 2 커밋)
- 2026-11-23 | claude-code | 설정을 LM Studio 연결로 교체(작업 3): lmstudio.rs, AppSettings 개편, API 키 필수 검사·추론 강도 UI 삭제 | c7a87c1
- 2026-11-23 | claude-code | 모델 목록·로드 계획·로드/언로드·서버 상태/시작 구현(작업 4), 실측 결정 기록 0002, lm_* 명령과 TS 바인딩 | 95e9899
- 2026-11-23 | claude-code | 작업 4 보완: 이미 로드된 모델의 두 번째 인스턴스 로드를 거부(ensure_not_loaded), 컨텍스트를 못 읽은 인스턴스도 로드된 것으로 유지 | 09f89de
- 2026-11-23 | claude-code | LM Studio 연결 창(ConnectionDialog, modelText)과 사이드바 연결 점 추가(작업 5) | ae417bd
- 2026-11-23 | claude-code | 작업 5 보완: 컨텍스트 입력 검증(빈 값·숫자 아님·1024 미만은 미설정, 최대 초과는 최대로)과 wanted_context의 1024 미만 무시 | 62d5427
- 2026-11-23 | claude-code | 대화별 모델 선택(작업 6): Conversation/ChatMessage.model_key, conversation_model, set_conversation_model, TurnRequest.model_key, ModelPicker·LoadChoiceDialog·sendFlow(보내기 전 planLoad 흐름) | 50dc8fb
- 2026-11-23 | claude-code | 작업 6 보완: 수정 후 다시 보내기에서 모델 준비를 화면 잘라내기 전에 하도록 순서 변경, 모델 준비 중 모델 선택 비활성화 | (이 커밋)
