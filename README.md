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

결과(`harness/runs/`)와 `harness/scenarios*.json`은 Git에 올라가지 않습니다. 가볍게 연결만 확인하려면 `ANYLM_SCENARIOS=../harness/smoke.json`을 쓰세요.

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
