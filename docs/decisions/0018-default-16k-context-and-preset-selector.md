# 0018 — 기본 컨텍스트 16K(16,384) 전환 및 8K~128K 콤보 선택 UI

## 배경

1. **32K 기본 컨텍스트의 한계와 8K의 협소함**:
   - 대상 환경(RTX 5060 Laptop 8GB, RAM 32GB)에서 `Q4_K_M` 양자화 로컬 모델(`qwen3.5-9b`, `qwen3.6-35b-a3b`)에 기본 `32K`(32,768)를 잡으면 KV Cache·문맥 부담 때문에 긴 턴에서 사고 루프나 처리 실패가 잦았다.
   - 반면 `8K`(8,192)는 프로젝트 모드 시스템 프롬프트·지침·도구 결과를 담기에 너무 작다.
   - 따라서 처음 설정 시 기본 컨텍스트를 **`16K`(16,384)** 로 맞춘다.
2. **숫자 직접 입력 UI의 불편함**:
   - 연결 설정 화면에서 토큰 수를 숫자로 직접 입력(`input type="number"`)해야 해 `8192`·`16384` 같은 2의 거듭제곱/1024 배수를 계산해 입력하기 번거로웠다.

## 결정

1. **백엔드·프런트엔드·하니스 기본값을 `16384`로 통일**:
   - `src-tauri/src/lmstudio.rs`: `DEFAULT_CONTEXT = 16384` (`providers/mod.rs`, `providers/ollama.rs`가 공용 참조).
   - `src-tauri/src/harness.rs`: `ANYLM_CONTEXT` 미지정 시 `crate::lmstudio::DEFAULT_CONTEXT` (`16384`) 사용.
   - `src/lmstudio/sendFlow.ts`: `DEFAULT_CONTEXT = 16384`, `ConnectionDialog.tsx`도 동일 상수 및 `wantedContext` 공유.
   - `AGENTS.md`: 기본 원하는 컨텍스트 및 실측 후 복구 기준을 `16384`로 갱신.
2. **8K~128K 프리셋 콤보 선택 (`<select>`)**:
   - `src/lmstudio/modelText.ts`에 `CONTEXT_PRESETS` (`8K`, `16K`, `24K`, `32K`, `48K`, `64K`, `96K`, `128K`), `formatContextSize`, `contextOptions`를 추가하고, `src/lmstudio/ConnectionDialog.tsx`의 숫자 입력란을 `<select>`로 교체했다.
   - 모델의 `maxContextLength` 이하 프리셋만 노출하되(`8K` 미만 모델이면 `maxContextLength` 자체 포함), 기존 저장값이나 `[N K로 다시 시도]`(예: `4K`)로 저장된 값이 유효(`>= 1024`)할 경우 오름차순으로 함께 보여 주어 기존 설정이 깨지지 않는다.

## 알아둘 점 (동작 특성)

- **16K 기준 예산**: `context_budget`의 70% 자동 요약 압축 기준은 약 **11,468 토큰**(글자 수 기준 약 24,576자, 추정 1.5자/토큰), 도구 1회 출력 상한은 **8,192자**(`max(4000, ctx / 2)`)로 동작한다.
- **기존 저장값과 이미 로드된 모델**: 이미 `modelPrefs[model].contextLength = 32768`로 저장된 기존 사용자의 설정은 임의로 덮어쓰지 않으며, LM Studio에 이미 32K로 로드된 모델도 앱이 자동으로 내리지 않는다(AGENTS.md 규칙 유지). 기존 모델에도 16K를 적용하려면 설정 창에서 `16K (기본)`을 고른 뒤 `[다시 로드]` → `[저장]`을 한 번 누르면 된다.
