import type { AppSettings, Conversation, LlmProvider, LmModel, LoadPlan } from "../types";
import { MIN_CONTEXT } from "./modelText";
import { PROVIDER_META } from "./providerMeta";

export type SendStep = "send" | "load" | "askUnload" | "send-with-reload-hint" | "missing";

export const DEFAULT_CONTEXT = 16384;

/** 보내기 전에 모델 상태(LoadPlan)를 보고 다음에 할 일을 정한다. */
export function nextSendStep(plan: LoadPlan): SendStep {
  switch (plan.kind) {
    case "ready":
      return "send";
    case "smallerLoaded":
      return "send-with-reload-hint";
    case "notLoaded":
      return plan.others.length > 0 ? "askUnload" : "load";
    case "missing":
      return "missing";
  }
}

/** 로드할 컨텍스트: 모델별 설정(1024 미만 무시) → 16384, 모델 최대를 넘지 않는다. 백엔드 wanted_context와 같다. */
export function wantedContext(settings: AppSettings, model: Pick<LmModel, "key" | "maxContextLength">): number {
  const saved = settings.modelPrefs?.[model.key]?.contextLength;
  const want = saved !== undefined && saved >= MIN_CONTEXT ? saved : DEFAULT_CONTEXT;
  return model.maxContextLength > 0 ? Math.min(want, model.maxContextLength) : want;
}

export const MODEL_MISSING_TEXT = "모델을 찾을 수 없습니다. 다른 모델을 고르세요.";

/** 백엔드 `providers::server_off_message` 앞부분. 이 문구가 든 오류 옆에 [서버 시작]을 둔다. */
export const SERVER_OFF_TEXT = "LM Studio 서버가 꺼져 있습니다";
const SERVER_OFF_MARKERS = [
  SERVER_OFF_TEXT,
  "Ollama 서버가 꺼져 있습니다",
  "OpenAI 호환 서버가 꺼져 있습니다",
];

export function isServerOffMessage(text: string | null | undefined): boolean {
  return !!text && SERVER_OFF_MARKERS.some((m) => text.includes(m));
}

export function canStartServerForError(text: string | null | undefined, provider: LlmProvider): boolean {
  return PROVIDER_META[provider].supportsStartServer && isServerOffMessage(text);
}

/** 작게 다시 시도할 때의 바닥값. 최소 컨텍스트(MIN_CONTEXT 1024)보다 크지만 실제로 쓸 만한 가장 작은 값. */
export const RETRY_CONTEXT_FLOOR = 4096;

/**
 * 로드 실패(대개 VRAM 부족) 뒤 제안할 더 작은 컨텍스트: 시도한 값의 절반(1024 단위로 내림), 바닥 4096.
 * 시도한 값이 이미 4096 이하라 더 줄일 수 없으면 null(버튼을 보이지 않는다).
 */
export function smallerContext(attempted: number): number | null {
  if (!Number.isFinite(attempted) || attempted <= 0) return null;
  const half = Math.floor(attempted / 2 / 1024) * 1024;
  const n = Math.max(RETRY_CONTEXT_FLOOR, half);
  return n < attempted ? n : null;
}

/** "16K로 다시 시도" 버튼 글자 */
export function smallerContextLabel(n: number): string {
  return `${Math.round(n / 1024)}K로 다시 시도`;
}

/**
 * 대화에 저장된 모델 key가 지금 공급자에서 고른 것이면 그 key, 아니면 빈 문자열(기본 모델을 따른다).
 * 공급자 표시가 없는 예전 대화는 LM Studio에서 고른 것으로 본다(백엔드 `Conversation::model_key_for`와 같다).
 */
export function conversationModelKey(
  conv: Pick<Conversation, "modelKey" | "modelProvider"> | null | undefined,
  provider: LlmProvider
): string {
  const key = conv?.modelKey?.trim() ?? "";
  return key && (conv?.modelProvider ?? "lmstudio") === provider ? key : "";
}

/** 받은 모델 목록과 그 목록을 준 공급자. 다른 공급자의 목록은 쓰지 않는다. */
export interface ModelList {
  provider: LlmProvider | null;
  models: LmModel[];
}

export const EMPTY_MODEL_LIST: ModelList = { provider: null, models: [] };

/** 지금 공급자의 모델 목록. 공급자를 바꾼 뒤 새 목록을 받기 전에는 비어 있다. */
export function modelsFor(list: ModelList, provider: LlmProvider): LmModel[] {
  return list.provider === provider ? list.models : [];
}

/**
 * 목록 새로 고침 결과를 반영한다. `fetched`가 null이면 실패(서버 꺼짐 등):
 * 같은 공급자의 목록은 그대로 두고(잠깐 끊겨도 선택이 사라지지 않게), 다른 공급자의 목록은 지운다.
 */
export function modelListAfterRefresh(list: ModelList, provider: LlmProvider, fetched: LmModel[] | null): ModelList {
  if (fetched) return { provider, models: fetched };
  return list.provider === provider ? list : { provider, models: [] };
}
