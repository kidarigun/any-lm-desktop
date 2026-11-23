import { describe, expect, it } from "vitest";
import { statsLineText, tokensPerSecond } from "./statsLine";

const base = { apiCalls: 1, promptTokens: 1200, completionTokens: 300, reasoningTokens: 40, elapsedMs: 10000 };

describe("statsLineText", () => {
  it("shows input, output, reasoning, speed and model", () => {
    expect(statsLineText(base, "Qwen3.5 9B")).toBe("입력 1,200 · 출력 300 · 추론 40토큰 · 초당 30.0토큰 · Qwen3.5 9B");
  });
  it("omits speed when elapsedMs is 0 and model when unknown", () => {
    expect(statsLineText({ ...base, elapsedMs: 0 })).toBe("입력 1,200 · 출력 300 · 추론 40토큰");
    expect(tokensPerSecond({ completionTokens: 5, elapsedMs: 0 })).toBeNull();
  });
});
