# EXAONE Desktop 설계 문서 (Design Specification)

- **작성일**: 2026-11-13
- **프레임워크**: Tauri v2 (Rust Backend + React/TypeScript/Tailwind CSS Frontend)
- **타겟 모델**: LG AI Research K-EXAONE 2.5 (`k-exaone_v2.5`, Endpoint: `https://api.exaone.ai/v1/chat/completions`)

## 1. 개요 및 목적
Claude Desktop 및 Claude Code(Codex)를 벤치마킹하여, K-EXAONE 2.5 API를 활용한 올인원 로컬 데스크톱 애플리케이션을 구축한다. 채팅, 글쓰기, 보고서/문서 작성(Artifacts) 기능과 로컬 디렉터리 기반의 프로젝트 관리 및 자율 파일/코드 제어(Read, Write, Modify, Delete, Shell 실행) 기능을 단일 워크스페이스에서 제공한다.

## 2. 핵심 아키텍처 (Rust 코어 + React UI)
- **Rust Backend (`src-tauri`)**:
  - **EXAONE API & Agent Loop**: `https://api.exaone.ai/v1/chat/completions` OpenAI 호환 엔드포인트 연동, 스트리밍(`stream: true`) 시 `reasoning_content`(사고 과정)와 `content`(답변), `tool_calls`를 파싱하여 프론트엔드로 실시간 이벤트 전송.
  - **로컬 파일 시스템 제어 (Project = Local Folder)**:
    - `list_directory`, `read_file`, `write_file`, `modify_file`, `create_directory`, `delete_path`, `search_files` 제공.
    - **자율 에이전트 권한 모델 (스타일 2)**:
      - 파일 읽기(`read_file`, `list_directory`, `search_files`), 파일 생성(`write_file`), 파일 수정(`modify_file`)은 프로젝트 폴더 내에서 AI가 즉시 자동 반영하며, 수정 전 원본 내용을 스냅샷으로 남겨 언제든 `되돌리기(Undo)` 가능하도록 구현.
      - 파일/폴더 삭제(`delete_path`) 및 터미널 명령어 실행(`run_command`)은 사용자 승인(`승인` / `거부` UI 카드) 후 실행.
  - **로컬 데이터 저장 (`SQLite` 또는 로컬 JSON 스토어)**:
    - API 키 및 기본 설정(`reasoning_effort`: `low` | `high` | `max`, 문체 프리셋)
    - 프로젝트 목록(이름, 연결된 로컬 폴더 경로, 프로젝트 맞춤 지침 System Prompt)
    - 프로젝트별 대화 세션 및 메시지 히스토리, 파일 수정 스냅샷(Diff/Undo용)

- **React Frontend (`src`)**:
  - **Claude 스타일 3단 레이아웃**:
    1. **좌측 사이드바**: 프로젝트 선택/추가(로컬 폴더 연결), 프로젝트 지침 설정, 실시간 로컬 파일/폴더 트리 탐색기(새 파일/폴더 생성, 삭제 가능), 대화 목록.
    2. **중앙 채팅 & 에이전트 패널**:
       - 상단: 현재 프로젝트/모델 상태 표시, 추론 강도(`low`/`high`/`max`) 및 글쓰기 스타일(기본, 기획 보고서, 논리적 글쓰기, 코드 에이전트 등) 선택기.
       - 본문: `reasoning_content` 접기/펼치기 블록, 마크다운 렌더링, Tool 실행 배지(읽기/쓰기/수정 자동 완료 표시 및 Undo 버튼, 삭제/터미널 승인 카드).
       - 하단: 메시지 입력창, 프로젝트 내 파일 빠른 첨부(`@파일`), 보고서/문서 템플릿 빠른 실행 버튼.
    3. **우측 아티팩트 & 파일 스튜디오 패널**:
       - 선택한 로컬 파일 또는 AI가 방금 작성/수정한 문서(보고서, 마크다운, HTML, 코드)를 즉시 표시.
       - 탭 구성: `[미리보기(Preview) | 편집기(Editor) | 변경 비교(Diff) | 터미널(Terminal)]`
       - 직접 편집 후 로컬 파일 즉시 저장(`Ctrl+S`), 다른 이름으로 내보내기 지원.

## 3. 에러 처리 및 검증 기준
- API 키 미설정 또는 호출 실패 시 명확한 한국어 안내 배너 및 설정 모달 유도.
- 로컬 파일 수정(`modify_file`) 시 대상 문자열을 찾지 못하면 AI에게 에러 결과를 반환하여 스스로 재시도하도록 처리.
- 프로젝트 루트 경로 외부의 상위 디렉터리 접근(`..`) 방지 검증.
