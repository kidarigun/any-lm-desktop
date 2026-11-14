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
- **문서 캔버스**: Markdown·Word(.docx) 편집, Excel(.xlsx)·PowerPoint(.pptx)·PDF 미리보기, 다른 형식으로 저장
- **문서 직접 생성**: Word·PowerPoint·Excel 파일을 앱이 직접 만들고, 차트 같은 복잡한 파일은 Python 스크립트로 생성
- **답변 저장**: 답변을 .md / .docx로 바로 저장
- **Skill**: 작업 지침 묶음(Agent Skills 형식)을 설치하면 엑사원이 필요할 때 읽어 따름
- **MCP**: 로컬(stdio)·원격(Streamable HTTP) MCP 서버 연결, OAuth 로그인 지원, 도구별 승인 설정
- **찾아보기**: [카탈로그](https://github.com/kidarigun/exaone-catalog)에서 Skill과 MCP 서버를 클릭 한 번으로 설치
- **실행 환경 관리**: Python·Node.js·uv가 없는 PC에서도 앱이 공식 배포본을 내려받아(버전 고정·해시 확인) 전용으로 설치

## 사용하려면

- Windows 10 / 11 (64비트)
- K-EXAONE API 키 — 앱의 **[API 및 모델 설정]**에 입력하면 Windows 자격 증명 관리자에 저장됩니다.

설치 파일은 준비 중입니다. 지금은 아래처럼 소스에서 빌드해 실행할 수 있습니다.

## 개발

필요한 것: Node.js 20 이상, Rust stable (MSVC 툴체인, 또는 GNU 툴체인 + MinGW 빌드 도구)

```bash
npm install
npm run tauri dev      # 개발 실행
npm test               # 프런트엔드 테스트
cd src-tauri && cargo test --lib   # Rust 테스트
npm run tauri build    # 설치 파일(NSIS) 만들기
```

## 데이터와 보안

- 대화·설정은 `%APPDATA%\io.github.kidarigun.kexaone-desktop`, 실행 환경은 `%LOCALAPPDATA%\…\runtimes`에 저장됩니다.
- API 키, MCP 서버의 비밀 값, OAuth 토큰은 설정 파일이 아니라 **Windows 자격 증명 관리자**에 저장됩니다.
- 엑사원이 파일을 지우거나 명령·스크립트를 실행하거나 MCP 도구를 쓸 때는 기본적으로 사용자 승인을 받습니다.
- 앱이 직접 접속하는 곳: 설정한 K-EXAONE API 주소, 사용자가 추가한 MCP 서버, 찾아보기 카탈로그, 실행 환경 공식 배포처(github.com/astral-sh/uv, nodejs.org, PyPI).

## 라이선스

[Apache License 2.0](LICENSE) — 함께 쓰는 외부 라이브러리는 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md), 상표 관련 안내는 [NOTICE](NOTICE)를 보세요.
