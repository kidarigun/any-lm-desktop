import { describe, expect, it } from "vitest";
import { SETTINGS_TABS, settingsButtonStatusTitle } from "./settingsTabs";

describe("settingsTabs", () => {
  it("defines the 4 unified settings tabs in order", () => {
    expect(SETTINGS_TABS.map((t) => t.id)).toEqual(["connection", "skills", "mcp", "system"]);
  });

  it("formats sidebar settings button status title", () => {
    expect(settingsButtonStatusTitle("LM Studio", true)).toBe("LM Studio: 연결됨");
    expect(settingsButtonStatusTitle("Ollama", false)).toBe("Ollama: 꺼져 있음");
    expect(settingsButtonStatusTitle("OpenAI 호환", null)).toBe("OpenAI 호환: 확인 중");
  });
});
