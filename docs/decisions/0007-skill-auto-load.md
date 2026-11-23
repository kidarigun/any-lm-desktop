# 0007 — Skill 자동 불러오기와 하니스 MCP

2026-11-24. 배경: qwen3.5-9b(8K)로 하니스 `proj-skill`을 돌리니 9B는 `load_skill`을 한 번도 부르지 않고 자기 양식으로 보고서를 썼다(1/5). 35B는 대체로 부른다. 사용자 질문 "설치한 Skill은 항상 작동하나?"에 답하려면 작은 모델에서도 Skill이 쓰여야 한다.

## 앱이 Skill을 고른다
- **고르기(`skills::select_for_message`)**: 켜져 있고 문제없고 가려지지 않은 Skill만 본다. 턴마다 최대 하나.
  - 이름을 부르면(`/weekly-report`, `weekly-report` — 대소문자 무시, 앞뒤가 `[a-z0-9_-]`가 아닐 때만, 한글 조사는 붙어도 됨) 그것. 여럿이면 긴 이름.
  - 아니면 지식 폴더와 같은 토큰(`knowledge::tokenize`: 영문·숫자 소문자 낱말 + 한글 2글자 묶음)으로 요청 쪽 비율을 잰다: (Skill 이름+설명에 든 요청 토큰 수) / (요청의 서로 다른 토큰 수). 맞은 토큰이 `SKILL_AUTO_MIN_HITS = 2`개 이상, 비율이 `SKILL_AUTO_MIN_COVERAGE = 0.25` 이상, 2등보다 `SKILL_AUTO_MIN_MARGIN = 0.05` 이상 앞설 때만 고른다(비기면 고르지 않음).
  - 단위 테스트 값: "이번 주 주간 업무 보고서를 만들어 줘" → weekly-report 0.70, proj-skill 요청 0.44, "어제 회의 내용으로 회의록 작성해 줘" → meeting-minutes 0.56(weekly-report 0.22), "안녕"·"엑셀 차트 그려 줘"·"계속"·"보고" → 없음.
  - 왜 요청 쪽 비율인가: Skill 설명은 길고 요청은 짧다. 설명 쪽 비율로 재면 설명이 긴 Skill이 늘 불리하다. 요청이 짧으면 맞은 토큰 2개 조건이 흔한 한 글자(`주`, `줘`)만으로 고르는 것을 막는다.
- **넣는 자리**: 지식 폴더 `[참고 자료]`와 같은 방식(`insert_skill_message`) — 마지막 사용자 메시지 바로 앞의 user+assistant 한 쌍, 저장하지 않음. 둘 다 있으면 참고 자료 쌍 → Skill 쌍 → 질문 순(Skill이 질문에 가장 가깝다). 시스템 메시지에 넣지 않는 이유도 같다(질문마다 바뀌면 프롬프트 앞부분 재사용이 깨진다).
- **머리 줄**: `[Skill: <이름> — 앱이 이 요청에 맞는 Skill로 판단해 미리 불러왔습니다. 아래 지침을 그대로 따르세요. 맞지 않으면 무시하세요.]` 뒤에 `load_skill` 결과와 같은 글(`load_for_model`: 함께 든 파일 안내, 프로젝트면 스크립트 실행 안내, 본문).
- **예산**: `min(10,000, 컨텍스트 × 1.5 × 0.20)`자(8K → 2,457자, 32K → 9,830자). 넘으면 앞부분만 넣고 끝에 `read_skill_file(skill=…, path="SKILL.md")` 또는 `load_skill`로 나머지를 읽으라는 줄을 붙인다. 줄이기 기준(`total_chars`)에 들어가고, 보내기의 자동 압축 어림에도 참고 자료 글자 수와 함께 더한다.
- **다시 부르기**: 같은 턴에 모델이 같은 Skill로 `load_skill`을 부르면 전문 대신 "이미 이 턴에 불러온 Skill입니다(위 지침 참고)."를 돌려준다. 단, 예산 때문에 잘라 넣었으면 전문을 준다(나머지를 읽을 길이 있어야 하므로).
- **프롬프트 한 줄**: `[Skill]` 절에 "질문에 맞는 Skill이 [Skill: …] 로 이미 들어와 있으면 load_skill을 다시 부르지 말고 그 지침을 따르세요." 설정을 꺼도 이 줄은 남긴다(시스템 프롬프트를 설정마다 바꾸지 않으려고; 들어온 것이 없으면 해당 없음).
- **화면 카드**: 답변의 도구 기록 맨 앞에 `load_skill` 기록을 하나 넣고 `automatic: true`로 표시한다(`AutoSkill::tool_record`). 도구 카드는 이름표만 "Skill 자동 불러옴: <이름>"으로 바꿔 그대로 쓴다(`src/skills/autoLoad.ts`의 `toolBadgeLabel`). 도구 기록이라 대화를 다시 열어도 남는다. `automatic` 기록은 다음 요청의 대화 기록(도구 호출·결과)으로 보내지 않는다(`message_rounds`가 뺀다) — 본문을 저장하지 않으므로 빈 도구 호출을 재현하지 않으려고. 라운드 번호 계산, "아무것도 받지 못함" 판단에서도 뺀다.
- **승인 뒤 이어가기**: 마지막 사용자 메시지에서 첨부 내용을 뺀 본문(`last_user_query`, 지식 폴더와 같은 `without_attachments`)으로 다시 고르고, 이어 가는 답변에 같은 Skill의 자동 기록이 있을 때만 같은 자리에 다시 넣는다(카드는 다시 만들지 않는다). 그사이 설정을 켰거나 Skill이 바뀌었으면 넣지 않는다.
- **설정**: `AppSettings.skill_auto_load`(기본 켜짐, 예전 설정 파일도 켜짐). Skill 창 "설치됨" 탭의 "Skill 자동 불러오기" 스위치.
- 일반 채팅도 같다(사용자 Skill만, 스크립트 실행 안내 없음).

## 하니스 MCP
- 시나리오 `mcpServers: [{ name, url?, headers?, command?, args?, env?, catalogId?, autoApprove? }]`: 앱과 같은 `mcp::McpManager`(설정 파일은 결과 폴더의 `mcp-servers.json`, 비밀 값은 `MemorySecrets` — 자격 증명 관리자에 쓰지 않음)에 `save`로 추가해 연결을 기다린 뒤(최대 60초) 앱과 같은 경로(`tool_specs` → `[MCP]` 절·내장 검색 줄, `run_approved_tool`)로 쓴다. 시나리오가 끝나면 `delete`로 끊고 지운다.
- `autoApprove`(서버의 원래 도구 이름)에 있는 도구는 승인 없이 실행된다. 나머지 MCP 도구는 앱이라면 승인을 물었을 도구로 멈추고, 하니스가 승인해 실행한 뒤 기록(`approvals[].wouldAskApproval/harnessApproved`)에 남긴다.
- 새 검사 `toolCalledContains`(도구 이름 일부, MCP 도구는 `mcp__<서버>__<도구>`로 보인다). 시나리오 `skillAutoLoad: false`로 자동 불러오기를 끈 비교도 할 수 있다. 결과 요약에 `autoSkills`·`mcp`(연결 상태·도구) 항목.
- `harness/search.json`(Git에 올림): Exa 키 없는 서버로 "LM Studio 최신 버전과 출시일" 검색. 키가 필요한 제공사는 부르지 않는다.
