import type { AppSettings, LlmProvider, ProviderProfile } from "../types";

export interface ProviderMeta {
  id: LlmProvider;
  label: string;
  defaultUrl: string;
  addressHint: string;
  tokenPlaceholder: string;
  emptyModelsText: string;
  contextHint?: string;
  supportsLoad: boolean;
  supportsUnload: boolean;
  supportsStartServer: boolean;
}

export const PROVIDERS: readonly LlmProvider[] = ["lmstudio", "ollama", "openai_compat"];

export const PROVIDER_META: Record<LlmProvider, ProviderMeta> = {
  lmstudio: {
    id: "lmstudio",
    label: "LM Studio",
    defaultUrl: "http://localhost:1234",
    addressHint: "이 PC의 LM Studio 주소만 쓸 수 있습니다 (예: http://localhost:1234).",
    tokenPlaceholder: "LM Studio에서 인증을 켠 경우에만 입력",
    emptyModelsText: "LM Studio에 설치된 모델이 없습니다.",
    supportsLoad: true,
    supportsUnload: true,
    supportsStartServer: true,
  },
  ollama: {
    id: "ollama",
    label: "Ollama",
    defaultUrl: "http://localhost:11434",
    addressHint: "이 PC의 Ollama 주소만 쓸 수 있습니다 (예: http://localhost:11434).",
    tokenPlaceholder: "로컬 프록시 등에서 인증을 켠 경우에만 입력",
    emptyModelsText: "Ollama에 설치된 모델이 없습니다.",
    contextHint: "모델별 컨텍스트는 Ollama 서버 기본값(OLLAMA_CONTEXT_LENGTH)과 다를 수 있습니다.",
    supportsLoad: true,
    supportsUnload: true,
    supportsStartServer: true,
  },
  openai_compat: {
    id: "openai_compat",
    label: "OpenAI 호환",
    defaultUrl: "http://localhost:8080",
    addressHint: "llama.cpp · vLLM · LocalAI 등의 로컬 주소만 쓸 수 있습니다 (예: http://localhost:8080/v1).",
    tokenPlaceholder: "서버에서 API 키 인증을 켠 경우에만 입력",
    emptyModelsText: "서버에서 모델 목록을 받지 못했습니다.",
    contextHint: "컨텍스트 값은 앱의 자동 요약 압축과 도구 결과 예산 계산에 쓰입니다.",
    supportsLoad: false,
    supportsUnload: false,
    supportsStartServer: false,
  },
};

export function activeProvider(settings: Pick<AppSettings, "provider"> | null | undefined): LlmProvider {
  const p = settings?.provider;
  if (p === "lmstudio" || p === "ollama" || p === "openai_compat") return p;
  return "lmstudio";
}

export function ensureProviderProfiles(settings: AppSettings): Record<LlmProvider, ProviderProfile> {
  const cur = activeProvider(settings);
  const existing = settings.providers ?? {};
  const result = {} as Record<LlmProvider, ProviderProfile>;
  for (const id of PROVIDERS) {
    const saved = existing[id];
    if (saved) {
      result[id] = {
        serverUrl: saved.serverUrl?.trim() ? saved.serverUrl : PROVIDER_META[id].defaultUrl,
        model: saved.model ?? "",
        apiKeySet: saved.apiKeySet,
        modelPrefs: { ...(saved.modelPrefs ?? {}) },
      };
    } else if (id === cur) {
      result[id] = {
        serverUrl: settings.serverUrl?.trim() ? settings.serverUrl : PROVIDER_META[id].defaultUrl,
        model: settings.model ?? "",
        apiKeySet: settings.apiKeySet,
        modelPrefs: { ...(settings.modelPrefs ?? {}) },
      };
    } else {
      result[id] = {
        serverUrl: PROVIDER_META[id].defaultUrl,
        model: "",
        apiKeySet: false,
        modelPrefs: {},
      };
    }
  }
  return result;
}

/** 현재 편집 중인 공급자의 최상위 필드를 프로필에 기록하고, 다음 공급자의 프로필을 최상위로 복원한다. */
export function switchDraftProvider(draft: AppSettings, next: LlmProvider): AppSettings {
  const current = activeProvider(draft);
  const profiles = ensureProviderProfiles(draft);
  profiles[current] = {
    serverUrl: draft.serverUrl,
    model: draft.model,
    apiKeySet: draft.apiKeySet,
    modelPrefs: { ...(draft.modelPrefs ?? {}) },
  };
  const target = profiles[next];
  return {
    ...draft,
    provider: next,
    serverUrl: target.serverUrl || PROVIDER_META[next].defaultUrl,
    model: target.model ?? "",
    apiKey: "",
    apiKeySet: target.apiKeySet ?? false,
    modelPrefs: { ...(target.modelPrefs ?? {}) },
    providers: profiles,
  };
}

/** 저장 전 최상위 필드를 활성 공급자 프로필에 반영한다. */
export function syncDraftActiveProfile(draft: AppSettings): AppSettings {
  const current = activeProvider(draft);
  const profiles = ensureProviderProfiles(draft);
  profiles[current] = {
    serverUrl: draft.serverUrl?.trim() ? draft.serverUrl : PROVIDER_META[current].defaultUrl,
    model: draft.model ?? "",
    apiKeySet: draft.apiKeySet,
    modelPrefs: { ...(draft.modelPrefs ?? {}) },
  };
  return {
    ...draft,
    provider: current,
    serverUrl: profiles[current].serverUrl,
    providers: profiles,
  };
}
