import { describe, expect, it } from "vitest";
import type { AppSettings, LmModel } from "../types";
import { isServerOffMessage, nextSendStep, smallerContext, smallerContextLabel, wantedContext } from "./sendFlow";

describe("sendFlow", () => {
  it("maps load plans to the next step", () => {
    expect(nextSendStep({ kind: "ready", instanceId: "a", contextLength: 32768 })).toBe("send");
    expect(nextSendStep({ kind: "notLoaded", others: [] })).toBe("load");
    expect(nextSendStep({ kind: "notLoaded", others: ["qwen3.5-9b"] })).toBe("askUnload");
    expect(nextSendStep({ kind: "smallerLoaded", instanceId: "a", loaded: 8192, wanted: 32768 })).toBe("send-with-reload-hint");
    expect(nextSendStep({ kind: "missing" })).toBe("missing");
  });

  it("picks the wanted context like the backend", () => {
    const model = { key: "m", maxContextLength: 16384 } as LmModel;
    const settings = (prefs: AppSettings["modelPrefs"]) => ({ modelPrefs: prefs }) as AppSettings;
    expect(wantedContext(settings({}), model)).toBe(16384);
    expect(wantedContext(settings({ m: { contextLength: 8192 } }), model)).toBe(8192);
    expect(wantedContext(settings({ m: { contextLength: 500 } }), model)).toBe(16384);
    expect(wantedContext(settings({ m: { contextLength: 999999 } }), model)).toBe(16384);
    expect(wantedContext(settings({}), { ...model, maxContextLength: 0 })).toBe(32768);
  });

  it("suggests half the attempted context with a 4K floor", () => {
    expect(smallerContext(32768)).toBe(16384);
    expect(smallerContext(16384)).toBe(8192);
    expect(smallerContext(8192)).toBe(4096);
    expect(smallerContext(6000)).toBe(4096);
    expect(smallerContext(262144)).toBe(131072);
    expect(smallerContext(20000)).toBe(9216); // 10000 → 1024 단위 내림
    // 이미 바닥이면 더 줄이지 않는다
    expect(smallerContext(4096)).toBeNull();
    expect(smallerContext(2048)).toBeNull();
    expect(smallerContext(0)).toBeNull();
    expect(smallerContext(NaN)).toBeNull();
    expect(smallerContextLabel(16384)).toBe("16K로 다시 시도");
  });

  it("recognizes the backend's server-off message", () => {
    expect(isServerOffMessage("LM Studio 서버가 꺼져 있습니다. LM Studio에서 서버를 켜거나 [서버 시작]을 눌러 주세요.")).toBe(true);
    expect(isServerOffMessage("Error: LM Studio 서버가 꺼져 있습니다.")).toBe(true);
    expect(isServerOffMessage("모델을 찾을 수 없습니다.")).toBe(false);
    expect(isServerOffMessage(null)).toBe(false);
  });
});
