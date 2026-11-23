# Any LM Desktop 설계 문서

- 작성일: 2026-11-23
- 상태: 초안 (승인 전)
- 원본: K-EXAONE Desktop (`C:\DOCS\EXAONE`, 브랜치 `feature/agent-loop-and-document-canvas`, 커밋 `e885b02`, 0.4.4)
- 새 프로젝트 위치: `C:\DOCS\any-lm-desktop` (승인 뒤 이 문서는 새 저장소의 `docs/superpowers/specs/`로 옮긴다)

## 1. 목적과 이유

사용자는 이 PC(RTX 5060 Laptop 8GB, RAM 32GB)에 LM Studio를 설치하고 로컬 모델(Qwen3.5-9B, Qwen3.6-35B-A3B)을 쓰고 있다. LM Studio 자체는 채팅만 지원하므로, 문서 작성·캔버스·프로젝트 단위 작업·코드 작업을 하려면 별도의 작업 앱이 필요하다.

사용자가 직접 만든 K-EXAONE Desktop(Tauri v2 + React + Rust)에 이미 그 기능(에이전트 루프, 프로젝트 폴더 도구, 문서 캔버스, HWPX/DOCX/PPTX/XLSX, Skill, MCP, 실행 환경 관리, 컨텍스트 압축)이 있다. 이 소스를 바탕으로 **LM Studio 전용 앱 "Any LM Desktop"** 을 별도 프로젝트로 만든다.

또한 사용자는 개발 도구(Claude Code, Codex, Antigravity)의 남은 토큰량에 맞춰 도구를 번갈아 쓸 것이므로, **어느 도구가 이어받아도 같은 맥락에서 작업을 이어갈 수 있는 인계 체계**를 저장소 안에 둔다.

## 2. 목표 · 비목표 · 반목표

**목표**
1. 원본과 동시에 설치·실행해도 충돌하지 않는 별도 앱(이름, 앱 ID, 데이터 폴더, 자격 증명, 포트 모두 분리).
2. LM Studio 로컬 서버에 연결해 설치된 모델 목록을 보여 주고, 대화마다 모델을 골라 바꿔 쓸 수 있다.
3. 선택한 모델을 앱이 원하는 컨텍스트 길이로 로드하고, 실제 로드된 컨텍스트에 맞춰 자동 압축한다.
4. 원본의 작업 기능(프로젝트 도구, 캔버스, 문서 생성, Skill, MCP, 실행 환경)은 그대로 동작한다.
5. K-EXAONE 전용 요소(API 키 만료, 하루 호출 한도, low/high/max 추론 강도, 자동 업데이트)를 걷어 낸다.
6. Claude Code · Codex · Antigravity 간 인계 체계(공통 지침, 현재 상태 문서, 계획 체크리스트, 커밋 규칙)를 갖춘다.

**비목표 (이번 범위 아님)**
- LM Studio 이외의 공급자(K-EXAONE, OpenAI, Ollama 등) 지원. 사용자가 "LM Studio 전용"으로 정했다.
- 다른 PC의 LM Studio(LAN, LM Link) 연결. 같은 PC(`localhost`)만 지원한다.
- 공개 배포: 설치 파일 서명, 자동 업데이트, 공개 README, 릴리스 절차. 사용자가 "비공개 프로젝트"로 정했다.
- 원본 K-EXAONE Desktop 저장소의 수정.

**반목표 (기술적으로 돌아가도 실패로 보는 것)**
- 새 앱이 원본 앱의 대화·설정·API 키를 읽거나 옮겨 오거나 덮어쓴다.
- 원본 저장소(`C:\DOCS\EXAONE`)나 그 GitHub 저장소에 커밋·푸시가 섞여 들어간다.
- 내부 파일·함수 이름을 대거 바꿔서, 원본에서 고친 버그를 `cherry-pick`으로 가져올 수 없게 된다.
- 인계 문서가 실제 코드 상태와 어긋나서, 다음 도구가 이미 끝난 일을 다시 하거나 깨진 상태에서 시작한다.

## 3. 제약

- Windows 11, Node.js(설치됨), Rust stable(설치됨), gh CLI(설치됨). 빌드 명령은 원본과 같다(`npm install`, `npm run tauri dev`, `npm test`, `cd src-tauri && cargo test --lib`).
- GPU VRAM 8GB. 9B 모델을 32K 컨텍스트로 로드하면 VRAM이 거의 찬다(7.7/8.1GB). 35B-A3B는 전문가 가중치를 CPU로 보내야 한다. 앱은 큰 컨텍스트를 무조건 요구하면 안 된다.
- LM Studio 0.4.28 확인 사항 (2026-11-23 실측):
  - `GET /v1/models` (OpenAI 호환) → 모델 ID 목록.
  - `GET /api/v1/models` (LM Studio 고유) → 모델별 `key`, `display_name`, `params_string`, `quantization`, `size_bytes`, `max_context_length`, `loaded_instances[].config.context_length`, `capabilities.vision`, `capabilities.trained_for_tool_use`, `capabilities.reasoning.allowed_options`(Qwen3.5: `["off","on"]`).
  - `POST /api/v1/models/load`, `POST /api/v1/models/unload` 존재(빈 본문에 400). 정확한 요청 형식은 LM Studio 문서(https://lmstudio.ai/docs/developer/rest)로 확인해야 한다.
  - `POST /v1/chat/completions`에 원본 앱과 같은 형태(stream, stream_options.include_usage, tools, tool_choice, reasoning_effort)로 보내면 Qwen3.5-9B가 정상적으로 `tool_calls`를 스트리밍하고 마지막에 `usage`(prompt_tokens, reasoning_tokens 포함)를 준다.
  - 요청한 모델이 로드돼 있지 않으면 LM Studio가 자동 로드(JIT)하는데, 이때 컨텍스트가 기본값(8192)이라 앱의 시스템 프롬프트·도구 정의만으로도 빠듯하다.
- 원본 라이선스는 Apache-2.0. 출처 표기(LICENSE, NOTICE)를 유지한다. "EXAONE"은 LG AI연구원의 상표이므로 새 앱 이름·화면에 쓰지 않는다(출처 설명에서만 언급).

## 4. 접근법

| | 방법 | 장점 | 단점 |
|---|---|---|---|
| **A (선택)** | 원본 git 저장소를 이력째 복제하고, 원본을 `upstream` 원격으로 연결 | 원본의 공통 버그 수정을 `git cherry-pick`으로 가져올 수 있다. 다음 도구가 커밋 이력으로 코드의 이유를 알 수 있다 | 이력에 EXAONE 기록이 남는다(비공개라 문제없음) |
| B | 현재 소스만 복사해 커밋 하나로 새 저장소 | 깨끗한 시작 | 원본 수정을 옮길 때마다 수작업 비교. 두 앱이 코드 대부분을 공유하므로 부담이 계속 쌓인다 |
| C (제외) | 원본 저장소 하나에서 설정으로 두 앱을 나눔 | 코드 한 벌 | "별도 프로젝트"라는 요청과 맞지 않고 구조가 복잡해진다 |

사용자가 **A**를 골랐다.

## 5. 사용자가 정한 것

- LM Studio 전용 (다른 공급자 없음).
- 비공개 프로젝트 (공개 배포 없음).
- 앱 이름: **Any LM Desktop**.
- 이력을 유지한 복제(접근법 A).
- 개발 도구: Claude Code, Codex, Antigravity를 번갈아 사용. 이들 사이의 작업 인계를 매끄럽게 한다.
- 컨텍스트가 70% 차면 요약 압축을 자동으로 한다(6.3).

## 5-1. 설계자(Claude)가 정한 것 — 사용자 확인 대상

- 폴더 `C:\DOCS\any-lm-desktop`(공백 없는 경로), GitHub 비공개 저장소 `kidarigun/any-lm-desktop`.
- 내부 파일·함수 이름은 유지하고 사용자에게 보이는 것만 바꾼다(원본 수정 가져오기 쉽게).
- `main`에서 바로 작업, 인계 규칙과 `Agent:` 커밋 트레일러.
- 서버 주소는 `localhost`만, API 토큰은 선택 사항.
- 같은 모델이 작게 로드돼 있으면 그대로 쓰고 [다시 로드] 버튼, 다른 모델은 [내리고 로드] 버튼으로만 내린다.
- GPU 배분(전문가 가중치 CPU 보내기)은 LM Studio의 모델별 저장값을 따른다.
- 기본 컨텍스트 32768, 찾아보기 카탈로그는 원본 것을 계속 사용.
- 앱에 [서버 시작] 버튼(`lms server start`).

## 6. 설계

### 6.1 저장소 만들기

1. `git clone C:\DOCS\EXAONE C:\DOCS\any-lm-desktop` 후 `feature/agent-loop-and-document-canvas`(e885b02)를 기준으로 `main` 브랜치를 만든다. 원본 작업 폴더는 파일이 지워진 상태지만 git 객체는 온전하므로 복제에는 영향이 없다. 원본 폴더는 읽기만 한다.
2. 원격: `origin`을 지우고, `upstream` = `https://github.com/kidarigun/k-exaone-desktop.git`(가져오기 전용, 푸시 URL은 `DISABLED` 같은 값으로 막는다), `origin` = 새 비공개 GitHub 저장소 `kidarigun/any-lm-desktop`(`gh repo create --private`). GitHub 저장소 생성과 첫 푸시는 사용자 확인 뒤에 한다.
3. 첫 커밋들은 "정체성 변경"과 "인계 체계"로 나눠, 이후 기능 작업과 섞이지 않게 한다.

### 6.2 정체성 분리 (원본과 충돌 방지)

| 항목 | 원본 | 새 값 |
|---|---|---|
| productName / 창 제목 | Desktop App for LG K-EXAONE | Any LM Desktop |
| Tauri identifier (→ `%APPDATA%` 폴더) | `io.github.kidarigun.kexaone-desktop` | `io.github.kidarigun.anylm-desktop` |
| package.json name | `desktop-app-for-lg-k-exaone` | `any-lm-desktop` |
| 버전 | 0.4.4 | 0.1.0 (네 곳: package.json, package-lock.json, Cargo.toml, tauri.conf.json) |
| Cargo 패키지 / lib 이름 | `exaone-desktop` / `exaone_desktop_lib` | `any-lm-desktop` / `any_lm_desktop_lib` |
| 자격 증명 서비스명 (`secrets.rs`, MCP 비밀 값) | `ai.exaone.desktop` | `io.github.kidarigun.anylm-desktop` |
| 중복 실행 방지 포트·인사말 (`single_instance.rs`) | 45788, `EXAONE-DESKTOP-SHOW` | 다른 고정 포트(예: 45789), `ANYLM-DESKTOP-SHOW` |
| HTML 미리보기 호스트 (`html_preview.rs`, CSP `frame-src`) | `exaone-preview.localhost` | `anylm-preview.localhost` |
| 대화상자 환경 변수 | `EXAONE_DLG_*` | `ANYLM_DLG_*` |
| localStorage 키 | `exaone_*` | `anylm_*` |
| 기본 작업 폴더 | `문서\EXAONE` | `문서\AnyLM` |
| 옛 데이터 폴더 이전 (`migrate_old_data_dir`, `ai.exaone.desktop`) | 있음 | **삭제** (반목표: 원본 데이터를 가져오지 않는다) |
| 자동 업데이트 | tauri-plugin-updater, UpdatePanel, `createUpdaterArtifacts`, `scripts/make_latest_json.py` | **전부 제거** |
| 설치 파일 설명·저작권 문구 | K-EXAONE 비공식 앱 | Any LM Desktop, 출처 문구 |
| NOTICE | LG 상표 안내 | "K-EXAONE Desktop(Apache-2.0)을 바탕으로 만들었다"는 출처 + 상표 안내 유지 |
| README / CHANGELOG | 원본 내용 | 새로 쓴다. CHANGELOG는 0.1.0부터, 원본 0.4.4에서 갈라졌다고 첫 줄에 적는다 |
| 아이콘 | 원본 아이콘 | v1은 유지해도 된다(빌더 판단). 원본과 구분하려면 색만 바꾼 아이콘 |

**내부 이름 유지 규칙**: `exaone.rs`, `run_exaone_turn`, `onExaoneStream`, `STREAM_EVENT = "exaone-stream"` 같은 **내부 파일·함수·이벤트 이름은 그대로 둔다.** 사용자에게 보이는 문자열과 위 표의 충돌 요소만 바꾼다. 이유: 원본 수정을 `cherry-pick`할 때 충돌을 줄이기 위해서(반목표 3). 대규모 이름 정리는 나중에 따로 결정한다.

### 6.3 LM Studio 연결

**설정 (`AppSettings`)**
- `base_url` 대신 **서버 주소** 하나: 기본 `http://localhost:1234`. 앱이 `/v1/chat/completions`, `/api/v1/models` 등을 붙여 쓴다. 기존 사용자 데이터가 없으므로 이전 형식 호환은 필요 없다.
- 주소 검사: `http://localhost`, `http://127.0.0.1`, `http://[::1]`만 허용(비목표: 원격 연결).
- API 키 → **선택 사항인 "LM Studio API 토큰"**. LM Studio의 "인증 요구"를 켠 경우에만 필요하다. 비어 있으면 `Authorization` 헤더를 보내지 않는다. 저장은 원본처럼 Windows 자격 증명 관리자.
- `model` → **기본 모델 키**(LM Studio `key`). 대화마다 바꿀 수 있다(아래).
- `reasoning_effort`(low/high/max) → 제거. 대신 모델별 **추론 켜기/끄기**(아래).
- `writing_style`은 유지.
- 모델별 설정 저장: `model_prefs: { [modelKey]: { context_length, reasoning: "on"|"off"|null } }`.

**모델 목록과 선택**
- `GET /api/v1/models`에서 `type == "llm"`만 쓴다. 화면에 표시: 표시 이름, 파라미터 수, 양자화, 파일 크기, 로드 여부와 로드된 컨텍스트, 이미지 입력·도구 사용·추론 지원 아이콘.
- 대화 화면 상단(원본의 추론 강도 선택기 자리)에 **모델 선택 드롭다운**을 둔다. 고른 모델은 그 대화에 저장된다(`Conversation.model_key`). 새 대화는 기본 모델로 시작한다.
- 대화 중 모델을 바꾸면 다음 요청부터 새 모델로 보낸다. 대화 기록은 OpenAI 형식 메시지라 그대로 이어진다. 메시지마다 어떤 모델이 답했는지 저장하고 답변 아래에 작게 표시한다.
- `trained_for_tool_use == false`인 모델을 프로젝트 모드에서 고르면 "이 모델은 도구 사용 학습이 안 돼 있어 파일 작업이 불안정할 수 있습니다" 경고를 보여 준다(막지는 않는다).

**로드와 컨텍스트**
- 요청 전에 선택한 모델이 원하는 컨텍스트로 로드돼 있는지 확인한다. 안 돼 있으면 `POST /api/v1/models/load`로 `context_length`를 지정해 로드하고, 로드 진행을 화면에 표시한다(35B는 1분 가까이 걸릴 수 있으므로 로드 요청 시간 제한은 5분).
- 원하는 컨텍스트 기본값: **32768**. 모델별로 바꿀 수 있다(최대 `max_context_length`). VRAM 부족으로 로드가 실패하면 LM Studio의 오류 문구를 그대로 보여 주고, 더 작은 값을 제안한다.
- **같은 모델이 다른 컨텍스트로 이미 로드돼 있을 때** (제 판단): 로드된 인스턴스의 컨텍스트가 원하는 값 **이상**이면 그대로 쓰고, 70% 기준도 그 **실제 값**으로 계산한다. 더 작으면(예: JIT로 8192에 로드됨) 쓰기는 하되 대화 상단에 "이 모델은 8K 컨텍스트로 로드돼 있습니다 · [32K로 다시 로드]" 안내를 띄운다. 다시 로드는 버튼을 눌렀을 때만 한다(사용자가 VRAM 때문에 일부러 작게 로드했을 수 있으므로). 같은 모델의 두 번째 인스턴스는 만들지 않는다.
- **다른 모델이 로드돼 있을 때** (제 판단): 앱이 알아서 언로드하지 않는다. 다만 모델을 바꿀 때 다른 LLM이 로드돼 있으면 "지금 로드된 {X}를 내리고 {Y}를 로드할까요? [내리고 로드] [그대로 로드 시도]" 선택을 보여 준다. VRAM 부족으로 로드가 실패한 경우에도 같은 [내리고 로드] 버튼을 오류 옆에 둔다. 8GB GPU에서는 9B↔35B 전환에 이 버튼이 사실상 필요하다. 설정 화면에도 "로드된 모델 언로드" 버튼을 둔다.
- **로드 옵션** (제 판단): 앱은 `context_length`만 보낸다. 35B-A3B의 "전문가 가중치를 CPU로" 같은 GPU 배분 설정은 LM Studio 화면에서 모델별로 저장해 둔 값에 맡긴다. 로드 API가 그 값을 덮어쓰지 않는지 빌더가 실측으로 확인하고, 덮어쓴다면 해당 옵션을 그대로 넘기는 방법을 결정 기록에 남긴다.

**컨텍스트 압축 기준**
- 원본은 `COMPACT_TRIGGER_CHARS = 120_000` 글자 고정이다. 새 앱은 **실제 로드된 `context_length`** 를 기준으로 한다.
- 원본에는 두 장치가 있다. (1) **자동 줄이기**: `compact_messages` 1~3단계. 요청에 실을 때만 오래된 도구 결과·메시지를 잘라 내고 저장하지 않으며 모델을 부르지 않는다. (2) **요약 압축**: `compact_conversation`. AI 요약 또는 빠른 로컬 요약을 `Conversation.compaction`에 저장하고, 대화 안의 압축 카드로 보기·고치기·해제가 된다. 원본에서는 버튼이나 `/compact`로만 실행된다.
- 현재 사용량: 직전 응답의 `usage.prompt_tokens`가 있으면 그것으로, 없으면 글자 수 추정(한국어 기준 1토큰 ≈ 1.5자, 보수적으로)으로 계산한다.
- **자동 요약 압축 (사용자 결정)**: 사용자가 메시지를 보낼 때(답변과 답변 사이) 현재 사용량이 **로드된 컨텍스트의 70%** 를 넘으면, 요청을 보내기 전에 기존 `compact_conversation`을 자동으로 실행한다.
  - 요약은 그 대화에서 지금 쓰는 모델로 한다(AI 요약). 요약 호출이 실패하면 원본의 "빠른 로컬 압축"으로 대신한다.
  - 최근 메시지는 원본 수동 압축의 기본값만큼 그대로 남긴다.
  - 화면에는 "컨텍스트가 70% 찼습니다. 이전 대화를 요약하는 중…" 진행 표시를 띄우고, 끝나면 원본과 같은 압축 카드가 대화에 남는다. 사용자는 그 카드에서 요약을 보고 고치거나 해제할 수 있다.
  - 이미 압축한 뒤에도 다시 70%를 넘으면 또 압축한다(이전 요약 + 그 뒤 대화를 합쳐 새 요약).
  - **에이전트가 도구를 쓰며 작업하는 도중(한 턴 안)에는 요약하지 않는다.** 그때는 자동 줄이기(기준도 로드된 컨텍스트 기반으로 바꾼다)와 원본의 3단계 긴급 압축 재시도만 쓴다.
  - 설정에 "자동 요약 압축" 켜기/끄기를 두고, 기본은 켜짐이다.
- `looks_like_context_overflow`에 LM Studio의 컨텍스트 초과 오류 문구를 추가한다. 실제 문구는 작은 컨텍스트(예: 4096)로 로드해 일부러 넘겨서 확인하고 테스트에 넣는다.
- 원본의 `MAX_TOOL_OUTPUT_CHARS = 40_000`은 32K 컨텍스트에서 한 번에 너무 크다. 로드된 컨텍스트에 비례하게 줄인다(예: 컨텍스트 토큰 수 × 0.5자, 상한 40,000). 정확한 비율은 빌더 판단.

**추론 켜기/끄기**
- 모델의 `capabilities.reasoning.allowed_options`가 있으면 대화 상단에 "생각하기" 켜기/끄기를 보여 준다. 없으면 숨긴다.
- LM Studio에 추론을 끄는 요청 방식(예: `reasoning_effort`, `chat_template_kwargs.enable_thinking` 등)은 **빌더가 LM Studio 문서와 실측으로 확인**하고 정한다. 확인 결과를 `docs/decisions/`에 남긴다.
- 원본의 `preserve_thinking`(K-EXAONE 전용)은 보내지 않는다. 이전 답변의 추론 내용은 다음 요청에 다시 보내지 않는다.

**스트리밍·오류·시간 제한**
- 스트림 파싱(`reasoning_content`/`content`/`tool_calls`/`usage`)과 자동 재시도·이어 쓰기 로직은 원본을 그대로 쓴다.
- 연결 거부(서버 꺼짐) → "LM Studio 서버가 꺼져 있습니다" 안내와 **[서버 시작]** 버튼. 버튼은 `%USERPROFILE%\.lmstudio\bin\lms.exe server start`를 실행한다(경로가 없으면 LM Studio를 열라고 안내).
- K-EXAONE 전용 오류 문구(키 만료 30일, 하루 한도 소진, `result_code` 형식)는 LM Studio에 맞는 문구로 바꾸거나 지운다.
- 원본의 응답 헤더 대기 180초, 스트림 무응답 900초는 유지한다(로컬 모델은 긴 프롬프트 처리에 시간이 걸린다).

**화면에서 걷어 낼 것**
- `RateBadge`, `LowBudgetNotice`, API 키 경과일 경고, "하루 한도에서 빠집니다" 문구, `rate_info` 명령·파일.
- 답변 통계는 유지하되 "API 호출 n회" 대신 **입력/출력/추론 토큰과 초당 토큰 수**를 보여 준다.
- "API 및 모델 설정" 창 → "LM Studio 연결" 창: 서버 주소, 연결 상태(초록/빨강), 선택적 토큰, 모델 목록과 모델별 컨텍스트·추론 설정, 로드/언로드.
- 시스템 프롬프트의 "LG AI연구원의 K-EXAONE 2.5 기반 데스크톱 어시스턴트" → "Any LM Desktop의 어시스턴트. 지금 답하는 모델: {display_name}". 그 밖의 작업 지침은 유지한다.
- 압축 버튼 설명의 "EXAONE이 ... (1회 호출)" 같은 문구 → 모델 중립 문구.

**그대로 두는 것**
- 프로젝트 도구·승인·되돌리기, 문서 캔버스, 문서 생성, Python/Node/uv 실행 환경, Skill, MCP(OAuth 포함), 찾아보기 카탈로그(기본 주소는 원본과 같은 `kidarigun/exaone-catalog`를 계속 쓴다 — 내용이 모델과 무관한 Skill·MCP 목록이라서. 제 판단).
- K-EXAONE의 하루 한도를 아끼려고 만든 도구 구성은 v1에서는 그대로 둔다. 로컬에는 한도가 없으므로 나중에 다시 볼 수 있다.

**실측 테스트 장치 (`harness/`)**
- K-EXAONE API 대신 LM Studio로 돌게 바꾼다. 하루 한도·이용약관 경고는 지우고, 대상 모델 키를 환경 변수(`ANYLM_MODEL`)로 받는다. 로컬이라 횟수 제한 없이 시나리오를 반복할 수 있다.

### 6.4 도구 간 작업 인계 체계

세 도구는 같은 PC의 같은 폴더에서 작업하므로, 코드 전달이 아니라 **맥락 전달**이 문제다. 모든 맥락을 저장소 안 파일로 남긴다.

**파일 구성**

| 파일 | 역할 | 누가 읽나 |
|---|---|---|
| `AGENTS.md` | **유일한 공통 지침**: 프로젝트 개요, 폴더 지도, 빌드·테스트 명령, 코딩 규칙(내부 이름 유지, 한국어 UI 문구, 원본 저장소 건드리지 않기), 인계 규칙 | Codex, Antigravity가 직접 읽음 |
| `CLAUDE.md` | 한 줄: `@AGENTS.md` (Claude Code의 파일 불러오기 문법) | Claude Code |
| `GEMINI.md` | 한 줄: "이 저장소의 지침은 AGENTS.md에 있다. 먼저 읽고 따른다." | Antigravity(GEMINI.md를 읽는 경우 대비) |
| `docs/handoff/STATUS.md` | **현재 상태 한 장**: 지금 목표, 진행 중인 계획 파일과 단계, 마지막 빌드·테스트 결과, 다음 할 일(구체적으로), 막힌 것·사용자 결정이 필요한 것, 마지막 작업 도구와 시각 | 모든 도구가 세션 시작 시 |
| `docs/handoff/LOG.md` | 세션 기록(맨 아래에 추가만): 날짜, 도구/모델, 한 일, 커밋 해시 | 필요할 때 |
| `docs/handoff/START.md` | 사람이 각 도구에 붙여 넣을 시작 문구와 마무리 문구 | 사용자 |
| `docs/superpowers/plans/*.md` | 구현 계획. 단계마다 체크박스. 한 단계는 한 세션 안에 끝낼 수 있는 크기 | 작업 중인 도구 |
| `docs/decisions/NNNN-*.md` | 짧은 결정 기록(무엇을, 왜). 다음 도구가 같은 논의를 반복하지 않게 | 필요할 때 |

**인계 규칙 (AGENTS.md에 적는다)**
1. 세션 시작: `AGENTS.md` → `docs/handoff/STATUS.md` → STATUS가 가리키는 계획 파일 순서로 읽고, `git status`·`git log -5`로 실제 상태가 STATUS와 맞는지 확인한다. 어긋나면 코드가 정답이고 STATUS를 먼저 고친다.
2. 작업 단위: 계획의 단계 하나씩. 단계가 끝나면 테스트 → 계획 체크박스 표시 → STATUS 갱신 → **한 커밋**으로 묶는다.
3. 커밋 메시지 끝에 `Agent: claude-code | codex | antigravity` 트레일러를 단다(어떤 도구가 한 일인지 나중에 알 수 있게).
4. 토큰이 떨어져 가면 **인계 커밋**을 먼저 한다: 단계 중간이면 `wip:` 접두어로 커밋하고, STATUS의 "다음 할 일"에 정확한 위치(파일, 함수, 남은 작업)를 적는다. 커밋하지 않은 변경을 남긴 채 도구를 바꾸지 않는다.
5. 계획에 없는 결정을 했으면 `docs/decisions/`에 남긴다. 사용자에게 물어야 할 것은 STATUS의 "결정 필요"에 적고 멈춘다.
6. 브랜치: 비공개 단독 프로젝트이므로 `main`에서 바로 작업한다(제 판단: 도구를 바꿀 때 브랜치 혼동을 없애려고). 큰 실험만 별도 브랜치.
7. 원본 수정 가져오기: `git fetch upstream` → `git cherry-pick <커밋>`. 충돌 해결 기록은 LOG에 남긴다.

**검증 명령 (AGENTS.md에 고정)**
- `npm test` · `npx tsc --noEmit` · `cd src-tauri && cargo test --lib`
- LM Studio 실측: `harness/` 시나리오(LM Studio 서버와 모델이 켜져 있을 때만)

## 7. 완료 기준

1. `npm run tauri dev`로 Any LM Desktop이 뜨고, 원본 K-EXAONE Desktop과 **동시에** 실행된다. 서로의 대화·설정이 보이지 않는다.
2. LM Studio 연결 창에서 설치된 모델 목록(Qwen3.5-9B, Qwen3.6-35B-A3B 등)이 보이고, 연결 상태가 표시된다.
3. 대화에서 모델을 바꾸면 그 모델로 답하고, 답변 아래에 모델 이름이 표시된다. 8GB GPU에서 9B→35B로 바꿀 때 [내리고 로드]로 전환이 된다. (35B `qwen3.6-35b-a3b`는 2026-11-23 다운로드 완료, 22.07GB.)
4. 로드되지 않은 모델을 고르면 앱이 설정한 컨텍스트로 로드한 뒤 답한다.
5. 프로젝트 모드에서 Qwen3.5-9B가 파일을 만들고 수정하며, 되돌리기가 동작한다.
6. 32K로 로드한 모델과 긴 대화를 이어 가면, 70%를 넘은 뒤 다음 메시지를 보낼 때 자동 요약 압축이 일어나고 압축 카드가 남는다. 에이전트 작업 도중 컨텍스트가 넘쳐도 긴급 압축으로 이어 간다.
7. 화면 어디에도 K-EXAONE 전용 요소(호출 한도, 키 만료, low/high/max, 업데이트)가 남아 있지 않다.
8. 모든 테스트가 통과한다(원본 테스트 중 K-EXAONE 전용 테스트는 LM Studio 기준으로 고치거나 지운다).
9. 인계 체계 파일이 있고, 실제로 한 도구에서 다른 도구로 인계해 보는 연습을 한 번 한다(예: Claude Code가 한 단계를 끝내고 Codex가 STATUS만 보고 다음 단계를 시작).

## 8. 진행 순서 (계획에서 단계로 쪼갠다)

1. 저장소 만들기 + 인계 체계 파일 (가장 먼저 — 이후 모든 작업이 이 규칙을 따른다)
2. 정체성 분리 + 업데이트 제거 + 옛 데이터 이전 제거
3. LM Studio 연결 계층(설정, 모델 목록, 로드, 서버 상태)
4. 대화별 모델 선택과 추론 켜기/끄기
5. 컨텍스트 기준 줄이기, 자동 요약 압축, 오류 처리
6. K-EXAONE 전용 화면 걷어 내기와 문구 정리
7. harness를 LM Studio용으로 전환, README·CHANGELOG
8. 완료 기준 확인과 인계 연습

## 9. 빌더에게 맡기는 것

- 모델 선택 드롭다운과 LM Studio 연결 창의 세부 화면 배치(원본 디자인 언어를 따른다).
- `MAX_TOOL_OUTPUT_CHARS` 비례 계산의 정확한 비율, 글자→토큰 추정 계수.
- 추론 끄기 요청 방식(문서·실측 확인 후 결정 기록).
- LM Studio load/unload 요청 형식(문서 확인).
- 새 아이콘 여부.
- 원본 테스트 중 무엇을 고치고 무엇을 지울지(K-EXAONE 전용인 것만 지운다).
