# 0014 — 기본 Skill·MCP 카탈로그 복구 및 재발 방지 규칙

이 결정은 [0011 — Any LM 저장 이름과 기본 카탈로그](0011-anylm-local-identity.md)의 4번째 항목("기본 찾아보기는 내장 MCP 목록을 사용한다")을 대체한다.

## 원인

- 커밋 `21143f3`에서 `DEFAULT_CATALOG_URL`(`https://raw.githubusercontent.com/kidarigun/exaone-catalog/main/catalog.json`)과 `catalog_default_url` 명령을 제거하고 `catalog::load("", cache_dir)`가 빈 카탈로그(`skills: []`, `mcp: []`)를 반환하도록 바꾸었다.
- 그 결과 웹뷰의 `CatalogViews.tsx`가 `builtinCatalog()`(내장 공식 웹 검색 MCP 8개, `skills: []`)와 빈 원격 응답을 합쳐, **Skill → 찾아보기**에서 기존 63개 Skill이 모두 사라지고(0개), **MCP 서버 → 찾아보기**에서는 내장 웹 검색 8개만 남아 기존 17개 MCP 서버 목록이 보이지 않았다.
- 이어진 커밋 `fe7e89c`에서는 좌측 하단 메뉴를 `[설정]` 창으로 통합하고 배포용 설치 파일(`Any LM Desktop_0.1.0_x64-setup.exe`)을 빌드·공개하면서, Skill·MCP 찾아보기 실제 목록이 비어 있는 회귀를 사전에 검증·발견하지 못한 채 그대로 배포했다.

## 복구 내용

- `src-tauri/src/catalog.rs`에 `DEFAULT_CATALOG_URL`과 `resolve_url`을 복구하고, 주소가 비어 있을 때 빈 스텁을 반환하던 분기를 제거해 기본 원격 카탈로그를 불러오도록 복구했다(오프라인 시 `catalog_cache.json` 저장본 폴백 유지).
- 기본 원격 카탈로그의 `name` 필드에 이전 앱 브랜드명(`EXAONE`/`LG`)이 포함된 경우에만 표시 제목을 `"Any LM Desktop 기본 목록"`으로 정리하고, 모든 Skill(63개)과 MCP(17개) 항목 및 내장 웹 검색 MCP 8개는 원형 그대로 합쳐(`mergeCatalogs`) 보여 준다.
- `src-tauri/src/lib.rs`(`catalog_default_url` 명령 복구), `src/api.ts`(`BackendAPI.catalogDefaultUrl` 복구), `src/catalog/CatalogViews.tsx`(기본 목록 메모이제이션, 실패 시 기존 목록 보존)를 복원했다.
- 바로잡음(전체 검토): 처음 기록에는 `CatalogViews.tsx`가 `catalogDefaultUrl`로 입력 안내를 복구했다고 적었지만, 화면은 이 명령을 쓰지 않는다. 주소 입력란의 안내는 고정 문구 "비우면 기본 목록 (https://...catalog.json)"뿐이고, 기본 주소 자체는 보여 주지 않는다. `BackendAPI.catalogDefaultUrl`은 지금 `catalog.test.ts`에서만 쓴다.

## 영구 재발 방지 규칙

1. **무단 삭제·단절 금지**: 사용자가 명시적으로 지시하지 않는 한, 기존에 제공되던 Skill·MCP 기본 목록 소스(`DEFAULT_CATALOG_URL`, `builtinCatalog`)나 사용자의 설치·저장 데이터를 삭제·비활성화·빈 스텁으로 대체하지 않는다.
2. **메뉴·설정 리팩터링 불변 조건**: 설정 통합·UI 개편 작업 전후로 기존 Skill(설치됨/찾아보기)·MCP(내 서버/찾아보기) 목록과 데이터 소스가 모두 유지되는지 반드시 확인한다.
3. **자동 회귀 테스트와 릴리스 확인**:
   - Rust(`catalog.rs`): 비어 있는 주소가 `DEFAULT_CATALOG_URL`을 가리키며 오프라인 캐시 폴백에서도 Skill/MCP 항목을 유지하는지, `live_default_catalog_has_skills_and_mcp`(`#[ignore]`)로 실제 원격 목록을 검증한다.
   - Vitest(`catalog.test.ts`): `catalogDefaultUrl`이 유효한 HTTPS `catalog.json` 주소를 반환하고, `mergeCatalogs`가 내장 웹 검색 MCP 8개와 원격 기본 Skill·MCP 항목을 모두 보존하는지 검증한다.
   - 모든 설정·카탈로그 변경 및 배포용 설치 프로그램 빌드 전에는 위 테스트와 함께 Skill/MCP 찾아보기 목록이 비어 있지 않은지 확인한다.
