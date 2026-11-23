import { describe, expect, it } from "vitest";
import { MEMORY_ITEM_MAX_CHARS, MEMORY_MAX_ITEMS, memoryStats, memorySourceLabel, newestFirst, validateMemoryText } from "./memoryFormat";
import type { MemoryItem } from "../types";

const item = (id: string, text: string, createdAt: string, source: "model" | "user" = "user"): MemoryItem => ({
  id,
  text,
  createdAt,
  updatedAt: createdAt,
  source,
});

describe("memoryFormat", () => {
  it("shows newest first without changing the input", () => {
    const items = [item("a", "x", "2026-01-01"), item("b", "y", "2026-03-01"), item("c", "z", "2026-02-01")];
    expect(newestFirst(items).map((m) => m.id)).toEqual(["b", "c", "a"]);
    expect(items.map((m) => m.id)).toEqual(["a", "b", "c"]);
  });
  it("counts items and chars", () => {
    expect(memoryStats([item("a", "가나다", "t"), item("b", "ab", "t")])).toEqual({ count: 2, chars: 5 });
    expect(memoryStats([])).toEqual({ count: 0, chars: 0 });
  });
  it("labels the source in Korean", () => {
    expect(memorySourceLabel("model")).toBe("모델");
    expect(memorySourceLabel("user")).toBe("직접");
  });
  it("validates text like the backend", () => {
    expect(validateMemoryText("  ")).toMatch(/비어/);
    expect(validateMemoryText("가".repeat(MEMORY_ITEM_MAX_CHARS + 1))).toMatch(/500/);
    expect(validateMemoryText("가".repeat(MEMORY_ITEM_MAX_CHARS))).toBeNull();
    expect(MEMORY_MAX_ITEMS).toBe(200);
  });
});
