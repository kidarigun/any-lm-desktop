# Any LM Desktop

LM Studio에 올린 로컬 모델로 보고서·문서를 쓰고, 내 PC의 프로젝트 폴더와 코드를 다루는 Windows 데스크톱 앱입니다. (Tauri v2 + React/TypeScript + Rust)

LM Studio는 채팅만 지원하므로, 이 앱이 문서 작성·캔버스·프로젝트 폴더 작업·코드 작업(에이전트 루프, HWPX/DOCX/PPTX/XLSX, Skill, MCP, 실행 환경 관리, 컨텍스트 압축)을 맡습니다.

> **비공개 프로젝트입니다.** K-EXAONE Desktop 0.4.4(Apache-2.0)에서 갈라져 나왔습니다. 출처와 상표 안내는 [NOTICE](NOTICE)를 보세요.
> LG AI연구원이나 LG가 만들거나 보증·지원하는 프로젝트가 아닙니다. "EXAONE"과 "K-EXAONE"은 LG AI연구원의 상표입니다.

## 주요 기능

- **채팅 · 프로젝트 · 코드** 세 화면. 여러 대화를 동시에 백그라운드로 진행
- **LM Studio 연결**: 서버 켜기, 모델 로드·다시 로드·언로드, 대화별 모델 선택, 모델별 컨텍스트 길이, "생각하기" 켜기/끄기
- **컨텍스트 관리**: 로드된 컨텍스트에 맞춘 도구 결과 상한, 70%가 차면 이전 대화를 자동 요약 압축, `/compact`로 수동 압축
- **프로젝트 폴더 도구**: 파일 읽기·쓰기·부분 수정·검색, 삭제와 명령 실행은 승인 후 실행, 모든 변경은 되돌리기 가능
- **문서 캔버스**: Markdown·Word(.docx) 편집, 한글(.hwpx) 보며 고치기, Excel·PowerPoint·PDF 미리보기, 다른 형식으로 저장
- **문서 직접 생성**: Word·HWPX·PowerPoint·Excel 파일을 앱이 만들고, 복잡한 파일은 Python 스크립트로 생성
- **Skill · MCP**: Agent Skills 형식 지침과 로컬(stdio)·원격(Streamable HTTP) MCP 서버, 도구별 승인 설정
- **실행 환경 관리**: Python·Node.js·uv가 없는 PC에서도 앱 전용으로 설치
- **프로젝트 메모리 · 지침 · 지식 폴더**: 모델과 함께 쓰는 메모리, 전용 지침 편집기, 참고 자료 폴더 색인
- **터미널**: 오른쪽 작업 패널에서 PowerShell을 바로 쓰는 진짜 터미널(색상·진행 표시·Ctrl+C·여러 탭)

### 웹 검색 MCP (내장)

[찾아보기] → MCP → "웹 검색"에 서비스 제공사의 **공식** MCP 서버 8개가 앱에 들어 있습니다(목록을 받지 못해도 보입니다). 연결하면 모델이 최신 뉴스·가격·버전을 웹에서 찾아 출처 주소와 함께 답합니다.

- **키 없이 바로 연결**: Exa, Firecrawl, Jina AI (사용량 제한이 있고, Jina는 `search_web` 등 일부 도구에 키가 필요). 키를 넣으면 한도가 늘어납니다.
- **API 키가 필요**: Tavily, Brave 검색(Node.js 필요, `npx`로 실행), Perplexity, Kagi, Linkup.
- 키는 요청 헤더(Brave는 환경 변수)로만 보내며 Windows 자격 증명 관리자에 저장됩니다. 주소에는 넣지 않습니다. 호출하면 제공사의 크레딧이 쓰일 수 있습니다.
- 읽기 전용 검색 도구는 승인 없이 실행하고, 비용이 큰 조사·크롤링 도구는 매번 묻습니다.

### 지식 폴더

프로젝트 화면의 "지식 폴더" 카드에서 [폴더 추가]로 참고 자료 폴더(프로젝트 폴더 밖이어도 됨)를 연결하면, 앱이 글을 1,200자 조각(200자 겹침)으로 나눠 색인합니다(BM25, 한글은 2글자 묶음). 질문할 때마다 관련 조각을 찾아 시스템 프롬프트 뒤 `[참고 자료]`에 출처 경로와 함께 넣고(로드된 컨텍스트의 15%, 최대 12,000자·6조각), 모델은 `search_knowledge` 도구로 더 찾을 수 있습니다.

- 대상: 텍스트(.md .txt .csv .json .yaml .html 등, 소스 코드)와 문서(.docx .hwpx .pptx .xlsx .pdf). `.git`·`node_modules`·`target`·`dist`·`build`·가상환경 폴더는 건너뜀. 파일 3,000개, 텍스트 5MB, 문서 30MB까지.
- 색인은 앱 데이터 폴더 `knowledge/<프로젝트 id>.json`에 둡니다. **앱은 지식 폴더를 읽기만 하고 아무것도 쓰지 않습니다.**
- 질문할 때 바뀐 파일만 빠르게 다시 읽습니다(5초 넘으면 다음 질문 때 이어서). [다시 색인]은 모든 파일을 다시 읽습니다.

### 터미널 (작업 패널)

상단 바 오른쪽의 터미널 아이콘이나 `` Ctrl+` ``로 오른쪽 작업 패널을 열고 닫습니다. 패널 안은 Windows ConPTY 위의 진짜 터미널이라 PowerShell에 직접 입력하고, 색상·진행 표시·대화형 프로그램·Ctrl+C가 그대로 동작합니다.

- 작업 폴더: 프로젝트·코드 화면에서는 프로젝트 폴더, 일반 채팅에서는 사용자 홈. 앱이 설치한 Node.js·uv가 PATH 앞에 붙습니다.
- 셸: 기본 Windows PowerShell. 패널 위 선택 상자에서 명령 프롬프트, (설치돼 있으면) PowerShell 7을 고를 수 있습니다(새로 여는 터미널부터).
- 탭은 최대 8개. [+ 새 터미널]로 더 열고 ×로 닫습니다. 셸 말고 실행 중인 프로그램이 있으면 닫기 전에 묻습니다.
- 패널을 닫아도 터미널은 계속 돌고, 다시 열면 최근 출력(세션마다 256KB)으로 화면을 복원합니다. 셸이 끝나면 탭에 "종료됨 (코드 n)"과 [다시 시작]이 보입니다.
- 선택한 글자는 Ctrl+C로 복사(선택이 없으면 인터럽트), Ctrl+V로 붙여넣기. 출력 속 링크는 기본 브라우저로 엽니다.
- 탭을 닫거나 앱을 끄면 그 터미널에서 띄운 프로세스(자식·손자까지)를 모두 끝냅니다. 자세한 방식은 `docs/decisions/0008-pty-terminal.md`.
- 프로젝트 화면의 작업 패널에서 [서버 실행]을 누르면 Node, Django, Streamlit, 정적 웹사이트의 실행 명령을 찾습니다. [직접 입력]으로 프로젝트 기준 폴더와 명령을 저장할 수 있습니다.
- 시작 출력에서 로컬 주소를 찾고 포트가 열리면 미리보기를 띄웁니다. [미리보기 자동 열기]를 끌 수 있고, 주소를 클릭하거나 [브라우저로 열기]를 사용할 수 있습니다. 브라우저 프레임을 허용하지 않는 사이트는 기본 브라우저에서 확인하세요.
- 패널 폭은 끌어서 바꾸고(320px ~ 창의 70%, 채팅창은 400px 이상 남김) 다음 실행에도 유지됩니다. 문서 캔버스와 함께 열면 캔버스 오른쪽에 붙고, 채팅창·캔버스·패널이 다 들어가지 않는 창(1100px 미만, 또는 사이드바를 뺀 폭이 1080px 미만)에서는 나중에 연 것만 보입니다.

## 필요한 것

- Windows 10 / 11 (64비트)
- [LM Studio](https://lmstudio.ai) 0.4 이상. 서버를 켜 두어야 합니다(앱의 [LM Studio 연결] 창에서 시작할 수도 있습니다). 기본 주소는 `http://localhost:1234`이며 이 PC(localhost, 127.0.0.1, [::1])만 허용합니다. `http://localhost:1234/v1`처럼 경로를 붙여도 주소와 포트만 씁니다.
- 모델이 하나 이상 내려받아져 있어야 합니다. 개발 기준 모델은 Qwen3.5-9B, Qwen3.6-35B-A3B입니다.

## 개발

필요한 것: Node.js 20 이상, Rust stable (MSVC 툴체인, 또는 GNU 툴체인 + MinGW 빌드 도구)

```bash
npm install
npm run tauri dev                  # 앱 전체 개발 실행 (npm run dev는 웹뷰만)
npm test                           # 프런트엔드 테스트
npx tsc --noEmit                   # 타입 검사
cd src-tauri && cargo test --lib   # Rust 테스트
```

### LM Studio로 실측하기 (harness)

`harness/`의 시나리오 파일을 사용자 삼아, 실제 LM Studio 서버로 앱의 에이전트 루프를 그대로 돌려 봅니다. 서버와 모델이 켜져 있을 때만 쓰세요. 하니스는 모델을 로드하거나 내리지 않으므로 LM Studio에서 모델을 미리 로드해 둡니다. 앱 설정이나 자격 증명 관리자는 읽지 않습니다.

```powershell
python harness/make_scenarios.py          # 시나리오 JSON 만들기 (harness/scenarios.json)
$env:ANYLM_MODEL = "qwen3.5-9b"
cd src-tauri
cargo test --lib harness::live -- --ignored --nocapture
python ../harness/report.py ../harness/runs/default   # 결과 요약
```

| 환경 변수 | 뜻 | 기본값 |
|---|---|---|
| `ANYLM_MODEL` | 시험할 모델 key (필수) | - |
| `ANYLM_SERVER` | LM Studio 서버 주소 | `http://localhost:1234` |
| `ANYLM_CONTEXT` | 모델이 로드된 컨텍스트 길이 | `32768` |
| `ANYLM_TOKEN` | LM Studio API 토큰 (선택) | 없음 |
| `ANYLM_SCENARIOS` | 시나리오 JSON | `../harness/scenarios.json` |
| `ANYLM_OUT` | 결과 폴더 | `../harness/runs/default` |
| `ANYLM_ONLY` | 실행할 시나리오 id (쉼표로 여러 개) | 전부 |
| `ANYLM_PAUSE_SEC` | 시나리오 사이 쉬는 시간(초) | `1` |
| `ANYLM_RERUN` | `1`이면 결과가 있는 시나리오도 다시 실행 | `0` |

결과(`harness/runs/`)와 `harness/scenarios*.json`은 Git에 올라가지 않습니다. 가볍게 연결만 확인하려면 `ANYLM_SCENARIOS=../harness/smoke.json`을 쓰세요. Git에 올린 시나리오: `smoke.json`(연결 확인), `knowledge.json`(지식 폴더), `search.json`(내장 웹 검색 MCP — Exa, 키 없음).

시나리오 항목 중 MCP·Skill 관련:

| 항목 | 뜻 |
|---|---|
| `mcpServers` | 연결할 MCP 서버 목록 `[{ "name", "url"?, "headers"?, "command"?, "args"?, "env"?, "catalogId"?, "autoApprove"? }]`. `url`이 있으면 원격, 없으면 `command`를 실행하는 로컬 서버. 앱과 같은 MCP 관리자로 연결해 `[MCP]` 절·도구 이름(`mcp__<서버>__<도구>`)이 앱과 같고, 시나리오가 끝나면 지웁니다. 헤더·환경 변수 값은 메모리에만 둡니다. `catalogId`가 내장 검색 항목(예: `"exa"`)이면 웹 검색 안내 줄이 붙습니다. `autoApprove`(서버의 원래 도구 이름)에 없는 MCP 도구는 앱이라면 승인을 물었을 도구로 기록하고 하니스가 승인합니다. |
| `skillAutoLoad` | `false`면 Skill 자동 불러오기를 끕니다(기본: 앱과 같이 켜짐). 자동으로 불러온 Skill은 결과 요약의 `autoSkills`에 남습니다. |
| 검사 `toolCalledContains` | 부른 도구 이름에 `text`가 들어 있으면 통과(MCP 도구 이름은 접두어가 붙으므로). `toolCalled`·`toolNotCalled`와 마찬가지로 앱이 미리 넣은 Skill 카드는 세지 않습니다. |
| 검사 `skillAutoLoaded` | 앱이 `text` 이름의 Skill을 자동으로 미리 넣었으면 통과. |
| `{today}`, `*` | 검사의 `path`·`text` 안 `{today}`는 시나리오를 시작한 날짜(YYYY-MM-DD)로 바뀝니다(예: `reports/주간보고_{today}.md`). `path`의 파일 이름에 `*`가 있으면 맞는 파일 중 가장 최근에 바뀐 하나를 봅니다(예: `reports/주간보고_*.md`). |

예: `$env:ANYLM_SCENARIOS = "../harness/search.json"`

## 데이터와 보안

- 대화·설정은 `%APPDATA%\io.github.kidarigun.anylm-desktop`, 실행 환경은 `%LOCALAPPDATA%\io.github.kidarigun.anylm-desktop\runtimes`에 저장됩니다.
- 앱이 이 PC에서 여는 포트: 중복 실행 방지 `45790`, 원격 MCP OAuth 로그인 결과 `45791`(`http://localhost:45791/oauth2callback`). 원본 K-EXAONE Desktop(45788·45789)과 겹치지 않습니다.
- LM Studio API 토큰(선택), MCP 서버의 비밀 값, OAuth 토큰은 설정 파일이 아니라 **Windows 자격 증명 관리자**에 저장됩니다.
- 모델이 파일을 지우거나 명령·스크립트를 실행하거나 MCP 도구를 쓸 때는 기본적으로 사용자 승인을 받습니다.
- 앱이 직접 접속하는 곳: 설정한 LM Studio 서버(이 PC), 사용자가 추가한 MCP 서버, 찾아보기 카탈로그, 실행 환경 공식 배포처(github.com/astral-sh/uv, nodejs.org, PyPI). 앱 자체의 자동 업데이트 확인은 없습니다.

## 여러 도구로 작업하기 (인계 체계)

Claude Code, Codex, Antigravity가 같은 폴더에서 번갈아 작업합니다. 공통 지침은 [AGENTS.md](AGENTS.md), 현재 상태와 다음 할 일은 `docs/handoff/STATUS.md`, 작업 기록은 `docs/handoff/LOG.md`, 계획에 없던 결정은 `docs/decisions/`에 있습니다. 세션을 시작할 때 이 순서로 읽으세요.

## 라이선스

[Apache License 2.0](LICENSE) — 함께 쓰는 외부 라이브러리는 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md), 출처와 상표 관련 안내는 [NOTICE](NOTICE)를 보세요.
