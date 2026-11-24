# 0013 — 앱 자동 업데이트 복구 · 통합 설정 메뉴 · 공개 저장소 전환

- Any LM Desktop을 공개 프로젝트로 전환하면서 GitHub Releases(`https://github.com/kidarigun/any-lm-desktop/releases/latest/download/latest.json`) 기반 `tauri-plugin-updater` v2 자동 업데이트를 복구한다.
- 서명 키는 원본 K-EXAONE 키(`k-exaone-desktop.key`)를 재사용하지 않고 Any LM Desktop 전용 키(`%USERPROFILE%\.tauri\any-lm-desktop.key`)를 새로 생성해 사용한다. 비밀키는 로컬 PC에만 보관하고 공개키만 `tauri.conf.json`에 둔다.
- 업데이트 설치로 앱을 닫기 직전(`on_before_exit` 및 `install_update`)에 기존 실행 취소·MCP 종료·단일 인스턴스 정리와 함께 `terminal::TerminalManager::close_all()`도 호출해, 실행 중인 터미널/개발 서버 프로세스 트리가 남지 않게 한다.
- 좌측 사이드바 하단의 `Skill`, `MCP 서버`, `{공급자} 연결` 3개 버튼을 `[설정]` 버튼 하나(활성 공급자명 + 연결 점 포함)로 통합하고, `SettingsDialog` 안의 4개 탭(`연결 · 모델`, `Skill`, `MCP 서버`, `환경 · 업데이트`)에서 모두 관리한다. 기존 `ConnectionDialog`/`SkillsDialog`/`McpDialog`는 `embedded` 모드를 통해 재사용한다.
