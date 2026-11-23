# STATUS — 현재 상태 한 장

## 지금 목표
원본 K-EXAONE Desktop 0.4.4를 LM Studio 전용 앱 "Any LM Desktop"(0.1.0)으로 바꾼다. 설계: `docs/superpowers/specs/2026-11-23-any-lm-desktop-design.md`.

## 진행 중
- 계획 파일: `docs/superpowers/plans/2026-11-23-any-lm-desktop.md`
- 작업 1(저장소 만들기와 인계 체계): 단계 1~5 완료. 단계 6(GitHub 비공개 저장소 `kidarigun/any-lm-desktop` 생성·푸시)은 사용자 승인 대기 중이라 아직 하지 않았다.
- 작업 2(정체성 분리와 업데이트 제거): 완료. 제품명·identifier·버전·크레이트명·자격 증명 서비스명·단일 실행 포트(45789)·미리보기 호스트·대화상자 환경 변수·localStorage 접두어(`anylm_`)·기본 작업 폴더(`문서\AnyLM`)를 바꾸고, 업데이트 기능(`tauri-plugin-updater`, UpdatePanel, `check_update`/`install_update`, `checkUpdates` 설정, `make_latest_json.py`)과 `migrate_old_data_dir`을 삭제했다.

- 작업 3(설정을 LM Studio 연결로 바꾸기): 완료. `lmstudio.rs`(server_urls·authorize·friendly_error) 추가, `AppSettings`를 serverUrl/선택 토큰/autoCompact/modelPrefs로 교체, exaone.rs의 API 키 필수 검사·reasoning_effort 삭제, 추론 강도 선택 UI 삭제(작업 6에서 모델 선택기가 들어갈 자리). 설정 대화상자는 최소 수정만 했다(작업 5에서 재구성).

- 작업 4(모델 목록·로드·서버 상태): 완료. `lmstudio.rs`에 parse_models·plan_load·wanted_context·list/load/unload·server_running·lms_path·start_server, Tauri 명령 `lm_*` 6개, TS `BackendAPI`(listModels 등)와 타입 추가. 결정 기록 `docs/decisions/0002-lmstudio-load-api.md`. 실측 후 LM Studio는 qwen3.5-9b(32768)만 로드된 상태로 복구함.
- 작업 5(LM Studio 연결 창): 완료. `src/lmstudio/`(modelText, ConnectionDialog)를 추가하고 설정 창을 교체. 사이드바 버튼은 [LM Studio 연결] + 연결 점(열 때·15초마다·저장 후 `serverRunning()` 확인). 창에서 서버 시작, 모델 로드/다시 로드(언로드 후 로드)/언로드, 기본 모델 선택, 모델별 컨텍스트(modelPrefs), 자동 요약 압축을 다룬다. 원본에 문체 선택 UI가 없어 추가하지 않았다.
- 작업 6(대화별 모델 선택): 완료. 대화마다 `modelKey`를 저장하고(비면 기본 모델) 요청 본문 `"model"`과 답변의 `modelKey`에 쓴다. 입력창 아래 ModelPicker(로드된 모델이 위, `loadedLabel` 표시), 보내기 전 `planLoad`→`nextSendStep` 흐름(바로 보내기 / 로드 중 표시 후 로드 / LoadChoiceDialog / 컨텍스트 작게 로드됨 안내와 [다시 로드] / 모델 없음 / 로드 실패 시 [내리고 로드]). 프로젝트·코드 화면에서 도구 미지원 모델이면 경고만 한다. 답변 아래 통계 줄 앞에 모델 이름. 하니스(harness.rs)는 settings.model만 넘긴다(작업 11에서 정리).
- 작업 7(추론 켜기/끄기): 완료. 실측 결과 `reasoning_effort: "none"`만 9B·35B 모두 reasoning_tokens 0(결정 0003). `lmstudio::reasoning_fields`/`REASONING_OFF_FIELDS`, 턴 시작 때 모델 목록으로 `TurnRequest.reasoning_options`를 채워 요청 본문에 반영(작업 8이 같은 목록을 재사용할 수 있음), `preserve_thinking` 삭제·`keep_thinking`은 항상 false. ModelPicker 옆 "생각하기" 체크(추론 선택지가 있는 모델에서만, `modelPrefs[key].reasoning` 저장). 실측 후 LM Studio는 qwen3.5-9b(32768)만 로드된 상태.
- 작업 8(컨텍스트 예산): 완료. `context_budget.rs`(FALLBACK 8192, 0.70, 1.5자/토큰, trim_trigger_chars, tool_output_cap, resolve_context, usage_ratio). 턴 시작 때 `lmstudio::turn_model_info`가 모델 목록을 한 번(3초 제한) 읽어 추론 선택지와 로드된 컨텍스트를 함께 주고, `TurnRequest.context_length`가 줄이기 기준(`COMPACT_TRIGGER_CHARS` 삭제)과 도구 결과 상한을 정한다. 초과 오류는 비스트리밍 HTTP 400, 스트리밍은 HTTP 200 안의 `event: error`(결정 0004). 실측 후 qwen3.5-9b(32768) 1개만 로드된 상태.

## 마지막 확인
2026-11-23, 작업 8 완료 시점:
- `npm test` → 통과 (18 파일, 111 통과, 1 건너뜀)
- `npx tsc --noEmit` → 통과 (오류 없음)
- `cd src-tauri && cargo test --lib` → 통과 (148 통과, 0 실패, 8 무시)
- `npm run build` → 성공 (GUI 확인은 못 함)
1. 계획의 작업 9부터 이어서 진행한다(`docs/superpowers/plans/2026-11-23-any-lm-desktop.md`).
2. 작업 1의 단계 6은 사용자가 승인하면 한다: "`kidarigun/any-lm-desktop` 비공개 저장소를 만들고 main을 올릴까요?" → 승인 시 `gh repo create kidarigun/any-lm-desktop --private --source . --remote origin --push`.

## 결정 필요
- GitHub 비공개 저장소 생성·푸시 여부(작업 1 단계 6).

## 기준 상태
작업을 시작하기 전(복제 직후, e885b02)의 테스트 결과다. 이후 실패가 나오면 이 기준과 비교해 새로 깬 것인지 구분한다.
- `npm install`: 성공
- `npm test`: 16 파일 통과, 102 통과 / 1 건너뜀 / 실패 0
- `npx tsc --noEmit`: 오류 0
- `cargo test --lib`: 121 통과 / 실패 0 / 7 무시(ignored)

## 수동 확인 목록
(사람이 화면에서 직접 확인해야 하는 항목을 이후 작업이 여기에 덧붙인다.)
- 원본 앱과 새 앱 창이 동시에 뜨는지 눈으로 확인 (작업 2: 프로세스·포트·데이터 폴더만 확인했고 창 모양은 보지 못했다)
- 앱 개발자 도구에서 lm_list_models 결과 확인: `await window.__TAURI__.core.invoke("lm_list_models")` → 모델 2개(작업 4는 GUI를 못 봐서 `#[ignore]` 실서버 테스트로 대신 확인)
- LM Studio 화면에서 모델별 GPU 설정(전문가 가중치 CPU로)을 저장해 둔 뒤 앱으로 로드해도 그 값이 유지되는지 (작업 4: 저장값이 없는 상태에서만 확인, 결정 기록 0002)
- LM Studio 연결 창에서 두 모델이 보이고 9B 행이 '로드됨 · 32K'인지 확인 (작업 5: GUI를 못 봐서 단위 테스트와 빌드로만 확인)
- 한 대화에서 9B로 한 번 묻고 모델 선택을 35B로 바꿔 한 번 더 묻기 → [내리고 로드] 후 35B가 답하고 두 답변 아래 모델 이름이 다른지, 입력창 아래 모델 선택 모양·로드 중 표시·'다시 로드' 안내 (작업 6: GUI를 못 봐서 단위 테스트·빌드·curl 요청 본문 확인만 했다. 8GB VRAM이라 35B는 로드하지 않았다)
- 입력창 아래 "생각하기" 체크가 추론 지원 모델(9B·35B)에서만 보이고, 끄고 보낸 답변이 생각 없이 바로 나오는지, 켜면 다시 생각하는지 (작업 7: GUI를 못 봐서 단위 테스트·빌드·curl 실측만 했다. 35B는 8192로 잠깐 로드해 curl로만 확인)

## 마지막 작업자
- 도구: claude-code
- 모델: Claude Sonnet 5.5
- 시각: 2026-11-23
