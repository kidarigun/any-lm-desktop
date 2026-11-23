//! 로드된 LM Studio 컨텍스트 길이에 맞춘 줄이기 기준과 도구 출력 상한.

/// 컨텍스트 길이를 알 수 없을 때 쓰는 값.
pub const FALLBACK_CONTEXT: u32 = 8192;
/// 요약 압축(줄이기)을 시작하는 컨텍스트 사용 비율.
pub const AUTO_COMPACT_RATIO: f64 = 0.70;
/// 글자 수로 토큰 수를 어림잡을 때 한 토큰의 글자 수(`chars()` 기준).
pub const CHARS_PER_TOKEN: f64 = 1.5;

/// 글자 수(바이트 아님)로 토큰 수를 어림한다(올림).
pub fn estimate_tokens(text_chars: usize) -> u64 {
    (text_chars as f64 / CHARS_PER_TOKEN).ceil() as u64
}

/// 마지막 `prompt_tokens`(없으면 0)에 새로 붙는 글자의 추정 토큰을 더해 컨텍스트로 나눈다.
pub fn usage_ratio(last_prompt_tokens: Option<u64>, extra_chars: usize, context_length: u32) -> f64 {
    let used = last_prompt_tokens.unwrap_or(0) + estimate_tokens(extra_chars);
    used as f64 / context_length.max(1) as f64
}

/// 요청 글자 수가 이 값을 넘으면 오래된 도구 결과를 줄여 보낸다.
pub fn trim_trigger_chars(context_length: u32) -> usize {
    (context_length as f64 * AUTO_COMPACT_RATIO * CHARS_PER_TOKEN) as usize
}

/// 도구 결과 한 개를 모델에 돌려줄 때의 글자 수 상한.
pub fn tool_output_cap(context_length: u32) -> usize {
    ((context_length / 2) as usize).max(4_000).min(40_000)
}

/// 로드된 컨텍스트 길이가 없으면 `FALLBACK_CONTEXT`.
pub fn resolve_context(loaded: Option<u32>) -> u32 {
    loaded.filter(|&c| c > 0).unwrap_or(FALLBACK_CONTEXT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_counts_chars_not_bytes() {
        assert_eq!(estimate_tokens("가나다라마바".chars().count()), 4);
    }
    #[test]
    fn unknown_context_falls_back_to_8192() {
        assert_eq!(resolve_context(None), 8192);
        assert_eq!(resolve_context(Some(0)), 8192);
        assert_eq!(resolve_context(Some(32768)), 32768);
    }
    #[test]
    fn ratio_uses_last_prompt_tokens_plus_new_text() {
        assert!((usage_ratio(Some(20_000), 3_000, 32768) - (22_000.0 / 32768.0)).abs() < 1e-9);
        assert!((usage_ratio(None, 0, 32768)).abs() < 1e-9);
    }
    #[test]
    fn caps_scale_with_context() {
        assert_eq!(trim_trigger_chars(32768), 34_406);
        assert_eq!(tool_output_cap(32768), 16_384);
        assert_eq!(tool_output_cap(4096), 4_000);
        assert_eq!(tool_output_cap(262144), 40_000);
    }
}
