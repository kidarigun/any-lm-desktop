# 라이브 업데이트(자동 업데이트) · 통합 설정 메뉴 · 공개 릴리스 — 설계

날짜: 2026-11-24 · 상태: 사용자 결정 반영 · 위치: `docs/superpowers/specs/2026-11-24-updater-settings-release-design.md`

## 1. 배경과 목표

Any LM Desktop을 비공개 실험 단계에서 공개 오픈소스 프로젝트로 전환하면서, 다음 요구 사항을 반영한다. (개발 서버·우측 미리보기 기능은 기존 동작을 유지한다.)

1. **앱 자동 업데이트(라이브 업데이트)**: GitHub Releases(`kidarigun/any-lm-desktop`)를 기준으로 새 버전을 확인·다운로드·설치할 수 있게 한다.
2. **좌측 하단 메뉴 통합**: 사이드바 하단의 `Skill` · `MCP 서버` · `{공급자} 연결` 3개 버튼을 **`[설정]` 버튼 하나**로 통합하고, 설정 창 안에서 탭으로 전환한다.
3. **배포용 설치 프로그램 빌드**: 전용 서명 키로 서명된 Windows NSIS 설치 프로그램(`.exe`)과 업데이터 아티팩트(`.sig`, `latest.json`)를 생성한다.
4. **공개 프로젝트 전환 및 GitHub 반영**: 민감정보·추적 파일을 점검하고, 문서를 공개 프로젝트 기준으로 갱신한 뒤 저장소를 `public`으로 전환하고 `main`을 푸시한다.

## 2. 앱 자동 업데이트 (`tauri-plugin-updater`)

- **전용 서명 키 분리**: 원본 K-EXAONE 키(`k-exaone-desktop.key`)와 분리된 Any LM Desktop 전용 서명 키(`%USERPROFILE%\.tauri\any-lm-desktop.key`)를 사용한다. 비밀키는 로컬 사용자 폴더에만 두고 저장소에 포함하지 않으며, 공개키(`.pub`)만 `src-tauri/tauri.conf.json`의 `plugins.updater.pubkey`에 넣는다.
- **백엔드(`src-tauri/`)**:
  - `Cargo.toml`: `tauri-plugin-updater = "2"` 의존성 추가.
  - `tauri.conf.json`: `bundle.createUpdaterArtifacts: true`, 엔드포인트 `https://github.com/kidarigun/any-lm-desktop/releases/latest/download/latest.json`, `windows.installMode: "passive"`.
  - `store.rs`: `AppSettings.check_updates` (`default_true`, 기본 켜짐) 추가.
  - `lib.rs`: `UPDATE_PROGRESS_EVENT("update-progress")`, `UpdateInfo`, `check_update`, `install_update` 명령 복구. 업데이트 설치로 앱을 닫기 직전(`on_before_exit` 및 `install_update` 직전)에 `runs.cancel_all()`, `mcp.shutdown_all()`, `terminal.close_all()`, `single_instance.shutdown()`을 모두 호출해 백그라운드 터미널/개발 서버 프로세스 트리가 남지 않도록 한다.
- **프런트엔드 및 릴리스 도구**:
  - `src/types.ts`, `src/api.ts`: `UpdateInfo`, `AppSettings.checkUpdates`, `checkUpdate`/`installUpdate`/`onUpdateProgress` 추가.
  - `src/update/UpdatePanel.tsx`: `useUpdater`, `UpdateBanner`(사이드바 알림), `UpdateSettings`(설정 창 업데이트 탭).
  - `scripts/make_latest_json.py`: `kidarigun/any-lm-desktop` 릴리스용 `Any-LM-Desktop_<ver>_x64-setup.exe`와 `latest.json` 생성.

## 3. 통합 `[설정]` 메뉴

- **사이드바 하단**: `<UpdateBanner>` 아래에 기존 3개 버튼 대신 **`[설정]` 버튼 하나**를 둔다. 오른쪽에는 활성 LLM 공급자 라벨(`LM Studio` | `Ollama` | `OpenAI 호환`)과 연결 상태 점(초록/빨강/회색)을 표시한다.
- **`src/settings/SettingsDialog.tsx`**: 4개 탭을 가진 단일 설정 창(`role="dialog" aria-modal="true" aria-label="설정"`):
  1. `연결 · 모델`: `ConnectionDialog` (`embedded` 모드)
  2. `Skill`: `SkillsDialog` (`embedded` 모드)
  3. `MCP 서버`: `McpDialog` (`embedded` 모드)
  4. `환경 · 업데이트`: `RuntimePanel` + `UpdateSettings`
- 기존 3개 다이얼로그는 `embedded?: boolean` 옵션을 추가해 단독 호출 호환성과 통합 탭 내장 표시를 모두 지원한다. `McpDialog` 내부 편집 폼(`draft` 또는 `importText`)이 열린 상태에서 `Escape`를 누르면 전체 설정 창 대신 내부 편집기만 닫는다.

## 4. 배포 빌드 · 공개 전환 검증 기준

1. `npm test`, `npx tsc --noEmit`, `cd src-tauri && cargo test --lib`, `cargo check --all-targets`, `npm run build` 모두 통과.
2. `THIRD_PARTY_NOTICES.md` 재생성(`tauri-plugin-updater` 및 관련 크레이트 포함).
3. 서명 키와 함께 `npm run tauri build` 및 `python scripts/make_latest_json.py --allow-same-version` 성공.
4. `git ls-files`에 비밀키/로컬 데이터 미포함 확인, `kidarigun/any-lm-desktop` `PUBLIC` 전환 및 `git push origin main`.
