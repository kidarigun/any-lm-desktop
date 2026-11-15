import { useCallback, useEffect, useRef, useState } from "react";
import { Download, RefreshCw } from "lucide-react";
import { BackendAPI } from "../api";
import type { UpdateInfo } from "../types";

export type UpdateStatus = "idle" | "checking" | "latest" | "available" | "downloading" | "error";

export interface Updater {
  /** 지금 실행 중인 앱의 버전 (브라우저 미리보기에서는 빈 문자열) */
  version: string;
  status: UpdateStatus;
  info: UpdateInfo | null;
  /** 내려받은 비율 0~100 (전체 크기를 모르면 null) */
  percent: number | null;
  error: string | null;
  check: () => Promise<void>;
  install: () => Promise<void>;
}

/**
 * 새 버전 확인과 설치.
 * `autoCheck`가 true가 되면 한 번 조용히 확인한다 (실패해도 알리지 않는다). undefined는 설정을 아직 읽지 못한 상태.
 */
export function useUpdater(autoCheck: boolean | undefined): Updater {
  const [version, setVersion] = useState("");
  const [status, setStatus] = useState<UpdateStatus>("idle");
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [percent, setPercent] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const autoChecked = useRef(false);

  useEffect(() => {
    BackendAPI.appVersion()
      .then(setVersion)
      .catch(() => {});
  }, []);

  const run = useCallback(async (silent: boolean) => {
    setStatus("checking");
    setError(null);
    try {
      const found = await BackendAPI.checkUpdate();
      setInfo(found);
      setStatus(found ? "available" : silent ? "idle" : "latest");
    } catch (err) {
      // 자동 확인은 인터넷이 없거나 릴리스가 아직 없을 때 조용히 넘어간다.
      setStatus(silent ? "idle" : "error");
      if (!silent) setError(String(err));
    }
  }, []);

  useEffect(() => {
    if (autoCheck !== true || autoChecked.current) return;
    autoChecked.current = true;
    // 앱을 켠 직후의 일을 방해하지 않게 조금 뒤에 확인한다.
    const timer = setTimeout(() => void run(true), 4000);
    return () => clearTimeout(timer);
  }, [autoCheck, run]);

  const install = useCallback(async () => {
    setStatus("downloading");
    setPercent(null);
    setError(null);
    const unlisten = await BackendAPI.onUpdateProgress((p) => {
      setPercent(p.total ? Math.min(100, Math.round((p.downloaded / p.total) * 100)) : null);
    });
    try {
      // 성공하면 설치 프로그램이 앱을 닫으므로 이 뒤로는 돌아오지 않는다.
      await BackendAPI.installUpdate();
    } catch (err) {
      setStatus("error");
      setError(String(err));
    } finally {
      unlisten();
    }
  }, []);

  return { version, status, info, percent, error, check: () => run(false), install };
}

/** 설치 전에 한 번 물어본다: 앱이 닫히고, 진행 중인 답변은 중단된다. */
export function confirmInstall(info: UpdateInfo, runningCount: number): boolean {
  const busy =
    runningCount > 0
      ? `\n\n지금 답변을 받고 있는 대화가 ${runningCount}개 있습니다. 설치하면 그 답변은 받은 데까지만 남습니다.`
      : "";
  return window.confirm(
    `새 버전 ${info.version}을 내려받아 설치할까요?\n설치하는 동안 앱이 닫혔다가 새 버전으로 다시 열립니다.${busy}`
  );
}

/** 사이드바에 보여 주는 새 버전 알림 */
export function UpdateBanner({ updater, runningCount }: { updater: Updater; runningCount: number }) {
  const { status, info, percent, error } = updater;
  if (!info || (status !== "available" && status !== "downloading" && status !== "error")) return null;
  const downloading = status === "downloading";
  return (
    <div className="rounded-xl border border-[#A50034]/30 dark:border-[#FF5C8D]/30 bg-[#A50034]/5 dark:bg-[#FF5C8D]/10 px-3 py-2 text-xs space-y-1.5">
      <div className="flex items-center justify-between gap-2">
        <span className="font-semibold text-stone-800 dark:text-stone-100">새 버전 {info.version}</span>
        <button
          disabled={downloading}
          onClick={() => confirmInstall(info, runningCount) && void updater.install()}
          className="px-2 py-1 rounded-lg bg-[#A50034] hover:bg-[#8A002B] disabled:opacity-60 text-white font-semibold flex items-center gap-1 cursor-pointer disabled:cursor-default"
        >
          {downloading ? <RefreshCw className="w-3 h-3 animate-spin" /> : <Download className="w-3 h-3" />}
          {downloading ? (percent === null ? "받는 중" : `${percent}%`) : "업데이트"}
        </button>
      </div>
      {error && <div className="text-red-600 dark:text-red-400 break-words">{error}</div>}
    </div>
  );
}

/** 설정 창의 업데이트 항목 */
export function UpdateSettings({
  updater,
  runningCount,
  autoCheck,
  onAutoCheckChange,
}: {
  updater: Updater;
  runningCount: number;
  autoCheck: boolean;
  onAutoCheckChange: (value: boolean) => void;
}) {
  const { version, status, info, percent, error } = updater;
  const busy = status === "checking" || status === "downloading";
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <span className="text-stone-600 dark:text-stone-300">
          현재 버전 <span className="font-semibold tabular-nums">{version || "(미리보기)"}</span>
          {status === "latest" && <span className="ml-2 text-emerald-700 dark:text-emerald-400">최신 버전입니다</span>}
          {info && status !== "latest" && (
            <span className="ml-2 text-[#A50034] dark:text-[#FF5C8D] font-semibold">새 버전 {info.version} 있음</span>
          )}
        </span>
        {info && status !== "checking" ? (
          <button
            type="button"
            disabled={busy}
            onClick={() => confirmInstall(info, runningCount) && void updater.install()}
            className="px-2.5 py-1 rounded-lg bg-[#A50034] hover:bg-[#8A002B] disabled:opacity-60 text-white font-semibold cursor-pointer disabled:cursor-default"
          >
            {status === "downloading" ? (percent === null ? "받는 중..." : `받는 중 ${percent}%`) : "지금 업데이트"}
          </button>
        ) : (
          <button
            type="button"
            disabled={busy}
            onClick={() => void updater.check()}
            className="px-2.5 py-1 rounded-lg bg-stone-100 dark:bg-[#32322F] hover:bg-stone-200 dark:hover:bg-[#3C3C38] disabled:opacity-60 text-stone-800 dark:text-stone-200 font-semibold cursor-pointer disabled:cursor-default"
          >
            {status === "checking" ? "확인하는 중..." : "업데이트 확인"}
          </button>
        )}
      </div>
      {info?.notes && (
        <pre className="max-h-32 overflow-auto whitespace-pre-wrap rounded-lg bg-[#FAF9F5] dark:bg-[#1E1E1C] border border-stone-200 dark:border-[#383834] px-2.5 py-2 text-[11px] text-stone-600 dark:text-stone-300 font-sans">
          {info.notes}
        </pre>
      )}
      {error && <div className="text-red-600 dark:text-red-400 break-words">{error}</div>}
      <label className="flex items-center gap-2 text-stone-600 dark:text-stone-300 cursor-pointer">
        <input type="checkbox" checked={autoCheck} onChange={(e) => onAutoCheckChange(e.target.checked)} />
        앱을 켤 때 새 버전이 있는지 확인 (설치는 눌렀을 때만 합니다)
      </label>
    </div>
  );
}
