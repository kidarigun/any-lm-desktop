# STATUS — 현재 상태 한 장

## 지금 목표
원본 K-EXAONE Desktop 0.4.4를 LM Studio 전용 앱 "Any LM Desktop"(0.1.0)으로 바꾼다. 설계: `docs/superpowers/specs/2026-11-23-any-lm-desktop-design.md`.

## 진행 중
- 계획 파일: `docs/superpowers/plans/2026-11-23-any-lm-desktop.md`
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
- 지식 폴더(스펙 5절): 완료. `knowledge.rs`(훑기·건너뛰기·한도, 조각 1,200자/겹침 200, 토큰 = 영문·숫자 소문자 + 한글 2글자 묶음, BM25 k1 1.2·b 0.75, 색인 `앱 데이터\knowledge\<id>.json`에 파일별 크기·수정 시각, 깨지거나 옛 형식이면 다시 만듦), `Project.knowledge_folders`, 명령 `add_knowledge_folder`·`remove_knowledge_folder`·`reindex_knowledge`·`knowledge_status`(색인은 뒤에서, 이벤트 `anylm-knowledge-status`), 턴 시작(보내기·다시 보내기·승인 뒤 이어가기) 빠른 갱신(5초, 남은 것은 다음으로) + 마지막 사용자 메시지로 `[참고 자료]` 절(시스템 프롬프트 뒤, min(12000, 컨텍스트×1.5×0.15)자, 6조각, `### 출처: <경로>`; 자동 압축 어림에도 넣음), `search_knowledge` 도구(승인 없음, 도구 출력 상한, 지식 폴더 있을 때만 목록에). 화면은 프로젝트 화면 "지식 폴더" 카드(`src/project/KnowledgeCard.tsx`). 세부 결정은 `docs/decisions/0005-knowledge-folders.md`. 하니스 시나리오 `harness/knowledge.json`(knowledgeFiles/knowledgeFolders). 실측: 그때 LM Studio에 로드돼 있던 모델이 qwen3.6-35b-a3b(32768)뿐이어서(9B는 로드 안 됨, 로드·언로드하지 않음) 35B로 확인 — 지식 폴더에만 있는 제품 코드의 출시일과 출처 경로가 답에 나옴(3/3, 4/4). 9B 실측은 아직.
- 지침 편집기 후속: 프로젝트 설정 창의 [큰 편집기로 열기]는 창의 값이 저장된 값과 같을 때만 쓸 수 있고(다르면 "큰 편집기: 먼저 저장하세요"), 화면의 프로젝트를 바꾸지 않는다. 편집기 Esc는 포커스가 창 밖이어도 듣고(창이 열려 있는 동안만 window 리스너), `role="dialog" aria-modal="true"`.

## 다음 할 일
1. **화면 수동 확인**: 아래 "수동 확인 목록"을 사용자와 함께 앱 화면에서 확인한다(`npm run tauri dev`). 문제가 나오면 고치고 목록에서 지운다.
2. **인계 연습(완료 기준 9, 작업 12 단계 2)**: 사용자가 Codex 또는 Antigravity 새 세션에 `docs/handoff/START.md`의 시작 문구를 붙여 넣어, 그 도구가 AGENTS → STATUS → 계획 순서로 읽고 이어서 작업할 수 있는지 본다. 결과는 LOG에 남긴다.
3. **GitHub 비공개 저장소**: 완료(2026-11-23, 사용자 승인). `origin` = https://github.com/kidarigun/any-lm-desktop (비공개), `main`이 `origin/main`을 추적. `upstream`(원본 k-exaone-desktop) push 주소는 `DISABLED` 유지. 이후 push는 작업을 마칠 때 사용자가 허락한 경우에만 한다.
   - 인계 연습용 작은 작업: 계획 문서(`docs/superpowers/plans/2026-11-23-any-lm-desktop.md`)에 남은 옛 포트 표기 `PORT = 45789`를 지금 값(중복 실행 45790, MCP OAuth 45791)으로 고친다 — 계획을 그대로 따르면 포트 테스트가 깨진다.
4. **미뤄 둔 사소한 점(최종 검토, 지금 고치지 않아도 됨)**:
   - 9: AI 요약의 대화 글자 상한이 고정 60,000자이고 생각 켜짐을 물려받아 8K/32K에서 로컬 요약으로 자주 떨어짐 → 컨텍스트×1.5×0.6 상한, 요약은 생각 끄기 검토.
   - 10: 승인 후 이어가기·수정 후 다시 보내기는 모델 준비를 하지 않고, 다시 보내기는 자동 압축도 하지 않음.
   - 11: [내리고 로드]가 다른 대화가 쓰는 모델을 내릴 수 있음(경고 없음).
   - 12: 통계 줄이 없는 답변(취소·실패)에는 모델 이름이 안 보임, 승인 뒤 다른 모델이 이어도 `model_key`는 처음 값.
   - 13: [다시 로드] 안내가 같은 모델의 인스턴스 하나만 내림(연결 창은 전부 내림).
   - 14: 컨텍스트를 모르는 인스턴스면 "0K 컨텍스트로 로드돼 있습니다"로 보임.
   - 15: 기본 모델이 없을 때 첫 보내기가 빈 대화를 남김.
   - 17: 답변 중 서버가 죽으면 연결 거부를 복구 가능 오류로 보고 약 67초 기다린 뒤 안내.
   - 그 밖에 검토의 "Can stay" 목록(서버 상태는 아무 HTTP 응답이면 켜짐으로 봄 등).

## 마지막 확인
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
| 9 | 인계 연습 | 사용자와 함께 진행 예정 | 작업 12 단계 2 |

## 결정 필요
- 작업 12 단계 2(인계 연습)는 사용자와 함께 한다.
- GitHub 비공개 저장소 생성·푸시 여부(작업 1 단계 6).

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
- 두 모델이 보이고 9B 행이 '로드됨 · 32K'인지, 연결 점이 맞게 바뀌는지
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
- 시각: 2026-11-23

### 프로젝트 화면 · 메모리
- 모델이 `remember`로 저장하면(예: "이 프로젝트 보고서는 항상 존댓말로 쓴다고 기억해") 대화 중 도구 카드가 뜨고 프로젝트 화면 "프로젝트 메모리" 카드에 [모델] 표시로 바로 보이는지, [추가]로 직접 쓴 항목은 [직접]으로 보이는지, 편집·삭제(삭제 전 확인)가 되는지, 새 대화에서 모델이 그 내용을 아는지 확인.
- 지침 편집기: 지침 카드 [편집] → [서식 예시]로 4종(보고서·문서 작성/코딩/자료 조사·요약/번역·교정) 넣기(비어 있으면 대체, 있으면 끝에 덧붙임), "n자 · 약 m토큰 · 컨텍스트의 x%"가 입력 중 바뀌고 10% 넘으면 노란색인지, 저장 뒤 새 대화에서 모델이 그 지침을 따르는지(시스템 프롬프트 반영), Esc·취소 때 변경 확인이 뜨는지

### 프로젝트 화면 · 지식 폴더
- [폴더 추가] → 폴더 고르기 대화상자 → 목록에 경로가 생기고 상태 줄이 "색인 중… (n/m)"에서 "파일 n개 · 조각 n개 · 건너뜀 n개 · 마지막 색인 YYYY-MM-DD HH:MM"로 바뀌는지, 폴더 줄에 파일·조각 수, 화면이 멈추지 않는지. 같은 폴더를 다시 더하면 "이미 추가한 지식 폴더입니다", docx/hwpx/pdf가 든 폴더면 그 파일도 색인되는지(웹뷰 변환기), 건너뛴 파일이 있으면 [건너뛴 파일 보기]에 경로와 이유
- 그 폴더 문서에만 있는 내용을 프로젝트 대화에서 물으면 답에 그 내용과 출처 경로("출처: ...")가 나오는지(9B 32K로도 확인할 것), 필요하면 도구 카드에 `search_knowledge` 결과가 보이는지
- 폴더의 파일을 고친 뒤 다시 물으면 고친 내용으로 답하는지(턴 시작 빠른 갱신), [다시 색인]을 누르면 "색인 중…" 뒤 마지막 색인 시각이 바뀌는지
- [제거] → 확인 창 → 목록에서 빠지고, 그 폴더 내용으로는 더 답하지 않는지. 지식 폴더 안에 앱이 만든 파일이 하나도 없는지
- 프로젝트 설정 창에서 지침을 고치면 [큰 편집기로 열기] 대신 "큰 편집기: 먼저 저장하세요"가 보이고, 고치지 않았으면 열리며 화면의 프로젝트가 바뀌지 않는지. 지침 편집기에서 바탕을 누른 뒤 Esc로도 닫히는지
