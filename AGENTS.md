# AGENTS.md — Any LM Desktop 작업 지침

이 파일은 Claude Code, Codex, Antigravity가 모두 따르는 **유일한 공통 지침**이다. (`CLAUDE.md`, `GEMINI.md`는 이 파일을 가리키기만 한다.)

## 1. 프로젝트

Any LM Desktop은 사용자가 만든 K-EXAONE Desktop 0.4.4(Tauri v2 + React/TypeScript + Rust, 커밋 `e885b02`)를 이력째 복제해 **로컬 LLM(LM Studio · Ollama · OpenAI 호환 로컬 서버) 전용 데스크톱 작업 앱**으로 바꾸는 비공개 프로젝트다. 로컬 LLM 서버는 주로 채팅만 지원하므로, 이 앱이 문서 작성·캔버스·프로젝트 폴더 작업·코드 작업(에이전트 루프, HWPX/DOCX/PPTX/XLSX, Skill, MCP, 실행 환경 관리, 컨텍스트 압축)을 맡는다. 대상 환경은 RTX 5060 Laptop 8GB, RAM 32GB PC의 로컬 모델(Qwen3.5-9B, Qwen3.6-35B-A3B 등)이다. 설계 문서는 `docs/superpowers/specs/2026-11-23-any-lm-desktop-design.md`, 멀티 공급자 설계는 `docs/superpowers/specs/2026-11-24-multi-local-providers-design.md`, 구현 계획은 `docs/superpowers/plans/2026-11-23-any-lm-desktop.md` 및 `docs/superpowers/plans/2026-11-24-multi-local-providers.md`.

## 2. 폴더 지도

Rust (`src-tauri/src/`)
- `lib.rs` — Tauri 앱 조립과 명령 등록. `main.rs` — 진입점.
- `exaone.rs` — 모델 호출·에이전트 루프(`run_exaone_turn`)·도구 실행·스트림 처리·대화 압축 요약(이름은 원본 그대로).
- `lmstudio.rs` — LM Studio 연결: 서버 주소 검증(`server_urls`, localhost만, 경로는 버림)·인증 헤더·오류 안내, 모델 목록·로드 계획(`plan_load`)·로드/언로드, 서버 상태·`lms server start`, 추론 끄기 필드, 턴 시작 모델 정보(`turn_model_info`).
- `providers/` — 멀티 로컬 LLM 공급자 어댑터(결정 0012): `mod.rs`(공통 로컬 URL 보안 검증·`ProviderCapabilities`·공급자별 오류/메시지·라우팅), `ollama.rs`(`/api/tags`·`/api/ps`·`/api/show`·`/api/generate` 로드/언로드·`ollama serve` 분리 시작), `openai_compat.rs`(경로 정규화·`/v1/models` 파싱·즉시 Ready `plan_load`).
- `context_budget.rs` — 로드된 컨텍스트에 맞춘 줄이기 기준·도구 결과 상한·사용 비율(70%, 1토큰 = 1.5자).
- `testdata/` — 테스트용 LM Studio 응답(`lmstudio_models.json`).
- `store.rs` — 대화·설정 저장 파일과 스냅샷 자료형.
- `memory.rs` — 프로젝트 메모리(항목 추가·수정·삭제 규칙, 한도, 시스템 프롬프트 절).
- `knowledge.rs` — 지식 폴더: 훑기·조각·토큰(한글 2글자 묶음)·BM25·색인 파일(`knowledge/<id>.json`)·빠른 갱신·`[참고 자료]` 절·`search_knowledge` 도구(테스트는 `knowledge_tests.rs`).
- `fs_ops.rs` — 프로젝트 폴더 안의 파일 읽기/쓰기/목록(경로 검증 포함).
- `doc_bridge.rs` — Rust가 웹뷰의 문서 변환 라이브러리(DOCX/HWPX/PPTX/PDF/XLSX)를 호출하는 다리.
- `snapshot_diff.rs` — 스냅샷의 이전/이후 내용 텍스트 추출(변경 보기).
- `html_preview.rs` — HTML 미리보기용 로컬 서버.
- `mcp.rs` / `mcp_oauth.rs` — MCP 클라이언트 / 원격 MCP OAuth 로그인.
- `skills.rs` — Skill 찾기·읽기·설치, 요청에 맞는 Skill 자동 불러오기(`select_for_message`·`auto_for_turn`, 결정 0007). `catalog.rs` — 원격 Skill·MCP 카탈로그 받기(내장 목록은 프런트 `src/catalog/builtin.ts`).
- `python_runner.rs` — 프로젝트 안 Python 스크립트 실행. `runtime.rs` — 앱 전용 Python/Node/uv 설치·관리.
- `runs.rs` — 대화별 실행 중복 방지와 중지 플래그.
- `terminal.rs` — 작업 패널 터미널(PTY, `portable-pty`/ConPTY): 세션 관리·링 버퍼·UTF-8 이어 붙이기·ANSI 제거(`plain_text`)·Job Object+taskkill 트리 종료, `terminal_*` 명령과 `anylm-terminal-*` 이벤트(결정 0008, 테스트는 `terminal_tests.rs`).
- `dev_servers.rs` — 프로젝트 개발 서버 감지·실행·중복 방지·로컬 미리보기, ConPTY 로그와 포트 확인, 승인형 시작·프로젝트별 로그/종료(결정 0010, 하니스 `harness/dev-server.json`).
- `secrets.rs` — Windows 자격 증명 관리자(공급자별 API 토큰 `lm-studio-api-token`·`ollama-api-token`·`openai-compat-api-token`, MCP 비밀 값 보관).
- `single_instance.rs` — 앱 중복 실행 방지.
- `harness.rs` — 실제 로컬 LLM 서버로 에이전트 루프를 돌려 보는 테스트 하니스(`ANYLM_*` 환경 변수, `ANYLM_PROVIDER`로 공급자 선택, 시나리오 `mcpServers`로 앱의 MCP 관리자 연결).

프런트엔드 (`src/`)
- `App.tsx`, `main.tsx`, `api.ts`, `types.ts`, `styles.css` — 앱 본체, Tauri 명령 호출, 공용 타입·스타일.
- `lmstudio/` — 로컬 LLM 연결 창(`ConnectionDialog`, 공급자 3종 탭 전환), 공급자 메타·프로필 동기화(`providerMeta`), 모델 선택(`ModelPicker`·"생각하기" 토글), 내리고 로드 선택(`LoadChoiceDialog`), 보내기 전 모델 준비 규칙(`sendFlow`: 로드 계획→다음 단계, 원하는 컨텍스트, 작게 다시 시도 값), [서버 시작] 버튼(`StartServerButton`), 모델 설명 글자(`modelText`).
- `chat/` — 대화별 실행 상태와 답변 통계 줄(`statsLine`). `canvas/` — 문서 캔버스(편집기·PDF/PPTX/XLSX/HWPX 뷰어).
- `docs/` — 문서 형식 변환(docx, hwpx, pptx, pdf, xlsx, markdown)과 Rust 다리 구현.
- `project/` — 프로젝트 메모리 카드(`ProjectMemoryCard`)와 표시 규칙(`memoryFormat`), 지침 전용 편집기(`InstructionsEditor`)와 글자·토큰 계산·서식 예시(`instructionsFormat`), 지식 폴더 카드(`KnowledgeCard`)와 상태 줄·설정 창 비교 규칙(`knowledgeFormat`).
- `explorer/` — 파일 탐색기·@파일 멘션. `diff/` — 변경 보기와 이력.
- `mcp/` — MCP 대화상자. `skills/` — Skill 대화상자와 "Skill 자동 불러오기" 스위치·카드 이름표 규칙(`autoLoad`). `catalog/` — 카탈로그 화면과 규칙(`catalog.ts`), 앱에 내장한 공식 웹 검색 MCP 8개(`builtin.ts`, 원격 목록과 `mergeCatalogs`로 합침. 결정 0006).
- `python/` — Python 실행 대화상자. `runtime/` — 실행 환경 패널.
- `workpanel/` — 오른쪽 작업 패널: 열기/닫기·폭·Ctrl+`·캔버스와 함께 보기(`panelState`), 셸/서버/미리보기 탭(`tabs`), 상태 훅(`useWorkPanel`), 패널(`WorkPanel`, 지연 로딩)·터미널(`TerminalView`, `terminalApi`)·서버 실행(`ServerMenu`, `devServerApi`)·로컬 미리보기(`PreviewView`).
- `security/` — HTML 정화. `ui/` — 패널 크기 조절·스크롤바 공용 UI, 화면 문구 브랜드 검사 테스트(`noBrandText`).

그 밖: `harness/` 실측 시나리오와 결과 요약 스크립트, `scripts/third_party_notices.py`(THIRD_PARTY_NOTICES 생성), `docs/handoff/` 인계 문서, `docs/decisions/` 결정 기록, `docs/superpowers/` 설계·계획.

## 3. 빌드·테스트 명령

- `npm test` · `npx tsc --noEmit` · `cd src-tauri && cargo test --lib`
- 개발 실행: `npm run dev`(웹뷰만), `npm run tauri dev`(앱 전체).
- LM Studio 실측: `harness/` 시나리오 (LM Studio 서버와 모델이 켜져 있을 때만).

## 4. 코딩 규칙

- 저장소는 `C:\DOCS\any-lm-desktop`. 원본 `C:\DOCS\EXAONE`와 그 GitHub 저장소에는 아무것도 쓰지 않는다(커밋·푸시·파일 수정 금지).
- 앱 이름 `Any LM Desktop`, identifier `io.github.kidarigun.anylm-desktop`, 버전 `0.1.0`.
- 포트: 중복 실행 방지 `45790`(`single_instance::PORT`), 원격 MCP OAuth 로그인 결과 `45791`(`mcp_oauth::CALLBACK_PORT`, 리디렉션 URI `http://localhost:45791/oauth2callback`). 원본 앱은 45788·45789를 쓰므로 이 값들과 겹치면 안 된다(테스트 `ports_do_not_collide_with_each_other_or_the_original_app`).
- 내부 파일·함수·이벤트 이름(`exaone.rs`, `run_exaone_turn`, `onExaoneStream`, `"exaone-stream"`)은 바꾸지 않는다. 사용자에게 보이는 문자열과 스펙 6.2 표의 충돌 요소만 바꾼다.
- 화면 문구는 한국어. 사용자에게 보이는 곳에 "EXAONE"/"K-EXAONE"/"LG"를 쓰지 않는다(NOTICE·README의 출처 설명만 예외).
- 로컬 LLM 서버 주소는 모든 공급자(LM Studio · Ollama · OpenAI 호환)에서 `http://localhost`, `http://127.0.0.1`, `http://[::1]`만 허용. 기본값은 `lmstudio` `http://localhost:1234`, `ollama` `http://localhost:11434`, `openai_compat` `http://localhost:8080`.
- 기본 원하는 컨텍스트 `32768`. 자동 요약 압축 기준 `0.70`(로드된 컨텍스트 대비). 글자→토큰 추정 `1토큰 = 1.5자`(`chars()` 기준).
- 모델 로드 요청 시간 제한 5분. 응답 헤더 대기 180초, 스트림 무응답 900초(원본 유지).
- 앱은 사용자가 [내리고 로드]·[다시 로드]·[언로드]를 누를 때만 모델을 내린다.
- 매 작업 끝: `npm test` · `npx tsc --noEmit` · `cd src-tauri && cargo test --lib` 모두 통과 → 계획 체크박스 · `docs/handoff/STATUS.md` 갱신 → 한 커밋. 커밋 메시지 끝에 `Agent: <claude-code|codex|antigravity>` 줄.
- LM Studio로 실측하는 작업은 끝나면 `qwen3.5-9b`를 컨텍스트 32768로 다시 로드해 둔다.
- 커밋 트레일러: 모든 커밋 메시지 끝에 `Agent: <claude-code|codex|antigravity>` 줄을 단다(실제로 작업한 도구).

## 5. 인계 규칙

세 도구는 같은 PC의 같은 폴더에서 번갈아 작업한다. 모든 맥락은 저장소 안 파일로 남긴다.

1. 세션 시작: `AGENTS.md` → `docs/handoff/STATUS.md` → STATUS가 가리키는 계획 파일 순서로 읽고, `git status`·`git log -5`로 실제 상태가 STATUS와 맞는지 확인한다. 어긋나면 코드가 정답이고 STATUS를 먼저 고친다.
2. 작업 단위: 계획의 단계 하나씩. 단계가 끝나면 테스트 → 계획 체크박스 표시 → STATUS 갱신 → **한 커밋**으로 묶는다.
3. 커밋 메시지 끝에 `Agent: claude-code | codex | antigravity` 트레일러를 단다(어떤 도구가 한 일인지 나중에 알 수 있게).
4. 토큰이 떨어져 가면 **인계 커밋**을 먼저 한다: 단계 중간이면 `wip:` 접두어로 커밋하고, STATUS의 "다음 할 일"에 정확한 위치(파일, 함수, 남은 작업)를 적는다. 커밋하지 않은 변경을 남긴 채 도구를 바꾸지 않는다.
5. 계획에 없는 결정을 했으면 `docs/decisions/`에 남긴다. 사용자에게 물어야 할 것은 STATUS의 "결정 필요"에 적고 멈춘다.
6. 브랜치: 비공개 단독 프로젝트이므로 `main`에서 바로 작업한다(도구를 바꿀 때 브랜치 혼동을 없애려고). 큰 실험만 별도 브랜치.
7. 원본 수정 가져오기: `git fetch upstream` → `git cherry-pick <커밋>`. 충돌 해결 기록은 LOG에 남긴다.

추가: 사람이 직접 화면에서 확인해야 하는 항목은 `docs/handoff/STATUS.md`의 "수동 확인 목록"에 덧붙인다. `docs/handoff/LOG.md`는 맨 아래에 추가만 한다.

## 6. 원본 수정 가져오기 절차

`upstream`은 원본(K-EXAONE Desktop) 저장소이며 push 주소는 `DISABLED`다(실수로 올리지 않게).

```
git fetch upstream
git log --oneline main..upstream/main      # 가져올 후보 확인
git cherry-pick <hash>                     # 필요한 커밋만 하나씩
```

- 충돌이 나면 이 앱의 LM Studio 전용 동작을 우선해 해결하고, 어떻게 풀었는지 `docs/handoff/LOG.md`에 남긴다.
- 가져온 뒤에는 3절의 세 명령을 모두 통과시킨다. 커밋 트레일러(`Agent: ...`)를 잊지 않는다.
