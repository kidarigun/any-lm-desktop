import { X } from "lucide-react";

export type LoadChoice = "unload" | "try" | "cancel";

interface Props {
  /** 지금 로드돼 있는 모델 이름들 */
  loadedNames: string[];
  /** 로드하려는 모델 이름 */
  wantedName: string;
  onChoose: (choice: LoadChoice) => void;
}

/** 다른 모델이 로드돼 있을 때 어떻게 할지 묻는다. */
export function LoadChoiceDialog({ loadedNames, wantedName, onChoose }: Props) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
      <div
        role="dialog"
        aria-label="모델 로드"
        className="bg-white dark:bg-[#262624] border border-stone-200 dark:border-[#3A3A36] rounded-2xl w-full max-w-md p-6 shadow-xl space-y-4"
      >
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-bold text-stone-900 dark:text-stone-100">모델 로드</h3>
          <button onClick={() => onChoose("cancel")} aria-label="닫기">
            <X className="w-4 h-4 text-stone-400" />
          </button>
        </div>
        <p className="text-sm text-stone-700 dark:text-stone-200">
          지금 로드된 {loadedNames.join(", ")}를 내리고 {wantedName}를 로드할까요?
        </p>
        <p className="text-xs text-stone-500 dark:text-stone-400">
          메모리가 모자라면 그대로 로드가 실패할 수 있습니다.
        </p>
        <div className="flex justify-end gap-2 text-xs">
          <button
            onClick={() => onChoose("cancel")}
            className="px-3 py-2 rounded-lg border border-stone-300 dark:border-[#3A3A36] text-stone-700 dark:text-stone-200 cursor-pointer"
          >
            취소
          </button>
          <button
            onClick={() => onChoose("try")}
            className="px-3 py-2 rounded-lg border border-stone-300 dark:border-[#3A3A36] text-stone-700 dark:text-stone-200 cursor-pointer"
          >
            그대로 로드 시도
          </button>
          <button
            autoFocus
            onClick={() => onChoose("unload")}
            className="px-3 py-2 rounded-lg bg-[#A50034] text-white font-semibold cursor-pointer"
          >
            내리고 로드
          </button>
        </div>
      </div>
    </div>
  );
}
