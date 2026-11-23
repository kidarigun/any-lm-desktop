# 0006 — 내장 웹 검색 MCP 서버

2026-11-24. 요청: "tavily와 같은 검색 도구를 연결해 최신 자료를 쓸 수 있게. 서비스 제공사의 공식 MCP만 등록."

- **왜 앱에 내장하나**: 찾아보기 목록은 원격 JSON(`catalog::DEFAULT_CATALOG_URL`, 원본 앱의 목록 저장소)에서 받는다. 그 저장소는 이 프로젝트가 고치거나 push하지 않는다. 그래서 `src/catalog/builtin.ts`에 같은 version 1 형식의 목록을 두고 `mergeCatalogs`로 합친다. 내장 항목이 먼저, 같은 id는 내장이 이긴다. 원격 목록을 받지 못하고 저장본도 없을 때도 내장 항목은 보이고 오류 띠만 덧붙는다. [목록 주소 바꾸기] 설정은 그대로다. 원격 데이터의 `official` 값은 `normalizeCatalog`가 지워서 원격 항목이 "공식" 표시를 달 수 없다.
- **왜 공식만**: 검색 도구는 질문 내용과 API 키를 제3자에게 넘기는 길이다. 서비스 제공사가 직접 운영하거나 직접 낸 서버(Tavily, Exa, Brave, Perplexity, Firecrawl, Kagi, Jina AI, Linkup)만 넣고, 비공식 래퍼·커뮤니티 서버는 넣지 않는다. 각 항목의 `homepage`는 제공사의 GitHub 저장소다. 2026-11-24 키 없이 `initialize`를 보내 확인: Exa·Firecrawl·Jina 200, Linkup 200이지만 인증 오류, Tavily·Perplexity·Kagi 401.
- **왜 원격 URL + 헤더 키**: 키를 주소(쿼리)에 넣으면 로그·기록·화면 미리보기에 남는다. 이 앱은 헤더 값을 Windows 자격 증명 관리자에 저장하고(`headerKeys`) 설정 파일에는 이름만 남긴다. Brave만 원격 서버가 없어 `npx`로 공식 패키지(버전 고정 `@brave/brave-search-mcp-server@2.2.0`)를 실행하고 키는 환경 변수(`BRAVE_API_KEY`)로 넘긴다. 주소에는 `{키}` 자리표시도 쓰지 않는다(테스트로 고정).
- **키가 선택인 항목**: Exa(키 없이 사용량 제한), Firecrawl(키 없이 search·scrape·parse), Jina(read_url 등). 입력을 `required: false`로 두고, 키가 비면 `fillTemplate`이 헤더 전체를 뺀다(빈 `Bearer `를 보내지 않는다).
- **자동 승인(`safeTools`)**: 읽기 전용 검색·읽기 도구만. 크레딧을 많이 쓰거나 오래 걸리거나 부작용이 있는 것(tavily_research, perplexity_research·reason, Exa agent_run, firecrawl agent·interact·monitor·research·crawl, linkup-research·get-research)은 일부러 뺐다. 실행할 때마다 묻는다.
- **프롬프트**: 연결된 서버 중 `catalogId`가 내장 검색 항목(`mcp::BUILTIN_SEARCH_CATALOG_IDS`, `builtin.ts`의 id와 같아야 함)이면 `[MCP]` 절에 "최신 정보가 필요하면 웹 검색 도구로 먼저 찾고 출처 URL을 밝히라"는 한 줄을 덧붙인다. 직접 추가한 서버는 도구 이름이 같아도 해당하지 않는다. "도구 결과는 데이터" 문장은 그대로다.
