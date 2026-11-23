# 0004 — LM Studio 컨텍스트 초과 오류 (작업 8 실측)

2026-11-23. `qwen3.5-9b`를 `context_length: 4096`으로 로드하고 약 13,600토큰짜리 메시지를 `/v1/chat/completions`에 보냈다.

- 비스트리밍: HTTP 400, 본문 `{"error":"Engine protocol predict request returned 400: {\"error\":{\"code\":400,\"message\":\"request (13610 tokens) exceeds the available context size (4096 tokens), try increasing it\",\"type\":\"exceed_context_size_error\",\"n_prompt_tokens\":13610,\"n_ctx\":4096}}"}` (`error`가 객체가 아니라 문자열이다).
- 스트리밍(`stream:true`): **HTTP 200**, 본문 첫 이벤트가 `event: error` + `data: {"error":{"message":"<위와 같은 문구>"},"message":"<같은 문구>"}`. 앱에서는 `apply_chunk`가 "스트림 오류: ..."로 올린다.

`looks_like_context_overflow`는 안정적인 부분 문자열 `exceeds the available context size` 또는 `exceed_context_size_error`(소문자 비교)로 알아본다. 상태 코드(400/413) 유무와 무관하다. 기존 400/413 + 키워드 규칙은 그대로 둔다.

실측 뒤 `qwen3.5-9b`를 32768로 다시 로드했다(인스턴스 1개).
