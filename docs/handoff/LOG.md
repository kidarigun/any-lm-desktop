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
- 2026-11-23 | claude-code | 작업 6 보완: 수정 후 다시 보내기에서 모델 준비를 화면 잘라내기 전에 하도록 순서 변경, 모델 준비 중 모델 선택 비활성화 | 984cd50
- 2026-11-23 | claude-code | 추론 켜기/끄기(작업 7): 실측으로 reasoning_effort:"none"만 reasoning_tokens 0 확인(결정 0003), lmstudio::reasoning_fields·REASONING_OFF_FIELDS, 요청 본문 반영, preserve_thinking/keep_thinking 제거, 입력창 아래 "생각하기" 토글 | 0c03213
- 2026-11-23 | claude-code | 컨텍스트 예산(작업 8): context_budget.rs, 턴 시작 때 모델 목록 한 번으로 추론 선택지+로드된 컨텍스트(turn_model_info), 줄이기 기준·도구 출력 상한을 컨텍스트에 비례, LM Studio 초과 오류 인식(결정 0004) | 1fa48fa
- 2026-11-23 | claude-code | 작업 8 보완: 스트림 안 컨텍스트 초과 오류(HTTP 200 + event: error)도 단계 압축으로 다시 보내기, read_file 한 쪽 크기를 도구 출력 상한에 맞춰 안내문(offset)이 잘리지 않게 함 | 9d7e634
- 2026-11-23 | claude-code | 자동 요약 압축(작업 9): compaction_cut·should_auto_compact(마지막 답변 prompt_tokens 기준 70%), send_chat_message가 사용자 메시지를 넣기 전 AI 요약(실패 시 로컬 요약)·저장 후 턴 진행, 이벤트 anylm-auto-compaction, 요약 요청에 대화 모델·생각 끄기 반영, onAutoCompaction과 진행 표시 | 9b6d6d1
- 2026-11-23 | claude-code | K-EXAONE 전용 요소 제거(작업 10): 호출 한도(RateInfo·rate_info 명령·RateBadge·LowBudgetNotice·하루 한도/키 만료 오류) 삭제, 시스템 프롬프트 정체성을 Any LM Desktop+모델 표시 이름으로(TurnModelInfo.display_name → TurnRequest.model_display_name), 오류 접두어 'LM Studio 응답 오류'·401/403 인증 안내, 통계 줄 '입력·출력·추론·초당 토큰·모델'(chat/statsLine.ts), 헤더·지침 자리표시·문서 메타데이터·MCP clientInfo·User-Agent 문구 교체, noBrandText 테스트 | 71ece0f
- 2026-11-23 | claude-code | 작업 10 보완: 화면·모델에 보이는 한글 브랜드 '엑사원'을 '모델'/'어시스턴트'로 교체(내보내기 제목, 승인 안내, 캔버스 안내, Skill·MCP·Python 창, 기본 답변 파일 이름 '답변 날짜', Rust 상태 문구·Skill 서식·MCP 이미지 안내), 브랜드 테스트 정규식에 엑사원 추가, secrets.rs 오류를 'API 토큰'으로 | 1b5c077
- 2026-11-23 | claude-code | 하니스를 LM Studio로(작업 11): harness::config_from_env/HarnessConfig(ANYLM_MODEL 필수·ANYLM_SERVER·ANYLM_CONTEXT·ANYLM_TOKEN·ANYLM_OUT·ANYLM_SCENARIOS·ANYLM_ONLY·ANYLM_PAUSE_SEC·ANYLM_RERUN), 자격 증명 관리자·EXA_MAX_CALLS·EXA_DAY_RESERVE·호출 상한 삭제, 모델 표시 이름·추론 선택지는 turn_model_info로, 시험용 EXA_* 변수도 ANYLM_*로. report.py 한도 줄·smoke.json 문구 정리, README·CHANGELOG(0.1.0)·NOTICE 재작성, THIRD_PARTY_NOTICES 재생성(updater 항목 제거). 실측: smoke-hello 통과(qwen3.5-9b, 52초) | 5cb2690
- 2026-11-23 | claude-code | 완료 기준 1~8 실측(작업 12 단계 1): 테스트 3종 통과, 앱·원본 동시 실행 확인, curl 모델 라우팅, 하니스 proj-multiturn 실측, 결과를 STATUS 표와 수동 확인 목록(화면별)에 기록. 실패 기준 없음 | (이 커밋)
