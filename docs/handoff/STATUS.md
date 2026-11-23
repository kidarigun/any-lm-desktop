# STATUS — 현재 상태 한 장

## 지금 목표
원본 K-EXAONE Desktop 0.4.4를 LM Studio 전용 앱 "Any LM Desktop"(0.1.0)으로 바꾼다. 설계: `docs/superpowers/specs/2026-11-23-any-lm-desktop-design.md`.

## 진행 중
- 계획 파일: `docs/superpowers/plans/2026-11-23-any-lm-desktop.md`
- 작업 1(저장소 만들기와 인계 체계): 단계 1~5 완료. 단계 6(GitHub 비공개 저장소 `kidarigun/any-lm-desktop` 생성·푸시)은 사용자 승인 대기 중이라 아직 하지 않았다.
- 작업 2(정체성 분리와 업데이트 제거): 완료. 제품명·identifier·버전·크레이트명·자격 증명 서비스명·단일 실행 포트(45789)·미리보기 호스트·대화상자 환경 변수·localStorage 접두어(`anylm_`)·기본 작업 폴더(`문서\AnyLM`)를 바꾸고, 업데이트 기능(`tauri-plugin-updater`, UpdatePanel, `check_update`/`install_update`, `checkUpdates` 설정, `make_latest_json.py`)과 `migrate_old_data_dir`을 삭제했다.

## 마지막 확인
2026-11-23, 작업 2 완료 시점:
- `npm test` → 통과 (16 파일, 102 통과, 1 건너뜀)
- `npx tsc --noEmit` → 통과 (오류 없음)
- `cd src-tauri && cargo test --lib` → 통과 (123 통과, 0 실패, 7 무시)
- 동시 실행: 원본 앱(포트 45788)과 `npm run tauri dev`(포트 45789)가 함께 떴고, 새 데이터가 `%APPDATA%\io.github.kidarigun.anylm-desktop`에 생겼으며 원본 데이터 폴더 수정 시각은 그대로였다.

## 다음 할 일
1. 계획의 작업 3부터 이어서 진행한다(`docs/superpowers/plans/2026-11-23-any-lm-desktop.md`의 작업 3 첫 단계부터).
2. 작업 1의 단계 6은 사용자가 승인하면 한다: "`kidarigun/any-lm-desktop` 비공개 저장소를 만들고 main을 올릴까요?" → 승인 시 `gh repo create kidarigun/any-lm-desktop --private --source . --remote origin --push`.

## 결정 필요
- GitHub 비공개 저장소 생성·푸시 여부(작업 1 단계 6).

## 기준 상태
작업을 시작하기 전(복제 직후, e885b02)의 테스트 결과다. 이후 실패가 나오면 이 기준과 비교해 새로 깬 것인지 구분한다.
- `npm install`: 성공
- `npm test`: 16 파일 통과, 102 통과 / 1 건너뜀 / 실패 0
- `npx tsc --noEmit`: 오류 0
- `cargo test --lib`: 121 통과 / 실패 0 / 7 무시(ignored)

## 수동 확인 목록
(사람이 화면에서 직접 확인해야 하는 항목을 이후 작업이 여기에 덧붙인다.)
- 원본 앱과 새 앱 창이 동시에 뜨는지 눈으로 확인 (작업 2: 프로세스·포트·데이터 폴더만 확인했고 창 모양은 보지 못했다)

## 마지막 작업자
- 도구: claude-code
- 모델: Claude Sonnet 5.5
- 시각: 2026-11-23
