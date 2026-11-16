import {
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  Copy,
  ExternalLink,
  FilePlus,
  FolderPlus,
  Paperclip,
  Pencil,
  RefreshCw,
  Search,
  Trash2,
  X,
  Play,
} from "lucide-react";
import { docKindOf, isPreviewOnlyKind, opensInExternalApp } from "../docs/formats";
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { FileNode } from "../types";
import { FileIcon } from "./fileIcons";

/** 탐색기가 쓰는 파일 작업 (프로젝트 루트 기준 상대 경로) */
export interface ExplorerApi {
  listDir(rel: string): Promise<FileNode[]>;
  createFile(rel: string): Promise<string>;
  createFolder(rel: string): Promise<string>;
  rename(from: string, to: string): Promise<string>;
  trash(rel: string): Promise<void>;
  reveal(rel: string): Promise<void>;
  find(query: string): Promise<string[]>;
}

export interface FileExplorerProps {
  api: ExplorerApi;
  rootName: string;
  /** 값이 바뀌면 열려 있는 폴더를 다시 읽는다 (AI가 파일을 바꾼 뒤 등) */
  refreshToken: number;
  /** 캔버스에 열려 있는 파일 (강조 표시) */
  activePath?: string;
  onOpen: (rel: string) => void;
  /** .py 파일을 Python 실행 창으로 */
  onRunPython?: (rel: string) => void;
  /** Windows 기본 앱(Excel·PowerPoint 등)으로 열기 */
  onOpenExternal?: (rel: string) => void;
  onAttach: (rel: string) => void;
  onRenamed: (from: string, to: string) => void;
  onDeleted: (rel: string) => void;
  onError: (msg: string) => void;
}

/** 탐색기에서 채팅 입력창으로 끌어다 놓을 때 쓰는 데이터 형식 */
export const DRAG_MIME = "application/x-exaone-path";

type Draft =
  | { mode: "file" | "folder"; parent: string }
  | { mode: "rename"; path: string; isDir: boolean };

interface MenuState {
  x: number;
  y: number;
  node: FileNode | null;
  confirmDelete?: boolean;
}

const INVALID_NAME = /[\\/:*?"<>|]/;

export function dirOf(rel: string): string {
  const i = rel.lastIndexOf("/");
  return i >= 0 ? rel.slice(0, i) : "";
}

function baseName(rel: string): string {
  return rel.slice(rel.lastIndexOf("/") + 1);
}

function joinRel(dir: string, name: string): string {
  return dir ? `${dir}/${name}` : name;
}

function isUnder(path: string, dir: string): boolean {
  return path === dir || path.startsWith(dir + "/");
}

function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export function FileExplorer({
  api,
  rootName,
  refreshToken,
  activePath,
  onOpen,
  onRunPython,
  onOpenExternal,
  onAttach,
  onRenamed,
  onDeleted,
  onError,
}: FileExplorerProps) {
  const [children, setChildren] = useState<Record<string, FileNode[]>>({});
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const [loading, setLoading] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<string[] | null>(null);
  const treeRef = useRef<HTMLDivElement | null>(null);
  const expandedRef = useRef(expanded);
  expandedRef.current = expanded;
  // 부모가 매번 새 함수를 넘겨도 탐색기 상태가 초기화되지 않도록 최신 콜백만 참조한다.
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;
  const reportError = useCallback((msg: string) => onErrorRef.current(msg), []);

  const load = useCallback(
    async (dir: string): Promise<boolean> => {
      try {
        const nodes = await api.listDir(dir);
        setChildren((prev) => ({ ...prev, [dir]: nodes }));
        return true;
      } catch (e) {
        if (dir === "") reportError(`파일 목록을 읽지 못했습니다: ${errText(e)}`);
        return false;
      }
    },
    [api, reportError]
  );

  /** 루트와 펼쳐 둔 폴더를 다시 읽는다. 사라진 폴더는 접는다. */
  const refresh = useCallback(async () => {
    setLoading(true);
    const dirs = ["", ...Array.from(expandedRef.current)];
    const ok = await Promise.all(dirs.map((d) => load(d)));
    const gone = dirs.filter((_, i) => !ok[i] && dirs[i] !== "");
    if (gone.length) {
      setExpanded((prev) => new Set([...prev].filter((d) => !gone.some((g) => isUnder(d, g)))));
    }
    setLoading(false);
  }, [load]);

  // 프로젝트가 바뀌면 처음부터 다시 읽는다.
  useEffect(() => {
    setChildren({});
    setExpanded(new Set());
    setSelected(null);
    setDraft(null);
    setQuery("");
    setResults(null);
    setLoading(true);
    load("").finally(() => setLoading(false));
  }, [api, load]);

  const firstRefresh = useRef(true);
  useEffect(() => {
    if (firstRefresh.current) {
      firstRefresh.current = false;
      return;
    }
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshToken]);

  // 파일 이름 검색
  useEffect(() => {
    const q = query.trim();
    if (!q) {
      setResults(null);
      return;
    }
    const t = setTimeout(() => {
      api
        .find(q)
        .then(setResults)
        .catch((e) => reportError(errText(e)));
    }, 150);
    return () => clearTimeout(t);
  }, [query, api, reportError]);

  // 메뉴 바깥을 누르거나 Esc를 누르면 닫는다.
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", onKey);
    window.addEventListener("blur", close);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("blur", close);
    };
  }, [menu]);

  async function toggle(dir: string) {
    if (expanded.has(dir)) {
      setExpanded((prev) => new Set([...prev].filter((d) => !isUnder(d, dir))));
    } else {
      setExpanded((prev) => new Set(prev).add(dir));
      await load(dir);
    }
  }

  async function expandTo(dir: string) {
    if (!dir) return;
    const parts = dir.split("/");
    const chain = parts.map((_, i) => parts.slice(0, i + 1).join("/"));
    setExpanded((prev) => {
      const next = new Set(prev);
      chain.forEach((d) => next.add(d));
      return next;
    });
    await Promise.all(chain.map((d) => load(d)));
  }

  /** 새 파일/폴더를 만들 위치: 선택한 폴더 또는 선택한 파일이 있는 폴더 */
  function targetDir(node?: FileNode | null): string {
    if (node) return node.isDir ? node.relativePath : dirOf(node.relativePath);
    if (!selected) return "";
    const sel = findNode(selected);
    return sel?.isDir ? selected : dirOf(selected);
  }

  function findNode(rel: string): FileNode | undefined {
    return children[dirOf(rel)]?.find((n) => n.relativePath === rel);
  }

  async function startCreate(mode: "file" | "folder", parent: string) {
    setMenu(null);
    setQuery("");
    await expandTo(parent);
    setDraft({ mode, parent });
  }

  async function commitDraft(name: string) {
    const d = draft;
    setDraft(null);
    const trimmed = name.trim();
    if (!d || !trimmed) return;
    if (INVALID_NAME.test(trimmed) || trimmed === "." || trimmed === "..") {
      onError(`이름에 쓸 수 없는 문자가 있습니다: ${trimmed}`);
      return;
    }
    try {
      if (d.mode === "rename") {
        if (trimmed === baseName(d.path)) return;
        const to = joinRel(dirOf(d.path), trimmed);
        await api.rename(d.path, to);
        if (d.isDir) {
          // 바로 아래 refresh가 새 경로를 읽도록 ref도 함께 바꾼다.
          const next = new Set(
            [...expandedRef.current].map((p) => (isUnder(p, d.path) ? to + p.slice(d.path.length) : p))
          );
          expandedRef.current = next;
          setExpanded(next);
        }
        setSelected(to);
        onRenamed(d.path, to);
        await refresh();
      } else {
        const rel = joinRel(d.parent, trimmed);
        if (d.mode === "file") await api.createFile(rel);
        else await api.createFolder(rel);
        await load(d.parent);
        setSelected(rel);
        if (d.mode === "file") onOpen(rel);
      }
    } catch (e) {
      onError(errText(e));
    }
  }

  async function doDelete(node: FileNode) {
    setMenu(null);
    try {
      await api.trash(node.relativePath);
      setExpanded((prev) => new Set([...prev].filter((p) => !isUnder(p, node.relativePath))));
      if (selected && isUnder(selected, node.relativePath)) setSelected(null);
      onDeleted(node.relativePath);
      await load(dirOf(node.relativePath));
    } catch (e) {
      onError(errText(e));
    }
  }

  function openMenu(e: React.MouseEvent, node: FileNode | null) {
    e.preventDefault();
    e.stopPropagation();
    if (node) setSelected(node.relativePath);
    setMenu({ x: e.clientX, y: e.clientY, node });
  }

  // 화면에 보이는 순서대로 펼친 노드 목록 (키보드 이동용)
  const visible = useMemo(() => {
    const out: FileNode[] = [];
    const walk = (dir: string) => {
      for (const n of children[dir] ?? []) {
        out.push(n);
        if (n.isDir && expanded.has(n.relativePath)) walk(n.relativePath);
      }
    };
    walk("");
    return out;
  }, [children, expanded]);

  function onTreeKeyDown(e: React.KeyboardEvent) {
    if (draft || results) return;
    const idx = visible.findIndex((n) => n.relativePath === selected);
    const node = idx >= 0 ? visible[idx] : undefined;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = visible[Math.max(0, Math.min(visible.length - 1, idx + (e.key === "ArrowDown" ? 1 : -1)))];
      if (next) setSelected(next.relativePath);
    } else if (e.key === "ArrowRight" && node?.isDir && !expanded.has(node.relativePath)) {
      toggle(node.relativePath);
    } else if (e.key === "ArrowLeft" && node) {
      if (node.isDir && expanded.has(node.relativePath)) toggle(node.relativePath);
      else if (dirOf(node.relativePath)) setSelected(dirOf(node.relativePath));
    } else if (e.key === "Enter" && node) {
      e.preventDefault();
      if (node.isDir) toggle(node.relativePath);
      else onOpen(node.relativePath);
    } else if (e.key === "F2" && node) {
      e.preventDefault();
      setDraft({ mode: "rename", path: node.relativePath, isDir: node.isDir });
    } else if (e.key === "Delete" && node) {
      e.preventDefault();
      const row = treeRef.current?.querySelector(`[data-path="${CSS.escape(node.relativePath)}"]`);
      const r = row?.getBoundingClientRect();
      setMenu({ x: (r?.left ?? 0) + 24, y: (r?.bottom ?? 0) + 2, node, confirmDelete: true });
    }
  }

  function renderDraftInput(depth: number, mode: "file" | "folder") {
    return (
      <div className="flex items-center gap-1 py-0.5 pr-2" style={{ paddingLeft: 8 + depth * 12 + 14 }}>
        <FileIcon name={mode === "folder" ? "" : "new.txt"} isDir={mode === "folder"} />
        <NameInput initial="" onCommit={commitDraft} onCancel={() => setDraft(null)} />
      </div>
    );
  }

  function renderDir(dir: string, depth: number): ReactNode {
    const nodes = children[dir];
    return (
      <>
        {draft && draft.mode !== "rename" && draft.parent === dir && renderDraftInput(depth, draft.mode)}
        {nodes?.map((node) => {
          const rel = node.relativePath;
          const isOpen = node.isDir && expanded.has(rel);
          const renaming = draft?.mode === "rename" && draft.path === rel;
          const isSelected = selected === rel;
          const isActive = activePath === rel;
          return (
            <div key={rel}>
              <div
                data-path={rel}
                draggable={!node.isDir && !renaming}
                onDragStart={(e) => {
                  e.dataTransfer.setData(DRAG_MIME, rel);
                  e.dataTransfer.setData("text/plain", rel);
                  e.dataTransfer.effectAllowed = "copy";
                }}
                onClick={() => {
                  setSelected(rel);
                  if (renaming) return;
                  if (node.isDir) toggle(rel);
                  else onOpen(rel);
                }}
                onContextMenu={(e) => openMenu(e, node)}
                title={rel}
                className={`group flex items-center gap-1 py-[3px] pr-2 rounded-md cursor-pointer text-xs ${
                  isSelected
                    ? "bg-[#A50034]/10 dark:bg-[#FF5C8D]/15 text-stone-900 dark:text-stone-100"
                    : "text-stone-700 dark:text-stone-300 hover:bg-stone-200/70 dark:hover:bg-[#262623]"
                }`}
                style={{ paddingLeft: 8 + depth * 12 }}
              >
                {node.isDir ? (
                  isOpen ? (
                    <ChevronDown className="w-3 h-3 shrink-0 text-stone-400" />
                  ) : (
                    <ChevronRight className="w-3 h-3 shrink-0 text-stone-400" />
                  )
                ) : (
                  <span className="w-3 shrink-0" />
                )}
                <FileIcon name={node.name} isDir={node.isDir} open={isOpen} />
                {renaming ? (
                  <NameInput initial={node.name} onCommit={commitDraft} onCancel={() => setDraft(null)} />
                ) : (
                  <span className={`truncate ${isActive ? "font-semibold text-[#A50034] dark:text-[#FF5C8D]" : ""}`}>
                    {node.name}
                  </span>
                )}
              </div>
              {isOpen && renderDir(rel, depth + 1)}
            </div>
          );
        })}
        {nodes && nodes.length === 0 && depth > 0 && !(draft && draft.mode !== "rename" && draft.parent === dir) && (
          <div className="text-[11px] text-stone-400 py-0.5" style={{ paddingLeft: 8 + depth * 12 + 14 }}>
            (비어 있음)
          </div>
        )}
      </>
    );
  }

  const menuNode = menu?.node ?? null;

  return (
    <div className="flex flex-col min-h-0 h-full">
      <div className="flex items-center justify-between px-1 mb-1.5">
        <span className="text-xs font-bold text-stone-500 dark:text-stone-400 truncate" title={rootName}>
          {rootName}
        </span>
        <div className="flex items-center text-stone-500 dark:text-stone-400">
          <IconButton title="새 파일" onClick={() => startCreate("file", targetDir())}>
            <FilePlus className="w-3.5 h-3.5" />
          </IconButton>
          <IconButton title="새 폴더" onClick={() => startCreate("folder", targetDir())}>
            <FolderPlus className="w-3.5 h-3.5" />
          </IconButton>
          <IconButton title="새로고침" onClick={() => refresh()}>
            <RefreshCw className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`} />
          </IconButton>
          <IconButton title="모두 접기" onClick={() => setExpanded(new Set())}>
            <ChevronsDownUp className="w-3.5 h-3.5" />
          </IconButton>
        </div>
      </div>

      <div className="relative mb-1.5">
        <Search className="w-3.5 h-3.5 text-stone-400 absolute left-2.5 top-1.5" />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Escape" && setQuery("")}
          placeholder="파일 이름 찾기..."
          className="w-full bg-white/80 dark:bg-[#232320] border border-stone-200/90 dark:border-[#343430] rounded-lg pl-8 pr-7 py-1 text-xs text-stone-800 dark:text-stone-200 placeholder:text-stone-400 focus:outline-none focus:border-stone-400"
        />
        {query && (
          <button onClick={() => setQuery("")} className="absolute right-2 top-1.5 text-stone-400 hover:text-stone-700 cursor-pointer">
            <X className="w-3.5 h-3.5" />
          </button>
        )}
      </div>

      <div
        ref={treeRef}
        tabIndex={0}
        onKeyDown={onTreeKeyDown}
        onContextMenu={(e) => openMenu(e, null)}
        className="flex-1 overflow-y-auto min-h-0 outline-none select-none"
      >
        {results ? (
          results.length === 0 ? (
            <div className="text-xs text-stone-400 px-2 py-2">일치하는 파일이 없습니다.</div>
          ) : (
            results.map((rel) => (
              <div
                key={rel}
                draggable
                onDragStart={(e) => {
                  e.dataTransfer.setData(DRAG_MIME, rel);
                  e.dataTransfer.setData("text/plain", rel);
                }}
                onClick={() => {
                  onOpen(rel);
                  setQuery("");
                  setSelected(rel);
                  expandTo(dirOf(rel));
                }}
                className="flex items-center gap-1.5 px-2 py-1 rounded-md cursor-pointer text-xs text-stone-700 dark:text-stone-300 hover:bg-stone-200/70 dark:hover:bg-[#262623]"
                title={rel}
              >
                <FileIcon name={rel} isDir={false} />
                <span className="truncate">{baseName(rel)}</span>
                <span className="truncate text-[10px] text-stone-400">{dirOf(rel)}</span>
              </div>
            ))
          )
        ) : (
          <>
            {renderDir("", 0)}
            {children[""] && children[""].length === 0 && !draft && (
              <div className="text-xs text-stone-400 px-2 py-2">폴더가 비어 있습니다.</div>
            )}
          </>
        )}
      </div>

      {menu && (
        <div
          onMouseDown={(e) => e.stopPropagation()}
          className="fixed z-50 min-w-44 rounded-xl border border-stone-200 dark:border-[#3A3A36] bg-white dark:bg-[#262624] shadow-lg py-1 text-xs text-stone-800 dark:text-stone-100"
          style={{
            left: Math.min(menu.x, window.innerWidth - 200),
            top: Math.min(menu.y, window.innerHeight - (menu.confirmDelete ? 110 : 260)),
          }}
        >
          {menu.confirmDelete && menuNode ? (
            <div className="px-3 py-2 space-y-2 w-60">
              <div>
                <b className="break-all">{menuNode.name}</b>
                {menuNode.isDir ? " 폴더와 안의 모든 파일을" : " 파일을"} 휴지통으로 옮길까요?
                <div className="text-stone-400 mt-0.5">Windows 휴지통에서 복원할 수 있습니다.</div>
              </div>
              <div className="flex justify-end gap-1.5">
                <button onClick={() => setMenu(null)} className="px-2 py-1 rounded-md bg-stone-100 dark:bg-[#32322F] cursor-pointer">
                  취소
                </button>
                <button autoFocus onClick={() => doDelete(menuNode)} className="px-2 py-1 rounded-md bg-red-600 text-white font-semibold cursor-pointer">
                  휴지통으로 이동
                </button>
              </div>
            </div>
          ) : (
            <>
              {menuNode && !menuNode.isDir && (
                <>
                  {onRunPython && /\.pyw?$/i.test(menuNode.name) && (
                    <MenuItem icon={<Play className="w-3.5 h-3.5 text-[#A50034] dark:text-[#FF5C8D]" />} label="Python으로 실행" onClick={() => { setMenu(null); onRunPython(menuNode.relativePath); }} />
                  )}
                  <MenuItem
                    icon={<FileIcon name={menuNode.name} isDir={false} />}
                    label={opensInExternalApp(menuNode.name) ? "기본 앱으로 열기" : "캔버스에서 열기"}
                    onClick={() => { setMenu(null); onOpen(menuNode.relativePath); }}
                  />
                  {onOpenExternal &&
                    !opensInExternalApp(menuNode.name) &&
                    (isPreviewOnlyKind(docKindOf(menuNode.name)) || docKindOf(menuNode.name) === "hwpx") && (
                    <MenuItem
                      icon={<ExternalLink className="w-3.5 h-3.5" />}
                      label={docKindOf(menuNode.name) === "hwpx" ? "한글에서 열기" : "기본 앱으로 열기"}
                      onClick={() => { setMenu(null); onOpenExternal(menuNode.relativePath); }}
                    />
                  )}
                  <MenuItem icon={<Paperclip className="w-3.5 h-3.5" />} label="채팅에 첨부" onClick={() => { setMenu(null); onAttach(menuNode.relativePath); }} />
                  <Separator />
                </>
              )}
              {(!menuNode || menuNode.isDir) && (
                <>
                  <MenuItem icon={<FilePlus className="w-3.5 h-3.5" />} label="새 파일" onClick={() => startCreate("file", targetDir(menuNode))} />
                  <MenuItem icon={<FolderPlus className="w-3.5 h-3.5" />} label="새 폴더" onClick={() => startCreate("folder", targetDir(menuNode))} />
                  <Separator />
                </>
              )}
              {menuNode && (
                <>
                  <MenuItem
                    icon={<Pencil className="w-3.5 h-3.5" />}
                    label="이름 바꾸기"
                    hint="F2"
                    onClick={() => {
                      setMenu(null);
                      setDraft({ mode: "rename", path: menuNode.relativePath, isDir: menuNode.isDir });
                    }}
                  />
                  <MenuItem
                    icon={<Copy className="w-3.5 h-3.5" />}
                    label="경로 복사"
                    onClick={() => {
                      setMenu(null);
                      navigator.clipboard.writeText(menuNode.relativePath).catch(() => {});
                    }}
                  />
                </>
              )}
              <MenuItem
                icon={<ExternalLink className="w-3.5 h-3.5" />}
                label="Windows 탐색기에서 보기"
                onClick={() => {
                  setMenu(null);
                  api.reveal(menuNode?.relativePath ?? "").catch((e) => onError(errText(e)));
                }}
              />
              {!menuNode && (
                <MenuItem icon={<RefreshCw className="w-3.5 h-3.5" />} label="새로고침" onClick={() => { setMenu(null); refresh(); }} />
              )}
              {menuNode && (
                <>
                  <Separator />
                  <MenuItem
                    icon={<Trash2 className="w-3.5 h-3.5 text-red-600" />}
                    label="삭제"
                    hint="Del"
                    danger
                    onClick={() => setMenu({ ...menu, confirmDelete: true })}
                  />
                </>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}

function NameInput({ initial, onCommit, onCancel }: { initial: string; onCommit: (v: string) => void; onCancel: () => void }) {
  const ref = useRef<HTMLInputElement | null>(null);
  const done = useRef(false);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.focus();
    // 확장자 앞까지만 선택해 이름만 바로 고칠 수 있게 한다.
    const dot = initial.lastIndexOf(".");
    el.setSelectionRange(0, dot > 0 ? dot : initial.length);
  }, [initial]);
  const finish = (commit: boolean) => {
    if (done.current) return;
    done.current = true;
    if (commit) onCommit(ref.current?.value ?? "");
    else onCancel();
  };
  return (
    <input
      ref={ref}
      defaultValue={initial}
      onClick={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") finish(true);
        if (e.key === "Escape") finish(false);
      }}
      onBlur={() => finish(true)}
      className="flex-1 min-w-0 bg-white dark:bg-[#1E1E1C] border border-[#A50034] rounded px-1 py-0 text-xs text-stone-900 dark:text-stone-100 focus:outline-none"
    />
  );
}

function IconButton({ title, onClick, children }: { title: string; onClick: () => void; children: ReactNode }) {
  return (
    <button title={title} onClick={onClick} className="p-1 rounded-md hover:bg-stone-200 dark:hover:bg-[#2C2C29] hover:text-stone-800 dark:hover:text-stone-100 cursor-pointer">
      {children}
    </button>
  );
}

function MenuItem({
  icon,
  label,
  hint,
  danger,
  onClick,
}: {
  icon: ReactNode;
  label: string;
  hint?: string;
  danger?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={`w-full flex items-center gap-2 px-3 py-1.5 text-left cursor-pointer hover:bg-stone-100 dark:hover:bg-[#32322F] ${
        danger ? "text-red-600 dark:text-red-400" : ""
      }`}
    >
      <span className="w-3.5 flex justify-center">{icon}</span>
      <span className="flex-1">{label}</span>
      {hint && <span className="text-[10px] text-stone-400">{hint}</span>}
    </button>
  );
}

function Separator() {
  return <div className="my-1 border-t border-stone-100 dark:border-[#363632]" />;
}
