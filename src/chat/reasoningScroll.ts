export const REASONING_SCROLL_THRESHOLD = 32;

/** 스크롤 컨테이너가 맨 아래(threshold 이내)에 있는지 판단한다. */
export function isNearBottom(
  scrollTop: number,
  clientHeight: number,
  scrollHeight: number,
  threshold = REASONING_SCROLL_THRESHOLD
): boolean {
  return scrollHeight - scrollTop - clientHeight <= threshold;
}
