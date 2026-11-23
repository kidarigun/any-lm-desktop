import { useEffect, useMemo, useRef, useState } from "react";
import { X } from "lucide-react";
import { BackendAPI } from "../api";
import type { Project } from "../types";
import { INSTRUCTION_TEMPLATES, formatInstructionsStats, insertTemplate, instructionsStats } from "./instructionsFormat";

/** 프로젝트 지침 전용 편집기(큰 모달). 저장은 기존 create_or_update_project 경로. */
export function InstructionsEditor({
  project,
  contextLength,
  onSaved,
  onClose,
}: {
  project: Project;
  /** 지금 로드된 컨텍스트 길이. 모르면 null(토큰만 표시) */
  contextLength: number | null;
  onSaved: (p: Project) => void;
  onClose: () => void;
}) {
  const [text, setText] = useState(project.instructions ?? "");
  const [menuOpen, setMenuOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const areaRef = useRef<HTMLTextAreaElement>(null);
  const dirty = text !== (project.instructions ?? "");
  const stats = useMemo(() => instructionsStats(text, contextLength), [text, contextLength]);

  const requestClose = () => {
    if (busy) return;
    if (dirty && !window.confirm("저장하지 않은 변경이 있습니다. 취소하고 닫을까요?")) return;
    onClose();
  };

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      const saved = await BackendAPI.createOrUpdateProject({
        id: project.id,
        name: project.name,
        rootPath: project.rootPath,
        instructions: text,
      });
      onSaved(saved);
      onClose();
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
      setBusy(false);
    }
  };

  useEffect(() => {
    areaRef.current?.focus();
  }, []);

  // Esc는 포커스가 창 밖(예: 바탕을 누른 뒤)에 있어도 듣는다. 열려 있는 동안만 등록하고 닫히면 뗀다.
  const escRef = useRef<() => void>(() => {});
  escRef.current = () => {
    if (menuOpen) setMenuOpen(false);
    else requestClose();
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.isComposing) return;
      e.preventDefault();
      e.stopPropagation();
      escRef.current();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="instructions-editor-title"
        className="bg-white dark:bg-[#262624] border border-stone-200 dark:border-[#3A3A36] rounded-2xl w-full max-w-2xl p-6 shadow-xl space-y-3"
      >
        <div className="flex items-center justify-between">
          <h3 id="instructions-editor-title" className="text-sm font-bold text-stone-900 dark:text-stone-100">프로젝트 지침 · {project.name}</h3>
          <button onClick={requestClose} aria-label="닫기">
            <X className="w-4 h-4 text-stone-400" />
          </button>
        </div>

        <div className="relative flex items-center justify-between gap-2 text-xs">
          <button
            type="button"
            onClick={() => setMenuOpen((v) => !v)}
            className="px-3 py-1.5 rounded-lg bg-stone-100 dark:bg-[#32322F] hover:bg-stone-200 dark:hover:bg-[#3C3C38] text-stone-800 dark:text-stone-200 font-semibold cursor-pointer"
          >
            서식 예시 ▾
          </button>
          {menuOpen && (
            <div className="absolute left-0 top-9 z-10 w-52 rounded-xl border border-stone-200 dark:border-[#3A3A36] bg-white dark:bg-[#2C2C28] shadow-lg py-1">
              {INSTRUCTION_TEMPLATES.map((t) => (
                <button
                  key={t.id}
                  type="button"
                  onClick={() => {
                    setText((cur) => insertTemplate(cur, t.text));
                    setMenuOpen(false);
                    areaRef.current?.focus();
                  }}
                  className="w-full text-left px-3 py-1.5 hover:bg-stone-100 dark:hover:bg-[#383833] text-stone-800 dark:text-stone-200 cursor-pointer"
                >
                  {t.label}
                </button>
              ))}
              <div className="px-3 py-1 text-[11px] text-stone-400">비어 있으면 넣고, 내용이 있으면 끝에 덧붙입니다.</div>
            </div>
          )}
          <span
            className={
              stats.warn
                ? "font-semibold text-amber-600 dark:text-amber-400"
                : "text-stone-500 dark:text-stone-400"
            }
          >
            {formatInstructionsStats(stats)}
            {stats.warn && " — 지침이 길면 대화에 쓸 공간이 줄어듭니다"}
          </span>
        </div>

        <textarea
          ref={areaRef}
          rows={14}
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="이 프로젝트에서 모델이 항상 따를 지침이나 역할을 쓰세요. [서식 예시]에서 시작할 수 있습니다."
          className="w-full min-h-[22rem] resize-y text-xs leading-relaxed bg-white dark:bg-[#1E1E1C] border border-stone-300 dark:border-[#3A3A36] rounded-xl p-3 text-stone-900 dark:text-stone-100 focus:outline-none focus:border-[#A50034]"
        />

        {error && <div className="text-xs text-red-600 dark:text-red-400">저장하지 못했습니다: {error}</div>}

        <div className="flex justify-end gap-2 pt-1">
          <button
            onClick={requestClose}
            className="px-4 py-2 rounded-xl bg-stone-100 dark:bg-[#32322F] text-stone-700 dark:text-stone-200 text-xs font-semibold cursor-pointer"
          >
            취소
          </button>
          <button
            onClick={save}
            disabled={busy || !dirty}
            className="px-4 py-2 rounded-xl bg-[#A50034] disabled:opacity-40 text-white text-xs font-bold cursor-pointer"
          >
            {busy ? "저장 중…" : "저장"}
          </button>
        </div>
      </div>
    </div>
  );
}
