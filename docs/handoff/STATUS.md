# STATUS — 현재 상태 한 장

## 지금 목표
원본 K-EXAONE Desktop 0.4.4를 멀티 로컬 LLM(LM Studio · Ollama · OpenAI 호환) 데스크톱 작업 앱 "Any LM Desktop"(0.1.1)으로 바꾸고 고도화한다. 설계: `docs/superpowers/specs/2026-11-23-any-lm-desktop-design.md`, 멀티 공급자 설계: `docs/superpowers/specs/2026-11-24-multi-local-providers-design.md`.

## 진행 중
- 계획 파일: `docs/superpowers/plans/2026-11-23-any-lm-desktop.md`
- 후속 계획: `docs/superpowers/plans/2026-11-24-resume-and-agent-loop.md`, `docs/superpowers/plans/2026-11-24-multi-local-providers.md`, `docs/superpowers/plans/2026-11-24-updater-settings-release.md`
- 작업 1(저장소 만들기와 인계 체계): 완료. 단계 6은 사용자 승인 뒤 2026-11-23에 GitHub 비공개 저장소 `kidarigun/any-lm-desktop`을 만들고 `origin`으로 `main`을 push했다.
- 작업 2(정체성 분리와 업데이트 제거): 완료. 제품명·identifier·버전·크레이트명·자격 증명 서비스명·단일 실행 포트(45789 → 최종 검토 뒤 45790, MCP OAuth 포트도 45789 → 45791)·미리보기 호스트·대화상자 환경 변수·localStorage 접두어(`anylm_`)·기본 작업 폴더(`문서\AnyLM`)를 바꾸고, 업데이트 기능(`tauri-plugin-updater`, UpdatePanel, `check_update`/`install_update`, `checkUpdates` 설정, `make_latest_json.py`)과 `migrate_old_data_dir`을 삭제했다.

- 작업 3(설정을 LM Studio 연결로 바꾸기): 완료. `lmstudio.rs`(server_urls·authorize·friendly_error) 추가, `AppSettings`를 serverUrl/선택 토큰/autoCompact/modelPrefs로 교체, exaone.rs의 API 키 필수 검사·reasoning_effort 삭제, 추론 강도 선택 UI 삭제(작업 6에서 모델 선택기가 들어갈 자리). 설정 대화상자는 최소 수정만 했다(작업 5에서 재구성).

- 작업 4(모델 목록·로드·서버 상태): 완료. `lmstudio.rs`에 parse_models·plan_load·wanted_context·list/load/unload·server_running·lms_path·start_server, Tauri 명령 `lm_*` 6개, TS `BackendAPI`(listModels 등)와 타입 추가. 결정 기록 `docs/decisions/0002-lmstudio-load-api.md`. 실측 후 LM Studio는 qwen3.5-9b(32768)만 로드된 상태로 복구함.
- 작업 5(LM Studio 연결 창): 완료. `src/lmstudio/`(modelText, ConnectionDialog)를 추가하고 설정 창을 교체. 사이드바 버튼은 [LM Studio 연결] + 연결 점(열 때·15초마다·저장 후 `serverRunning()` 확인). 창에서 서버 시작, 모델 로드/다시 로드(언로드 후 로드)/언로드, 기본 모델 선택, 모델별 컨텍스트(modelPrefs), 자동 요약 압축을 다룬다. 원본에 문체 선택 UI가 없어 추가하지 않았다.
- 작업 6(대화별 모델 선택): 완료. 대화마다 `modelKey`를 저장하고(비면 기본 모델) 요청 본문 `"model"`과 답변의 `modelKey`에 쓴다. 입력창 아래 ModelPicker(로드된 모델이 위, `loadedLabel` 표시), 보내기 전 `planLoad`→`nextSendStep` 흐름(바로 보내기 / 로드 중 표시 후 로드 / LoadChoiceDialog / 컨텍스트 작게 로드됨 안내와 [다시 로드] / 모델 없음 / 로드 실패 시 [내리고 로드]). 프로젝트·코드 화면에서 도구 미지원 모델이면 경고만 한다. 답변 아래 통계 줄 앞에 모델 이름. 하니스(harness.rs)는 settings.model만 넘긴다(작업 11에서 정리).
- 작업 7(추론 켜기/끄기): 완료. 실측 결과 `reasoning_effort: "none"`만 9B·35B 모두 reasoning_tokens 0(결정 0003). `lmstudio::reasoning_fields`/`REASONING_OFF_FIELDS`, 턴 시작 때 모델 목록으로 `TurnRequest.reasoning_options`를 채워 요청 본문에 반영(작업 8이 같은 목록을 재사용할 수 있음), `preserve_thinking` 삭제·`keep_thinking`은 항상 false. ModelPicker 옆 "생각하기" 체크(추론 선택지가 있는 모델에서만, `modelPrefs[key].reasoning` 저장). 실측 후 LM Studio는 qwen3.5-9b(32768)만 로드된 상태.
- 작업 8(컨텍스트 예산): 완료. `context_budget.rs`(FALLBACK 8192, 0.70, 1.5자/토큰, trim_trigger_chars, tool_output_cap, resolve_context, usage_ratio). 턴 시작 때 `lmstudio::turn_model_info`가 모델 목록을 한 번(3초 제한) 읽어 추론 선택지와 로드된 컨텍스트를 함께 주고, `TurnRequest.context_length`가 줄이기 기준(`COMPACT_TRIGGER_CHARS` 삭제)과 도구 결과 상한을 정한다. 초과 오류는 비스트리밍 HTTP 400, 스트리밍은 HTTP 200 안의 `event: error`(결정 0004). 실측 후 qwen3.5-9b(32768) 1개만 로드된 상태.
- 작업 9(자동 요약 압축): 완료. `send_chat_message`가 사용자 메시지를 넣기 전에 `should_auto_compact`(자동 압축 켜짐 && 마지막 답변 `prompt_tokens`+새 글자 추정이 로드된 컨텍스트의 70% 이상 && 덮을 대화 있음)를 보고, 참이면 `anylm-auto-compaction` started → AI 요약(대화의 모델, 생각 끄기 반영; 실패하면 로컬 요약 fallback) → 저장 → 턴 진행. 모델 목록은 턴당 한 번만 읽는다(보내기 전에 읽어 압축 판단과 턴이 함께 씀). 수동 압축도 같은 요약 요청 규칙을 쓴다. 다시 보내기·승인 후 이어가기에서는 자동 압축을 하지 않는다.

- 작업 10(K-EXAONE 전용 요소 걷어 내기): 완료. 호출 한도 표시·추적·오류 문구와 `rate_info` 명령을 지우고, 시스템 프롬프트·헤더·문서 메타데이터·MCP clientInfo·User-Agent 등 사용자에게 보이는 브랜드 문구를 Any LM Desktop/LM Studio로 바꿨다. 모델 표시 이름은 턴 시작 모델 목록(`TurnModelInfo.display_name`)에서 온다. 통계 줄은 `입력 · 출력 · 추론토큰 · 초당 토큰 · 모델`. 하니스의 하루 예비 한도 로직은 삭제(작업 11에서 하니스 전체 정리).
- 작업 11(harness를 LM Studio로, 문서 정리): 완료. `harness::config_from_env`(ANYLM_MODEL 필수, ANYLM_SERVER 기본 http://localhost:1234, ANYLM_CONTEXT 기본 32768, 선택 ANYLM_TOKEN)로 설정을 만들고 앱 설정·자격 증명 관리자는 읽지 않는다. 호출 상한 로직 삭제, 시험용 변수는 모두 `ANYLM_*`(EXA_* 없음). README·CHANGELOG(0.1.0)·NOTICE 재작성, THIRD_PARTY_NOTICES 재생성(updater 항목 제거).

- 최종 검토 수정(작업 12 단계 1 뒤, 검토 `.superpowers/sdd/2026-11-23-any-lm-desktop/final-review.md`): 완료.
  1. 포트: 중복 실행 45790, MCP OAuth 로그인 결과 45791(원본 45788·45789와 분리, 서로도 분리, 테스트로 고정).
  2. 통계 줄 추론 토큰: LM Studio의 `usage.completion_tokens_details.reasoning_tokens`를 읽는다(전에는 늘 0).
  3. 자동 압축: 압축(또는 요약 카드 수정)보다 먼저 잰 `prompt_tokens`는 쓰지 않고 실제로 보낼 부분을 글자로 어림, 새 지점이 이전 압축 지점 뒤일 때만 압축, 이전 요약 위에 이어 요약.
  4. 서버 주소에 경로·질의가 있어도(`/v1` 등) scheme·호스트·포트만 쓴다.
  5. 인계 문서: 이 "다음 할 일" 절 복구, AGENTS 폴더 지도 갱신.
  6. 오류 띠의 [서버 시작](연결 창과 같은 경로), `lms server start`가 파이프 때문에 멈추지 않게(빈 입력·로그 파일·60초 제한).
  7. 로드 실패 시 [{N}K로 다시 시도](시도한 컨텍스트의 절반, 바닥 4096), 오류 띠와 연결 창 모두. 사소한 점 8(컨텍스트 표시를 로드된 컨텍스트·1.5자 기준으로)과 16(runtime.rs 주석)도 고쳤다.

- 프로젝트 지침 편집기(스펙 4절): 완료. 프로젝트 화면 지침 카드 [편집] → 전용 큰 모달(`src/project/InstructionsEditor.tsx`, 규칙 `instructionsFormat.ts`: 글자·약 토큰(1.5자)·로드된 컨텍스트 대비 %, 10% 초과 노란 경고, 서식 예시 4종 넣기/덧붙이기, Esc=취소(변경 있으면 확인), 저장 오류는 모달 안). 컨텍스트를 모르면 토큰만 표시. 프로젝트 설정 창의 작은 입력란은 유지하고 [큰 편집기로 열기] 링크 추가. 메모리 항목은 저장 때 줄바꿈·연속 공백을 한 칸으로 합친다.
- 프로젝트 메모리(스펙 `2026-11-23-project-memory-knowledge.md` 3절): 완료(4절 지침 편집기, 5절 지식 폴더도 아래 줄에서 완료). 구현 위치: `store.rs`(`Project.memory`, `MemoryItem`), `memory.rs`(한도·중복·프롬프트 절), `exaone.rs`(`remember` 도구 `remember_tool`, 프롬프트 절 `build_system_prompt(..., context_length)`), `lib.rs`(`upsert_project`, `*_project_memory` 명령), `src/project/`(ProjectMemoryCard, memoryFormat). 메모리는 턴 시작 때의 프로젝트 복사본으로 프롬프트에 들어가므로 같은 턴 중에 저장한 것은 다음 턴부터 프롬프트에 보인다. 하니스는 앱 상태가 없어 `remember`를 실행하지 못한다.
- 지식 폴더(스펙 5절): 완료. `knowledge.rs`(훑기·건너뛰기·한도, 조각 1,200자/겹침 200, 토큰 = 영문·숫자 소문자 + 한글 2글자 묶음, BM25 k1 1.2·b 0.75, 색인 `앱 데이터\knowledge\<id>.json`에 파일별 크기·수정 시각, 깨지거나 옛 형식이면 다시 만듦), `Project.knowledge_folders`, 명령 `add_knowledge_folder`·`remove_knowledge_folder`·`reindex_knowledge`·`knowledge_status`(색인은 뒤에서, 이벤트 `anylm-knowledge-status`), 턴 시작(보내기·다시 보내기·승인 뒤 이어가기) 빠른 갱신(5초, 남은 것은 다음으로) + 마지막 사용자 메시지로 `[참고 자료]`(마지막 사용자 메시지 바로 앞의 user+assistant 한 쌍, 저장 안 함, min(12000, 컨텍스트×1.5×0.15)자, 6조각, `### 출처: <경로>`; 줄이기 기준·자동 압축 어림에도 넣음. 턴은 갱신을 최대 1.5초만 기다리고 메모리의 색인으로 찾음), `search_knowledge` 도구(승인 없음, 도구 출력 상한, 지식 폴더 있을 때만 목록에). 화면은 프로젝트 화면 "지식 폴더" 카드(`src/project/KnowledgeCard.tsx`). 세부 결정은 `docs/decisions/0005-knowledge-folders.md`. 하니스 시나리오 `harness/knowledge.json`(knowledgeFiles/knowledgeFolders). 실측: 그때 LM Studio에 로드돼 있던 모델이 qwen3.6-35b-a3b(32768)뿐이어서(9B는 로드 안 됨, 로드·언로드하지 않음) 35B로 확인 — 지식 폴더에만 있는 제품 코드의 출시일과 출처 경로가 답에 나옴(3/3, 4/4). 9B 실측은 아직.
- 내장 웹 검색 MCP(결정 0006): 완료. `src/catalog/builtin.ts`에 공식 서버 8개(Tavily·Exa·Brave·Perplexity·Firecrawl·Kagi·Jina·Linkup, 분류 "웹 검색"), `mergeCatalogs`(내장 먼저·같은 id는 내장 우선·원격 `official` 무시), 찾아보기는 원격 목록이 없어도 내장 항목과 오류 띠를 보여 주고 카드에 "공식" 표시. 연결된 서버의 `catalogId`가 내장 검색 항목이면 `[MCP]` 절에 최신 정보는 웹 검색으로 찾고 URL을 밝히라는 한 줄(`ToolSpec.catalog_id`). 키 없이 Exa 실측(`cargo test --lib live_exa -- --ignored`): tools/list에 web_search_exa·web_fetch_exa, 검색 결과 본문 반환. 키가 필요한 제공사는 호출해 보지 않았다.
- 지침 편집기 후속: 프로젝트 설정 창의 [큰 편집기로 열기]는 창의 값이 저장된 값과 같을 때만 쓸 수 있고(다르면 "큰 편집기: 먼저 저장하세요"), 화면의 프로젝트를 바꾸지 않는다. 편집기 Esc는 포커스가 창 밖이어도 듣고(창이 열려 있는 동안만 window 리스너), `role="dialog" aria-modal="true"`.
- Skill 자동 불러오기 + 하니스 MCP(결정 0007): 완료. 앱이 턴 시작 때 요청에 맞는 Skill 하나를 고르고(`skills::select_for_message`: `/이름`, 또는 `-`가 들어 있거나 4글자 이상인 영문 이름(한글 이름은 `/`로만), 아니면 지식 폴더 토큰에서 한 글자·불용어를 뺀 토큰의 IDF 합 ≥2.0·맞은 토큰 ≥2·요청 쪽 비율 ≥0.25·2등과 0.5 차, 흔하지 않은 맞은 토큰이 이름 토큰이거나 2개 이상 — 수정 1·2차) 그 지침을 `[참고 자료]`처럼 질문 바로 앞 한 쌍으로 넣는다(머리 줄 `[Skill: 이름 — …]`, 예산 min(10000, ctx×1.5×0.20), 저장 안 함, 줄이기 기준·자동 압축 어림 포함). 같은 턴 첫 `load_skill` 재호출만 짧은 답, 승인 뒤 이어가기는 카드에 적힌 Skill을 다시 넣음, 고르지 못한 후속 요청은 앞 답변의 Skill을 최대 3턴 이어 넣음(Sticky, 전용 머리 줄 "앞 요청에서 쓰던 Skill… 다른 일이면 무시", 낱말 3개 이상인데 Skill과 겹치는 토큰이 없으면 안 넣음), 참고 자료와 합쳐 컨텍스트×1.5×0.25자 상한(참고 자료가 먼저 줄어듦), 추론만 하고 빈 답으로 멈춘 라운드는 한 번 재촉(`EMPTY_ANSWER_NUDGE`, 저장 안 함). 답변 맨 앞 도구 카드 "Skill 자동 불러옴: 이름"(`ToolExecutionRecord.automatic`, 다음 요청 기록에는 안 보냄). 설정 `skillAutoLoad`(기본 켜짐) — Skill 창 "설치됨" 탭 스위치. 하니스: 시나리오 `mcpServers`(앱의 McpManager로 연결, 끝나면 지움, `autoApprove` 밖 MCP 도구는 하니스가 승인하고 기록), `skillAutoLoad`, 검사 `toolCalledContains`·`skillAutoLoaded`(도구 검사는 자동 카드를 세지 않음), 검사 경로의 `{today}`·파일 이름 `*`, `harness/search.json`(Exa 키 없음). 실측(9B 8192 / 35B 32768, 로드돼 있던 그대로): proj-skill은 두 모델 모두 Skill이 자동으로 들어갔고 35B는 양식 그대로, 9B는 3번 중 2번 양식을 따름(1번은 생각만 하고 빈 답) — 검사 점수는 둘 다 2/5인데 `make_scenarios.py`의 검사 경로 날짜가 `2026-11-15`로 고정돼 있어서다(Skill은 "보고서를 쓰는 날짜"로 저장하라고 함). search-exa: 35B 3/3, 9B 2/3 두 번(검색은 하지만 답을 생각 안에만 쓰고 빈 답으로 끝남). 수정 1차 뒤 실측: proj-skill 9B 2/5·5/5·5/5(2/5는 양식은 지키고 파일 이름을 메모 날짜로 씀), 35B 5/5; search.json 9B 3/3·3/3(빈 답 재촉이 둘 다 들어가 답을 씀), 35B 3/3. 수정 2차 뒤 proj-skill 9B 5/5, 35B 5/5.
- 오른쪽 작업 패널 + PTY 터미널(설계 `docs/superpowers/specs/2026-11-24-dev-server-preview-console.md` T1, 결정 0008): 완료. `terminal.rs`(portable-pty/ConPTY, 기본 `powershell.exe -NoLogo`, 설정 `terminalShell`로 pwsh(PATH에 있을 때만)/cmd, PATH 앞에 앱 Node.js·uv, 링 버퍼 256KB, 16ms 모아 보내기·UTF-8 경계 처리, 시작 DSR 백엔드 응답, 한도 8개, 닫기·앱 종료 때 taskkill /T + Job Object로 트리 전체 종료), 명령 `terminal_open/write/resize/close/list/snapshot/busy/shells`, 이벤트 `anylm-terminal-output {id,data,seq}`·`anylm-terminal-exit {id,code}`. 스냅샷은 문자열이 아니라 `{data, seq, running, exitCode}`(이벤트와 겹치지 않게, 결정 0008). 화면 `src/workpanel/`(xterm 6 + fit + web-links, 상단 터미널 버튼·Ctrl+`, 폭 320px~70% 기억, 탭·[+ 새 터미널]·×·종료됨 (코드 n)·[다시 시작], 캔버스 오른쪽·채팅창 400px+캔버스 360px+패널 320px이 안 들어가면 나중에 연 것만, 지연 로딩 실패 시 [다시 시도]). Windows 실제 앱에서 표시와 프로젝트·홈 경로, 열기·닫기를 확인했다. 나머지 조작은 아래 수동 확인 목록에 남긴다.

- 개발 서버 T2/T3와 작업 패널 미리보기: 구현과 로컬 검증 완료. 실제 Windows 창에서 PowerShell, [서버 실행] 메뉴, 로컬 HTTP 페이지의 JavaScript 모듈을 확인했다. 프로젝트별 로그·종료, 시작 승인과 취소를 앱과 하니스가 같은 코드에서 처리한다. IPv6 리터럴은 기본 브라우저에서 연다(결정 0010).
- 에이전트 루프: 도구 라운드 상한 50회(`MAX_TOOL_ROUNDS`, 결정 0015에서 복원, 0009는 대체됨). 진행 중 턴의 도구 기록은 잘라 내지 않는다. 요청이 길면 1단계는 지난 턴의 도구 인자·결과만 줄이고, 이번 턴 결과는 실제 컨텍스트 초과 오류로 2단계 이상이 될 때만 줄인다. 같은 오류·같은 읽기 반복은 `ToolLoopGuard`가 경고 뒤 멈춘다. 중지·승인과 도구별 되돌리기 기록을 저장한다(결정 0015 갱신).
- 원본 잔재: 저장 파일·LM Studio 토큰 계정·런타임 표식·카탈로그 기본 주소·테스트 정리. 같은 Any LM 앱 데이터 안의 이전 저장 파일을 새 이름으로 불러온 실제 파일의 해시가 일치했다(결정 0011).
- 멀티 로컬 LLM 공급자(LM Studio · Ollama · OpenAI 호환, 설계 `docs/superpowers/specs/2026-11-24-multi-local-providers-design.md`, 계획 `docs/superpowers/plans/2026-11-24-multi-local-providers.md`, 결정 0012): 완료. `store.rs`(`LlmProvider`·`ProviderProfile`·최상위 필드 하위 호환 동기화), `secrets.rs`(공급자별 Windows 자격 증명 계정 `lm-studio-api-token`·`ollama-api-token`·`openai-compat-api-token`), `src-tauri/src/providers/`(`mod.rs` 공통 로컬 URL 검증·`ProviderCapabilities`·공급자별 오류 문구·라우팅, `ollama.rs` `/api/tags`·`/api/ps`·`/api/show`·`/api/generate` 로드/언로드·`ollama serve` 분리 실행, `openai_compat.rs` 경로 정규화·`/v1/models`·즉시 `Ready` `plan_load`·빈 목록 시 수동 모델 ID 허용), `exaone.rs`(공급자별 채팅 URL·오류 문구 일반화·Ollama/llama.cpp/vLLM 컨텍스트 초과 패턴 추가), `harness.rs`(`ANYLM_PROVIDER`), 프런트엔드 `src/lmstudio/providerMeta.ts`·`ConnectionDialog.tsx`(3종 공급자 전환 탭·전환 대기 안내·기능별 버튼 표시)·`ModelPicker.tsx`·`App.tsx`(사이드바 `{공급자} 연결`·오류 띠 `[서버 시작]` 조건부 표시). LM Studio 기존 구현과 테스트는 그대로 유지하고 모의 HTTP 서버로 Ollama·OpenAI 호환 전 흐름을 검증함. 실제 Ollama/llama.cpp 데몬 실측은 아래 수동 확인 목록에 남긴다.
- 라이브 업데이트(자동 업데이트) · 통합 `[설정]` 메뉴 · 배포 NSIS 빌드 · 공개 저장소 전환(설계 `docs/superpowers/specs/2026-11-24-updater-settings-release-design.md`, 계획 `docs/superpowers/plans/2026-11-24-updater-settings-release.md`, 결정 0013): 완료. Any LM Desktop 전용 서명 키(`%USERPROFILE%\.tauri\any-lm-desktop.key`, 비밀키는 로컬에만 보관)를 생성해 `tauri-plugin-updater` v2(`kidarigun/any-lm-desktop`, `createUpdaterArtifacts: true`)와 `check_update`/`install_update`(종료 직전 `terminal.close_all()` 포함)·`AppSettings.check_updates`·`src/update/UpdatePanel.tsx`·`scripts/make_latest_json.py`를 복구했다. 좌측 하단의 Skill·MCP 서버·연결 3개 버튼을 연결 상태 점이 포함된 단일 `[설정]` 버튼으로 통합하고, `src/settings/SettingsDialog.tsx`(4개 탭: 연결·모델 / Skill / MCP 서버 / 환경·업데이트)와 각 다이얼로그 `embedded` 모드를 구현했다. `THIRD_PARTY_NOTICES.md`를 재생성하고 서명된 NSIS 설치 파일(`Any LM Desktop_0.1.0_x64-setup.exe`, 17.76 MiB) 및 `release/latest.json`을 생성했으며, 저장소를 공개(Public)로 전환하고 `main`을 푸시한다.
- 기본 Skill·MCP 카탈로그 복구 및 재발 방지 가드레일(결정 0014): 완료. 커밋 `21143f3`에서 `DEFAULT_CATALOG_URL`을 제거하고 빈 스텁을 돌려주게 바꾸면서 `fe7e89c`의 설정 통합·배포 빌드 당시 Skill 찾아보기(63개)·MCP 찾아보기(17개)가 누락되고 내장 웹 검색 MCP 8개만 보이던 회귀를 복구했다. `src-tauri/src/catalog.rs`(`DEFAULT_CATALOG_URL`, `resolve_url`, 기본 카탈로그 레거시 제목만 `"Any LM Desktop 기본 목록"`으로 정규화, 오프라인 `catalog_cache.json` 폴백 유지), `src-tauri/src/lib.rs`(`catalog_default_url`), `src/api.ts`, `src/catalog/CatalogViews.tsx`를 복원하고, Rust/Vitest 회귀 테스트와 `AGENTS.md` 영구 보호 규칙을 추가했다. 서명된 NSIS 설치 파일(17.74 MiB)과 `release/latest.json`도 재생성했다.
- 에이전트 무한 루프 원인 제거·루프/반복 가드 · 생각 창 자동 스크롤(결정 0015): 완료. 실제 세션(`conv-e11e9347c31b4cf9b24b7fb880138da3`)에서 확인된 진행 중 턴 도구 잘라내기(`roll_tool_history`의 `api_messages.drain`으로 인한 파일 기억 상실·KV 캐시 매 라운드 무효화)와 `user_index` 앞 `[앱이 보관한 이전 도구 실행 기록]` 삽입을 제거하고, 완료된 과거 턴의 큰 도구 인자만 `compact_tool_arguments(args, 500)`로 줄여 현재 턴의 최근 읽기 결과를 보존한다. 또한 `MAX_TOOL_ROUNDS = 50` 복원, `ToolLoopGuard`(연속 오류 2회 경고/4회 중단, 동일 도구+인자 오류 2회 경고/3회 중단, 변경 없는 동일 읽기 반복 3회 경고/4회 중단, 비-읽기 성공 시 초기화), `detect_repetition_loop`(스트리밍 중/완료 시 `*`·`_`·공백 정규화 기반 반복 사고/답변 즉시 중단 및 이어 쓰기 생략)을 추가했다. 프런트엔드 실시간 생각 창(`src/App.tsx`, `src/chat/reasoningScroll.ts`)은 사용자가 위로 올려 읽지 않을 때만 자동으로 맨 아래(`scrollHeight`)를 따라 내려간다. 서명된 NSIS 설치 파일(17.71 MiB)과 `release/latest.json` 재생성.
- 핵심 에이전트 모듈(`exaone.rs` → `agent.rs`) 및 스트림 심볼 정리(결정 0016): 완료. `src-tauri/src/exaone.rs` → `src-tauri/src/agent.rs`, `run_exaone_turn` → `run_agent_turn`, `"exaone-stream"` → `"anylm-stream"`, `onExaoneStream` → `onAgentStream`으로 변경하고 `noBrandText.test.ts` 예외를 제거했다. `lib.rs`·`harness.rs`·`mcp.rs`·`dev_servers.rs`·`api.ts`·`App.tsx`·`AGENTS.md`를 함께 갱신했고, 레거시 마이그레이션 상수·`DEFAULT_CATALOG_URL`은 유지했다. 서명된 NSIS 설치 파일(17.72 MiB) 및 `release/latest.json` 재생성.
- 추론 반복 억제(`presence_penalty`) 및 의역 사고 루프 탐지·자동 복구(결정 0017): 완료. 실행 중인 앱(`PID 3520`, `qwen3.6-35b-a3b` 32K) 모니터링에서 확인한 `<think>` 내부 한국어 의역 자기검열 루프를 막기 위해, `src-tauri/src/agent.rs`에 `thinking_presence_penalty`(추론 ON·미지원/OFF 제외 시 코딩 모드 `0.8`, 일반·프로젝트 등 `1.2`)를 추가해 요청에 자동 반영한다. 또한 `acc.reasoning` 전용 `detect_thinking_loop`(절 단위 한국어 전환 추임새/강조 제거 정규화 + 5어절 슬라이딩 싱글 검사; `acc.content`는 기존 엄격한 `detect_repetition_loop` 유지)를 구현하고, 본문·도구 출력 전 `<think>` 루프 감지 시 모델 문맥에 망가진 사고를 넣지 않고 `reasoning_effort: "none"`(지원 시)으로 라운드당 최대 1회(턴당 최대 2회) 자동 재시도한 뒤 여전히 반복되면 즉시 멈추도록 했다. 서명된 NSIS 설치 파일(17.77 MiB) 및 `release/latest.json` 재생성. (값과 재시도 순서는 전체 검토 수정 2차에서 바뀜: 프로젝트·코딩 0, 일반 채팅 0.5, 반복 직후 한 라운드 1.0 — 결정 0017 갱신)
- 기본 컨텍스트 16K 전환 및 8K~128K 콤보 선택 UI(결정 0018): 완료. `src-tauri/src/lmstudio.rs`(`DEFAULT_CONTEXT = 16384`), `src-tauri/src/harness.rs`(`ANYLM_CONTEXT` 기본 `DEFAULT_CONTEXT`), `src/lmstudio/sendFlow.ts`(`DEFAULT_CONTEXT = 16384`), `AGENTS.md`의 기본 원하는 컨텍스트를 `16384`(16K)로 바꿨다. 또한 `src/lmstudio/modelText.ts`(`CONTEXT_PRESETS`, `formatContextSize`, `contextOptions`)와 `src/lmstudio/ConnectionDialog.tsx`를 수정해 숫자 직접 입력란 대신 `8K`~`128K`(`8K · 16K (기본) · 24K · 32K · 48K · 64K · 96K · 128K`) 콤보 선택(`<select>`)을 제공하며, 모델 `maxContextLength` 이하만 표시하되 기존 저장값·`[N K로 다시 시도]` 값도 함께 보존한다. 서명된 NSIS 설치 파일(17.76 MiB) 및 `release/latest.json` 재생성.
- 전체 검토 수정 1차(보안·개발 서버 시작, 결정 0019): 완료. 커밋 `75fdcdb`·`648dc93`·`17b2d75`·`b35003f`.
  - C1: 미리보기 iframe이 `http://tauri.localhost:<포트>`로 앱 IPC를 로컬로 쓰던 경로를 막았다. `ipc_guard.rs`의 `guard_invoke`는 Origin이 앱 출처와 정확히 같을 때만 앱 명령을 실행한다. WebView2 프레임 이벤트로 하위 프레임이 예약 호스트로 이동하는 것을 취소한다. CSP `frame-src`는 `*.localhost`를 빼고 `anylm-preview.localhost`만 남겼다. `is_app_url`은 포트가 붙은 주소를 거부한다.
  - C2: 개발 서버와 터미널의 작업 폴더에서 `\\?\` 접두사를 뗀다(`fs_ops::process_path`).
  - C-I1: 서버 준비 확인을 127.0.0.1과 ::1에 동시에 시도한다.
  - B-I1: 로컬 LLM 클라이언트는 프록시와 리디렉션을 쓰지 않는다(`providers::local_http_builder`).
  - 남은 것: 플러그인 명령은 프레임 차단에만 기댄다(이벤트 권한은 보강 `5bc6c17`에서 listen·unlisten만 남김). 실제 창 확인은 2026-11-25에 마쳤다(수동 확인 목록 "미리보기 IPC 격리"). WebView2 1.0.2420 미만의 중첩 프레임은 확인하지 않았다.
- 전체 검토 수정 2차(에이전트 루프 오탐, 결정 0015·0017 갱신): 완료. 커밋 `c3d7d9f`·`bf4fa2e`·`f5b6991`·`ac8a451`.
  - A-I1·A-M3: 사고·답변 반복 탐지기가 코드 블록·코드 같은 줄·표 줄을 빼고 글만 본다. 같은 절 반복은 최근 사고에서 차지하는 몫까지 보고, 싱글 기준은 길이에 비례한다. 코드·표는 끝 2,000자 전체가 같은 주기일 때만 반복이다.
  - A-I2·A-I4·A-M5: 상태를 바꾸는 도구(명시 목록, MCP 포함)가 성공하면 같은 오류 횟수도 초기화한다. `server_logs`는 읽기 반복으로 세지 않는다. `open_preview`·`remember`는 초기화하지 않는다.
  - A-I3·A-M2: 1단계 줄이기는 이번 턴을 건드리지 않는다. 턴 시작 위치는 턴 시작 때 한 번 정한다(이어 쓰기·빈 답 재요청을 턴 경계로 오인하지 않음). 앱이 잘라 낸 결과를 다시 읽으면 반복으로 세지 않는다.
  - 보강(검토 수정 1차, 커밋 `448a27e`):
    - 탐지기: 들여쓰기만으로는 코드로 보지 않는다. 걷어 낸 코드 묶음이 4번(연달아 3번) 되풀이되면 루프다. 원문 주기 검사는 2,000자 주기까지 본다. 같은 틀 산문은 위치를 보는 싱글로 오탐에서 뺐다.
    - 라운드 추론 길이 상한: max(40,000자, 컨텍스트 3개 분량). 답·도구 호출과 함께 온 반복 판정은 출력을 살리고 다음 라운드만 1.0을 넣는다.
    - 추론을 꺼 두었으면 같은 요청이 되는 재시도를 건너뛴다. 추론 선택지가 없는 모델도 1.0 재시도를 한 번 거친다. 프로젝트 대화에는 기본 `presence_penalty`가 없다.
    - 압축은 4단계다. 첫 초과는 예전 1단계 규칙을 이번 턴까지 넓혀 적용한다. 4단계 전까지 이번 턴 질문은 줄이지 않는다. 크게 작아지면 되돌린다(0.5).
    - 같은 오류 횟수는 파일 변경이나 처음 성공한 실행에서만 비운다. `server_logs`의 여러 서버 출력은 상한을 넘지 않는다.
    - 남은 것: 재시도 라운드 1.0이 도구 인자에도 붙는 문제(M4), 2단계 이상에서 여러 파일을 돌아가며 다시 읽는 경우(M6). 0015·0017 보강에 적었다.
  - A-I5·A-M1: `presence_penalty`는 프로젝트·코딩 0, 일반 채팅 0.5, 사고 반복 직후 한 라운드만 1.0. 반복 감지 시 반복 억제 → 추론 끄기 순서로 다시 시도하고, 재시도 단계는 그 라운드가 정상으로 끝날 때까지 유지한다.
  - C-I4·C-I3: 프로젝트·코딩 도구 안내에 설계 §6의 start_server 문장을 넣었다. `server_logs`·`start_server` 결과는 도구 상한을 넘으면 끝을 남기고 "[앞부분 N자 생략]"을 적는다.
  - 남은 것: 실제 모델(9B·35B)로 오탐이 사라졌는지와 0.5/1.0 값의 효과는 아직 실측하지 않았다(아래 수동 확인 목록).
- 전체 검토 수정 3차(공급자·터미널·버전·문서): 완료. 버전 0.1.1. 커밋 `5d90555`·`9ba842b`·`0459f66`·`c7f74c7`·`9d49771`·`62bf269`·`d929a2b`·`15f427c`·`0ddedf8`.
  - T0: `dev_servers::tests::server_commands_run_in_the_project_folder_not_windows`가 ConPTY 화면 대신 `cd` 결과를 쓴 파일을 읽는다(전체 테스트 3번 연속 통과, `process_path`를 되돌리면 `C:\Windows`로 실패).
  - B-I2: Ollama 채팅은 `/api/chat`으로 보내고 요청마다 `options.num_ctx`(턴 컨텍스트)·`keep_alive`(5m)를 싣는다. 변환은 `providers/ollama.rs`의 순수 함수(`native_chat_body`·`NativeStream`·`native_response_to_openai`)이고, 에이전트 루프는 보내기 직전(`providers::chat_body`)과 줄 해석 직전(`providers::ChatStream`)에만 끼운다. 로드 안 된 모델의 턴 컨텍스트는 원하는 값(설정, 없으면 16K, 모델 최대로 자름). `ollama serve`에 설정 주소의 `OLLAMA_HOST`를 주고 PATH 상대 항목은 건너뛴다. Ollama가 이 PC에 없어 문서 예시 표본으로만 시험했다(수동 확인 목록).
  - B-I3: llama.cpp식 목록이면 `/props`의 `default_generation_settings.n_ctx`를 실행 컨텍스트로 쓰고, 모르면 `min(n_ctx_train, 16K)`. 사용자 값은 서버 한도를 넘지 않는다.
  - B-I5: 대화의 `modelProvider`(예전 대화는 LM Studio)가 지금 공급자와 다르면 `modelKey`를 쓰지 않는다. 화면 모델 목록은 받은 공급자와 함께 들고, 공급자가 다르거나 새 공급자 목록 받기가 실패하면 비운다. OpenAI 호환 서버가 빈 목록이면 기본 모델로 입력한 ID만 Ready.
  - C-I2: 읽기 대기열 16 KB × 64(sync_channel). 링 버퍼는 모두 받고, 화면은 16 ms마다·초당 2 MB까지, 대기 256 KB를 넘으면 오래된 것을 버리고 `[출력 N바이트 생략]`. 화면 쪽은 xterm write 콜백으로 256 K자 넘게 밀리면 대기열, 1 M자 넘으면 같은 표시로 버림(`outputFlow.ts`).
  - 사소한 것: 모델 최대 컨텍스트 선택지(40K·256K 등), README `ANYLM_CONTEXT` 16384, NOTICE(공개·세 공급자·Ollama 비보증), 결정 0014 기록 바로잡기, `.gitignore`의 `*.key`·`*.sig`, LOG의 "(이 커밋)" 17개를 실제 해시로.

## 다음 할 일
0. **전체 검토 수정 3차 검토와 실측**: 3차 커밋(`5d90555`..`0ddedf8`)을 검토한다. Ollama가 있는 PC에서 16K 확인(아래 "Ollama 컨텍스트"), 터미널 홍수 확인(아래 "터미널 출력 홍수")을 한다. 0.1.1 설치 파일은 아직 만들지 않았다.
1. **남은 화면 QA**: 아래 수동 확인 목록의 기존 기능과 공급자 로그인·계정 확인을 이어서 점검한다. Windows 개발 앱에서 이번 변경의 터미널·로컬 미리보기·연결 창은 확인했다.
2. **원격 반영**: GitHub 비공개 저장소 개설과 인계 연습은 완료했다. `main`이 `origin/main`을 추적한다. 이후 원격 푸시는 사용자가 허락한 경우에만 진행한다.
3. **미뤄 둔 사소한 점(최종 검토, 지금 고치지 않아도 됨)**:
   - 9: AI 요약의 대화 글자 상한이 고정 60,000자이고 생각 켜짐을 물려받아 8K/32K에서 로컬 요약으로 자주 떨어짐 → 컨텍스트×1.5×0.6 상한, 요약은 생각 끄기 검토.
   - 10: 수정 후 다시 보내기는 화면을 되돌리기 전에 모델 준비를 한다. 승인 후 이어가기에는 별도 준비 단계가 없으며, 다시 보내기에는 자동 요약 압축이 없다.
   - 11: [내리고 로드]가 다른 대화가 쓰는 모델을 내릴 수 있음(경고 없음).
   - 12: 통계 줄이 없는 답변(취소·실패)에는 모델 이름이 안 보임, 승인 뒤 다른 모델이 이어도 `model_key`는 처음 값.
   - 13: [다시 로드] 안내가 같은 모델의 인스턴스 하나만 내림(연결 창은 전부 내림).
   - 14: 컨텍스트를 모르는 인스턴스면 "0K 컨텍스트로 로드돼 있습니다"로 보임.
   - 15: 기본 모델이 없을 때 첫 보내기가 빈 대화를 남김.
   - 17: 답변 중 서버가 죽으면 연결 거부를 복구 가능 오류로 보고 약 67초 기다린 뒤 안내.
   - 그 밖에 검토의 "Can stay" 목록(서버 상태는 아무 HTTP 응답이면 켜짐으로 봄 등).

## 마지막 확인
2026-11-25, 전체 검토 수정 3차 뒤: `cd src-tauri && cargo test --lib` 359 통과·0 실패·12 무시를 3번 연속(분리 타깃 폴더, 이전 간헐 실패 테스트 포함), `cargo test --lib terminal_flood -- --ignored` 통과(실제 PTY 20만 줄, 약 23초), `cargo check --all-targets` 경고·오류 없음, `npm test` 통과(33 파일, 198 통과·1 건너뜀), `npx tsc --noEmit` 오류 없음, `npm run build` 성공. Ollama·llama.cpp는 이 PC에 없어 실제 서버로는 시험하지 않았다. 앱은 띄우지 않았다.
2026-11-25, 전체 검토 수정 2차 보강(검토 수정 1차, 결정 0015·0017 보강) 뒤: `npm test` 통과(32 파일, 192 통과·1 건너뜀), `npx tsc --noEmit` 오류 없음, `cd src-tauri && cargo test --lib` 341 통과·1 실패·11 무시(분리 타깃 폴더). 실패한 1개는 `dev_servers::tests::server_commands_run_in_the_project_folder_not_windows`다. cmd 출력이 비어 간헐적으로 실패하며, 단독으로 다시 돌리면 통과한다. 이번 수정 전 커밋 `04b34c1`에서도 3번 중 2번 같은 식으로 실패해, 이번 수정과는 관계없다. `cargo check --all-targets` 경고·오류 없음. 앱은 띄우지 않았다.
2026-11-25, 전체 검토 수정 2차(결정 0015·0017 갱신) 뒤: `npm test` 통과(32 파일, 192 통과·1 건너뜀), `npx tsc --noEmit` 오류 없음, `cd src-tauri && cargo test --lib` 330 통과·0 실패·11 무시(사고 탐지 오탐 회귀, 도구 가드 시퀀스, 이번 턴 줄이기·이어 쓰기 인자, 반복 재시도 단계, 서버 로그 끝부분 테스트 포함, 분리 타깃 폴더), `cargo check --all-targets` 경고·오류 없음. 앱은 띄우지 않았다.
2026-11-25, 전체 검토 수정 1차(결정 0019) 뒤: `npm test` 통과(32 파일, 192 통과·1 건너뜀), `npx tsc --noEmit` 오류 없음, `cd src-tauri && cargo test --lib` 313 통과·0 실패·11 무시(`ipc_guard::tests::`, `dev_servers::tests::server_commands_run_in_the_project_folder_not_windows`·`ipv6_only_server_is_seen_through_localhost`, `fs_ops::tests::process_path_drops_the_verbatim_prefix_only`, `providers::tests::local_client_returns_redirects_as_is` 포함), `cargo check --all-targets` 경고·오류 없음, `npm run build` 성공. 프레임 차단의 실제 창 확인은 수동 확인 목록에 남겼다.

2026-11-25, 기본 컨텍스트 16K 전환 및 8K~128K 콤보 선택 UI(결정 0018) 뒤: `npm test` 통과(32 파일, 192 통과·1 건너뜀, `noBrandText.test.ts`·`modelText.test.ts`·`sendFlow.test.ts` 포함), `npx tsc --noEmit` 오류 없음, `cd src-tauri && cargo test --lib` 305 통과·0 실패·11 무시(`lmstudio::tests::`·`harness::tests::` 포함), `cargo check --all-targets` 경고·오류 없음, `npm run build` 성공, `npm run tauri build`로 `Any LM Desktop_0.1.0_x64-setup.exe`(17.76 MiB)·`.sig` 및 `release/latest.json` 재생성.

2026-11-24, 추론 ON `presence_penalty`·의역 사고 루프 탐지 및 자동 복구(결정 0017) 뒤: `npm test` 통과(32 파일, 191 통과·1 건너뜀, `noBrandText.test.ts` 포함), `npx tsc --noEmit` 오류 없음, `cd src-tauri && cargo test --lib` 305 통과·0 실패·11 무시(`agent::tests::` 92 통과, 실행 중 개발 앱 잠금 회피를 위해 분리 타깃 폴더에서 실행), `cargo check --all-targets` 경고·오류 없음, `npm run build` 성공, `npm run tauri build`로 `Any LM Desktop_0.1.0_x64-setup.exe`(17.77 MiB)·`.sig` 및 `release/latest.json` 재생성.

2026-11-24, 핵심 에이전트 모듈 이름 변경(`exaone.rs` → `agent.rs`, 결정 0016) 뒤: `git grep` 잔존 심볼 0건, `npm test` 통과(32 파일, 191 통과·1 건너뜀, 예외 없이 `noBrandText.test.ts` 통과), `npx tsc --noEmit` 오류 없음, `cd src-tauri && cargo test --lib` 300 통과·0 실패·11 무시(`agent::tests::` 87 통과), `cargo check --all-targets` 경고·오류 없음, `npm run build` 성공, `npm run tauri build`로 `Any LM Desktop_0.1.0_x64-setup.exe`(17.72 MiB)·`.sig` 및 `release/latest.json` 재생성.

2026-11-24, 에이전트 루프 수정·루프/반복 가드·생각 창 자동 스크롤(결정 0015) 뒤: `npm test` 통과(32 파일, 191 통과·1 건너뜀, `reasoningScroll.test.ts` 및 `noBrandText.test.ts` 포함), `npx tsc --noEmit` 오류 없음, `cd src-tauri && cargo test --lib` 300 통과·0 실패·11 무시(`exaone::tests::` 87 통과, 진행 중 턴 최근 파일 유지·동일 오류/읽기 반복 중단·스트림 반복 탐지 차단 포함), `cargo check --all-targets` 경고·오류 없음, `npm run build` 성공, `npm run tauri build`로 `Any LM Desktop_0.1.0_x64-setup.exe`(17.71 MiB)·`.sig` 및 `release/latest.json` 재생성.

2026-11-24, 기본 Skill·MCP 카탈로그 복구 및 재발 방지 가드레일(결정 0014) 뒤: `npm test` 통과(31 파일, 187 통과·1 건너뜀, `noBrandText` 및 기본 카탈로그 보존 회귀 테스트 포함), `npx tsc --noEmit` 오류 없음, `cd src-tauri && cargo test --lib` 295 통과·0 실패·11 무시(실행 중인 개발 앱 파일 잠금 회피를 위해 분리 타깃 폴더에서 실행), `cargo test --lib live_default_catalog_has_skills_and_mcp -- --ignored --nocapture` 통과(`skills=63`, `mcp=17`), `cargo check --all-targets` 경고·오류 없음, `npm run build` 성공, `npm run tauri build`로 `Any LM Desktop_0.1.0_x64-setup.exe`(17.74 MiB)·`.sig` 및 `release/latest.json` 재생성.

2026-11-24, 자동 업데이트 복구 · `[설정]` 메뉴 통합 · NSIS 배포 빌드 뒤: `npm test` 통과(31 파일, 185 통과·1 건너뜀), `npx tsc --noEmit` 오류 없음, `cd src-tauri && cargo test --lib` 293 통과·0 실패·10 무시(실행 중인 개발 앱 파일 잠금 회피를 위해 분리 타깃 폴더에서 실행), `cargo check --all-targets` 경고·오류 없음, `npm run build` 성공, `THIRD_PARTY_NOTICES.md` 갱신(npm 111개·Rust 321개), `npm run tauri build`로 `Any LM Desktop_0.1.0_x64-setup.exe`(17.76 MiB)·`.sig` 및 `release/latest.json` 생성 성공.

2026-11-24, 멀티 로컬 LLM 공급자(LM Studio · Ollama · OpenAI 호환) 구현 뒤: `npm test` 통과(30 파일, 183 통과·1 건너뜀), `npx tsc --noEmit` 오류 없음, `cd src-tauri && cargo test --lib` 293 통과·0 실패·10 무시(Ollama·OpenAI 호환 모의 HTTP, 설정 마이그레이션, 로컬 URL 보안, 컨텍스트 초과 문구, 하니스 `ANYLM_PROVIDER` 테스트 포함), `cargo check --all-targets` 경고·오류 없음, `npm run build` 성공.

2026-11-24, 루프와 미리보기 리뷰 보완 뒤: `npm test` 181 통과·1 건너뜀, `npx tsc --noEmit` 통과, `cargo test --lib` 280 통과·10 무시, `npm run build` 성공. 후속 턴의 큰 도구 인자 줄이기, 실행 직전 자동 승인 확인, 점유된 포트와 서버 시작 취소를 회귀 검증했다. 실제 앱에서 9B와 35B가 32K로 표시되는 것, 로컬 모듈 실행과 부모 접근·Tauri 명령 거부를 확인했다.

최종 LM Studio 9B 32K 실측: 기본 추론의 단일 요청은 첫 도구 호출 전에 하니스 10분 제한에 도달했다. 추론을 끈 단일 요청에서는 모델이 후속 도구들을 제안하고 호출하지 않는 경우가 있었다. `harness/dev-server.json` 최종 시나리오는 추론을 끈 4개 사용자 요청으로 진행해 시작 승인·로그·미리보기·종료 검사 6/6, 호출 8회, 73초였다. 9B는 32K로 로드되어 있다. `harness/report.py`의 추론 토큰 표기도 LM Studio 응답에 맞췄다.

2026-11-24, 루프 체크포인트 검토 뒤: `npm test` 180 통과·1 건너뜀, `npx tsc --noEmit` 통과, `cargo test --lib` 278 통과·10 무시. 하나의 응답에서 요청한 두 쓰기 중 첫 번째 직후 중지하면 두 번째 쓰기는 실행되지 않으며, 첫 번째 작업 기록과 스냅샷이 함께 전달된다.

2026-11-24, 원본 의존 이름 정리 뒤: `npm test` 180 통과·1 건너뜀, `npx tsc --noEmit` 통과, `cargo test --lib` 277 통과·10 무시. 기본 목록의 오프라인 동작과 기존 Any LM 대화 파일의 새 경로 저장을 확인했다.

2026-11-24, 개발 서버 모델 도구(T3) 뒤: `npm test` 180 통과·1 건너뜀, `npx tsc --noEmit` 통과, `cargo test --lib` 275 통과·10 무시. 실제 qwen3.5-9b 8K에서 `harness/dev-server.json` 6/6(승인 → 시작 → 로그 → 미리보기 → 종료 → 최종 답변), 시험 뒤 9B를 32768로 재로드. 실제 앱 창에서 PowerShell 탭과 [서버 실행] 메뉴 표시 확인.

2026-11-24, T2 개발 서버·미리보기 뒤: `npm test` 180 통과·1 건너뜀, `npx tsc --noEmit` 통과, `cargo test --lib` 274 통과·10 무시. Windows ConPTY 테스트에서 시작 로그의 주소 감지, 중복 방지, 프로젝트별 목록 격리, 종료 뒤 포트 해제를 확인했다.

2026-11-24, 도구 라운드 상한 제거와 턴 내 오래된 도구 기록 압축 뒤: `npm test` 178 통과·1 건너뜀, `npx tsc --noEmit` 통과, `cargo test --lib` 271 통과·10 무시. 모의 SSE 테스트는 수정 전 50회에서 멈췄고 수정 뒤 55회와 최종 답변까지 확인했다. Rust는 개발 앱과 대상 폴더를 분리해 실행했다.

2026-11-24, 작업 패널 터미널(T1) 수정 1차 뒤: `npm test` → 통과 (28 파일, 178 통과, 1 건너뜀) · `npx tsc --noEmit` → 오류 없음 · `cd src-tauri && cargo test --lib` → 270 통과, 0 실패, 10 무시 · `cargo check --all-targets` 경고 없음 · `npm run build` 성공.

2026-11-24, 작업 패널 터미널(T1) 뒤: `npm test` → 통과 (26 파일, 166 통과, 1 건너뜀) · `npx tsc --noEmit` → 오류 없음 · `cd src-tauri && cargo test --lib` → 269 통과, 0 실패, 10 무시(터미널 18개는 실제 ConPTY로 돈다) · `cargo check --all-targets` 경고 없음 · `npm run build` 성공(터미널은 별도 묶음 WorkPanel 약 346KB).

2026-11-24, Skill 자동 불러오기 수정 2차 뒤: `npm test` → 통과 (24 파일, 146 통과, 1 건너뜀) · `npx tsc --noEmit` → 오류 없음 · `cd src-tauri && cargo test --lib` → 251 통과, 0 실패, 10 무시 · `cargo check --all-targets` 경고 없음. 하니스 proj-skill 9B(8192) 5/5, 35B(32768) 5/5.

2026-11-24, Skill 자동 불러오기 수정 1차 뒤: `npm test` → 통과 (24 파일, 146 통과, 1 건너뜀) · `npx tsc --noEmit` → 오류 없음 · `cd src-tauri && cargo test --lib` → 250 통과, 0 실패, 10 무시 · `cargo check --all-targets` 경고 없음. 실측은 위 "진행 중"의 Skill 자동 불러오기 줄.

2026-11-24, Skill 자동 불러오기 + 하니스 MCP 뒤: `npm test` → 통과 (24 파일, 146 통과, 1 건너뜀) · `npx tsc --noEmit` → 오류 없음 · `cd src-tauri && cargo test --lib` → 240 통과, 0 실패, 10 무시 · `cargo check --all-targets` 경고 없음. 실측은 위 "진행 중"의 Skill 자동 불러오기 줄.

2026-11-24, 지식 폴더 검토 2차 수정 뒤: `npm test` → 통과 (23 파일, 142 통과, 1 건너뜀) · `npx tsc --noEmit` → 오류 없음 · `cd src-tauri && cargo test --lib` → 224 통과, 0 실패, 10 무시 · `cargo check --all-targets` 경고 없음.

2026-11-24, 지식 폴더 검토 1차 수정 뒤: `npm test` → 통과 (23 파일, 133 통과, 1 건너뜀) · `npx tsc --noEmit` → 오류 없음 · `cd src-tauri && cargo test --lib` → 219 통과, 0 실패, 9 무시 · `cargo check --all-targets` 경고 없음. 하니스 `harness/knowledge.json`(qwen3.6-35b-a3b 32768, 로드돼 있던 그대로) 4/4.

2026-11-23, 지식 폴더 뒤: `npm test` → 통과 (23 파일, 133 통과, 1 건너뜀) · `npx tsc --noEmit` → 오류 없음 · `cd src-tauri && cargo test --lib` → 211 통과, 0 실패, 9 무시 · `npm run build` 성공. LM Studio는 qwen3.6-35b-a3b(32768)가 로드된 상태 그대로(로드·언로드 안 함).

2026-11-23, 최종 검토 수정 뒤: `npm test` → 통과 (20 파일, 116 통과, 1 건너뜀) · `npx tsc --noEmit` → 오류 없음 · `cd src-tauri && cargo test --lib` → 167 통과, 0 실패, 9 무시 · `npm run build` 성공. LM Studio는 qwen3.5-9b(32768) 1개만 로드된 상태 그대로(로드·언로드·서버 정지 안 함). curl로 스트림 usage가 `completion_tokens_details.reasoning_tokens`(60)로 오는 것과 `lms server status`가 새 실행 방식으로 바로 돌아오는 것을 확인.

아래는 작업 12 단계 1(완료 기준 실측) 때의 기록이다. 명령으로 확인한 것만 적었고 화면은 보지 못했다.
- `npm test` → 통과 (20 파일, 114 통과, 1 건너뜀) · `npx tsc --noEmit` → 오류 없음 · `cd src-tauri && cargo test --lib` → 159 통과, 0 실패, 8 무시

| # | 완료 기준 | 결과 | 근거 |
|---|---|---|---|
| 1 | 앱이 뜨고 원본과 동시 실행, 서로 안 보임 | 부분 통과 — 화면 확인 필요 | `npm run tauri dev`로 창 "Any LM Desktop"(그때 포트 45789 LISTEN — 원본의 MCP OAuth 포트와 같아 최종 검토 뒤 45790으로 옮김) 실행. 설치된 원본(`%LOCALAPPDATA%\Desktop App for LG K-EXAONE\exaone-desktop.exe`)을 같이 띄우니 창 "Desktop App for LG K-EXAONE"(포트 45788)이 동시에 떴다. identifier `io.github.kidarigun.anylm-desktop` vs `...kexaone-desktop`, 데이터 폴더 `%APPDATA%` 아래 서로 따로, 둘 다 종료함. 창 모양·대화가 안 섞이는지는 눈으로 |
| 2 | 모델 목록·연결 상태 | 부분 통과 — 화면 확인 필요 | `GET /api/v1/models` → qwen3.6-35b-a3b, qwen3.5-9b, 임베딩 1개. `parse_models`·`server_urls` 테스트 통과, `lm_server_running`은 같은 서버를 본다. 연결 창에 보이는 모습은 눈으로 |
| 3 | 대화별 모델 전환, 모델 이름 표시 | 부분 통과 — 화면 확인 필요 | `/v1/chat/completions`에 `"model":"qwen3.5-9b"` → 응답 model 일치(추론 토큰 0). `sendFlow`·`statsLine`(모델 이름 끝에) 테스트 통과. 9B→35B [내리고 로드]는 8GB라 로드하지 않음 |
| 4 | 로드 안 된 모델은 앱 컨텍스트로 로드 후 답 | 부분 통과 — 화면 확인 필요 | `plan_load_cases`·`plan_load_missing_model`·`wanted_context_respects_prefs_and_max`·`sendFlow` "maps load plans to the next step"(notLoaded→load) 통과. 실제 로드 후 답하는 화면은 눈으로 |
| 5 | 프로젝트 모드 파일 만들기·수정·되돌리기 | 통과(되돌리기는 단위 테스트) | 하니스 `proj-multiturn`을 qwen3.5-9b로 실행: `read_file`→`write_file`(summary.md 생성)→`modify_file`(맨 끝에 "다음 회의: 11월 20일 오전 10시" 추가, 파일에서 확인)→`write_file`(summary_en.md), 8회·54초, 검사 6/7(실패 1건은 날짜 표기 "11월 27일" vs 모델이 쓴 "2026-11-27", 파일 만들기·수정과 무관). 되돌리기는 `fs_ops::test_read_write_modify_delete_and_undo`·`binary_overwrite_and_delete_can_be_undone`·`snapshot_diff` 테스트. 되돌리기 버튼 화면은 눈으로 |
| 6 | 70% 자동 압축·긴급 압축 | 부분 통과 — 화면 확인 필요 | `auto_compact_triggers_at_70_percent`·`auto_compact_respects_setting`·`auto_compact_skips_when_nothing_to_cover`, `summary_request_uses_conversation_model_and_reasoning_off`, `stream_overflow_error_escalates_compaction_instead_of_plain_retry`, `context_overflow_is_retried_with_shorter_history`, `detects_lm_studio_overflow` 통과. 압축 카드와 진행 문구는 눈으로 |
| 7 | K-EXAONE 전용 요소 없음 | 통과(화면 문구는 눈으로 한 번 더) | `ui/noBrandText.test.ts` 통과. `git grep -i -E "rate|만료|low|high|max|update|업데이트|exaone|엑사원" -- src`를 훑어 판정: 남은 것은 Tailwind `max-w-*`·`Math.max`·`updated`(대화 갱신)·내부 이름(`onExaoneStream`, `exaone-stream`, `exaone-ordered`, 드래그 MIME)뿐이고 호출 한도·키 만료·추론 강도 low/high/max·업데이트 화면은 없음. 사소한 잔재 2건(화면에 안 보임): `src/types.ts:183` 주석의 "하루 호출 한도", `src/api.ts:488` 카탈로그 기본 주소 `kidarigun/exaone-catalog` |
| 8 | 모든 테스트 통과 | 통과 | 위 세 명령: 114 / 오류 0 / 159 |
| 9 | 인계 연습 | 통과 | 작업 12 단계 2. Codex가 인계 문서와 실제 Git을 대조하고 후속 작업을 커밋했다. |

## 결정 필요
- (없음 — 2026-11-24 사용자 지시에 따라 공개 저장소 전환 및 원격 푸시 진행.)

## 기준 상태
작업을 시작하기 전(복제 직후, e885b02)의 테스트 결과다. 이후 실패가 나오면 이 기준과 비교해 새로 깬 것인지 구분한다.
- `npm install`: 성공
- `npm test`: 16 파일 통과, 102 통과 / 1 건너뜀 / 실패 0
- `npx tsc --noEmit`: 오류 0
- `cargo test --lib`: 121 통과 / 실패 0 / 7 무시(ignored)

## 수동 확인 목록
(사람이 화면에서 직접 확인해야 하는 항목. 화면별로 묶음.)

### 시작·창
- 새 아이콘(연결된 노드 + 말풍선): 앱 창·작업 표시줄·사이드바 로고가 새 아이콘인지(옛 "EX"/"LM" 상자가 아닌지). 탐색기의 exe 아이콘은 Windows 아이콘 캐시 때문에 옛 그림이 남을 수 있음
- 원본 앱과 새 앱 창이 동시에 뜨고(작업 12에서 두 창이 같이 뜨는 것은 프로세스로 확인), 서로의 대화·설정이 보이지 않는지, 창 모양이 어색하지 않은지
- 헤더 이름 "Any LM Desktop"과 "LM" 로고 칸, 프로젝트 지침 자리표시·압축 창 문구가 어색하지 않은지
- 화면 어디에도 호출 한도·키 만료·추론 강도(low/high/max)·업데이트 표시가 없는지
- 원본 앱을 켠 채 MCP OAuth 로그인: 원본 앱(45788·45789)을 켜 둔 상태에서 새 앱의 원격 MCP [로그인]이 브라우저 로그인 후 성공하는지(새 앱은 중복 실행 45790, 로그인 결과 45791). `netstat -ano | findstr 4579`로 45790이 LISTEN인지도 확인

### LM Studio 연결 창
- 서버 주소에 `http://localhost:1234/v1`을 넣고 저장해도 모델 목록이 보이고 대화가 되는지
- 연결 점이 서버 정지·재시작에 맞게 바뀌는지. 두 모델의 '로드됨 · 32K' 표시는 실제 앱에서 확인함
- 개발자 도구에서 `await window.__TAURI__.core.invoke("lm_list_models")` → 모델 2개
- LM Studio 화면에서 모델별 GPU 설정(전문가 가중치 CPU로)을 저장해 둔 뒤 앱으로 로드해도 그 값이 유지되는지 (결정 기록 0002)

### 대화 화면
- 한 대화에서 9B로 한 번 묻고 모델 선택을 35B로 바꿔 한 번 더 묻기 → [내리고 로드] 후 35B가 답하고, 두 답변 아래 모델 이름이 다른지. 입력창 아래 모델 선택 모양·로드 중 표시·'다시 로드' 안내 (35B는 8GB VRAM에서 로드하지 않았다)
- 로드되지 않은 모델을 고르고 보내면 앱이 정한 컨텍스트로 로드한 뒤 답하는지
- 입력창 아래 "생각하기" 체크가 추론 지원 모델(9B·35B)에서만 보이고, 끄면 생각 없이 바로 답하며 켜면 다시 생각하는지
- 답변 아래 통계 줄(입력 · 출력 · 추론 · 초당 토큰 · 모델 이름)이 어색하지 않은지
- "생각하기"를 켠 채 물으면 통계 줄의 "추론 n토큰"이 0이 아닌지(끄면 0)
- LM Studio 서버를 끈 채 보내기 → 오류 띠의 "서버가 꺼져 있습니다" 옆 [서버 시작]을 누르면 "시작 중…" 뒤 돌아오고(멈추지 않음), 서버가 켜지면 띠가 사라지고 연결 점이 초록이 되는지. 확인 뒤 qwen3.5-9b가 32768로 로드돼 있는지 다시 볼 것
- 로드 실패(예: 35B를 32K로, 또는 컨텍스트 칸에 아주 큰 값) → 오류 옆 [16K로 다시 시도]와 [내리고 로드]가 보이고, 다시 시도하면 절반 컨텍스트로 로드되는지(연결 창의 [로드] 실패에도 같은 버튼). 확인 뒤 32768로 되돌릴 것
- 입력창의 [컨텍스트 압축] 버튼 색과 "~Nk"가 로드된 컨텍스트 기준인지(32K에서 70% 넘으면 붉은색)
- 수동 압축 바로 뒤 다음 보내기에서 자동 압축이 다시 돌지 않는지(요약 카드 내용이 그대로인지)
- 9B를 8192로 로드해 긴 답변이 나오는 질문을 몇 번 해서, 70%를 넘을 때 "컨텍스트가 70% 찼습니다. 이전 대화를 요약하는 중…"이 보이고 압축 카드가 생기며 대화가 이어지는지. 확인 뒤 32768로 되돌릴 것

### 프로젝트 화면
- 프로젝트에서 파일을 만들고 고친 뒤 되돌리기 버튼으로 이전 내용이 돌아오는지 (백엔드는 테스트로 확인)
- 에이전트 작업 도중 컨텍스트가 넘칠 때 긴급 압축으로 이어 가는지(필요하면 9B를 8192로 로드해 확인)

## 마지막 작업자
- 도구: claude-code
- 모델: Claude Opus 5.5
- 시각: 2026-11-25

### 프로젝트 화면 · 메모리
- 모델이 `remember`로 저장하면(예: "이 프로젝트 보고서는 항상 존댓말로 쓴다고 기억해") 대화 중 도구 카드가 뜨고 프로젝트 화면 "프로젝트 메모리" 카드에 [모델] 표시로 바로 보이는지, [추가]로 직접 쓴 항목은 [직접]으로 보이는지, 편집·삭제(삭제 전 확인)가 되는지, 새 대화에서 모델이 그 내용을 아는지 확인.
- 지침 편집기: 지침 카드 [편집] → [서식 예시]로 4종(보고서·문서 작성/코딩/자료 조사·요약/번역·교정) 넣기(비어 있으면 대체, 있으면 끝에 덧붙임), "n자 · 약 m토큰 · 컨텍스트의 x%"가 입력 중 바뀌고 10% 넘으면 노란색인지, 저장 뒤 새 대화에서 모델이 그 지침을 따르는지(시스템 프롬프트 반영), Esc·취소 때 변경 확인이 뜨는지

### 작업 패널 · 터미널
- Ctrl+`(채팅 입력란에 커서가 있을 때도) 단축키와 다시 켰을 때 폭 복원. 상단 버튼으로 열기·닫기, PowerShell 첫 탭·프로젝트 및 홈 시작 경로는 실제 Windows 창에서 확인함
- 터미널 텍스트 선택 확인. 현재 CSP의 글자·커서와 어두운 테마는 실제 Windows 창에서 확인함
- `dir`, `node -v`(앱이 Node.js를 설치했으면 그 버전), `Write-Host -ForegroundColor Green hi`(초록), 진행 표시(임시 폴더에서 `npm i lodash`, 또는 `Invoke-WebRequest https://nodejs.org/dist/index.json -OutFile $env:TEMP\index.json`), 한글 출력(`echo 한글`)
- `ping -t localhost` 중 Ctrl+C로 멈추는지, 글자를 선택하고 Ctrl+C → 복사(멈추지 않음), Ctrl+V 붙여넣기
- 탭 여러 개 열기·고르기·닫기, 9번째는 "최대 8개" 안내, 패널을 닫았다 열어도 출력이 남아 있는지, 셸 선택 상자(명령 프롬프트/PowerShell 7)
- `ping -t localhost`가 도는 탭의 ×를 누르면 확인 창이 뜨고, 닫은 뒤 작업 관리자에 `PING.EXE`가 남지 않는지. 앱을 끄면 터미널에서 띄운 프로세스가 남지 않는지
- `exit` 하면 탭 아래 "종료됨 (코드 0)"과 [다시 시작], 누르면 같은 자리에서 새 셸
- 한글 입력(조합 중 Enter·Space·Backspace, 조합 중 Ctrl+C/V)
- 창 1024px·1280px에서 사이드바를 열고/접고 패널을 끝까지 넓혀도 채팅창이 400px 아래로 줄지 않는지, 캔버스와 함께 열 때(사이드바 열림 1336px 미만이면 나중에 연 것만)
- 대화상자(설정·Skill 등)가 떠 있을 때 Ctrl+`가 패널을 열지 않고, 대화상자 입력란의 포커스를 터미널이 가져가지 않는지
- 문서 캔버스와 함께 열면 패널이 캔버스 오른쪽에 붙는지, 창을 1100px보다 좁히면 나중에 연 것만 보이는지, 폭 끌기(320px~창의 70%)
- 어두운 테마에서 터미널 색이 맞는지, 출력 속 링크를 누르면 기본 브라우저로 열리는지

### Skill 자동 불러오기
- Skill 창 "설치됨" 탭 위 "Skill 자동 불러오기" 스위치가 켜져 있고(처음 실행·예전 설정도), 끄고 다시 열면 꺼진 채인지
- 프로젝트에 `.skills/weekly-report/SKILL.md`(harness/make_scenarios.py의 proj-skill 내용)를 두고 "이번 주 주간 업무 보고서를 만들어 줘" → 답변 맨 위 도구 카드 이름표가 "Skill 자동 불러옴: weekly-report"인지, 대화를 다시 열어도 카드가 남는지, 모델이 load_skill을 또 부르면 그 카드 내용이 "앱이 이 턴에 미리 넣었던 Skill입니다. 지침이 필요하면 load_skill로 다시 읽으세요."인지, 이어서 "계속"이라고 보내면 카드가 "앞 요청에서 쓰던 Skill이라 이어서"로 다시 뜨는지
- "안녕"·"이번 주 날씨 알려 줘"·"회사 소개 글 작성해 줘"처럼 관계없는 요청에는 카드가 안 뜨는지, `/weekly-report`로 부르면 뜨는지, 스위치를 끄면 안 뜨는지

### MCP · 웹 검색
- 사이드바 MCP → 찾아보기 → 분류 "웹 검색": 8개 항목에 "공식" 표시가 보이고, 인터넷을 끈 채 열어도(저장본이 없을 때) 이 항목들과 오류 띠가 보이는지
- Exa [연결](키 칸을 비운 채) → 서버가 "연결됨"이고 도구 2개(web_search_exa·web_fetch_exa)가 보이는지 → 프로젝트 또는 채팅에서 "오늘 나온 최신 뉴스를 찾아서 알려 줘"처럼 물으면 승인 없이 검색 도구 카드가 뜨고 답에 참고한 페이지 주소(URL)가 나오는지
- 키가 필요한 항목(예: Tavily)을 키 없이 [연결]하면 "'API 키'을(를) 입력하세요."가 나오고, 키를 넣고 연결하면 서버가 연결됨이 되는지(키가 있을 때만 확인)

### 프로젝트 화면 · 지식 폴더
- [폴더 추가] → 폴더 고르기 대화상자 → 목록에 경로가 생기고 상태 줄이 "색인 중… (n/m)"에서 "파일 n개 · 조각 n개 · 건너뜀 n개 · 마지막 색인 YYYY-MM-DD HH:MM"로 바뀌는지, 폴더 줄에 파일·조각 수, 화면이 멈추지 않는지. 같은 폴더를 다시 더하면 "이미 추가한 지식 폴더입니다", docx/hwpx/pdf가 든 폴더면 그 파일도 색인되는지(웹뷰 변환기), 건너뛴 파일이 있으면 [건너뛴 파일 보기]에 경로와 이유
- 그 폴더 문서에만 있는 내용을 프로젝트 대화에서 물으면 답에 그 내용과 출처 경로("출처: ...")가 나오는지(9B 32K로도 확인할 것), 필요하면 도구 카드에 `search_knowledge` 결과가 보이는지
- 폴더의 파일을 고친 뒤 다시 물으면 고친 내용으로 답하는지(턴 시작 빠른 갱신), [다시 색인]을 누르면 "색인 중…" 뒤 마지막 색인 시각이 바뀌는지
- [제거] → 확인 창 → 목록에서 빠지고, 그 폴더 내용으로는 더 답하지 않는지. 지식 폴더 안에 앱이 만든 파일이 하나도 없는지
- 프로젝트 설정 창에서 지침을 고치면 [큰 편집기로 열기] 대신 "큰 편집기: 먼저 저장하세요"가 보이고, 고치지 않았으면 열리며 화면의 프로젝트가 바뀌지 않는지. 지침 편집기에서 바탕을 누른 뒤 Esc로도 닫히는지

### 멀티 로컬 공급자 (Ollama · OpenAI 호환)
- 연결 창 상단 탭(LM Studio · Ollama · OpenAI 호환)을 전환했을 때 각 공급자의 서버 주소(기본 1234 / 11434 / 8080)·기본 모델·모델별 컨텍스트·토큰 보유 표시가 서로 섞이지 않고 따로 기억되는지
- 저장 전 다른 공급자 탭을 고르면 상태가 "전환 대기"로 보이고 이전 공급자 모델이 섞여 보이지 않으며, [저장]을 누른 뒤 사이드바 버튼 이름(`[Ollama 연결]`, `[OpenAI 호환 연결]`)과 모델 목록이 바뀌는지
- Ollama가 꺼져 있을 때 [서버 시작]으로 `ollama serve`가 뜨는지, 모델 목록·로드 상태(`/api/ps`)·비전/도구/추론 배지(`/api/show`)·로드(num_ctx)·언로드(`keep_alive: 0`)·대화/도구 호출·추론 끄기가 실제 Ollama에서 정상 동작하는지
- OpenAI 호환 로컬 서버(llama.cpp `llama-server` 또는 vLLM 등) 연결 시 [서버 시작]·[로드]·[언로드] 버튼이 숨겨지고, 대화창 모델 선택기가 단일 목록으로 표시되며, 입력한 컨텍스트 길이가 자동 압축 비율 계산에 반영되는지
- 외부 주소(`https://api.openai.com/v1` 또는 LAN IP) 입력 시 로컬 전용 보안 오류 메시지로 차단되는지

### 통합 설정 창 · 자동 업데이트
- 좌측 사이드바 하단에 `[설정]` 버튼 하나만 표시되고 오른쪽에 활성 공급자 이름과 연결 점이 맞는지
- `[설정]` 창 4개 탭(`연결 · 모델`, `Skill`, `MCP 서버`, `환경 · 업데이트`) 전환과 각 탭 동작이 매끄럽고, `MCP 서버`에서 서버 추가 폼이나 JSON 붙여넣기 화면을 연 채 `Esc`를 누르면 설정 창 전체가 아니라 편집 화면만 닫히는지
- `[설정]` → `Skill` → `찾아보기`에 기본 원격 Skill 63개가 표시되고 기존 설치된 Skill이 `설치됨`에 그대로 보이는지, `[설정]` → `MCP 서버` → `찾아보기`에 내장 웹 검색 8개 + 기본 원격 17개 = 25개와 `내 서버`의 기존 연결이 모두 보이는지
- 긴 프로젝트 작업 시 모델이 최근 읽은 파일을 잊고 되풀이하지 않으며, 동일 오류/읽기 반복 또는 생각 반복 시 안내 후 멈추는지, 실시간 생각 창이 스트리밍에 맞춰 자동 스크롤되고 위로 올리면 멈추는지 확인
- `환경 · 업데이트` 탭에서 현재 버전 `0.1.0`, [업데이트 확인], 자동 확인 체크가 정상 동작하는지(GitHub 릴리스가 올라간 이후 실제 업데이트 알림·설치 확인)
- 빌드된 `Any LM Desktop_0.1.0_x64-setup.exe` 설치 및 실행 확인

### 미리보기 IPC 격리 (결정 0019)
- 완료: 2026-11-25에 실행 중 확인(controller, `tauri dev` + WebView2 CDP): 메인 창이 그려지고 앱 invoke가 동작하며, `plugin:event|listen`은 되고 `plugin:event|emit`은 거부된다. 127.0.0.1:5173 페이지에는 `__TAURI_INTERNALS__`가 있었지만 invoke·emit이 거절됐다. `http://tauri.localhost:5173`·`http://ipc.localhost:5173`으로 가는 iframe(만들 때와 다시 이동할 때 모두)은 취소되고 "차단한 iframe 이동"이 찍혔다. WebView2 1.0.2420 미만 런타임의 중첩 프레임 경우는 확인하지 않았다. 아래 절차는 다시 확인할 때 쓴다.
- `npm run tauri dev`로 띄운 창에서 확인한다. 임시 폴더에 `index.html`을 만들고 아래 내용을 넣는다: `<iframe src="http://tauri.localhost:5173/x.html"></iframe>`. 같은 폴더의 `x.html`에는 `<pre id=o>…</pre><script>__TAURI_INTERNALS__.invoke('terminal_list').then(r=>o.textContent='열림 '+JSON.stringify(r),e=>o.textContent='거절 '+e)</script>`를 넣는다. 그 폴더에서 `python -m http.server 5173 --bind 127.0.0.1`을 실행하고, 작업 패널 미리보기에 `http://localhost:5173/`을 연다.
- 기대 결과: 안쪽 프레임이 열리지 않아야 한다. 개발 콘솔(stderr)에 "차단한 iframe 이동: http://tauri.localhost:5173/x.html"이 찍힌다. 프레임 차단을 끄고 시험했다면 화면에 "거절 허용되지 않은 출처의 요청입니다."가 떠야 한다. "열림 [...]"이 보이면 실패다.
- `x.html`을 미리보기 주소창에 직접 `http://tauri.localhost:5173/x.html`로 넣으면 "올바른 HTTP 또는 HTTPS 주소" 안내가 떠야 한다. `http://test.localhost:5173/`은 "외부 사이트는 브라우저에서 엽니다." 안내와 함께 브라우저로 열려야 한다.
- 개발 콘솔에 "중첩 iframe 이동은 감시하지 못합니다"가 찍히면 WebView2 런타임이 오래된 것이다(1.0.2420 미만). 런타임을 갱신한다.
- 끝나면 5173 서버를 끈다.
- 이벤트 권한을 listen·unlisten으로 줄였다(결정 0019의 5번). 확인할 것: 채팅 스트리밍, 터미널 출력과 종료 표시, 개발 서버 상태(시작 중 → 실행 중), 문서 변환이 예전처럼 화면에 오는지. 개발 콘솔에 `event.emit not allowed` 같은 권한 오류가 없어야 한다.

### 에이전트 루프 오탐 (전체 검토 수정 2차)
- 코딩 화면에서 Tailwind 표 컴포넌트처럼 같은 줄이 많은 코드를 쓰게 한다. 추론을 켠 채 끝까지 진행되고 "생각 반복이 감지되어" 표시가 뜨지 않아야 한다.
- 스크립트를 실행 → 실패 → 고침 → 다시 실행을 세 번 이상 되풀이해도 "같은 도구 오류가 반복되어"로 멈추지 않는지, `start_server` 뒤 `server_logs`를 여러 번 불러도 멈추지 않는지 확인한다.
- 16K 컨텍스트에서 큰 파일 5개 이상을 읽는 작업이 "필요하면 다시 읽으세요" 없이 진행되는지 본다. 컨텍스트가 실제로 넘치면 압축 안내가 뜨는 것은 정상이다. 첫 안내는 2단계(최근 4개 결과는 그대로)여야 하고, 이번 턴 질문은 잘리지 않아야 한다.
- 프로젝트·코딩 화면에서 생각 반복 뒤 1.0 재시도 라운드가 도구를 부를 때, 도구 JSON 인자와 `write_file` 내용이 깨지지 않는지 본다(결정 0017 보강의 남은 한계 M4).
- 일반 채팅(추론 ON)의 요청 본문에 `presence_penalty` 0.5, 프로젝트·코딩에는 없음을 개발 콘솔이나 하니스 기록으로 확인한다. 생각이 실제로 반복되면 다음 시도에 1.0, 또 반복되면 추론 끄기로 넘어가는지 본다.

### Ollama 컨텍스트 (전체 검토 수정 3차, B-I2)
- Ollama가 있는 PC에서 [Ollama 연결]로 모델을 16K로 로드하고 한 번 대화한다. `ollama ps`의 CONTEXT가 16384여야 한다. 5분 넘게 쉬었다가(모델이 내려감) 다시 보내도 `ollama ps`가 16384로 다시 올라와야 한다(4096이면 실패).
- 도구 호출(파일 읽기 등)과 결과가 이어지고, "생각하기"를 끄면 추론이 나오지 않는지(`think: false`), 생각 창에 추론이 보이는지 확인한다.
- [서버 시작]으로 켠 Ollama가 설정한 주소·포트에서 열리는지(예: 주소를 `http://127.0.0.1:11500`으로 바꾼 뒤 [서버 시작]).

### OpenAI 호환 컨텍스트 (전체 검토 수정 3차, B-I3·B-I5)
- `llama-server -m <모델> -c 8192`로 띄우고 [OpenAI 호환 연결]에서 모델이 "사용 가능 · 8K"로 보이는지(학습 길이가 아니라).
- LM Studio에서 모델을 고른 대화를 연 채 공급자를 Ollama로 바꾸면 "모델을 찾을 수 없습니다" 없이 Ollama 기본 모델로 보내지는지, 새 공급자 서버가 꺼져 있으면 모델 목록이 비는지.

### 터미널 출력 홍수 (전체 검토 수정 3차, C-I2)
- 작업 패널 PowerShell 탭에서 `1..200000 | % { $_ }`를 실행한다. 출력 중에도 앱(채팅 입력·탭 전환·스크롤)이 멈추지 않고, 넘친 곳에 `[출력 N바이트 생략]` 줄이 보이며, 끝나면 마지막 숫자 200000이 보여야 한다. 작업 관리자에서 앱 메모리가 계속 늘지 않아야 한다.

### 개발 서버 작업 폴더·준비 확인 (전체 검토 수정 1차)
- 설정에서 터미널 셸을 명령 프롬프트로 바꾼 뒤 Vite 프로젝트의 `npm run dev`를 시작한다. 로그에 "UNC 경로는 지원되지 않습니다"가 없어야 한다. 서버는 프로젝트 폴더에서 돌아야 한다.
- 로그에 `http://localhost:5173/`(::1에만 열림)이 찍힌 뒤 1~2초 안에 상태가 "실행 중"으로 바뀌는지 확인한다.
