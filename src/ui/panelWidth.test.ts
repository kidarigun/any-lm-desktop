import { describe, expect, it } from "vitest";
import {
  CANVAS_MIN_WIDTH,
  availableWidth,
  clampCanvasWidth,
  maxCanvasWidth,
  wideCanvasWidth,
} from "./panelWidth";

describe("캔버스 폭", () => {
  it("채팅창 최소 폭을 남기고, 캔버스 최소 폭 아래로는 줄지 않는다", () => {
    const avail = availableWidth(1440); // 1184
    expect(maxCanvasWidth(avail)).toBe(784);
    expect(clampCanvasWidth(2000, avail)).toBe(784);
    expect(clampCanvasWidth(100, avail)).toBe(CANVAS_MIN_WIDTH);
    expect(clampCanvasWidth(600.4, avail)).toBe(600);
  });

  it("아주 좁은 창에서는 최소 폭을 지킨다", () => {
    expect(clampCanvasWidth(520, availableWidth(800))).toBe(CANVAS_MIN_WIDTH);
  });

  it("넓게 보기는 화면의 64%이되 최대 1180px, 채팅창 최소 폭은 남긴다", () => {
    expect(wideCanvasWidth(1440)).toBe(784); // 64%=922이지만 채팅창 400px을 남겨야 함
    expect(wideCanvasWidth(2560)).toBe(1180);
  });

  it("사이드바를 접으면 그만큼 가용 폭이 넓어진다", () => {
    expect(availableWidth(1440, true)).toBe(1440);
    expect(wideCanvasWidth(1440, true)).toBe(922);
  });
});
