import { SquareTerminal } from "lucide-react";

export const WORK_PANEL_TOGGLE_TITLE = "터미널 (Ctrl+`)";

/** 상단 바의 작업 패널 열기/닫기 버튼 */
export function WorkPanelToggle({ open, onToggle }: { open: boolean; onToggle: () => void }) {
  return (
    <button
      type="button"
      onClick={onToggle}
      aria-pressed={open}
      aria-label="터미널"
      title={WORK_PANEL_TOGGLE_TITLE}
      className={`p-1.5 rounded-lg text-stone-700 dark:text-stone-200 cursor-pointer transition shrink-0 ${
        open
          ? "bg-[#A50034]/10 dark:bg-[#FF5C8D]/15 text-[#A50034] dark:text-[#FF5C8D]"
          : "bg-stone-200/80 dark:bg-[#2C2C28] hover:bg-stone-300 dark:hover:bg-[#383833]"
      }`}
    >
      <SquareTerminal className="w-4 h-4" />
    </button>
  );
}
