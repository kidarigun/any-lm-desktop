import { describe, expect, it } from "vitest";
import { isNearBottom, REASONING_SCROLL_THRESHOLD } from "./reasoningScroll";

describe("생각 창 자동 스크롤 판단", () => {
  it("맨 아래에 있을 때는 참이다", () => {
    expect(isNearBottom(240, 160, 400)).toBe(true);
  });

  it("임계값 이내일 때는 참이다", () => {
    expect(isNearBottom(400 - 160 - REASONING_SCROLL_THRESHOLD, 160, 400)).toBe(true);
    expect(isNearBottom(400 - 160 - 10, 160, 400)).toBe(true);
  });

  it("사용자가 위쪽으로 스크롤했을 때는 거짓이다", () => {
    expect(isNearBottom(400 - 160 - REASONING_SCROLL_THRESHOLD - 1, 160, 400)).toBe(false);
    expect(isNearBottom(0, 160, 400)).toBe(false);
  });

  it("내용이 넘치지 않는 짧은 창에서는 항상 참이다", () => {
    expect(isNearBottom(0, 160, 100)).toBe(true);
    expect(isNearBottom(0, 160, 160)).toBe(true);
  });
});
