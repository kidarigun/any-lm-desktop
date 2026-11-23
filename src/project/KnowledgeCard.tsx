import { useEffect, useState } from "react";
import { BackendAPI } from "../api";
import type { KnowledgeStatus, Project } from "../types";
import { folderLine, knowledgeStatusLine } from "./knowledgeFormat";

/**
 * 프로젝트 화면의 "지식 폴더" 카드: 참고 자료 폴더를 더하고 빼고, 색인 상태를 보여 준다.
 * 색인은 백엔드가 뒤에서 하고 `anylm-knowledge-status` 이벤트로 진행을 알린다(화면은 멈추지 않는다).
 */
export function KnowledgeCard({ project, onChange }: { project: Project; onChange: (p: Project) => void }) {
  const folders = project.knowledgeFolders ?? [];
  const [status, setStatus] = useState<KnowledgeStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [showSkipped, setShowSkipped] = useState(false);
  const folderKey = folders.join("\n");

  useEffect(() => {
    let alive = true;
    BackendAPI.knowledgeStatus(project.id)
      .then((s) => alive && setStatus(s))
      .catch((e) => alive && setError(String(e instanceof Error ? e.message : e)));
    return () => {
      alive = false;
    };
  }, [project.id, folderKey]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    BackendAPI.onKnowledgeStatus((pid, s) => {
      if (pid === project.id) setStatus(s);
    }).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [project.id]);

  const run = async (job: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await job();
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  };

  const addFolder = () =>
    run(async () => {
      const picked = await BackendAPI.pickFolderDialog(project.rootPath);
      if (!picked) return;
      onChange(await BackendAPI.addKnowledgeFolder(project.id, picked));
    });

  const removeFolder = (path: string) => {
    if (!window.confirm(`이 지식 폴더를 목록에서 뺄까요?\n${path}\n(폴더 안의 파일은 지우지 않습니다.)`)) return;
    run(async () => {
      onChange(await BackendAPI.removeKnowledgeFolder(project.id, path));
    });
  };

  const reindex = () => run(() => BackendAPI.reindexKnowledge(project.id));
  const indexing = !!status?.indexing;
  const folderStatus = (path: string) => status?.folders.find((f) => f.path === path);

  return (
    <div className="p-4 rounded-xl bg-[#FAF9F5] dark:bg-[#1E1E1C] border border-stone-200/80 dark:border-[#343430] space-y-2">
      <div className="flex items-center justify-between">
        <span className="text-xs font-bold text-stone-700 dark:text-stone-300">지식 폴더</span>
        <span className="flex gap-3">
          {folders.length > 0 && (
            <button
              onClick={reindex}
              disabled={busy || indexing}
              className="text-xs text-[#A50034] dark:text-[#FF5C8D] font-semibold hover:underline cursor-pointer disabled:opacity-50 disabled:cursor-default"
            >
              다시 색인
            </button>
          )}
          <button
            onClick={addFolder}
            disabled={busy}
            className="text-xs text-[#A50034] dark:text-[#FF5C8D] font-semibold hover:underline cursor-pointer disabled:opacity-50"
          >
            폴더 추가
          </button>
        </span>
      </div>
      <p className="text-xs text-stone-500 dark:text-stone-400 leading-relaxed">
        폴더의 문서를 조각내 색인해 두고, 질문할 때마다 관련 있는 부분을 찾아 모델에게 함께 보냅니다. 모델이 직접 더 찾아보기도
        합니다. 앱은 이 폴더를 읽기만 하고 아무것도 쓰지 않습니다.
      </p>
      {folders.length === 0 ? (
        <p className="text-xs text-stone-500 dark:text-stone-400">연결된 지식 폴더가 없습니다.</p>
      ) : (
        <ul className="space-y-1.5">
          {folders.map((path) => {
            const fs = folderStatus(path);
            return (
              <li key={path} className="flex items-start gap-2 text-xs">
                <div className="flex-1 min-w-0">
                  <div className="font-mono text-stone-700 dark:text-stone-300 break-all">{path}</div>
                  <div className={fs?.missing ? "text-amber-600 dark:text-amber-400" : "text-stone-500 dark:text-stone-400"}>
                    {folderLine(fs)}
                  </div>
                </div>
                <button
                  onClick={() => removeFolder(path)}
                  disabled={busy}
                  className="shrink-0 text-stone-500 dark:text-stone-400 hover:underline cursor-pointer"
                >
                  제거
                </button>
              </li>
            );
          })}
        </ul>
      )}
      {folders.length > 0 && status && (
        <p className={`text-[11px] ${indexing ? "text-sky-700 dark:text-sky-300" : "text-stone-500 dark:text-stone-400"}`}>
          {knowledgeStatusLine(status)}
          {status.skipped > 0 && status.skippedSamples.length > 0 && (
            <button onClick={() => setShowSkipped((v) => !v)} className="ml-2 hover:underline cursor-pointer">
              {showSkipped ? "건너뛴 파일 숨기기" : "건너뛴 파일 보기"}
            </button>
          )}
        </p>
      )}
      {showSkipped && status && (
        <ul className="text-[11px] text-stone-500 dark:text-stone-400 space-y-0.5 max-h-40 overflow-y-auto">
          {status.skippedSamples.map((s) => (
            <li key={s.path} className="break-all">
              <span className="font-mono">{s.path}</span> — {s.reason}
            </li>
          ))}
          {status.skipped > status.skippedSamples.length && <li>외 {status.skipped - status.skippedSamples.length}개</li>}
        </ul>
      )}
      {status?.error && <p className="text-xs text-red-600 dark:text-red-400">색인 오류: {status.error}</p>}
      {error && <p className="text-xs text-red-600 dark:text-red-400">{error}</p>}
    </div>
  );
}
