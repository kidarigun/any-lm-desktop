# 0007 — Skill 자동 불러오기와 하니스 MCP

2026-11-24. 배경: qwen3.5-9b(8K)로 하니스 `proj-skill`을 돌리니 9B는 `load_skill`을 한 번도 부르지 않고 자기 양식으로 보고서를 썼다(1/5). 35B는 대체로 부른다. 사용자 질문 "설치한 Skill은 항상 작동하나?"에 답하려면 작은 모델에서도 Skill이 쓰여야 한다.

## 앱이 Skill을 고른다
- **고르기(`skills::select_for_message`)**: 켜져 있고 문제없고 가려지지 않은 Skill만 본다. 턴마다 최대 하나.
  - **이름으로 부르기**: `/이름`은 언제나, 슬래시 없는 이름은 영문이면서 `-`가 들어 있거나 4글자 이상일 때만(`weekly-report`, `translate` — `pdf`·`docx`는 `/pdf`처럼). 한글 이름 Skill은 `/요약`처럼 슬래시로만 부른다. 대소문자 무시, 앞뒤가 이름 글자가 아닐 때만 맞는다 — 영문 이름은 `[a-z0-9_-]`만 이름 글자라 "weekly-report로"처럼 조사가 붙어도 되고, 한글 이름은 한글도 이름 글자라 "/요약하지" 안에서는 맞지 않는다. 여럿이면 긴 이름.
  - **설명으로 고르기**: 지식 폴더와 같은 토큰(`knowledge::tokenize`: 영문·숫자 소문자 낱말 + 한글 2글자 묶음)에서 한 글자 토큰과 불용어(`STOPWORDS`: 사용·용자·요청·작성·정리·만들·이번·으로·한다·쓴다… / the·to·use·this·when·user·skill·wants·with·how…)를 뺀 "의미 있는 토큰"만 쓴다. Skill마다 맞은 토큰의 IDF 합을 잰다: `idf = ln(1 + (설치된 Skill 수 + SKILL_AUTO_GENERIC_DOCS) / 그 토큰이 든 Skill 수)`. `SKILL_AUTO_GENERIC_DOCS = 1.0`은 "어느 Skill에도 해당하지 않는 일반 요청" 가상 문서 — Skill이 하나뿐이어도 토큰 하나의 무게가 ln 3 = 1.10이라 맞은 토큰이 둘은 있어야 한다.
  - 고르는 조건(모두): 맞은 토큰 `SKILL_AUTO_MIN_HITS = 2`개 이상, 요청 쪽 비율(맞은 수 / 요청의 의미 있는 토큰 수) `SKILL_AUTO_MIN_COVERAGE = 0.25` 이상, IDF 합 `SKILL_AUTO_MIN_WEIGHT = 2.0` 이상, 2등보다 `SKILL_AUTO_MIN_MARGIN = 0.5` 이상 앞섬.
  - 단위 테스트: "이번 주 주간 업무 보고서를 만들어 줘"·proj-skill 요청 → weekly-report, "어제 회의 내용으로 회의록 작성해 줘" → meeting-minutes, "extract the text from this pdf" → pdf. 고르지 않음(Skill 셋일 때와 weekly-report 하나뿐일 때 모두): "이번 주 날씨 알려 줘", "이번 주 일정 정리해 줘", "사용자 요청을 처리하는 코드를 작성해 줘", "회사 소개 글 작성해 줘", "사용자 목록 요청하는 API 만들어 줘", "안녕", "엑셀 차트 그려 줘". PDF·DOCX 설명(영문)에 "explain this code to the user", "how do I use the git rebase command"도 고르지 않음.
  - 왜: 첫 판은 토큰 비율만 봐서 Skill 설명에 흔한 낱말("사용자가 … 요청할 때", "Use this skill whenever the user wants")만 맞아도 골랐다(검토 지적). 한 글자 토큰·불용어를 빼고, 여러 Skill에 흔한 토큰은 IDF로 덜 친다.
- **이어 쓰기(Sticky)**: 이번 요청으로 고르지 못했고 바로 앞 답변에 자동 불러오기 기록이 있으면 같은 Skill을 다시 넣는다("계속", "3번 항목만 고쳐 줘" 같은 후속 요청). 이어 쓴 턴이 연달아 `SKILL_STICKY_TURNS = 3`번이면 멈춘다. 다른 Skill을 고르면 그것으로 바뀌고, 설정을 끄면 넣지 않는다. 카드 이유는 "앞 요청에서 쓰던 Skill이라 이어서"(`reason: "sticky"`).
- **넣는 자리**: 지식 폴더 `[참고 자료]`와 같은 방식(`insert_skill_message`) — 마지막 사용자 메시지 바로 앞의 user+assistant 한 쌍, 저장하지 않음. 둘 다 있으면 참고 자료 쌍 → Skill 쌍 → 질문 순(Skill이 질문에 가장 가깝다). 시스템 메시지에 넣지 않는 이유도 같다(질문마다 바뀌면 프롬프트 앞부분 재사용이 깨진다).
- **머리 줄**: `[Skill: <이름> — 앱이 이 요청에 맞는 Skill로 판단해 미리 불러왔습니다. 아래 지침을 그대로 따르세요. 맞지 않으면 무시하세요.]` 뒤에 `load_skill` 결과와 같은 글(`load_for_model`: 함께 든 파일 안내, 프로젝트면 스크립트 실행 안내, 본문).
- **예산**: `min(10,000, 컨텍스트 × 1.5 × 0.20)`자(8K → 2,457자, 32K → 9,830자). 넘으면 앞부분만 넣고 끝에 `read_skill_file(skill=…, path="SKILL.md")` 또는 `load_skill`로 나머지를 읽으라는 줄을 붙인다. 줄이기 기준(`total_chars`)에 들어가고, 보내기의 자동 압축 어림에도 참고 자료 글자 수와 함께 더한다.
- **참고 자료와 합친 상한**: 둘 다 넣을 때 합계를 `컨텍스트 × 1.5 × knowledge::TURN_CONTEXT_RATIO(0.25)`자(8K → 3,072자)로 막는다. 참고 자료가 먼저 줄어든다(`knowledge::fit_beside_skill`: 조각 단위로 뒤에서부터 빼고, 한 조각도 안 들어가면 첫 조각을 잘라 `…`, 그것도 50자가 안 남으면 참고 자료를 뺀다). 보내기·다시 보내기·승인 뒤 이어가기·하니스 모두 같은 함수.
- **다시 부르기**: 같은 턴에 모델이 같은 Skill로 `load_skill`을 처음 부르면 전문 대신 "앱이 이 턴에 미리 넣었던 Skill입니다. 지침이 필요하면 load_skill로 다시 읽으세요."(`ALREADY_LOADED`)를 돌려준다. 이 답은 도구 결과로 저장되어 다음 턴에도 보이므로 "위에 있다"고 하지 않는다. 한 턴에 한 번만 — 그 뒤에 또 부르면(같은 라운드 안이라도) 전문을 준다. 예산 때문에 잘라 넣었으면 처음부터 전문을 준다(나머지를 읽을 길이 있어야 하므로).
- **프롬프트 한 줄**: `[Skill]` 절에 "질문에 맞는 Skill이 [Skill: …] 로 이미 들어와 있으면 load_skill을 다시 부르지 말고 그 지침을 따르세요." 설정을 꺼도 이 줄은 남긴다(시스템 프롬프트를 설정마다 바꾸지 않으려고; 들어온 것이 없으면 해당 없음).
- **화면 카드**: 답변의 도구 기록 맨 앞에 `load_skill` 기록을 하나 넣고 `automatic: true`로 표시한다(`AutoSkill::tool_record`). 도구 카드는 이름표만 "Skill 자동 불러옴: <이름>"으로 바꿔 그대로 쓴다(`src/skills/autoLoad.ts`의 `toolBadgeLabel`). 도구 기록이라 대화를 다시 열어도 남는다. `automatic` 기록은 다음 요청의 대화 기록(도구 호출·결과)으로 보내지 않는다(`message_rounds`가 뺀다) — 본문을 저장하지 않으므로 빈 도구 호출을 재현하지 않으려고. 라운드 번호 계산, "아무것도 받지 못함" 판단, 대화 압축(요약에 보내는 대화 글·로컬 요약의 "도구 N개 실행")에서도 뺀다.
- **승인 뒤 이어가기**: 다시 고르지 않고 멈췄던 답변의 자동 불러오기 카드에 적힌 Skill(이름·이유)을 그대로 같은 자리에 다시 넣는다(`skills::auto_for_resume`, 카드는 다시 만들지 않는다). 참고 자료는 마지막 사용자 메시지에서 첨부 내용을 뺀 본문(`last_user_query`)으로 다시 찾는다. 그사이 설정을 껐거나 Skill을 지웠·껐으면 넣지 않는다.
- **빈 답 한 번 재촉**: 한 라운드가 `finish_reason: "stop"`, 도구 호출 없음, 보이는 답 빈칸(추론만)으로 끝나면 `exaone::EMPTY_ANSWER_NUDGE` "도구 결과를 바탕으로 이제 사용자에게 보일 최종 답을 한국어로 써 주세요."를 사용자 메시지로 붙여 한 번 더 요청한다(마지막이 사용자 메시지면 그 끝에 덧붙인다). 저장하지 않고 화면 말풍선도 없다. 턴마다 한 번, 도구 라운드 한도에 센다. 9B가 Skill 양식대로 파일을 쓴 뒤 답을 추론에만 남기고 끝내던 일 때문(검토 A).
- **설정**: `AppSettings.skill_auto_load`(기본 켜짐, 예전 설정 파일도 켜짐). Skill 창 "설치됨" 탭의 "Skill 자동 불러오기" 스위치.
- 일반 채팅도 같다(사용자 Skill만, 스크립트 실행 안내 없음).

## 하니스 MCP
- 시나리오 `mcpServers: [{ name, url?, headers?, command?, args?, env?, catalogId?, autoApprove? }]`: 앱과 같은 `mcp::McpManager`(설정 파일은 결과 폴더의 `mcp-servers.json`, 비밀 값은 `MemorySecrets` — 자격 증명 관리자에 쓰지 않음)에 `save`로 추가해 연결을 기다린 뒤(최대 60초) 앱과 같은 경로(`tool_specs` → `[MCP]` 절·내장 검색 줄, `run_approved_tool`)로 쓴다. 시나리오가 끝나면 `delete`로 끊고 지운다.
- `autoApprove`(서버의 원래 도구 이름)에 있는 도구는 승인 없이 실행된다. 나머지 MCP 도구는 앱이라면 승인을 물었을 도구로 멈추고, 하니스가 승인해 실행한 뒤 기록(`approvals[].wouldAskApproval/harnessApproved`)에 남긴다.
- 새 검사 `toolCalledContains`(도구 이름 일부, MCP 도구는 `mcp__<서버>__<도구>`로 보인다). `toolCalled`·`toolCalledContains`·`toolNotCalled`는 앱이 넣은 자동 카드(`automatic`)를 세지 않는다 — 대신 `skillAutoLoaded`(text = Skill 이름)로 자동 불러오기를 본다. 검사의 `path`·`text` 안 `{today}`는 시나리오를 시작한 날(YYYY-MM-DD)로 바뀐다. 파일 이름에 `*`가 있으면 맞는 파일 중 가장 최근에 바뀐 하나를 본다. `proj-skill`은 `fileExists reports/주간보고_{today}.md`(Skill이 말한 "보고서를 쓰는 날짜")와 `fileContains reports/주간보고_*.md …`(양식)를 따로 본다 — 9B가 양식은 지키고 메모의 날짜(11/13)로 저장한 실행이 있어서, 날짜 하나 틀렸다고 양식 검사까지 모두 떨어지지 않게. 시나리오 `skillAutoLoad: false`로 자동 불러오기를 끈 비교도 할 수 있다. 결과 요약에 `autoSkills`·`mcp`(연결 상태·도구) 항목.
- `harness/search.json`(Git에 올림): Exa 키 없는 서버로 "LM Studio 최신 버전과 출시일" 검색. 키가 필요한 제공사는 부르지 않는다.

## 수정 1차 실측 (2026-11-24, 로드돼 있던 그대로: qwen3.5-9b 8192 / qwen3.6-35b-a3b 32768)
- proj-skill: 9B 3회 — 2/5(자동으로 넣음·양식 W-7 그대로, 파일 이름만 메모 날짜 `2026-11-13`; 빈 답 재촉이 한 번 들어간 뒤 파일을 씀. 이때는 `*` 검사 전이라 양식 검사도 날짜 경로였음), 5/5, 5/5. 35B 1회 5/5.
- search.json: 9B 2회 3/3·3/3(둘 다 2라운드가 추론만 하고 빈 답 → 재촉 한 번 → 3라운드에 URL 든 답), 35B 1회 3/3.
