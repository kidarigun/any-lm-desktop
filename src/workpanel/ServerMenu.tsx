import { useEffect, useState, type FormEvent } from "react";
import { ChevronDown, Play, X } from "lucide-react";
import { DevServerAPI, type DevServerConfig } from "./devServerApi";
import type { WorkPanelController } from "./useWorkPanel";

const blankServer: DevServerConfig = { id: "", name: "", command: "", cwd: "", url: "" };
const field = "min-w-0 rounded border border-stone-300 bg-white px-2 py-1 text-xs dark:border-stone-600 dark:bg-stone-800";

export function ServerMenu({ panel }: { panel: WorkPanelController }) {
  const { projectId } = panel;
  const [open, setOpen] = useState(false);
  const [options, setOptions] = useState<DevServerConfig[] | null>(null);
  const [config, setConfig] = useState<DevServerConfig | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!open || !projectId) return;
    let current = true;
    setOptions(null);
    setConfig(null);
    setError("");
    void DevServerAPI.options(projectId).then((items) => {
      if (current) setOptions(items);
    }).catch((e) => {
      if (current) { setOptions([]); setError(String(e)); }
    });
    return () => { current = false; };
  }, [open, projectId]);

  async function start(next: DevServerConfig) {
    setBusy(true);
    setError("");
    try {
      await panel.startServer(next);
      setOpen(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (config && !busy) void start(config);
  }

  if (!projectId) return null;
  return (
    <div className="relative shrink-0">
      <button type="button" title="서버 실행" aria-expanded={open} onClick={() => setOpen((v) => !v)}
        className="flex h-7 items-center gap-1 rounded-md px-1.5 text-xs font-semibold hover:bg-stone-100 dark:hover:bg-stone-700">
        <Play size={13} /> 서버 실행 <ChevronDown size={12} />
      </button>
      {open && (
        <div className="absolute right-0 top-full z-20 mt-2 w-64 rounded-lg border border-stone-200 bg-white p-2 shadow-lg dark:border-stone-700 dark:bg-[#262624]">
          <div className="mb-1 flex justify-between gap-2">
            <label className="flex items-center gap-1 text-xs">
              <input type="checkbox" checked={panel.autoPreview} onChange={(e) => panel.setAutoPreview(e.target.checked)} />
              미리보기 자동 열기
            </label>
            <button type="button" aria-label="서버 메뉴 닫기" onClick={() => setOpen(false)}><X size={14} /></button>
          </div>
          {options === null && <p className="px-1 py-2 text-xs">서버 설정을 찾는 중…</p>}
          <div className="max-h-56 overflow-auto">
            {options?.map((option) => (
              <button type="button" disabled={busy} key={option.id} onClick={() => void start(option)}
                className="block w-full rounded px-2 py-1.5 text-left text-xs hover:bg-stone-100 dark:hover:bg-stone-700">
                <span className="block font-semibold">{option.name}</span>
                <span className="block truncate text-stone-500 dark:text-stone-400">{option.command}</span>
              </button>
            ))}
          </div>
          {!config && <button type="button" onClick={() => setConfig(blankServer)} className="rounded px-2 py-1 text-xs hover:bg-stone-100 dark:hover:bg-stone-700">직접 입력…</button>}
          {config && (
            <form onSubmit={submit} className="grid gap-2 pt-2">
              <input required aria-label="서버 이름" placeholder="서버 이름" value={config.name} className={field}
                onChange={(e) => setConfig({ ...config, name: e.target.value })} />
              <input required aria-label="실행 명령" placeholder="실행 명령 (예: npm run dev)" value={config.command} className={field}
                onChange={(e) => setConfig({ ...config, command: e.target.value })} />
              <input aria-label="프로젝트 기준 폴더" placeholder="프로젝트 기준 폴더 (선택)" value={config.cwd} className={field}
                onChange={(e) => setConfig({ ...config, cwd: e.target.value })} />
              <input aria-label="미리보기 주소(선택)" placeholder="로컬 주소 (선택)" value={config.url ?? ""} className={field}
                onChange={(e) => setConfig({ ...config, url: e.target.value })} />
              <button type="submit" disabled={busy} className="rounded bg-stone-800 px-2 py-1 text-xs text-white dark:bg-stone-600">저장하고 실행</button>
            </form>
          )}
          {error && <p role="alert" className="break-words px-1 py-2 text-xs text-red-600 dark:text-red-300">{error}</p>}
        </div>
      )}
    </div>
  );
}
