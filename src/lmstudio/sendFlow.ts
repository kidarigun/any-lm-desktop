import type { AppSettings, LmModel, LoadPlan } from "../types";
import { MIN_CONTEXT } from "./modelText";

export type SendStep = "send" | "load" | "askUnload" | "send-with-reload-hint" | "missing";

export const DEFAULT_CONTEXT = 32768;

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

/** 로드할 컨텍스트: 모델별 설정(1024 미만 무시) → 32768, 모델 최대를 넘지 않는다. 백엔드 wanted_context와 같다. */
export function wantedContext(settings: AppSettings, model: Pick<LmModel, "key" | "maxContextLength">): number {
  const saved = settings.modelPrefs?.[model.key]?.contextLength;
  const want = saved !== undefined && saved >= MIN_CONTEXT ? saved : DEFAULT_CONTEXT;
  return model.maxContextLength > 0 ? Math.min(want, model.maxContextLength) : want;
}

export const MODEL_MISSING_TEXT = "모델을 찾을 수 없습니다. 다른 모델을 고르세요.";
