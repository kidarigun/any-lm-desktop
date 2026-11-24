# 라이브 업데이트 · 통합 설정 메뉴 · NSIS 빌드 · 공개 전환 계획

설계: `docs/superpowers/specs/2026-11-24-updater-settings-release-design.md`

- [x] **단계 1: 설계·계획·결정 문서 작성**
  - `docs/superpowers/specs/2026-11-24-updater-settings-release-design.md`
  - `docs/superpowers/plans/2026-11-24-updater-settings-release.md`
  - `docs/decisions/0013-updater-and-unified-settings.md`
- [x] **단계 2: 전용 서명 키 생성 및 앱 자동 업데이트 복구**
  - Any LM Desktop 전용 서명 키 생성(`%USERPROFILE%\.tauri\any-lm-desktop.key`)
  - `Cargo.toml`, `tauri.conf.json`, `store.rs`, `lib.rs` 업데이터 및 종료 정리(`terminal.close_all()`) 복구
  - `src/types.ts`, `src/api.ts`, `src/update/UpdatePanel.tsx`, `scripts/make_latest_json.py` 복구
- [x] **단계 3: 좌측 하단 3개 메뉴 → `[설정]` 메뉴 하나로 통합**
  - `ConnectionDialog`, `SkillsDialog`, `McpDialog` `embedded` 모드 추가
  - `src/settings/settingsTabs.ts` + `settingsTabs.test.ts`, `src/settings/SettingsDialog.tsx` 구현
  - `src/App.tsx` 사이드바 단일 `[설정]` 버튼 + `UpdateBanner` + `SettingsDialog` 연동
- [x] **단계 4: 서드파티 고지 갱신 및 전체 자동 검증**
  - `python scripts/third_party_notices.py` 실행
  - `npm test`, `npx tsc --noEmit`, `cargo test --lib`, `cargo check --all-targets`, `npm run build` 통과
- [x] **단계 5: 배포용 NSIS 설치 프로그램 빌드**
  - 서명 키 환경 변수와 함께 `npm run tauri build` 실행 및 `release/latest.json` 생성 확인
- [x] **단계 6: 공개 전환 준비 · 인계 문서 갱신 · 커밋 · Public 전환 · 푸시**
  - `README.md`, `AGENTS.md`, `docs/handoff/STATUS.md`, `docs/handoff/LOG.md` 갱신
  - 민감정보 점검 및 커밋(`Agent: antigravity`)
  - `gh repo edit` 공개 전환 및 `git push origin main`
