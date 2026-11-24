import { describe, expect, it } from "vitest";
import type { AppSettings } from "../types";
import {
  PROVIDER_META,
  activeProvider,
  ensureProviderProfiles,
  switchDraftProvider,
  syncDraftActiveProfile,
} from "./providerMeta";

const baseSettings = (): AppSettings => ({
  apiKey: "",
  apiKeySet: true,
  serverUrl: "http://127.0.0.1:2345",
  model: "qwen3.5-9b",
  writingStyle: "executive_report",
  autoCompact: true,
  modelPrefs: { "qwen3.5-9b": { contextLength: 16384 } },
});

describe("providerMeta", () => {
  it("defaults to lmstudio and migrates legacy top-level fields into profiles", () => {
    const s = baseSettings();
    expect(activeProvider(s)).toBe("lmstudio");
    const profiles = ensureProviderProfiles(s);
    expect(profiles.lmstudio.serverUrl).toBe("http://127.0.0.1:2345");
    expect(profiles.lmstudio.model).toBe("qwen3.5-9b");
    expect(profiles.lmstudio.apiKeySet).toBe(true);
    expect(profiles.ollama.serverUrl).toBe(PROVIDER_META.ollama.defaultUrl);
    expect(profiles.openai_compat.serverUrl).toBe(PROVIDER_META.openai_compat.defaultUrl);
  });

  it("preserves per-provider settings across provider switches", () => {
    let draft = switchDraftProvider(baseSettings(), "ollama");
    expect(draft.provider).toBe("ollama");
    expect(draft.serverUrl).toBe("http://localhost:11434");
    expect(draft.model).toBe("");
    expect(draft.apiKeySet).toBe(false);

    draft = {
      ...draft,
      model: "qwen3:8b",
      serverUrl: "http://localhost:11435",
      modelPrefs: { "qwen3:8b": { contextLength: 32768 } },
    };

    const backToLm = switchDraftProvider(draft, "lmstudio");
    expect(backToLm.serverUrl).toBe("http://127.0.0.1:2345");
    expect(backToLm.model).toBe("qwen3.5-9b");
    expect(backToLm.apiKeySet).toBe(true);
    expect(backToLm.providers?.ollama?.model).toBe("qwen3:8b");
    expect(backToLm.providers?.ollama?.serverUrl).toBe("http://localhost:11435");

    const synced = syncDraftActiveProfile(backToLm);
    expect(synced.providers?.lmstudio?.model).toBe("qwen3.5-9b");
  });
});
