# 0010 — 개발 서버와 로컬 미리보기

프로젝트 ID, 실제 작업 폴더, 명령이 같은 실행 중 세션은 공유한다. 사용자 설정은 프로젝트에 저장하고, 실행 상태는 기존 ConPTY 세션을 참조하는 앱 수명 메모리에 둔다.

미리보기 iframe은 스크립트를 허용하되 부모 앱과 다른 불투명 출처로 격리한다(sandbox의 same-origin 권한 없음). 앱에 대해 원격 Tauri 권한을 부여하지 않는다. 이 때문에 쿠키, CORS 또는 저장소가 필요한 웹 앱은 브라우저에서 여는 기능이 필요할 수 있다. 화면의 IPv6 루프백 주소는 CSP 호스트 표기 제약 때문에 iframe에서 localhost로 연다.

- [Tauri 기능 권한과 원격 API 접근](https://tauri.app/security/capabilities/)
- [W3C CSP Level 3 출처 목록](https://w3c.github.io/webappsec-csp/)
