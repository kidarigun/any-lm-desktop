import { useEffect, useMemo, useState, type ReactNode } from "react";
import { Columns2, ExternalLink, FileDiff, Info, Loader2, RotateCcw, Rows3, X } from "lucide-react";
import { BackendAPI } from "../api";
import type { SnapshotDiff } from "../types";
import { collapseContext, computeDiff, toSplitRows, type DiffResult, type DiffRow, type Segment } from "./diffModel";
import { formatWhen, sourceLabel } from "./history";

type Mode = "unified" | "split";
const MODE_KEY = "anylm_diff_mode";

type Entry =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ok"; diff: SnapshotDiff; result: DiffResult };

export interface DiffViewerProps {
  /** 함께 볼 변경 기록들 (보통 한 답변에서 바뀐 파일들) */
  snapshotIds: string[];
  initialId: string;
  onClose: () => void;
  /** 되돌리기에 성공하면 true */
  onUndo: (snapshotId: string) => Promise<boolean>;
  onOpenFile?: (relativePath: string) => void;
  /**
   * files: 한 답변에서 바뀐 여러 파일 (목록에 파일 이름을 보여 줌)
   * history: 한 파일의 변경 기록 (목록에 시각·누가 바꿨는지를 보여 줌)
   */
  variant?: "files" | "history";
}

function readMode(): Mode {
  try {
    return localStorage.getItem(MODE_KEY) === "split" ? "split" : "unified";
  } catch {
    return "unified";
  }
}

function fileName(rel: string) {
  return rel.slice(rel.lastIndexOf("/") + 1) || rel;
}

function dirName(rel: string) {
  const i = rel.lastIndexOf("/");
  return i > 0 ? rel.slice(0, i) : "";
}

function actionLabel(d: SnapshotDiff): { text: string; cls: string } {
  if (!d.beforeExists) return { text: "새 파일", cls: "bg-emerald-100 text-emerald-800 dark:bg-emerald-900/50 dark:text-emerald-300" };
  if (!d.afterExists) return { text: "삭제", cls: "bg-rose-100 text-rose-800 dark:bg-rose-900/50 dark:text-rose-300" };
  return { text: "수정", cls: "bg-amber-100 text-amber-800 dark:bg-amber-900/40 dark:text-amber-300" };
}

const ROW_BG: Record<DiffRow["kind"], string> = {
  ctx: "",
  add: "bg-emerald-50 dark:bg-emerald-950/40",
  del: "bg-rose-50 dark:bg-rose-950/40",
};
const SEG_BG: Record<DiffRow["kind"], string> = {
  ctx: "",
  add: "bg-emerald-200/80 dark:bg-emerald-700/50 rounded-sm",
  del: "bg-rose-200/80 dark:bg-rose-800/60 rounded-sm",
};
const SIGN: Record<DiffRow["kind"], ReactNode> = {
  ctx: " ",
  add: <span className="text-emerald-700 dark:text-emerald-400">+</span>,
  del: <span className="text-rose-700 dark:text-rose-400">−</span>,
};

function Line({ row, segments }: { row: DiffRow; segments: Segment[] }) {
  return (
    <span className="whitespace-pre-wrap break-all">
      {segments.map((s, i) =>
        s.changed ? (
          <span key={i} className={SEG_BG[row.kind]}>
            {s.text}
          </span>
        ) : (
          <span key={i}>{s.text}</span>
        )
      )}
      {segments.every((s) => s.text === "") && "​"}
    </span>
  );
}

function GapButton({ count, onClick }: { count: number; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className="w-full py-1 text-[11px] text-center text-stone-500 dark:text-stone-400 bg-stone-100/80 dark:bg-[#2A2A27] hover:bg-stone-200 dark:hover:bg-[#32322F] border-y border-stone-200 dark:border-[#383834] cursor-pointer"
    >
      ⋯ 변경 없는 {count}줄 펼치기
    </button>
  );
}

const GUTTER = "select-none text-right pr-2 text-stone-400 dark:text-stone-500 tabular-nums";

function UnifiedView({ rows, expanded, onExpand }: { rows: DiffRow[]; expanded: Set<string>; onExpand: (k: string) => void }) {
  const items = useMemo(() => collapseContext(rows, (r) => r.kind === "ctx"), [rows]);
  const renderRow = (r: DiffRow, key: string) => (
    <div key={key} className={`grid grid-cols-[3rem_3rem_1.25rem_1fr] min-h-5 ${ROW_BG[r.kind]}`}>
      <span className={GUTTER}>{r.oldNo ?? ""}</span>
      <span className={GUTTER}>{r.newNo ?? ""}</span>
      <span className="select-none text-center">{SIGN[r.kind]}</span>
      <Line row={r} segments={r.segments} />
    </div>
  );
  return (
    <div className="font-mono text-[12px] leading-5 text-stone-800 dark:text-stone-200 py-1">
      {items.map((it, i) =>
        it.type === "item" ? (
          renderRow(it.item, `r${i}`)
        ) : expanded.has(it.key) ? (
          it.items.map((r, k) => renderRow(r, `${it.key}-${k}`))
        ) : (
          <GapButton key={it.key} count={it.items.length} onClick={() => onExpand(it.key)} />
        )
      )}
    </div>
  );
}

function SplitView({ rows, expanded, onExpand }: { rows: DiffRow[]; expanded: Set<string>; onExpand: (k: string) => void }) {
  const split = useMemo(() => toSplitRows(rows), [rows]);
  const items = useMemo(() => collapseContext(split, (r) => r.left?.kind === "ctx"), [split]);
  const cell = (r: DiffRow | null, side: "left" | "right") =>
    r ? (
      <>
        <span className={`${GUTTER} ${ROW_BG[r.kind]}`}>{side === "left" ? r.oldNo : r.newNo}</span>
        <span className={`pl-1 pr-2 ${ROW_BG[r.kind]}`}>
          <Line row={r} segments={r.segments} />
        </span>
      </>
    ) : (
      <>
        <span className="bg-stone-100/70 dark:bg-[#252523]" />
        <span className="bg-stone-100/70 dark:bg-[#252523]" />
      </>
    );
  const renderRow = (r: (typeof split)[number], key: string) => (
    <div key={key} className="grid grid-cols-[3rem_minmax(0,1fr)_3rem_minmax(0,1fr)] min-h-5">
      {cell(r.left, "left")}
      {cell(r.right, "right")}
    </div>
  );
  return (
    <div className="font-mono text-[12px] leading-5 text-stone-800 dark:text-stone-200 py-1">
      {items.map((it, i) =>
        it.type === "item" ? (
          renderRow(it.item, `r${i}`)
        ) : expanded.has(it.key) ? (
          it.items.map((r, k) => renderRow(r, `${it.key}-${k}`))
        ) : (
          <GapButton key={it.key} count={it.items.length} onClick={() => onExpand(it.key)} />
        )
      )}
    </div>
  );
}

/** 에이전트·캔버스가 바꾼 파일의 이전/이후를 비교해 보여 준다. */
export default function DiffViewer({ snapshotIds, initialId, onClose, onUndo, onOpenFile, variant = "files" }: DiffViewerProps) {
  const isHistory = variant === "history";
  const ids = useMemo(() => Array.from(new Set(snapshotIds)), [snapshotIds]);
  const [current, setCurrent] = useState(ids.includes(initialId) ? initialId : ids[0]);
  const [entries, setEntries] = useState<Record<string, Entry>>({});
  const [mode, setModeState] = useState<Mode>(readMode);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [undone, setUndone] = useState<Set<string>>(new Set());
  const [undoing, setUndoing] = useState(false);

  function setMode(m: Mode) {
    setModeState(m);
    try {
      localStorage.setItem(MODE_KEY, m);
    } catch {
      // 저장하지 못해도 이번 화면에서는 동작한다.
    }
  }

  useEffect(() => {
    let alive = true;
    for (const id of ids) {
      setEntries((e) => (e[id] ? e : { ...e, [id]: { status: "loading" } }));
      BackendAPI.getSnapshotDiff(id)
        .then((diff) => {
          const result = computeDiff(diff.before ?? "", diff.after ?? "");
          if (alive) setEntries((e) => ({ ...e, [id]: { status: "ok", diff, result } }));
        })
        .catch((err) => {
          if (alive) setEntries((e) => ({ ...e, [id]: { status: "error", message: String(err) } }));
        });
    }
    return () => {
      alive = false;
    };
  }, [ids]);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      } else if (e.altKey && (e.key === "ArrowDown" || e.key === "ArrowUp") && ids.length > 1) {
        e.preventDefault();
        setCurrent((c) => {
          const i = ids.indexOf(c);
          return ids[(i + (e.key === "ArrowDown" ? 1 : ids.length - 1)) % ids.length];
        });
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [ids, onClose]);

  const entry = entries[current];
  const diff = entry?.status === "ok" ? entry.diff : null;

  async function handleUndo() {
    if (!diff || undoing) return;
    const msg = diff.superseded
      ? `'${diff.relativePath}'은(는) 이 변경 뒤에 다시 바뀌었습니다.\n이 변경 이전으로 되돌리면 이후의 변경도 함께 사라집니다. 계속할까요?`
      : `'${diff.relativePath}'을(를) 이 변경 이전 상태로 되돌릴까요?`;
    if (!window.confirm(msg)) return;
    setUndoing(true);
    try {
      if (await onUndo(diff.snapshotId)) setUndone((s) => new Set(s).add(diff.snapshotId));
    } finally {
      setUndoing(false);
    }
  }

  const expandedFor = useMemo(() => {
    const prefix = `${current}:`;
    return new Set([...expanded].filter((k) => k.startsWith(prefix)).map((k) => k.slice(prefix.length)));
  }, [expanded, current]);
  const expand = (key: string) => setExpanded((s) => new Set(s).add(`${current}:${key}`));

  let body: ReactNode;
  if (!entry || entry.status === "loading") {
    body = (
      <div className="h-full flex items-center justify-center gap-2 text-sm text-stone-500">
        <Loader2 className="w-4 h-4 animate-spin" /> 변경 내용을 불러오는 중…
      </div>
    );
  } else if (entry.status === "error") {
    body = <div className="p-6 text-sm text-rose-700 dark:text-rose-300">불러오지 못했습니다: {entry.message}</div>;
  } else if (entry.diff.kind === "binary") {
    body = (
      <div className="h-full flex items-center justify-center text-sm text-stone-500 dark:text-stone-400">
        텍스트로 볼 수 없는 파일이라 내용을 비교할 수 없습니다.
      </div>
    );
  } else if (entry.result.added + entry.result.removed === 0) {
    body = (
      <div className="h-full flex items-center justify-center text-sm text-stone-500 dark:text-stone-400">
        {entry.diff.beforeExists || entry.diff.afterExists ? "내용 변화가 없습니다." : "빈 파일입니다."}
      </div>
    );
  } else if (mode === "split" && entry.diff.beforeExists && entry.diff.afterExists) {
    // 새 파일·삭제된 파일은 한쪽이 비므로 나란히 볼 필요가 없다.
    body = <SplitView rows={entry.result.rows} expanded={expandedFor} onExpand={expand} />;
  } else {
    body = <UnifiedView rows={entry.result.rows} expanded={expandedFor} onExpand={expand} />;
  }

  const notes = diff
    ? [
        ...(diff.superseded ? ["이 파일은 이 변경 뒤에 다시 바뀌었습니다. 아래는 이 변경 시점의 내용입니다."] : []),
        ...diff.notes,
      ]
    : [];
  const label = diff ? actionLabel(diff) : null;
  const isUndone = diff ? undone.has(diff.snapshotId) : false;

  return (
    <div
      className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-6"
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        role="dialog"
        aria-label={isHistory ? "파일 변경 기록" : "변경 내용 보기"}
        className="bg-white dark:bg-[#1E1E1C] border border-stone-200 dark:border-[#3A3A36] rounded-2xl shadow-2xl w-full max-w-6xl h-[85vh] flex flex-col overflow-hidden"
      >
        {/* 머리글 */}
        <div className="flex items-center gap-3 px-4 py-2.5 border-b border-stone-200 dark:border-[#383834]">
          <FileDiff className="w-4 h-4 text-[#A50034] dark:text-[#FF5C8D] shrink-0" />
          <div className="min-w-0 flex-1 flex items-center gap-2">
            <span className="font-mono text-xs font-semibold text-stone-800 dark:text-stone-100 truncate">
              {diff?.relativePath ?? "변경 내용"}
            </span>
            {label && <span className={`px-1.5 py-0.5 rounded text-[10px] font-semibold ${label.cls}`}>{label.text}</span>}
            {diff?.kind === "document" && (
              <span className="px-1.5 py-0.5 rounded text-[10px] font-semibold bg-stone-100 text-stone-600 dark:bg-[#2E2E2A] dark:text-stone-300">
                문서 · 글자 비교
              </span>
            )}
            {diff && (
              <span className="text-[11px] text-stone-500 dark:text-stone-400 shrink-0">
                {formatWhen(diff.createdAt)}
                {sourceLabel(diff.source) && ` · ${sourceLabel(diff.source)}`}
              </span>
            )}
            {entry?.status === "ok" && (
              <span className="text-[11px] font-mono">
                <span className="text-emerald-700 dark:text-emerald-400">+{entry.result.added}</span>{" "}
                <span className="text-rose-700 dark:text-rose-400">−{entry.result.removed}</span>
              </span>
            )}
          </div>
          <div className="flex items-center rounded-lg border border-stone-200 dark:border-[#3A3A36] p-0.5 text-[11px]">
            {(
              [
                ["unified", "한 줄로", Rows3],
                ["split", "나란히", Columns2],
              ] as const
            ).map(([m, text, Icon]) => (
              <button
                key={m}
                onClick={() => setMode(m)}
                className={`px-2 py-1 rounded-md flex items-center gap-1 cursor-pointer ${
                  mode === m
                    ? "bg-stone-800 text-white dark:bg-stone-200 dark:text-stone-900"
                    : "text-stone-600 dark:text-stone-300 hover:bg-stone-100 dark:hover:bg-[#2A2A27]"
                }`}
              >
                <Icon className="w-3 h-3" /> {text}
              </button>
            ))}
          </div>
          {diff && diff.afterExists && onOpenFile && (
            <button
              onClick={() => {
                onOpenFile(diff.relativePath);
                onClose();
              }}
              className="px-2 py-1 rounded-lg text-[11px] font-medium text-stone-700 dark:text-stone-200 hover:bg-stone-100 dark:hover:bg-[#2A2A27] flex items-center gap-1 cursor-pointer"
            >
              <ExternalLink className="w-3 h-3" /> 파일 열기
            </button>
          )}
          {diff && (
            <button
              onClick={handleUndo}
              disabled={undoing || isUndone}
              className="px-2 py-1 rounded-lg text-[11px] font-medium bg-stone-100 dark:bg-[#2E2E2A] text-stone-700 dark:text-stone-200 hover:bg-stone-200 dark:hover:bg-[#3A3A36] flex items-center gap-1 cursor-pointer disabled:opacity-50 disabled:cursor-default"
            >
              <RotateCcw className="w-3 h-3" /> {isUndone ? "되돌림" : "이 변경 되돌리기"}
            </button>
          )}
          <button onClick={onClose} aria-label="닫기" className="p-1 rounded-lg hover:bg-stone-100 dark:hover:bg-[#2A2A27] cursor-pointer">
            <X className="w-4 h-4 text-stone-500" />
          </button>
        </div>

        <div className="flex-1 min-h-0 flex">
          {/* 파일 목록 (여러 파일이 바뀐 경우) */}
          {ids.length > 1 && (
            <div className="w-56 shrink-0 border-r border-stone-200 dark:border-[#383834] overflow-y-auto py-1">
              <div className="px-3 py-1 text-[10px] font-semibold text-stone-400 uppercase tracking-wide">
                {isHistory ? `변경 기록 ${ids.length}개 · 최신순` : `바뀐 파일 ${ids.length}개`}
              </div>
              {ids.map((id) => {
                const e = entries[id];
                const d = e?.status === "ok" ? e.diff : null;
                const lb = d ? actionLabel(d) : null;
                return (
                  <button
                    key={id}
                    onClick={() => setCurrent(id)}
                    className={`w-full text-left px-3 py-1.5 text-xs cursor-pointer ${
                      id === current ? "bg-[#A50034]/10 dark:bg-[#FF5C8D]/15" : "hover:bg-stone-100 dark:hover:bg-[#2A2A27]"
                    }`}
                  >
                    <div className="flex items-center gap-1.5">
                      <span className="font-medium text-stone-800 dark:text-stone-100 truncate flex-1">
                        {d ? (isHistory ? formatWhen(d.createdAt) : fileName(d.relativePath)) : "…"}
                      </span>
                      {undone.has(id) && <RotateCcw className="w-3 h-3 text-stone-400 shrink-0" />}
                      {lb && <span className={`px-1 rounded text-[9px] font-semibold shrink-0 ${lb.cls}`}>{lb.text}</span>}
                    </div>
                    <div className="flex items-center gap-1.5 text-[10px] text-stone-400">
                      <span className="truncate flex-1">
                        {d ? (isHistory ? sourceLabel(d.source) ?? "" : dirName(d.relativePath)) : ""}
                      </span>
                      {e?.status === "ok" && (
                        <span className="font-mono shrink-0">
                          <span className="text-emerald-700 dark:text-emerald-400">+{e.result.added}</span>{" "}
                          <span className="text-rose-700 dark:text-rose-400">−{e.result.removed}</span>
                        </span>
                      )}
                    </div>
                  </button>
                );
              })}
            </div>
          )}

          <div className="flex-1 min-w-0 flex flex-col">
            {notes.length > 0 && (
              <div className="px-4 py-2 border-b border-stone-200 dark:border-[#383834] bg-amber-50/70 dark:bg-amber-950/20 space-y-0.5">
                {notes.map((n) => (
                  <div key={n} className="flex items-start gap-1.5 text-[11px] text-amber-800 dark:text-amber-300">
                    <Info className="w-3 h-3 mt-0.5 shrink-0" /> {n}
                  </div>
                ))}
              </div>
            )}
            <div className="flex-1 min-h-0 overflow-auto">{body}</div>
          </div>
        </div>
      </div>
    </div>
  );
}
