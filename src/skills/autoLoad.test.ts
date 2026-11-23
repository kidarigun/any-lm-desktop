import { describe, expect, it } from "vitest";
import type { AppSettings, ToolExecutionRecord } from "../types";
import { skillAutoLoadOn, toolBadgeLabel, withSkillAutoLoad } from "./autoLoad";

const rec = (over: Partial<ToolExecutionRecord>): ToolExecutionRecord => ({
  id: "r",
  toolName: "load_skill",
  argumentsJson: '{"name":"weekly-report"}',
  status: "completed",
  output: "",
  ...over,
});

const settings: AppSettings = {
  apiKey: "",
  serverUrl: "http://localhost:1234",
  model: "",
  writingStyle: "default",
  autoCompact: true,
  modelPrefs: {},
};

describe("toolBadgeLabel", () => {
  it("shows the auto-loaded skill name for automatic records", () => {
    expect(toolBadgeLabel(rec({ automatic: true }))).toBe("Skill 자동 불러옴: weekly-report");
  });
  it("keeps the tool name for model calls and broken arguments", () => {
    expect(toolBadgeLabel(rec({}))).toBe("load_skill");
    expect(toolBadgeLabel(rec({ toolName: "read_file", argumentsJson: "{}" }))).toBe("read_file");
    expect(toolBadgeLabel(rec({ automatic: true, argumentsJson: "{" }))).toBe("Skill 자동 불러옴");
  });
});

describe("skill auto-load setting", () => {
  it("is on unless explicitly turned off (old settings files have no value)", () => {
    expect(skillAutoLoadOn(settings)).toBe(true);
    expect(skillAutoLoadOn({ ...settings, skillAutoLoad: true })).toBe(true);
    expect(skillAutoLoadOn({ ...settings, skillAutoLoad: false })).toBe(false);
  });
  it("changes only the switch and never sends a stored token back", () => {
    const next = withSkillAutoLoad({ ...settings, apiKey: "x" }, false);
    expect(next.skillAutoLoad).toBe(false);
    expect(next.apiKey).toBe("");
    expect(next.serverUrl).toBe(settings.serverUrl);
  });
});
