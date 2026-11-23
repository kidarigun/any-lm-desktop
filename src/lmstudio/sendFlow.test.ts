import { describe, expect, it } from "vitest";
import type { AppSettings, LmModel } from "../types";
import { nextSendStep, wantedContext } from "./sendFlow";

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
});
