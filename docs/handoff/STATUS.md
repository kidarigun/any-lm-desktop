# STATUS — 현재 상태 한 장

## 지금 목표
원본 K-EXAONE Desktop 0.4.4를 LM Studio 전용 앱 "Any LM Desktop"(0.1.0)으로 바꾼다. 설계: `docs/superpowers/specs/2026-11-23-any-lm-desktop-design.md`.

## 진행 중
- 계획 파일: `docs/superpowers/plans/2026-11-23-any-lm-desktop.md`
- 작업 1(저장소 만들기와 인계 체계): 단계 1~5 완료. 단계 6(GitHub 비공개 저장소 `kidarigun/any-lm-desktop` 생성·푸시)은 사용자 승인 대기 중이라 아직 하지 않았다.
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

## 마지막 확인
2026-11-23, 작업 12 단계 1(완료 기준 실측). 명령으로 확인한 것만 적었고 화면은 보지 못했다.
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
- 원본 앱과 새 앱 창이 동시에 뜨고(작업 12에서 두 창이 같이 뜨는 것은 프로세스로 확인), 서로의 대화·설정이 보이지 않는지, 창 모양이 어색하지 않은지
- 헤더 이름 "Any LM Desktop"과 "LM" 로고 칸, 프로젝트 지침 자리표시·압축 창 문구가 어색하지 않은지
- 화면 어디에도 호출 한도·키 만료·추론 강도(low/high/max)·업데이트 표시가 없는지
- 원본 앱을 켠 채 MCP OAuth 로그인: 원본 앱(45788·45789)을 켜 둔 상태에서 새 앱의 원격 MCP [로그인]이 브라우저 로그인 후 성공하는지(새 앱은 중복 실행 45790, 로그인 결과 45791). `netstat -ano | findstr 4579`로 45790이 LISTEN인지도 확인

### LM Studio 연결 창
- 두 모델이 보이고 9B 행이 '로드됨 · 32K'인지, 연결 점이 맞게 바뀌는지
- 개발자 도구에서 `await window.__TAURI__.core.invoke("lm_list_models")` → 모델 2개
- LM Studio 화면에서 모델별 GPU 설정(전문가 가중치 CPU로)을 저장해 둔 뒤 앱으로 로드해도 그 값이 유지되는지 (결정 기록 0002)

### 대화 화면
- 한 대화에서 9B로 한 번 묻고 모델 선택을 35B로 바꿔 한 번 더 묻기 → [내리고 로드] 후 35B가 답하고, 두 답변 아래 모델 이름이 다른지. 입력창 아래 모델 선택 모양·로드 중 표시·'다시 로드' 안내 (35B는 8GB VRAM에서 로드하지 않았다)
- 로드되지 않은 모델을 고르고 보내면 앱이 정한 컨텍스트로 로드한 뒤 답하는지
- 입력창 아래 "생각하기" 체크가 추론 지원 모델(9B·35B)에서만 보이고, 끄면 생각 없이 바로 답하며 켜면 다시 생각하는지
- 답변 아래 통계 줄(입력 · 출력 · 추론 · 초당 토큰 · 모델 이름)이 어색하지 않은지
- 9B를 8192로 로드해 긴 답변이 나오는 질문을 몇 번 해서, 70%를 넘을 때 "컨텍스트가 70% 찼습니다. 이전 대화를 요약하는 중…"이 보이고 압축 카드가 생기며 대화가 이어지는지. 확인 뒤 32768로 되돌릴 것

### 프로젝트 화면
- 프로젝트에서 파일을 만들고 고친 뒤 되돌리기 버튼으로 이전 내용이 돌아오는지 (백엔드는 테스트로 확인)
- 에이전트 작업 도중 컨텍스트가 넘칠 때 긴급 압축으로 이어 가는지(필요하면 9B를 8192로 로드해 확인)

## 마지막 작업자
- 도구: claude-code
- 모델: Claude Opus 5.5
- 시각: 2026-11-23
