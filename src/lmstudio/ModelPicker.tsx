import { AlertTriangle } from "lucide-react";
import type { LmModel } from "../types";
import { loadedLabel } from "./modelText";

interface Props {
  models: LmModel[];
  /** 지금 이 대화에서 쓸 모델 key (비어 있으면 아직 고르지 않음) */
  value: string;
  onChange: (key: string) => void;
  disabled?: boolean;
  /** 프로젝트·코드 화면인지 (도구 사용 경고 대상) */
  needsTools?: boolean;
  /** 공급자가 모델 로드/언로드 구분을 지원하는지 (기본 true) */
  supportsLoad?: boolean;
  onOpen?: () => void;
}

export const NO_TOOLS_WARNING =
  "이 모델은 도구 사용 학습이 안 돼 있어 파일 작업이 불안정할 수 있습니다";

/** 대화 입력창 아래의 모델 선택. 로드 지원 공급자는 로드된 모델이 위에 온다. */
export function ModelPicker({
  models,
  value,
  onChange,
  disabled,
  needsTools,
  supportsLoad = true,
  onOpen,
}: Props) {
  const loaded = supportsLoad ? models.filter((m) => m.loaded.length > 0) : [];
  const others = supportsLoad ? models.filter((m) => m.loaded.length === 0) : models;
  const current = models.find((m) => m.key === value);
  const stale = value !== "" && !current;
  const warn = needsTools && current && !current.toolUse;

  const option = (m: LmModel) => (
    <option key={m.key} value={m.key}>
      {supportsLoad ? `${m.displayName} · ${loadedLabel(m)}` : m.displayName}
    </option>
  );

  return (
    <div className="flex items-center gap-1 pl-1.5 ml-0.5 border-l border-stone-200 dark:border-[#383834]">
      <span className="px-1 text-[11px] text-stone-400 dark:text-stone-500 select-none">모델</span>
      <select
        aria-label="이 대화에서 쓸 모델"
        value={value}
        disabled={disabled}
        onFocus={onOpen}
        onMouseDown={onOpen}
        onChange={(e) => onChange(e.target.value)}
        className="max-w-[220px] truncate bg-transparent text-xs font-medium text-stone-700 dark:text-stone-200 rounded-md px-1 py-1 cursor-pointer hover:bg-stone-100 dark:hover:bg-[#32322F] focus:outline-none disabled:opacity-40"
      >
        {value === "" && <option value="">모델 고르기</option>}
        {stale && <option value={value}>{value} · 찾을 수 없음</option>}
        {supportsLoad ? (
          <>
            {loaded.length > 0 && <optgroup label="로드됨">{loaded.map(option)}</optgroup>}
            {others.length > 0 && <optgroup label="로드 안 됨">{others.map(option)}</optgroup>}
          </>
        ) : (
          others.map(option)
        )}
      </select>
      {warn && (
        <span
          title={NO_TOOLS_WARNING}
          className="flex items-center gap-1 text-[11px] text-amber-700 dark:text-amber-300"
        >
          <AlertTriangle className="w-3 h-3" />
          도구 사용 미지원
        </span>
      )}
    </div>
  );
}

interface ToggleProps {
  model: LmModel | undefined;
  enabled: boolean;
  onChange: (value: "on" | "off") => void;
  disabled?: boolean;
}

/** 모델 선택 옆의 "생각하기" 켜기/끄기. 추론 선택지가 있는 모델에서만 보인다. */
export function ReasoningToggle({ model, enabled, onChange, disabled }: ToggleProps) {
  if (!model || model.reasoningOptions.length === 0) return null;
  return (
    <label
      title="끄면 답변 전에 길게 생각하지 않아 더 빨리 답합니다"
      className="flex items-center gap-1 pl-1.5 text-[11px] text-stone-500 dark:text-stone-400 cursor-pointer select-none"
    >
      <input
        type="checkbox"
        checked={enabled}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked ? "on" : "off")}
        className="accent-stone-600"
      />
      생각하기
    </label>
  );
}
