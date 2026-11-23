# 0002 — LM Studio 모델 로드/언로드 API 형식과 앱이 보낼 값

날짜: 2026-11-23 (LM Studio 0.4.28, 실측 + 공식 문서 https://lmstudio.ai/docs/developer/rest)

## 요청·응답 형식 (실측)

`POST /api/v1/models/load`
- 본문: `{"model": "<key>", "context_length": 32768, "echo_load_config": true}`. 그 밖에 선택값: `eval_batch_size`, `flash_attention`, `num_experts`, `offload_kv_cache_to_gpu`.
- 응답 200: `{"type":"llm","instance_id":"qwen3.5-9b","load_time_seconds":9.59,"status":"loaded","load_config":{...}}`. `load_config`는 `echo_load_config:true`일 때만 온다(없으면 `type/instance_id/load_time_seconds/status`만).
- 로드 직후 `GET /api/v1/models`의 `loaded_instances[].config.context_length`에 요청한 값(32768)이 그대로 보였다.
- 9B 로드 약 10초, 35B-A3B 로드 약 14~28초(이 PC). 시간 제한은 계획대로 5분.

`POST /api/v1/models/unload`
- 본문: `{"instance_id": "<id>"}`. 응답 200: `{"instance_id": "<id>"}`.

오류: HTTP 404 + `{"error":{"type":"model_not_found","message":"Model nope not found in downloaded models"}}` (언로드는 `Model with instance identifier 'nope' is not loaded.`). 앱은 `error.message`를 그대로 보여 준다.

## 같은 모델을 다시 로드하면 인스턴스가 늘어난다

이미 로드된 `qwen3.5-9b`에 같은 요청을 또 보내면 거절되지 않고 `instance_id: "qwen3.5-9b:2"`가 새로 생긴다(두 벌이 VRAM을 차지). 그래서 앱은 로드 전에 반드시 `plan_load`로 로드 여부를 확인하고, 같은 모델의 두 번째 인스턴스는 만들지 않는다. [다시 로드]는 기존 인스턴스를 먼저 언로드한 뒤 로드한다.

## 모델별 GPU 설정(전문가 가중치 CPU로)을 덮어쓰는가

- 로드 API에는 GPU 오프로드 비율이나 "전문가 가중치 CPU" 매개변수가 **없다**(위 선택값만 있다). 따라서 앱이 보내지 않는 한 덮어쓸 수 없고, 보내지 않은 값은 LM Studio의 저장값/기본값이 쓰인다.
- 실측 한계: 이 PC에는 LM Studio에 저장된 모델별 설정이 없었다(`.lmstudio/.internal/user-concrete-model-default-config`가 비어 있음). 그래서 "저장값이 유지되는가"를 직접 비교하지는 못했다. 대신 `context_length`만 보내 35B-A3B(22 GB)를 로드하니 14초 만에 성공했고, GPU 메모리는 약 7.2 GB(8 GB 중)만 쓰였으며 대화 요청도 정상 응답했다. 즉 8 GB GPU에서 `context_length`만으로 쓸 수 있는 로드가 된다. 응답의 `load_config`에는 `num_experts: 8`이 보인다.
- 앱이 보낼 것: **`model`, `context_length`, `echo_load_config: true`만.** GPU 배분은 LM Studio에 맡긴다. 사용자가 LM Studio 화면에서 모델별 설정을 저장해 둔 경우 그 값이 유지되는지는 수동 확인 항목(STATUS)으로 남긴다.

## 8 GB GPU에서의 동시 로드

9B(32768)가 로드된 채 35B를 로드해도 API는 성공했다(느리게/메모리 분산될 수 있음). 그래도 앱은 자동으로 내리지 않고, 사용자가 [내리고 로드]를 눌렀을 때만 `unload`한다(설계 6.3).
