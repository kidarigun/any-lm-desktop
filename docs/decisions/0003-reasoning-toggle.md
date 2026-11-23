# 0003 — 추론 끄기: 요청에 보낼 값

날짜: 2026-11-23 (LM Studio 0.4.28, `POST /v1/chat/completions` 실측, stream false, max_tokens 256, 질문 "17*23은?")

## 실측: 후보 요청별 `usage.completion_tokens_details.reasoning_tokens`

| 후보 | qwen3.5-9b (32768) | qwen3.6-35b-a3b (8192) |
|---|---|---|
| (아무것도 안 보냄) | 241 (추론만 하다 256에서 잘림) | 242 (같음) |
| `"reasoning_effort": "none"` | **0** (답변 234토큰) | **0** (답변 161토큰) |
| `"reasoning": {"effort": "none"}` | 240 (효과 없음) | 242 (효과 없음) |
| `"chat_template_kwargs": {"enable_thinking": false}` | 240 (효과 없음) | 242 (효과 없음) |
| `"reasoning_effort": "off"` | HTTP 400 `Invalid 'reasoning_effort' value: 'off'. Supported values: none, minimal, low, medium, high, xhigh.` | (확인 안 함) |

## 결정

- `REASONING_OFF_FIELDS = {"reasoning_effort": "none"}`. 두 모델 모두에서 reasoning_tokens가 0이 된 유일한 방식이다. `reasoning` 객체와 `chat_template_kwargs`는 LM Studio가 무시한다.
- 모델 목록의 `capabilities.reasoning.allowed_options`는 `["off","on"]`(기본 `"on"`)이지만 이것은 LM Studio 화면용 이름이고, 요청의 `reasoning_effort`에는 `"off"`를 쓰면 안 된다(400). 그래서 앱 저장값 `"off"`를 `reasoning_effort: "none"`으로 바꿔 보낸다.
- "켜기"는 아무것도 보내지 않는다(모델 기본에 맡김). 추론 선택지가 없는 모델에도 아무것도 보내지 않는다.
- 저장값이 없으면 모델 기본(켜짐)으로 보고, 토글에서 바꾸면 `modelPrefs[key].reasoning`에 `"on"`/`"off"`를 명시해 저장한다.
- 이전 답변의 추론은 다음 요청에 다시 보내지 않는다. 원본의 `preserve_thinking`(K-EXAONE 전용 `chat_template_kwargs`)은 삭제했다. `build_api_messages_with_compaction`의 `keep_thinking` 인자는 원본 cherry-pick 호환을 위해 두고 항상 false를 넘긴다.
- 모델의 추론 선택지는 턴 시작 때 `lmstudio::model_reasoning_options`로 모델 목록을 한 번 가져와 `TurnRequest.reasoning_options`로 넘긴다. 못 가져오면 빈 목록이라 추론 항목을 보내지 않고 턴은 그대로 진행한다.

## 실측 뒤 상태

35B 확인을 위해 9B를 언로드하고 35B를 8192로 로드(약 21초)했다가 내리고, qwen3.5-9b를 32768로 다시 로드했다(로드된 모델은 9B 한 개).
