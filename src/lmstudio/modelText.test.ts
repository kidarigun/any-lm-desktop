import { describe, expect, it } from "vitest";
import type { LmModel } from "../types";
import { capabilityBadges, describeModel, loadedLabel, normalizeContext } from "./modelText";

// src-tauri/src/testdata/lmstudio_models.json 과 같은 값
const qwen9b: LmModel = {
  key: "qwen3.5-9b",
  displayName: "Qwen3.5 9B",
  params: "9B",
  quantization: "Q4_K_M",
  sizeBytes: 6548748736,
  maxContextLength: 262144,
  loaded: [{ id: "qwen3.5-9b", contextLength: 32768 }],
  vision: true,
  toolUse: true,
  reasoningOptions: ["off", "on"],
};
const qwen35: LmModel = {
  key: "qwen3.6-35b-a3b",
  displayName: "Qwen3.6 35B A3B",
  params: "35B-A3B",
  quantization: "Q4_K_M",
  sizeBytes: 22069579744,
  maxContextLength: 262144,
  loaded: [],
  vision: true,
  toolUse: true,
  reasoningOptions: ["off", "on"],
};

describe("modelText", () => {
  it("describes a model in one line", () => {
    expect(describeModel(qwen9b)).toBe("9B · Q4_K_M · 6.1GB");
    expect(describeModel(qwen35)).toBe("35B-A3B · Q4_K_M · 20.6GB");
  });
  it("skips missing parts", () => {
    expect(describeModel({ ...qwen9b, params: null, quantization: undefined })).toBe("6.1GB");
  });
  it("labels load state", () => {
    expect(loadedLabel(qwen9b)).toBe("로드됨 · 32K");
    expect(loadedLabel(qwen35)).toBe("로드 안 됨");
  });
  it("lists capability badges", () => {
    expect(capabilityBadges(qwen9b)).toEqual(["vision", "tools", "reasoning"]);
    expect(capabilityBadges({ ...qwen9b, vision: false, toolUse: false, reasoningOptions: [] })).toEqual([]);
  });
  it("normalizes context input", () => {
    expect(normalizeContext("", 262144)).toBeNull();
    expect(normalizeContext("abc", 262144)).toBeNull();
    expect(normalizeContext("0", 262144)).toBeNull();
    expect(normalizeContext("512", 262144)).toBeNull();
    expect(normalizeContext("4096", 262144)).toBe(4096);
    expect(normalizeContext("999999", 262144)).toBe(262144);
  });
});
