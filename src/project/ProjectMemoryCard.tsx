import { useState } from "react";
import { BackendAPI } from "../api";
import type { Project } from "../types";
import { MEMORY_ITEM_MAX_CHARS, MEMORY_MAX_ITEMS, memorySourceLabel, memoryStats, newestFirst, validateMemoryText } from "./memoryFormat";

/** 프로젝트 화면의 "프로젝트 메모리" 카드: 모델이 remember로 저장한 것과 직접 쓴 것을 함께 보여 주고 고친다. */
export function ProjectMemoryCard({ project, onChange }: { project: Project; onChange: (p: Project) => void }) {
  const items = project.memory ?? [];
  const stats = memoryStats(items);
  const [adding, setAdding] = useState(false);
  const [draft, setDraft] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const run = async (job: () => Promise<Project>, done: () => void) => {
    setBusy(true);
    setError(null);
    try {
      onChange(await job());
      done();
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  };

  const submit = () => {
    const problem = validateMemoryText(draft);
    if (problem) {
      setError(problem);
      return;
    }
    if (editingId) {
      run(() => BackendAPI.updateProjectMemory(project.id, editingId, draft), cancel);
    } else {
      run(() => BackendAPI.addProjectMemory(project.id, draft), cancel);
    }
  };

  const cancel = () => {
    setAdding(false);
    setEditingId(null);
    setDraft("");
    setError(null);
  };

  const editor = (
    <div className="space-y-1.5">
      <textarea
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        autoFocus
        rows={2}
        placeholder="기억할 내용 (한두 문장)"
        className="w-full text-xs rounded-lg border border-stone-300 dark:border-[#454540] bg-white dark:bg-[#262624] p-2 focus:outline-none"
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
            e.preventDefault();
            submit();
          } else if (e.key === "Escape") {
            cancel();
          }
        }}
      />
      <div className="flex items-center justify-between text-[11px] text-stone-500 dark:text-stone-400">
        <span>
          {[...draft].length}/{MEMORY_ITEM_MAX_CHARS}자
        </span>
        <span className="flex gap-2">
          <button onClick={cancel} disabled={busy} className="hover:underline cursor-pointer">
            취소
          </button>
          <button onClick={submit} disabled={busy} className="text-[#A50034] dark:text-[#FF5C8D] font-semibold hover:underline cursor-pointer">
            저장
          </button>
        </span>
      </div>
    </div>
  );

  return (
    <div className="p-4 rounded-xl bg-[#FAF9F5] dark:bg-[#1E1E1C] border border-stone-200/80 dark:border-[#343430] space-y-2">
      <div className="flex items-center justify-between">
        <span className="text-xs font-bold text-stone-700 dark:text-stone-300">
          프로젝트 메모리
          <span className="ml-2 font-normal text-stone-500 dark:text-stone-400">
            {stats.count}/{MEMORY_MAX_ITEMS}개 · {stats.chars.toLocaleString()}자
          </span>
        </span>
        {!adding && !editingId && (
          <button
            onClick={() => {
              setAdding(true);
              setDraft("");
              setError(null);
            }}
            className="text-xs text-[#A50034] dark:text-[#FF5C8D] font-semibold hover:underline cursor-pointer"
          >
            추가
          </button>
        )}
      </div>
      <p className="text-xs text-stone-500 dark:text-stone-400 leading-relaxed">
        이 프로젝트의 모든 대화에 들어갑니다. 모델이 기억할 만한 것을 알게 되면 스스로 저장하기도 합니다.
      </p>
      {adding && editor}
      {items.length === 0 && !adding ? (
        <p className="text-xs text-stone-500 dark:text-stone-400">저장된 메모리가 없습니다.</p>
      ) : (
        <ul className="space-y-1.5">
          {newestFirst(items).map((m) => (
            <li key={m.id} className="text-xs text-stone-700 dark:text-stone-300">
              {editingId === m.id ? (
                editor
              ) : (
                <div className="flex items-start gap-2">
                  <span
                    className={`shrink-0 mt-0.5 px-1.5 rounded text-[10px] font-semibold ${
                      m.source === "model"
                        ? "bg-sky-100 text-sky-800 dark:bg-sky-900/40 dark:text-sky-200"
                        : "bg-stone-200 text-stone-700 dark:bg-stone-700 dark:text-stone-200"
                    }`}
                  >
                    {memorySourceLabel(m.source)}
                  </span>
                  <span className="flex-1 whitespace-pre-wrap break-words leading-relaxed">{m.text}</span>
                  <span className="shrink-0 flex gap-2 text-stone-500 dark:text-stone-400">
                    <button
                      onClick={() => {
                        setEditingId(m.id);
                        setAdding(false);
                        setDraft(m.text);
                        setError(null);
                      }}
                      disabled={busy}
                      className="hover:underline cursor-pointer"
                    >
                      편집
                    </button>
                    <button
                      onClick={() => {
                        if (window.confirm("이 메모리를 지울까요?")) {
                          run(() => BackendAPI.deleteProjectMemory(project.id, m.id), () => {});
                        }
                      }}
                      disabled={busy}
                      className="hover:underline cursor-pointer"
                    >
                      삭제
                    </button>
                  </span>
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
      {error && <p className="text-xs text-red-600 dark:text-red-400">{error}</p>}
    </div>
  );
}
