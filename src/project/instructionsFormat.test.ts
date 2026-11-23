import { describe, expect, it } from "vitest";
import {
  INSTRUCTION_TEMPLATES,
  formatInstructionsStats,
  insertTemplate,
  instructionsStats,
} from "./instructionsFormat";

describe("instructionsStats", () => {
  it("counts characters by code point and estimates tokens at 1.5 chars", () => {
    const s = instructionsStats("가".repeat(300), 8192);
    expect(s.chars).toBe(300);
    expect(s.tokens).toBe(200);
    expect(s.percent).toBe(2.4);
    expect(s.warn).toBe(false);
  });
  it("warns only above 10% of the loaded context", () => {
    expect(instructionsStats("a".repeat(1500), 10000).warn).toBe(false); // 정확히 10%
    expect(instructionsStats("a".repeat(1503), 10000).warn).toBe(true);
  });
  it("has no percent without a context", () => {
    const s = instructionsStats("abc", null);
    expect(s.percent).toBeNull();
    expect(s.warn).toBe(false);
    expect(formatInstructionsStats(s)).toBe("3자 · 약 2토큰");
  });
  it("formats the full line", () => {
    expect(formatInstructionsStats(instructionsStats("가".repeat(300), 8192))).toBe(
      "300자 · 약 200토큰 · 컨텍스트의 2.4%"
    );
  });
  it("handles empty text", () => {
    expect(instructionsStats("", 8192)).toEqual({ chars: 0, tokens: 0, percent: 0, warn: false });
  });
});

describe("insertTemplate", () => {
  it("replaces an empty or blank editor", () => {
    expect(insertTemplate("", "T")).toBe("T");
    expect(insertTemplate("  \n ", "T")).toBe("T");
  });
  it("appends after a blank line otherwise", () => {
    expect(insertTemplate("기존\n", "T")).toBe("기존\n\nT");
  });
});

describe("templates", () => {
  it("has four Korean templates of 5-10 lines", () => {
    expect(INSTRUCTION_TEMPLATES).toHaveLength(4);
    for (const t of INSTRUCTION_TEMPLATES) {
      const body = t.text.split("\n").filter((l) => l.startsWith("- "));
      expect(body.length).toBeGreaterThanOrEqual(5);
      expect(body.length).toBeLessThanOrEqual(10);
    }
  });
});
