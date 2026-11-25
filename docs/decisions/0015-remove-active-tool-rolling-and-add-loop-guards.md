# 0015 — 진행 중 턴 도구 기록 잘라내기 제거, 루프·반복 차단 및 생각 창 자동 스크롤

이 결정은 [0009 — 도구 라운드 상한 제거](0009-agent-loop-without-round-cap.md)의 라운드 상한 제거 및 진행 중 턴 도구 잘라내기(`roll_tool_history`) 결정을 대체한다.

## 실제 세션에서 확인된 원인

실제 대화 기록(`conv-e11e9347c31b4cf9b24b7fb880138da3`, 모델 `qwen3.6-35b-a3b`, 32K 컨텍스트) 분석 결과, 무한 루프의 직접 원인은 다음과 같았다.

1. **진행 중 턴 기억 상실(`roll_tool_history`)과 KV 캐시 무효화**:
   - 시스템 프롬프트·압축 요약(약 2.3만 자)이 이미 있는 상태에서 `index.php`(약 1만 자)와 `app.js`(약 1.08만 자)를 읽으면 전체가 `rolling_budget`(32K 기준 32,606자)을 곧바로 넘겼고, `roll_tool_history`가 `api_messages.drain(first..end)`로 방금 읽은 `index.php` 결과를 요청에서 지워 버렸다.
   - 실제로는 LM Studio 컨텍스트의 약 17K~18.6K/32K만 사용 중이었는데도 모델이 방금 읽은 파일을 잊고 `index.php` → `app.js` → `index.php`를 끝없이 다시 읽었으며(`msg[29]`에서 45회 호출, 도구 63회, 77분 소요), 매 라운드 `user_index` 앞에 `[앱이 보관한 이전 도구 실행 기록]`을 새로 끼워 넣어 KV 캐시도 매번 깨졌다.
2. **라운드 상한 제거 + 반복·고착 차단 부재**:
   - `MAX_TOOL_ROUNDS`를 없앤 상태에서 같은 도구 오류/같은 조회 반복을 막는 장치가 없어 63회 이상 헛돌았다.
   - 스트리밍 생각 과정(`reasoning_content`) 안에서 굵게 표시(`**`)만 번갈아 바뀌며 같은 3개 항목이 20회 이상(73,858자) 반복되는 퇴화 루프도 차단되지 않았다.
3. **생각 창 스크롤 고정**:
   - 실시간 생각 과정 박스(`max-h-40 overflow-y-auto`)가 `scrollTop = 0`에 고정되어 최신 사고 내용이 보이지 않았다.

## 변경 내용

- **진행 중 턴 기억 유지 및 지난 턴 인자 압축 (`src-tauri/src/exaone.rs`)**:
  - `roll_tool_history`, `roll_completed_tool_history`, `request_message_chars`, `remember_pruned_tool`과 `user_index` 앞 `[앱이 보관한 이전 도구 실행 기록]` 삽입을 완전히 제거했다.
  - `total_chars`가 `content`와 `tool_calls` JSON 길이를 함께 계산하도록 고치고, `compact_messages`에서 **이미 완료된 과거 턴**(`i < last_user_idx`) 또는 2단계 이상에서 최근 도구를 제외한 오래된 호출만 `compact_tool_arguments(args, 500)`로 축약한다. 현재 턴에서 읽은 최근 파일들은 그대로 유지한다.
- **라운드 상한 복원 및 루프 차단 가드 (`src-tauri/src/exaone.rs`)**:
  - `MAX_TOOL_ROUNDS = 50`을 복원했다.
  - `ToolLoopGuard`: 연속 도구 오류 2회에서 경고·4회에서 중단(`MAX_CONSECUTIVE_ERROR_ROUNDS = 4`), 동일 도구+인자 오류 2회 경고·3회 중단(`MAX_SAME_ERROR_CALLS = 3`), 상태 변경 없는 동일 읽기 전용 도구(`read_file`, `list_directory`, `search_files`, `search_knowledge`, `server_logs`, `load_skill`, `read_skill_file`) 반복 3회 경고·4회 중단(`MAX_SAME_READ_CALLS = 4`). 비-읽기 전용 도구(쓰기·수정·명령 등)가 성공하면 읽기 카운터를 초기화한다.
  - `detect_repetition_loop`: 스트리밍 중 256바이트마다 및 라운드 종료 시 끝부분 2,400자에서 `*`/`_`·공백 차이를 정규화해 긴 문구(`36..=450`자 주기, 서로 다른 문자 8개 이상)가 3~4회 연속 반복되면 즉시 생성을 멈추고 `continue_prompt` 이어 받기를 건너뛴다.
- **생각 창 자동 스크롤 (`src/chat/reasoningScroll.ts`, `src/App.tsx`)**:
  - 실시간 생각 과정 창에 `ref`와 `onScroll` 추적(`isNearBottom`, 임계값 32px)을 추가해 사용자가 위를 올려 읽고 있지 않을 때만 자동으로 맨 아래(`scrollHeight`)를 따라 내려간다.

## 갱신 (2026-11-25, 전체 검토 수정 2차)

검토에서 루프 가드와 1단계 줄이기가 정상 작업을 멈추는 경로가 확인되어 다음처럼 고쳤다. 라운드 상한(50), `ToolLoopGuard`, 반복 탐지기는 그대로 두고 기준만 좁혔다.

- **같은 오류 횟수 초기화**: 예전에는 읽기 횟수만 초기화해서, `run_python`(실패) → `modify_file` → `run_python`(실패) → `modify_file` → `run_python`(실패)처럼 고칠 때마다 다시 실행하는 정상 흐름이 세 번째 실패에서 멈췄다. 이제 상태를 바꾸는 도구가 성공하면 같은 오류 횟수도 함께 초기화한다. 고치지 않고 같은 호출이 3번 실패하면 여전히 멈춘다. 연속 오류 라운드 규칙(성공이 있으면 0)은 그대로다.
- **상태를 바꾸는 도구 명시 목록**: `write_file`, `modify_file`, `delete_path`, `run_command`, `run_python`, `start_server`, `stop_server`, MCP 도구(`mcp__…`, 무엇을 하는지 몰라 상태를 바꾸는 것으로 본다). 예전 기준("읽기 전용이 아닌 도구")에서는 `open_preview`, `remember`도 초기화를 일으켜 읽기 반복을 숨겼다.
- **`server_logs`는 읽기 반복으로 세지 않는다**: 서버 출력은 시간이 지나며 바뀌어, 서버를 띄운 뒤 로그를 여러 번 확인하는 것이 정상이다. 읽기 전용 도구 목록에는 남는다.
- **1단계 줄이기는 이번 턴을 건드리지 않는다**: 16K 기본 컨텍스트(0018)와 `tool_calls`를 포함한 글자 수 계산 때문에 1단계가 거의 늘 켜졌고, 이번 턴 결과도 마지막 4개를 빼고 3,000자로 줄여 "필요하면 다시 읽으세요"를 붙였다. 모델이 A를 다시 읽으면 B가 잘리는 식으로 돌다가 같은 파일 세 번째 읽기에서 경고, 네 번째에서 멈췄고, 줄이는 범위가 라운드마다 움직여 KV 캐시도 매번 깨졌다. 이제 1단계는 지난 턴의 도구 결과(그중 마지막 4개 제외)와 지난 턴 도구 인자만 줄인다. 이번 턴 결과는 실제 컨텍스트 초과 오류로 2단계 이상이 되었을 때만 줄인다.
- **턴 시작 위치를 한 번만 정한다**: 빈 답 재요청과 이어 쓰기 요청(`continue_prompt`)은 사용자 역할 메시지라서, 매번 "마지막 사용자 메시지"를 찾으면 그 앞의 이번 턴 `write_file` 인자를 지난 턴 것으로 보고 500자 요약으로 바꿨다. 이제 턴 시작 때 실제 사용자 메시지 위치(`turn_start`)를 정해 `compact_messages`에 넘긴다.
- **앱이 잘라 낸 결과를 다시 읽는 것은 반복이 아니다**: 줄이기 단계가 어떤 조회의 가장 최근 결과를 잘라 냈으면, 그 조회의 읽기 횟수를 지운다(`compacted_tool_keys`, `ToolLoopGuard::forget_reads`). 온전한 최근 결과가 남아 있는데 또 읽으면 그대로 센다.
