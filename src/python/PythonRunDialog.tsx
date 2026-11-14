import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  AlertTriangle,
  Check,
  ChevronDown,
  ChevronRight,
  Copy,
  Download,
  ExternalLink,
  FileCode2,
  FolderOpen,
  Loader2,
  MessageSquareShare,
  Play,
  RefreshCw,
  X,
} from "lucide-react";
import { BackendAPI } from "../api";
import { RuntimeInstallButton } from "../runtime/RuntimePanel";
import type { PythonInfo, PythonRunOutcome } from "../types";
import { FileIcon } from "../explorer/fileIcons";

const WINGET_COMMAND = "winget install Python.Python.3.13";
const PYTHON_DOWNLOAD_URL = "https://www.python.org/downloads/windows/";

type Phase =
  | { kind: "checking" }
  | { kind: "no-python" }
  | { kind: "ready" }
  | { kind: "running"; startedAt: number }
  | { kind: "installing"; pkg: string }
  | { kind: "done"; outcome: PythonRunOutcome }
  | { kind: "error"; message: string };

export interface PythonRunDialogProps {
  projectId: string;
  projectRoot?: string;
  /** 실행할 .py (프로젝트 기준 상대 경로) */
  relativePath: string;
  onClose: () => void;
  /** 만들어진 파일 열기: 캔버스로 열 수 있으면 캔버스, 아니면 기본 앱(엑셀 등) */
  onOpenFile: (relativePath: string) => void;
  onReveal: (relativePath: string) => void;
  /** 스크립트 내용 보기 (캔버스) */
  onViewScript: (relativePath: string) => void;
  /** 실행·설치로 파일이 바뀌었을 수 있을 때 (탐색기 새로고침) */
  onFilesChanged: () => void;
  /** 실패했을 때: 오류 내용을 담은 요청 문장을 채팅 입력창에 넣는다 */
  onAskFix?: (prompt: string) => void;
}

/** 실패한 실행을 엑사원에게 고쳐 달라고 할 때 보낼 문장 */
export function fixRequestPrompt(o: PythonRunOutcome): string {
  // 오류 설명은 보통 끝에 있으므로 마지막 25줄만 보낸다.
  // 들여쓰기(Traceback의 "  File …")는 살리고, 앞뒤 빈 줄만 걷어 낸다.
  const raw = o.stderr.trim() ? o.stderr : o.stdout;
  const err = raw.replace(/^(\s*\n)+/, "").trimEnd().split("\n").slice(-25).join("\n");
  const what = o.timedOut ? "3분 안에 끝나지 않아 중단됐어" : `오류가 났어 (종료 코드 ${o.exitCode ?? "?"})`;
  const fence = "```";
  const ask = `\`${o.script}\`을 실행했더니 ${what}. 새 파일을 만들지 말고 같은 파일을 고친 뒤 run_python으로 다시 실행해서 결과 파일까지 만들어 줘.`;
  return err ? `${ask}\n\n${fence}\n${err}\n${fence}` : ask;
}

function seconds(ms: number) {
  return `${(ms / 1000).toFixed(1)}초`;
}

/** AI가 만든 Python 스크립트를 앱 안에서 실행하고, 만들어진 파일을 바로 열 수 있게 한다. */
export default function PythonRunDialog({
  projectId,
  projectRoot,
  relativePath,
  onClose,
  onOpenFile,
  onReveal,
  onViewScript,
  onFilesChanged,
  onAskFix,
}: PythonRunDialogProps) {
  const [phase, setPhase] = useState<Phase>({ kind: "checking" });
  const [python, setPython] = useState<PythonInfo | null>(null);
  const [installLog, setInstallLog] = useState<string | null>(null);
  const [showOutput, setShowOutput] = useState(false);
  const [now, setNow] = useState(Date.now());
  const [copied, setCopied] = useState(false);
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  async function check() {
    setPhase({ kind: "checking" });
    try {
      const info = await BackendAPI.pythonStatus(projectId);
      if (!alive.current) return;
      setPython(info);
      setPhase(info ? { kind: "ready" } : { kind: "no-python" });
    } catch (e) {
      if (alive.current) setPhase({ kind: "error", message: String(e) });
    }
  }

  useEffect(() => {
    check();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId, relativePath]);

  // 실행 중 경과 시간
  useEffect(() => {
    if (phase.kind !== "running") return;
    const t = setInterval(() => setNow(Date.now()), 250);
    return () => clearInterval(t);
  }, [phase.kind]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  async function run() {
    setPhase({ kind: "running", startedAt: Date.now() });
    setNow(Date.now());
    try {
      const outcome = await BackendAPI.runPythonScript(projectId, relativePath);
      if (!alive.current) return;
      setPython(outcome.python);
      // 성공하면 출력은 접어 두고, 실패하면 바로 보여 준다.
      setShowOutput(outcome.exitCode !== 0 && !outcome.missingModule);
      setPhase({ kind: "done", outcome });
      onFilesChanged();
    } catch (e) {
      if (alive.current) setPhase({ kind: "error", message: String(e) });
    }
  }

  async function installAndRun(pkg: string) {
    setPhase({ kind: "installing", pkg });
    setInstallLog(null);
    try {
      await BackendAPI.installPythonPackage(projectId, pkg);
      if (!alive.current) return;
      await run();
    } catch (e) {
      if (!alive.current) return;
      setInstallLog(String(e));
      setPhase({ kind: "error", message: `'${pkg}' 패키지를 설치하지 못했습니다.` });
    }
  }

  const busy = phase.kind === "running" || phase.kind === "installing" || phase.kind === "checking";
  const canRun = python !== null && !busy;
  const done = phase.kind === "done" ? phase.outcome : null;

  let body: ReactNode = null;
  switch (phase.kind) {
    case "checking":
      body = (
        <div className="flex items-center gap-2 text-stone-500 dark:text-stone-400">
          <Loader2 className="w-4 h-4 animate-spin" /> Python을 찾는 중…
        </div>
      );
      break;
    case "no-python":
      body = (
        <div className="space-y-3">
          <div className="flex items-start gap-2 text-amber-800 dark:text-amber-300">
            <AlertTriangle className="w-4 h-4 mt-0.5 shrink-0" />
            <div>
              Python이 아직 없습니다. 아래 버튼을 누르면 앱이 전용 Python과 문서·차트 패키지를 설치합니다 (내려받기 약 100MB, 한 번만).
            </div>
          </div>
          <RuntimeInstallButton kind="python" label="Python 자동 설치" onDone={check} />
          <details className="text-[11px]">
          <summary className="cursor-pointer text-stone-500 dark:text-stone-400">직접 설치하려면</summary>
          <div className="mt-2 space-y-2">
          <div className="rounded-lg border border-stone-200 dark:border-[#3A3A36] p-3 space-y-2">
            <div className="font-semibold text-stone-700 dark:text-stone-200">PowerShell에서 설치</div>
            <div className="flex items-center gap-2">
              <code className="flex-1 font-mono text-[11px] px-2 py-1.5 rounded bg-stone-900 text-stone-100">{WINGET_COMMAND}</code>
              <button
                onClick={() => {
                  navigator.clipboard.writeText(WINGET_COMMAND).catch(() => {});
                  setCopied(true);
                  setTimeout(() => setCopied(false), 1500);
                }}
                className="px-2 py-1.5 rounded-lg bg-stone-100 dark:bg-[#2E2E2A] hover:bg-stone-200 dark:hover:bg-[#3A3A36] flex items-center gap-1 cursor-pointer"
              >
                {copied ? <Check className="w-3.5 h-3.5 text-emerald-600" /> : <Copy className="w-3.5 h-3.5" />} 복사
              </button>
            </div>
          </div>
          <button
            onClick={() => BackendAPI.openExternalUrl(PYTHON_DOWNLOAD_URL).catch(() => {})}
            className="text-[#A50034] dark:text-[#FF5C8D] font-semibold hover:underline flex items-center gap-1 cursor-pointer"
          >
            <ExternalLink className="w-3.5 h-3.5" /> python.org에서 내려받기
          </button>
          </div>
          </details>
        </div>
      );
      break;
    case "ready":
      body = (
        <div className="flex items-start gap-2 rounded-lg bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-900/60 px-3 py-2 text-amber-900 dark:text-amber-200">
          <AlertTriangle className="w-4 h-4 mt-0.5 shrink-0" />
          <div>
            이 스크립트는 이 컴퓨터에서 그대로 실행됩니다. AI가 만든 스크립트라면 실행 전에{" "}
            <button onClick={() => onViewScript(relativePath)} className="font-semibold underline cursor-pointer">
              내용을 확인
            </button>
            하세요. 최대 3분 동안 실행됩니다.
          </div>
        </div>
      );
      break;
    case "running":
      body = (
        <div className="flex items-center gap-2 text-stone-600 dark:text-stone-300">
          <Loader2 className="w-4 h-4 animate-spin text-[#A50034] dark:text-[#FF5C8D]" />
          실행 중… {seconds(Math.max(0, now - phase.startedAt))}
        </div>
      );
      break;
    case "installing":
      body = (
        <div className="flex items-center gap-2 text-stone-600 dark:text-stone-300">
          <Loader2 className="w-4 h-4 animate-spin text-[#A50034] dark:text-[#FF5C8D]" />
          '{phase.pkg}' 패키지를 설치하는 중… 끝나면 다시 실행합니다.
        </div>
      );
      break;
    case "error":
      body = (
        <div className="space-y-2">
          <div className="flex items-start gap-2 text-rose-700 dark:text-rose-300">
            <AlertTriangle className="w-4 h-4 mt-0.5 shrink-0" />
            <div className="whitespace-pre-wrap break-words">{phase.message}</div>
          </div>
          {installLog && <OutputBlock title="설치 기록" text={installLog} />}
        </div>
      );
      break;
    case "done": {
      const o = phase.outcome;
      const ok = o.exitCode === 0;
      const files = [...o.created.map((f) => ({ f, isNew: true })), ...o.modified.map((f) => ({ f, isNew: false }))];
      body = (
        <div className="space-y-3">
          <div
            className={`flex items-center gap-2 font-semibold ${
              ok ? "text-emerald-700 dark:text-emerald-400" : "text-rose-700 dark:text-rose-300"
            }`}
          >
            {ok ? <Check className="w-4 h-4" /> : <AlertTriangle className="w-4 h-4" />}
            {o.timedOut
              ? "3분 안에 끝나지 않아 중단했습니다."
              : ok
                ? `실행 완료 (${seconds(o.durationMs)})`
                : `실행 실패 (종료 코드 ${o.exitCode ?? "?"}, ${seconds(o.durationMs)})`}
          </div>

          {o.missingModule && o.suggestedPackage && (
            <div className="rounded-lg border border-amber-200 dark:border-amber-900/60 bg-amber-50 dark:bg-amber-950/30 px-3 py-2.5 space-y-2">
              <div className="text-amber-900 dark:text-amber-200">
                스크립트에 필요한 <b className="font-mono">{o.missingModule}</b> 모듈이 설치되어 있지 않습니다.
                {o.suggestedPackage !== o.missingModule && (
                  <>
                    {" "}
                    (패키지 이름: <span className="font-mono">{o.suggestedPackage}</span>)
                  </>
                )}
              </div>
              <button
                onClick={() => installAndRun(o.suggestedPackage!)}
                className="px-3 py-1.5 rounded-lg bg-[#A50034] hover:bg-[#8A002B] text-white font-semibold flex items-center gap-1.5 cursor-pointer"
              >
                <Download className="w-3.5 h-3.5" /> {o.suggestedPackage} 설치 후 다시 실행
              </button>
              <div className="text-[11px] text-amber-800/80 dark:text-amber-300/80">
                pip으로 인터넷에서 내려받아 {o.python.inVenv ? "이 가상환경에" : "사용자 폴더에"} 설치합니다.
              </div>
            </div>
          )}

          {files.length > 0 ? (
            <div className="space-y-1">
              <div className="text-[11px] font-semibold text-stone-500 dark:text-stone-400">만들어지거나 바뀐 파일</div>
              {files.map(({ f, isNew }) => (
                <div
                  key={f}
                  className="flex items-center gap-2 rounded-lg border border-stone-200 dark:border-[#3A3A36] px-2.5 py-1.5"
                >
                  <FileIcon name={f.split("/").pop() || f} isDir={false} />
                  <span className="font-mono truncate flex-1" title={f}>
                    {f}
                  </span>
                  <span
                    className={`px-1.5 rounded text-[10px] font-semibold shrink-0 ${
                      isNew
                        ? "bg-emerald-100 text-emerald-800 dark:bg-emerald-900/50 dark:text-emerald-300"
                        : "bg-amber-100 text-amber-800 dark:bg-amber-900/40 dark:text-amber-300"
                    }`}
                  >
                    {isNew ? "새 파일" : "바뀜"}
                  </span>
                  <button
                    onClick={() => onOpenFile(f)}
                    className="px-2 py-0.5 rounded bg-[#A50034] hover:bg-[#8A002B] text-white font-semibold shrink-0 cursor-pointer"
                  >
                    열기
                  </button>
                  <button
                    onClick={() => onReveal(f)}
                    title="Windows 탐색기에서 보기"
                    className="p-1 rounded hover:bg-stone-100 dark:hover:bg-[#2E2E2A] shrink-0 cursor-pointer"
                  >
                    <FolderOpen className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
          ) : (
            ok && (
              <div className="text-stone-500 dark:text-stone-400">
                프로젝트 폴더에 새로 생기거나 바뀐 파일이 없습니다. 스크립트가 다른 위치에 저장했는지 출력을 확인하세요.
              </div>
            )
          )}

          {!ok && !o.missingModule && onAskFix && (
            <div className="flex items-center gap-2 rounded-lg border border-stone-200 dark:border-[#3A3A36] px-3 py-2">
              <span className="flex-1 text-stone-600 dark:text-stone-300">
                스크립트 자체의 오류입니다. 오류 내용을 엑사원에게 보내 고치게 할 수 있습니다.
              </span>
              <button
                onClick={() => onAskFix(fixRequestPrompt(o))}
                className="px-2.5 py-1.5 rounded-lg bg-stone-800 hover:bg-stone-700 dark:bg-stone-200 dark:hover:bg-white text-white dark:text-stone-900 font-semibold flex items-center gap-1.5 shrink-0 cursor-pointer"
              >
                <MessageSquareShare className="w-3.5 h-3.5" /> 엑사원에게 고쳐 달라고 하기
              </button>
            </div>
          )}

          {(o.stdout.trim() || o.stderr.trim()) && (
            <div>
              <button
                onClick={() => setShowOutput((v) => !v)}
                className="flex items-center gap-1 text-[11px] font-semibold text-stone-500 dark:text-stone-400 hover:text-stone-800 dark:hover:text-stone-200 cursor-pointer"
              >
                {showOutput ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
                실행 출력
              </button>
              {showOutput && (
                <div className="mt-1 space-y-1.5">
                  {o.stdout.trim() && <OutputBlock text={o.stdout} />}
                  {o.stderr.trim() && <OutputBlock text={o.stderr} error />}
                </div>
              )}
            </div>
          )}
        </div>
      );
      break;
    }
  }

  return (
    <div className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-6" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div
        role="dialog"
        aria-label="Python 스크립트 실행"
        className="bg-white dark:bg-[#1E1E1C] border border-stone-200 dark:border-[#3A3A36] rounded-2xl shadow-2xl w-full max-w-xl max-h-[85vh] flex flex-col overflow-hidden text-xs text-stone-800 dark:text-stone-200"
      >
        <div className="flex items-center gap-2 px-4 py-3 border-b border-stone-200 dark:border-[#383834]">
          <Play className="w-4 h-4 text-[#A50034] dark:text-[#FF5C8D]" />
          <span className="font-bold text-sm flex-1">Python 스크립트 실행</span>
          <button onClick={onClose} aria-label="닫기" className="p-1 rounded-lg hover:bg-stone-100 dark:hover:bg-[#2A2A27] cursor-pointer">
            <X className="w-4 h-4 text-stone-500" />
          </button>
        </div>

        <div className="px-4 py-3 space-y-1.5 border-b border-stone-200 dark:border-[#383834] bg-stone-50/60 dark:bg-[#232321]">
          <div className="flex items-center gap-2">
            <FileCode2 className="w-3.5 h-3.5 text-stone-500 shrink-0" />
            <span className="font-mono font-semibold truncate flex-1" title={relativePath}>
              {relativePath}
            </span>
            <button
              onClick={() => onViewScript(relativePath)}
              className="text-[11px] text-[#A50034] dark:text-[#FF5C8D] font-semibold hover:underline shrink-0 cursor-pointer"
            >
              스크립트 보기
            </button>
          </div>
          <div className="text-[11px] text-stone-500 dark:text-stone-400 space-y-0.5">
            <div className="truncate" title={python?.path}>
              {python ? (
                <>
                  Python {python.version}
                  {python.projectVenv && (
                    <span className="ml-1.5 px-1 rounded bg-sky-100 text-sky-800 dark:bg-sky-900/40 dark:text-sky-300 font-semibold">
                      프로젝트 가상환경
                    </span>
                  )}
                  <span className="ml-1.5 font-mono">{python.path}</span>
                </>
              ) : phase.kind === "checking" ? (
                "Python 확인 중…"
              ) : (
                "Python 없음"
              )}
            </div>
            <div className="truncate" title={projectRoot}>
              실행 위치: 프로젝트 폴더{projectRoot ? ` (${projectRoot})` : ""}
            </div>
          </div>
        </div>

        <div className="px-4 py-3 overflow-y-auto flex-1 min-h-[88px]">{body}</div>

        <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-stone-200 dark:border-[#383834]">
          {phase.kind === "no-python" && (
            <button
              onClick={check}
              className="px-3 py-1.5 rounded-lg bg-stone-100 dark:bg-[#2E2E2A] hover:bg-stone-200 dark:hover:bg-[#3A3A36] font-semibold flex items-center gap-1.5 cursor-pointer"
            >
              <RefreshCw className="w-3.5 h-3.5" /> 다시 확인
            </button>
          )}
          <button
            onClick={onClose}
            className="px-3 py-1.5 rounded-lg hover:bg-stone-100 dark:hover:bg-[#2A2A27] font-semibold cursor-pointer"
          >
            닫기
          </button>
          {phase.kind !== "no-python" && (
            <button
              onClick={run}
              disabled={!canRun}
              autoFocus
              className="px-3.5 py-1.5 rounded-lg bg-[#A50034] hover:bg-[#8A002B] disabled:opacity-40 disabled:cursor-default text-white font-semibold flex items-center gap-1.5 cursor-pointer"
            >
              {done || phase.kind === "error" ? <RefreshCw className="w-3.5 h-3.5" /> : <Play className="w-3.5 h-3.5" />}
              {done || phase.kind === "error" ? "다시 실행" : "실행"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function OutputBlock({ text, title, error = false }: { text: string; title?: string; error?: boolean }) {
  return (
    <div>
      {title && <div className="text-[11px] font-semibold text-stone-500 mb-0.5">{title}</div>}
      <pre
        className={`max-h-56 overflow-auto whitespace-pre-wrap break-all rounded-lg px-2.5 py-2 font-mono text-[11px] ${
          error ? "bg-rose-950 text-rose-100" : "bg-stone-900 text-stone-100"
        }`}
      >
        {text.trimEnd()}
      </pre>
    </div>
  );
}
