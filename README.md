# Desktop App for LG K-EXAONE

> **비공식 프로젝트입니다.** 이 앱은 LG AI연구원이나 LG가 만들거나 보증·지원하지 않는 개인 오픈소스 프로젝트입니다.
> "EXAONE"과 "K-EXAONE"은 LG AI연구원의 상표이며, 이 프로젝트는 그 API를 사용하는 클라이언트일 뿐입니다.
>
> *Unofficial, community project — not made, endorsed or supported by LG AI Research or LG Corp.
> "EXAONE" and "K-EXAONE" are trademarks of LG AI Research.*

LG AI연구원의 **K-EXAONE** 모델 API로 보고서·문서를 쓰고, 내 PC의 프로젝트 폴더를 다루고,
Skill과 MCP로 기능을 넓히는 Windows 데스크톱 앱입니다. (Tauri v2 + React)

## 주요 기능

- **채팅 · 프로젝트 · 코드** 세 화면. 여러 대화를 동시에 백그라운드로 진행
- **프로젝트 폴더 도구**: 파일 읽기·쓰기·부분 수정·검색, 삭제와 명령 실행은 승인 후 실행, 모든 변경은 되돌리기 가능
- **문서 캔버스**: Markdown·Word(.docx) 편집, 한글(.hwpx)은 원래 모양 그대로 보며 고치기([rhwp](https://github.com/edwardkim/rhwp) 엔진), Excel(.xlsx)·PowerPoint(.pptx)·PDF 미리보기, 다른 형식으로 저장
- **문서 직접 생성**: Word·한글(HWPX)·PowerPoint·Excel 파일을 앱이 직접 만들고, 차트 같은 복잡한 파일은 Python 스크립트로 생성
- **답변 저장**: 답변을 .md / .docx / .hwpx로 바로 저장
- **Skill**: 작업 지침 묶음(Agent Skills 형식)을 설치하면 엑사원이 필요할 때 읽어 따름
- **MCP**: 로컬(stdio)·원격(Streamable HTTP) MCP 서버 연결, OAuth 로그인 지원, 도구별 승인 설정
- **찾아보기**: [카탈로그](https://github.com/kidarigun/exaone-catalog)에서 Skill과 MCP 서버를 클릭 한 번으로 설치
- **실행 환경 관리**: Python·Node.js·uv가 없는 PC에서도 앱이 공식 배포본을 내려받아(버전 고정·해시 확인) 전용으로 설치
- **K-EXAONE에 맞춘 동작**: 추론 강도 선택, 오늘 남은 API 호출 수 표시, 하루 호출 한도를 아끼는 도구 구성, 긴 문서 작성 중 끊김 방지 ([변경 이력](CHANGELOG.md))

## 사용하려면

- Windows 10 / 11 (64비트)
- K-EXAONE API 키 — 앱의 **[API 및 모델 설정]**에 입력하면 Windows 자격 증명 관리자에 저장됩니다.

설치 파일은 [릴리스 페이지](https://github.com/kidarigun/k-exaone-desktop/releases/latest)에서 내려받습니다.
서명되지 않은 설치 파일이라 Windows가 "PC 보호" 경고를 보여 줄 수 있습니다. **추가 정보 → 실행**을 누르면 설치됩니다.

0.3.0부터는 새 버전이 나오면 앱이 알려 주고, **[업데이트]**를 누르면 내려받아 설치합니다. (0.2.0 이하를 쓰고 있다면 한 번은 직접 설치해야 합니다.)

소스에서 직접 빌드하려면 아래 "개발"을 보세요.

## 개발

필요한 것: Node.js 20 이상, Rust stable (MSVC 툴체인, 또는 GNU 툴체인 + MinGW 빌드 도구)

```bash
npm install
npm run tauri dev      # 개발 실행
npm test               # 프런트엔드 테스트
cd src-tauri && cargo test --lib   # Rust 테스트
npm run tauri build    # 설치 파일(NSIS) 만들기
```

`npm run tauri build`는 자동 업데이트용 서명까지 만들기 때문에 서명 키가 필요합니다.
키 없이 설치 파일만 만들려면 서명을 끄고 빌드하세요.

```bash
npm run tauri build -- --config "{\"bundle\":{\"createUpdaterArtifacts\":false}}"
```

### 릴리스 (관리자용)

자동 업데이트는 깃허브 릴리스에 올린 `latest.json`을 읽어 동작합니다. 설치 파일은 서명 키로 서명되어야 앱이 받아들입니다.

```bash
TAURI_SIGNING_PRIVATE_KEY="$HOME/.tauri/k-exaone-desktop.key" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" npm run tauri build
python scripts/make_latest_json.py --notes "바뀐 점 요약"
gh release create v<버전> release/K-EXAONE-Desktop_<버전>_x64-setup.exe release/latest.json --title "<버전>" --notes-file <노트 파일>
```

- 설치된 앱은 버전 번호가 더 높을 때만 업데이트로 봅니다. 기능을 바꿨으면 네 곳(package.json, package-lock.json, src-tauri/Cargo.toml, src-tauri/tauri.conf.json)의 버전을 올리고 빌드하세요. `make_latest_json.py`는 올라가 있는 버전보다 높지 않으면 멈춥니다.
- 서명 개인 키는 저장소에 넣지 않습니다. 잃어버리면 이미 설치된 앱에 업데이트를 보낼 수 없으니 따로 보관하세요.
- 공개 키는 `src-tauri/tauri.conf.json`의 `plugins.updater.pubkey`에 있습니다.

### 실제 API로 시험하기

`harness/`의 시나리오 파일을 사용자 삼아, 실제 K-EXAONE API로 앱의 에이전트 루프를 그대로 돌려 봅니다.
API 키는 앱에 저장해 둔 것을 Windows 자격 증명 관리자에서 읽습니다.

```bash
python harness/make_scenarios.py     # 시나리오 JSON 만들기
cd src-tauri
EXA_SCENARIOS=../harness/scenarios.json EXA_OUT=../harness/runs/내이름 EXA_MAX_CALLS=20   cargo test --lib harness::live -- --ignored --nocapture
python ../harness/report.py ../harness/runs/내이름    # 결과 요약
```

- 하루 호출 한도를 함께 쓰므로 `EXA_MAX_CALLS`(이번 실행의 상한)와 `EXA_DAY_RESERVE`(남겨 둘 호출 수)를 정해 두세요.
- 이용약관상 부하 시험과 같은 요청의 반복은 금지되어 있습니다. 한 번에 하나씩 실행하고, 시험 자료는 지어낸 것만 쓰세요.
- 결과(`harness/runs/`)는 Git에 올라가지 않습니다.

## 데이터와 보안

- 대화·설정은 `%APPDATA%\io.github.kidarigun.kexaone-desktop`, 실행 환경은 `%LOCALAPPDATA%\…\runtimes`에 저장됩니다.
- API 키, MCP 서버의 비밀 값, OAuth 토큰은 설정 파일이 아니라 **Windows 자격 증명 관리자**에 저장됩니다.
- 엑사원이 파일을 지우거나 명령·스크립트를 실행하거나 MCP 도구를 쓸 때는 기본적으로 사용자 승인을 받습니다.
- 앱이 직접 접속하는 곳: 설정한 K-EXAONE API 주소, 사용자가 추가한 MCP 서버, 찾아보기 카탈로그, 실행 환경 공식 배포처(github.com/astral-sh/uv, nodejs.org, PyPI), 새 버전 확인(github.com의 이 저장소 릴리스 — 설정에서 끌 수 있음).

## 라이선스

[Apache License 2.0](LICENSE) — 함께 쓰는 외부 라이브러리는 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md), 상표 관련 안내는 [NOTICE](NOTICE)를 보세요.
