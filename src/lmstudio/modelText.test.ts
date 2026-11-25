import { describe, expect, it } from "vitest";
import type { LmModel } from "../types";
import { capabilityBadges, contextOptions, describeModel, formatContextSize, loadedLabel, normalizeContext, reasoningEnabled, withReasoning } from "./modelText";

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
    expect(describeModel({ ...qwen9b, params: null, quantization: undefined, sizeBytes: 0 })).toBe("qwen3.5-9b");
    expect(describeModel({ ...qwen9b, displayName: "qwen3.5-9b", params: null, quantization: undefined, sizeBytes: 0 })).toBe("");
  });
  it("labels load state", () => {
    expect(loadedLabel(qwen9b)).toBe("로드됨 · 32K");
    expect(loadedLabel(qwen35)).toBe("로드 안 됨");
    expect(loadedLabel(qwen9b, false)).toBe("사용 가능 · 32K");
    expect(loadedLabel(qwen35, false)).toBe("사용 가능");
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
  it("shows the reasoning toggle only for models with options, default on", () => {
    expect(reasoningEnabled(undefined)).toBe(true);
    expect(reasoningEnabled({ reasoning: "off" })).toBe(false);
    expect(reasoningEnabled({ reasoning: "on" })).toBe(true);
  });
  it("saves reasoning into modelPrefs without touching other prefs", () => {
    const prefs = { a: { contextLength: 8192 } };
    expect(withReasoning(prefs, "a", "off")).toEqual({ a: { contextLength: 8192, reasoning: "off" } });
    expect(withReasoning(prefs, "b", "on")).toEqual({ a: { contextLength: 8192 }, b: { reasoning: "on" } });
    expect(prefs).toEqual({ a: { contextLength: 8192 } });
  });
  it("formats and builds preset options up to the model max safely", () => {
    expect(formatContextSize(16384)).toBe("16K");
    expect(formatContextSize(131072)).toBe("128K");
    expect(formatContextSize(20000)).toBe("20,000");

    const presets = [8192, 16384, 24576, 32768, 49152, 65536, 98304, 131072];
    // 128K보다 큰 모델은 모델 최대까지 고를 수 있다.
    expect(contextOptions(262144)).toEqual([...presets, 262144]);
    expect(contextOptions(0)).toEqual(presets);
    expect(contextOptions(32768)).toEqual([8192, 16384, 24576, 32768]);
    expect(contextOptions(131072)).toEqual(presets);
    expect(contextOptions(4096)).toEqual([4096]);
    // 프리셋이 아닌 모델 최대(예: Qwen3 40960)도 고를 수 있다.
    expect(contextOptions(40960)).toEqual([8192, 16384, 24576, 32768, 40960]);
    expect(contextOptions(262144, 4096)).toEqual([4096, ...presets, 262144]);
    expect(contextOptions(262144, 20000)).toEqual([8192, 16384, 20000, 24576, 32768, 49152, 65536, 98304, 131072, 262144]);
    expect(contextOptions(262144, 196608)).toEqual([...presets, 196608, 262144]);
    expect(contextOptions(32768, 131072)).toEqual([8192, 16384, 24576, 32768]);
    expect(contextOptions(40960, 65536)).toEqual([8192, 16384, 24576, 32768, 40960]);
  });
});
