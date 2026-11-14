import { useCallback, useEffect, useRef, useState } from "react";
import { AlertTriangle, Check, Download, Loader2, Trash2 } from "lucide-react";
import { BackendAPI } from "../api";
import type { RuntimeInfo, RuntimeKind } from "../types";

const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** 실행 환경 상태. 설치 중이면 자주 다시 읽는다. */
export function useRuntimes() {
  const [list, setList] = useState<RuntimeInfo[] | null>(null);
  const alive = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const l = await BackendAPI.runtimeStatus();
      if (alive.current) setList(l);
      return l;
    } catch {
      return null;
    }
  }, []);

  useEffect(() => {
    alive.current = true;
    void refresh();
    return () => {
      alive.current = false;
    };
  }, [refresh]);

  const running = (list ?? []).some((r) => r.job.running);
  useEffect(() => {
    if (!running) return;
    const t = window.setInterval(refresh, 700);
    return () => window.clearInterval(t);
  }, [running, refresh]);

  return { list, refresh };
}

/** 설치 진행 표시 */
function JobLine({ info }: { info: RuntimeInfo }) {
  const { job } = info;
  if (job.running)
    return (
      <div className="space-y-1">
        <div className="flex items-center gap-1.5 text-[11px] text-stone-600 dark:text-stone-300">
          <Loader2 className="w-3 h-3 animate-spin" /> {job.message}
        </div>
        <div className="h-1 rounded bg-stone-200 dark:bg-[#34342F] overflow-hidden">
          <div
            className={`h-full bg-[#A50034] transition-all ${job.percent == null ? "w-1/3 animate-pulse" : ""}`}
            style={job.percent != null ? { width: `${job.percent}%` } : undefined}
          />
        </div>
      </div>
    );
  if (job.error)
    return (
      <div className="flex items-start gap-1 text-[11px] text-red-700 dark:text-red-400 whitespace-pre-wrap break-words">
        <AlertTriangle className="w-3 h-3 mt-0.5 shrink-0" /> {job.error}
      </div>
    );
  return null;
}

/**
 * 실행 환경 하나를 설치하는 버튼 (찾아보기·Python 실행 창에서 쓴다).
 * 설치하는 동안 진행 상황을 보여 주고, 끝나면 onDone을 부른다.
 */
export function RuntimeInstallButton({ kind, label, onDone }: { kind: RuntimeKind; label?: string; onDone?: () => void }) {
  const { list, refresh } = useRuntimes();
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const info = list?.find((r) => r.id === kind);

  const install = async () => {
    setBusy(true);
    setError("");
    // 설치는 오래 걸리므로 진행 상황을 따로 읽는다.
    const t = window.setInterval(refresh, 700);
    try {
      // 설치는 오래 걸리므로 시작 직후 한 번 읽어 진행 표시를 바로 띄운다.
      const started = BackendAPI.runtimeInstall(kind);
      window.setTimeout(() => void refresh(), 150);
      await started;
      await refresh();
      onDone?.();
    } catch (e) {
      setError(errorText(e));
    } finally {
      window.clearInterval(t);
      setBusy(false);
      void refresh();
    }
  };

  if (info?.job.running || busy) {
    return <div className="min-w-[200px]">{info ? <JobLine info={{ ...info, job: { ...info.job, running: true, message: info.job.message || "준비 중..." } }} /> : <Loader2 className="w-3.5 h-3.5 animate-spin" />}</div>;
  }
  return (
    <div className="space-y-1">
      <button
        onClick={install}
        className="px-2.5 py-1 rounded-lg bg-[#A50034] hover:bg-[#8A002B] text-white text-[11px] font-semibold flex items-center gap-1 cursor-pointer"
        title={info?.sizeHint}
      >
        <Download className="w-3 h-3" /> {label ?? `${info?.label ?? kind} 자동 설치`}
      </button>
      {error && <div className="text-[11px] text-red-700 dark:text-red-400 whitespace-pre-wrap">{error}</div>}
    </div>
  );
}

/** 설정 창의 '실행 환경' 칸: Python·Node.js·uv 상태와 설치·삭제 */
export function RuntimePanel() {
  const { list, refresh } = useRuntimes();
  const [error, setError] = useState("");

  const install = async (kind: RuntimeKind) => {
    setError("");
    const t = window.setInterval(refresh, 700);
    try {
      // 설치는 오래 걸리므로 시작 직후 한 번 읽어 진행 표시를 바로 띄운다.
      const started = BackendAPI.runtimeInstall(kind);
      window.setTimeout(() => void refresh(), 150);
      await started;
    } catch {
      // 오류는 각 줄의 job.error로 보인다
    } finally {
      window.clearInterval(t);
      void refresh();
    }
  };
  const remove = async (r: RuntimeInfo) => {
    if (!window.confirm(`앱이 설치한 ${r.label}을(를) 지울까요? 필요하면 다시 설치할 수 있습니다.`)) return;
    setError("");
    try {
      await BackendAPI.runtimeRemove(r.id);
    } catch (e) {
      setError(errorText(e));
    } finally {
      void refresh();
    }
  };

  return (
    <div className="space-y-2" data-testid="runtime-panel">
      <p className="text-[11px] text-stone-500 dark:text-stone-400 leading-relaxed">
        Python 스크립트와 일부 MCP 서버를 실행하는 데 필요한 프로그램입니다. [설치]를 누르면 공식 배포본을 내려받아(파일 검증 후) 이 앱 전용으로 설치합니다. PC의 다른 프로그램에는 영향을 주지 않습니다.
      </p>
      {list === null ? (
        <div className="text-[11px] text-stone-500">확인하는 중...</div>
      ) : (
        <ul className="space-y-1.5">
          {list.map((r) => (
            <li key={r.id} className="rounded-lg border border-stone-200 dark:border-[#383834] px-3 py-2 space-y-1">
              <div className="flex items-start gap-2">
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5 flex-wrap">
                    <span className="font-semibold text-stone-800 dark:text-stone-100">{r.label}</span>
                    {r.appInstalled ? (
                      <span className="flex items-center gap-0.5 text-[11px] text-emerald-700 dark:text-emerald-400">
                        <Check className="w-3 h-3" /> 앱에 설치됨 {r.version && `(${r.version})`}
                      </span>
                    ) : r.system ? (
                      <span className="text-[11px] text-emerald-700 dark:text-emerald-400">PC에 설치된 것을 사용</span>
                    ) : (
                      <span className="text-[11px] text-amber-700 dark:text-amber-400">없음</span>
                    )}
                  </div>
                  <div className="text-[11px] text-stone-500 dark:text-stone-400">{r.purpose}</div>
                </div>
                <div className="shrink-0 flex items-center gap-1">
                  {!r.job.running &&
                    (r.appInstalled ? (
                      <button onClick={() => remove(r)} title="앱이 설치한 것 지우기" className="p-1.5 rounded-lg hover:bg-stone-100 dark:hover:bg-[#2C2C29] text-stone-500 cursor-pointer">
                        <Trash2 className="w-3.5 h-3.5" />
                      </button>
                    ) : (
                      <button
                        onClick={() => install(r.id)}
                        title={r.sizeHint}
                        className="px-2.5 py-1 rounded-lg bg-[#A50034] hover:bg-[#8A002B] text-white text-[11px] font-semibold flex items-center gap-1 cursor-pointer"
                      >
                        <Download className="w-3 h-3" /> 설치
                      </button>
                    ))}
                </div>
              </div>
              {!r.appInstalled && !r.job.running && !r.job.error && <div className="text-[10px] text-stone-400">{r.sizeHint}</div>}
              <JobLine info={r} />
            </li>
          ))}
        </ul>
      )}
      {error && <div className="text-[11px] text-red-700 dark:text-red-400">{error}</div>}
    </div>
  );
}
