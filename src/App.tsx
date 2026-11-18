import React, { useEffect, useMemo, useRef, useState } from "react";
import { isExternalLink, renderMarkdownSafe } from "./security/sanitize";
import {
  Check,
  ChevronDown,
  ChevronRight,
  Code2,
  Copy,
  Download,
  Edit2,
  FileText,
  FolderOpen,
  MessageSquare,
  Moon,
  Paperclip,
  PanelLeftClose,
  PanelLeftOpen,
  Plug,
  PanelRightClose,
  PanelRightOpen,
  Plus,
  RefreshCw,
  RotateCcw,
  FileDiff,
  Play,
  Save,
  Search,
  Send,
  Settings,
  Sparkles,
  Square,
  Sun,
  Trash2,
  X,
} from "lucide-react";
import { BackendAPI, isTauri } from "./api";
import { bytesToBase64 } from "./docs/base64";
import { applyChunk, countByMode, countRunning, endRun, startRun, type RunMode, type Runs } from "./chat/runs";
import type { AnswerFormat } from "./docs/answerExport";
import {
  canvasDocFromBase64,
  type CanvasDoc,
  type CanvasSavedInfo,
} from "./canvas/canvasDoc";

// 문서 편집 라이브러리(TipTap, docx, pptxgenjs, pdf.js)는 캔버스를 처음 열 때 불러온다.
const DocumentCanvas = React.lazy(() => import("./canvas/DocumentCanvas"));
const DiffViewer = React.lazy(() => import("./diff/DiffViewer"));
const PythonRunDialog = React.lazy(() => import("./python/PythonRunDialog"));
import { startDocBridge } from "./docs/bridge";
import { DRAG_MIME, FileExplorer, type ExplorerApi } from "./explorer/FileExplorer";
import { useFileMention } from "./explorer/FileMention";
import { fileHistory } from "./diff/history";
import { PanelResizer } from "./ui/PanelResizer";
import { SkillsDialog } from "./skills/SkillsDialog";
import { McpDialog } from "./mcp/McpDialog";
import { RuntimePanel } from "./runtime/RuntimePanel";
import { UpdateBanner, UpdateSettings, useUpdater } from "./update/UpdatePanel";
import {
  CANVAS_DEFAULT_WIDTH,
  CANVAS_MIN_WIDTH,
  availableWidth,
  clampCanvasWidth,
  loadCanvasWidth,
  maxCanvasWidth,
  saveCanvasWidth,
  wideCanvasWidth,
} from "./ui/panelWidth";
import { docKindOf, docKindOfLanguage, isBinaryKind, isPythonScript, opensInExternalApp } from "./docs/formats";
import {
  AppSettings,
  ChatMessage,
  Conversation,
  FileSnapshot,
  Project,
  RateInfo,
  ToolExecutionRecord,
  TurnStats,
} from "./types";

/** 메시지에서 캔버스로 열 문서: 파일(경로) 또는 코드 블록(본문) */
type ArtifactRef = Omit<CanvasDoc, "id">;

function extractArtifactFromMessage(msg: ChatMessage): ArtifactRef | null {
  // 가장 마지막에 작성·수정·생성한 파일(캔버스로 열 수 있는 형식)을 연다.
  let lastPath: string | null = null;
  for (const tr of msg.toolRecords || []) {
    if (tr.status !== "completed") continue;
    if ((tr.toolName === "write_file" || tr.toolName === "modify_file") && tr.filePath && !opensInExternalApp(tr.filePath)) {
      lastPath = tr.filePath;
    }
    if (tr.producedFiles && tr.producedFiles.length > 0) {
      for (const f of tr.producedFiles) {
        if (f && !opensInExternalApp(f)) lastPath = f;
      }
    }
  }
  if (lastPath) {
    return {
      title: lastPath.split("/").pop() || lastPath,
      kind: docKindOf(lastPath),
      filePath: lastPath,
    };
  }

  const codeBlockRegex = /```([a-zA-Z0-9_-]*)\n([\s\S]*?)```/g;
  let match: RegExpExecArray | null;
  let bestBlock: ArtifactRef | null = null;
  while ((match = codeBlockRegex.exec(msg.content)) !== null) {
    const lang = match[1] || "text";
    const body = match[2].trim();
    if (body.split("\n").length >= 10) {
      const kind = docKindOfLanguage(lang);
      bestBlock = {
        title:
          kind === "markdown"
            ? "작성된 문서.md"
            : kind === "html"
              ? "작성된 페이지.html"
              : `코드 스니펫 (${lang})`,
        kind,
        text: body,
      };
    }
  }
  return bestBlock;
}

/** 답변 속 링크는 앱 화면을 바꾸지 않고 기본 브라우저로 연다. */
function handleMarkdownLinkClick(e: React.MouseEvent<HTMLElement>) {
  const a = (e.target as HTMLElement).closest("a");
  if (!a) return;
  e.preventDefault();
  const href = a.getAttribute("href");
  if (isExternalLink(href)) BackendAPI.openExternalUrl(href).catch(() => {});
}

function isInsideRoot(root: string, absolute: string): string | null {
  const norm = (p: string) => p.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();
  const r = norm(root);
  const a = absolute.replace(/\//g, "\\");
  if (!norm(a).startsWith(r + "\\")) return null;
  return a.slice(r.length + 1).replace(/\\/g, "/");
}

/** 마지막 답변에 승인을 기다리는 작업이 있는가 */
function waitsForApproval(conv: Conversation): boolean {
  const last = conv.messages[conv.messages.length - 1];
  return !!last && last.role === "assistant" && last.toolRecords.some((r) => r.status === "pending_approval");
}

/** 탭에 표시하는 진행 중인 대화 수 */
function RunBadge({ count }: { count: number }) {
  if (count <= 0) return null;
  return (
    <span
      className="ml-0.5 min-w-[15px] h-[15px] px-1 rounded-full bg-[#A50034] text-white text-[10px] leading-[15px] font-bold text-center"
      title={`진행 중인 대화 ${count}개`}
    >
      {count}
    </span>
  );
}

/**
 * Markdown 답변. 글이 바뀔 때만 다시 변환한다.
 * (답변을 받는 동안 화면이 초당 수십 번 다시 그려지는데, 그때마다 지난 답변을 모두 다시 변환하지 않게 한다.)
 */
const MarkdownBody = React.memo(function MarkdownBody({
  text,
  onClick,
}: {
  text: string;
  onClick: (e: React.MouseEvent<HTMLDivElement>) => void;
}) {
  const html = useMemo(() => renderMarkdownSafe(text), [text]);
  return (
    <div
      className="markdown-body text-sm text-stone-800 dark:text-stone-100 select-text"
      onClick={onClick}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
});

/** 자주 바뀌는 값을 `ms`마다 한 번씩만 화면에 반영한다. */
function useThrottled<T>(value: T, ms: number): T {
  const [shown, setShown] = useState(value);
  const last = useRef(0);
  useEffect(() => {
    const wait = ms - (Date.now() - last.current);
    if (wait <= 0) {
      last.current = Date.now();
      setShown(value);
      return;
    }
    const timer = setTimeout(() => {
      last.current = Date.now();
      setShown(value);
    }, wait);
    return () => clearTimeout(timer);
  }, [value, ms]);
  return shown;
}

/** 받는 중인 답변. 글자 조각이 초당 수십 번 와도 변환은 초당 8번쯤만 한다. */
function StreamingMarkdown(props: { text: string; onClick: (e: React.MouseEvent<HTMLDivElement>) => void }) {
  const text = useThrottled(props.text, 120);
  return <MarkdownBody text={text} onClick={props.onClick} />;
}

function formatDuration(ms: number): string {
  const sec = Math.round(ms / 1000);
  return sec >= 60 ? `${Math.floor(sec / 60)}분 ${sec % 60}초` : `${sec}초`;
}

/** 답변 아래에 작게 보여 주는 호출·토큰·시간 */
function TurnStatsLine({ stats }: { stats?: TurnStats | null }) {
  if (!stats || !stats.apiCalls) return null;
  const parts = [`호출 ${stats.apiCalls}회`];
  if (stats.completionTokens) parts.push(`출력 ${stats.completionTokens.toLocaleString()}토큰`);
  if (stats.elapsedMs) parts.push(formatDuration(stats.elapsedMs));
  return (
    <div
      className="text-[11px] text-stone-400 dark:text-stone-500 tabular-nums select-none"
      title={`이 답변에 쓴 API 호출 ${stats.apiCalls}회 (하루 한도에서 빠집니다) · 입력 ${stats.promptTokens.toLocaleString()}토큰 · 출력 ${stats.completionTokens.toLocaleString()}토큰 (그중 추론 ${stats.reasoningTokens.toLocaleString()}토큰)`}
    >
      {parts.join(" · ")}
    </div>
  );
}

/** K-EXAONE API 키는 발급 30일 뒤 만료된다. 앱에 입력한 지 며칠 됐는지. */
const KEY_VALID_DAYS = 30;
function keyAgeDays(savedAt?: string | null): number | null {
  if (!savedAt) return null;
  const t = new Date(savedAt).getTime();
  return Number.isNaN(t) ? null : Math.floor((Date.now() - t) / (24 * 60 * 60 * 1000));
}

/** 대화 전체를 Markdown 글로 만든다 (내보내기용). */
function conversationToMarkdown(conv: Conversation): string {
  const lines: string[] = [`# ${conv.title || "대화"}`, ""];
  for (const m of conv.messages) {
    if (m.role !== "user" && m.role !== "assistant") continue;
    lines.push(m.role === "user" ? "## 사용자" : "## 엑사원", "");
    lines.push(m.content.trim() || "(내용 없음)", "");
    const files = m.toolRecords.filter((r) => r.filePath && r.status === "completed").map((r) => r.filePath);
    if (files.length) lines.push(`> 이 답변에서 다룬 파일: ${Array.from(new Set(files)).join(", ")}`, "");
  }
  return lines.join("\n");
}

/** 오늘 남은 API 호출 수. 도구를 한 번 쓸 때마다 호출이 한 번씩 들기 때문에 입력란 옆에 늘 보여 준다. */
function RateBadge({ info }: { info: RateInfo | null }) {
  // 받은 지 하루가 넘은 값은 이미 바뀌었을 테니 보이지 않는다.
  if (!info || info.dayRemaining == null) return null;
  if (Date.now() - new Date(info.updatedAt).getTime() > 24 * 60 * 60 * 1000) return null;
  const left = info.dayRemaining;
  const tone =
    left <= 5
      ? "text-red-600 dark:text-red-400"
      : left <= 20
        ? "text-amber-600 dark:text-amber-400"
        : "text-stone-400 dark:text-stone-500";
  return (
    <span
      className={`text-[11px] tabular-nums select-none ${tone}`}
      title="K-EXAONE API가 알려 준 오늘 남은 호출 수입니다. 질문 한 번에 한 번, 파일 읽기·쓰기 같은 도구를 쓸 때마다 한 번씩 더 듭니다."
    >
      오늘 남은 호출 {left}
      {info.dayLimit != null ? `/${info.dayLimit}` : ""}
    </span>
  );
}

/** 오늘 남은 호출이 거의 없으면 보내기 전에 알려 준다. */
function LowBudgetNotice({ info }: { info: RateInfo | null }) {
  if (!info || info.dayRemaining == null || info.dayRemaining > 5) return null;
  if (Date.now() - new Date(info.updatedAt).getTime() > 24 * 60 * 60 * 1000) return null;
  return (
    <div className="text-[11px] text-red-600 dark:text-red-400 pb-0.5">
      {info.dayRemaining <= 0
        ? "오늘 쓸 수 있는 API 호출을 모두 썼습니다. 한도가 다시 채워진 뒤 보내 주세요."
        : `오늘 남은 API 호출이 ${info.dayRemaining}회입니다. 파일을 여러 번 다루는 작업은 중간에 멈출 수 있습니다.`}
    </div>
  );
}

/** 프로젝트에서 "항상 허용"할 수 있는 도구 */
const AUTO_APPROVE_TOOLS = [
  { tool: "run_python", label: "Python 스크립트 실행" },
  { tool: "run_command", label: "PowerShell 명령 실행" },
];

/** K-EXAONE 2.5가 받는 추론 강도 (공식 문서: low / high / max, 서버 기본값은 max) */
const REASONING_EFFORTS: { value: AppSettings["reasoningEffort"]; label: string; hint: string }[] = [
  { value: "low", label: "낮음", hint: "추론을 짧게 합니다. 간단한 질문에 빠르게 답할 때 고르세요." },
  { value: "high", label: "높음", hint: "더 깊게 추론합니다. 보고서·코드·여러 단계 작업에 알맞습니다." },
  { value: "max", label: "최대", hint: "가장 길게 추론합니다. 정확도가 중요한 어려운 문제용이며 가장 느립니다." },
];

export default function App() {
  const [isDark, setIsDark] = useState<boolean>(() => {
    return localStorage.getItem("exaone_theme") === "dark";
  });

  useEffect(() => {
    localStorage.setItem("exaone_theme", isDark ? "dark" : "light");
    if (isDark) {
      document.documentElement.classList.add("dark");
    } else {
      document.documentElement.classList.remove("dark");
    }
  }, [isDark]);

  const [sidebarCollapsed, setSidebarCollapsed] = useState<boolean>(() => {
    try {
      return localStorage.getItem("exaone_sidebar_collapsed") === "true";
    } catch {
      return false;
    }
  });

  useEffect(() => {
    try {
      localStorage.setItem("exaone_sidebar_collapsed", sidebarCollapsed ? "true" : "false");
    } catch {
      // 저장하지 못해도 이번 실행에서는 그대로 동작한다.
    }
  }, [sidebarCollapsed]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "b") {
        const target = e.target as HTMLElement | null;
        if (target && (target.isContentEditable || target.closest(".ProseMirror") || target.closest("[role='dialog']"))) {
          return;
        }
        e.preventDefault();
        setSidebarCollapsed((prev) => !prev);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const [settings, setSettings] = useState<AppSettings>({
    apiKey: "",
    baseUrl: "https://api.exaone.ai/v1/chat/completions",
    model: "k-exaone_v2.5",
    reasoningEffort: "high",
    writingStyle: "default",
  });

  // 3 Core Modes: "chat" | "project" | "code"
  const [activeMode, setActiveMode] = useState<"chat" | "project" | "code">(
    "chat"
  );
  const [showProjectOverview, setShowProjectOverview] =
    useState<boolean>(false);

  const [projects, setProjects] = useState<Project[]>([]);
  const [activeProjectId, setActiveProjectId] = useState<string>("");
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [activeConvId, setActiveConvId] = useState<string>("");
  const [snapshots, setSnapshots] = useState<FileSnapshot[]>([]);
  const [searchQuery, setSearchQuery] = useState<string>("");

  // Composer & Streaming State
  const [inputPrompt, setInputPrompt] = useState<string>("");
  const [attachedFiles, setAttachedFiles] = useState<string[]>([]);
  /** 대화별로 진행 중인 답변 생성 (다른 대화로 옮겨도 백그라운드에서 계속된다) */
  const [runs, setRuns] = useState<Runs>({});
  /** 대화별 마지막 실행 오류 */
  const [runErrors, setRunErrors] = useState<Record<string, string>>({});
  /** 다른 대화를 보는 사이 끝난 대화 (사이드바 표시) */
  const [unseenDone, setUnseenDone] = useState<Record<string, true>>({});
  /** 실패해서 돌려줄 입력 (그 대화로 돌아오면 입력창에 채운다) */
  const failedDrafts = useRef<Record<string, string>>({});
  const [errorBanner, setErrorBanner] = useState<string | null>(null);

  // Inline prompt editing
  const [editingMessageId, setEditingMessageId] = useState<string | null>(null);
  const [editingMessageText, setEditingMessageText] = useState<string>("");
  const [copiedId, setCopiedId] = useState<string | null>(null);
  /** 저장 형식 메뉴를 연 답변 */
  const [answerSaveMenu, setAnswerSaveMenu] = useState<string | null>(null);
  /** 아래 공간이 모자라면 메뉴를 위로 연다. */
  const [answerSaveMenuUp, setAnswerSaveMenuUp] = useState(false);
  const [savingAnswerId, setSavingAnswerId] = useState<string | null>(null);
  /** 방금 저장한 답변 파일 */
  const [savedAnswer, setSavedAnswer] = useState<{ name: string; absolutePath: string; relativePath: string | null } | null>(null);

  // Collapsible Claude-style Right Artifact Drawer
  const [canvasExpanded, setCanvasExpanded] = useState<boolean>(false);
  /** 사용자가 경계선을 끌어 정한 캔버스 폭 (다음 실행에도 유지) */
  const [canvasWidth, setCanvasWidth] = useState<number>(loadCanvasWidth);
  const [windowWidth, setWindowWidth] = useState<number>(() => window.innerWidth);
  const canvasRef = useRef<HTMLElement>(null);
  // 사이드바 하단: 대화 목록 / 프로젝트 파일 탐색기
  const [sidebarTab, setSidebarTab] = useState<"chats" | "files">("chats");
  const [fileTreeVersion, setFileTreeVersion] = useState(0);
  const bumpFileTree = () => setFileTreeVersion((v) => v + 1);
  const composerRef = useRef<HTMLTextAreaElement | null>(null);

  // 입력란 높이를 내용에 맞춘다: 기본 3줄, 늘어나면 화면 높이의 40%까지 커지고 그 뒤로는 스크롤.
  const fitComposer = React.useCallback(() => {
    const el = composerRef.current;
    if (!el) return;
    el.style.height = "auto";
    const max = Math.round(window.innerHeight * 0.4);
    const next = Math.min(el.scrollHeight, max);
    el.style.height = `${next}px`;
    el.style.overflowY = el.scrollHeight > max ? "auto" : "hidden";
  }, []);
  React.useLayoutEffect(fitComposer, [inputPrompt, fitComposer]);
  useEffect(() => {
    window.addEventListener("resize", fitComposer);
    return () => window.removeEventListener("resize", fitComposer);
  }, [fitComposer]);
  const [activeArtifact, setActiveArtifact] = useState<CanvasDoc | null>(
    null
  );
  // Modals
  const [showSettingsModal, setShowSettingsModal] = useState<boolean>(false);
  const [showSkills, setShowSkills] = useState(false);
  const [showMcp, setShowMcp] = useState(false);
  /** 변경 보기 창: 함께 볼 변경 기록들과 처음 보여 줄 기록 */
  const [diffView, setDiffView] = useState<{ ids: string[]; initial: string; variant?: "files" | "history" } | null>(null);
  /** Python 실행 창에 띄운 스크립트 (프로젝트 기준 상대 경로) */
  const [pythonRun, setPythonRun] = useState<string | null>(null);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  /** 저장하지 않고 닫으면 입력하던 키는 버린다. */
  function closeSettingsModal() {
    setSettings((s) => ({ ...s, apiKey: "" }));
    setSettingsError(null);
    setShowSettingsModal(false);
  }
  const [showProjectModal, setShowProjectModal] = useState<boolean>(false);
  const [editingProject, setEditingProject] = useState<{
    id?: string;
    name: string;
    rootPath: string;
    instructions: string;
  }>({ name: "", rootPath: "", instructions: "" });

  const [expandedReasoning, setExpandedReasoning] = useState<
    Record<string, boolean>
  >({});

  const chatEndRef = useRef<HTMLDivElement | null>(null);

  const activeProject = useMemo(
    () => projects.find((p) => p.id === activeProjectId) || projects[0],
    [projects, activeProjectId]
  );

  // In "chat" mode, conversations are independent of any project ("general-chat")
  const currentScopeId = useMemo(
    () => (activeMode === "chat" ? "general-chat" : activeProject?.id || ""),
    [activeMode, activeProject]
  );

  const filteredConversations = useMemo(() => {
    const list = conversations.filter((c) =>
      activeMode === "chat"
        ? c.projectId === "general-chat"
        : activeProject && c.projectId === activeProject.id
    );
    if (!searchQuery.trim()) return list;
    const q = searchQuery.toLowerCase();
    return list.filter(
      (c) => c.title.toLowerCase().includes(q) || c.messages.some((m) => m.content.toLowerCase().includes(q))
    );
  }, [conversations, activeMode, activeProject, searchQuery]);

  const activeConversation = useMemo(
    () =>
      filteredConversations.find((c) => c.id === activeConvId) ||
      filteredConversations[0] ||
      null,
    [filteredConversations, activeConvId]
  );

  // 지금 보고 있는 대화의 실행 상태
  const shownConvId = activeConversation?.id ?? "";
  const shownConvIdRef = useRef(shownConvId);
  shownConvIdRef.current = shownConvId;
  const activeRun = shownConvId ? runs[shownConvId] : undefined;
  const isStreaming = !!activeRun;
  const streamingReasoning = activeRun?.reasoning ?? "";
  const streamingContent = activeRun?.content ?? "";
  const streamingTools = activeRun?.tools ?? [];
  const retryNotice = activeRun?.notice ?? "";
  const runCounts = useMemo(() => countByMode(runs), [runs]);
  const runningByProject = useMemo(() => {
    const ids: Record<string, string[]> = {};
    for (const c of conversations) (ids[c.projectId] ??= []).push(c.id);
    const out: Record<string, number> = {};
    for (const [pid, list] of Object.entries(ids)) out[pid] = countRunning(runs, list);
    return out;
  }, [conversations, runs]);

  // 대화를 열면 '끝남' 표시를 지우고, 실패해서 돌려줄 입력이 있으면 채운다.
  useEffect(() => {
    if (!shownConvId) return;
    setUnseenDone((prev) => {
      if (!prev[shownConvId]) return prev;
      const next = { ...prev };
      delete next[shownConvId];
      return next;
    });
    const draft = failedDrafts.current[shownConvId];
    if (draft) {
      delete failedDrafts.current[shownConvId];
      setInputPrompt((prev) => (prev.trim() ? prev : draft));
    }
  }, [shownConvId]);

  /** 실행 하나를 마친다. 다른 대화를 보고 있었다면 사이드바에 표시한다. */
  function finishRun(convId: string, failure?: { error: string; text?: string }) {
    setRuns((prev) => endRun(prev, convId));
    setFileTreeVersion((v) => v + 1);
    const shown = shownConvIdRef.current === convId;
    if (!shown) setUnseenDone((prev) => ({ ...prev, [convId]: true }));
    setRunErrors((prev) => {
      if (!failure && !(convId in prev)) return prev;
      const next = { ...prev };
      if (failure) next[convId] = failure.error;
      else delete next[convId];
      return next;
    });
    if (failure?.text) {
      const text = failure.text;
      if (shown) setInputPrompt((prev) => (prev.trim() ? prev : text));
      else failedDrafts.current[convId] = text;
    }
  }

  function modeOfConversation(conv: Conversation): RunMode {
    return conv.projectId === "general-chat" ? "chat" : activeMode === "code" ? "code" : "project";
  }

  // 프로젝트 파일 탐색기 (일반 채팅에서는 사용하지 않음)
  const explorerProjectId = currentScopeId !== "general-chat" ? activeProject?.id : undefined;
  const explorerApi = useMemo<ExplorerApi | null>(() => {
    if (!explorerProjectId) return null;
    const id = explorerProjectId;
    return {
      listDir: (rel) => BackendAPI.listProjectDir(id, rel),
      createFile: (rel) => BackendAPI.createProjectFile(id, rel),
      createFolder: (rel) => BackendAPI.createProjectFolder(id, rel),
      rename: (from, to) => BackendAPI.renameProjectPath(id, from, to),
      trash: (rel) => BackendAPI.trashProjectPath(id, rel),
      reveal: (rel) => BackendAPI.revealProjectPath(id, rel),
      find: (q) => BackendAPI.findProjectFiles(id, q),
    };
  }, [explorerProjectId]);

  const attachFile = (path: string) =>
    setAttachedFiles((prev) => (prev.includes(path) ? prev : [...prev, path]));

  const mention = useFileMention({
    search: explorerApi ? explorerApi.find : null,
    value: inputPrompt,
    setValue: setInputPrompt,
    textareaRef: composerRef,
    onPick: attachFile,
  });

  // 다른 프로그램에서 파일을 바꿨을 수 있으니 창으로 돌아오면 파일 목록을 다시 읽는다.
  useEffect(() => {
    const onFocus = () => setFileTreeVersion((v) => v + 1);
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  const isUnderPath = (path: string, dir: string) => path === dir || path.startsWith(dir + "/");

  function handleExplorerRenamed(from: string, to: string) {
    setActiveArtifact((prev) =>
      prev?.filePath && isUnderPath(prev.filePath, from)
        ? {
            ...prev,
            filePath: to + prev.filePath.slice(from.length),
            title: (to + prev.filePath.slice(from.length)).split("/").pop() || prev.title,
          }
        : prev
    );
    setAttachedFiles((prev) => prev.map((f) => (isUnderPath(f, from) ? to + f.slice(from.length) : f)));
  }

  function handleExplorerDeleted(rel: string) {
    // 캔버스의 편집 내용은 남겨 두고, 저장하면 새 위치를 묻도록 경로만 지운다.
    setActiveArtifact((prev) =>
      prev?.filePath && isUnderPath(prev.filePath, rel) ? { ...prev, filePath: undefined } : prev
    );
    setAttachedFiles((prev) => prev.filter((f) => !isUnderPath(f, rel)));
  }

  // Initial load
  useEffect(() => {
    (async () => {
      try {
        const db = await BackendAPI.getInitialState();
        // 저장된 API 키는 화면으로 오지 않는다 (apiKeySet으로 존재 여부만 안다).
        setSettings({ ...db.settings, apiKey: "" });
        setProjects(db.projects);
        setConversations(db.conversations);
        setSnapshots(db.snapshots);
        if (db.projects.length > 0) {
          setActiveProjectId(db.projects[0].id);
        }
        const firstGeneralConv = db.conversations.find(
          (c) => c.projectId === "general-chat"
        );
        if (firstGeneralConv) {
          setActiveConvId(firstGeneralConv.id);
        }
        // 화면을 새로 고쳐도 백엔드에서 진행 중인 대화는 계속 표시한다.
        const running = await BackendAPI.runningConversations();
        if (running.length > 0) {
          setRuns((prev) => {
            let next = prev;
            for (const id of running) {
              if (next[id]) continue;
              const conv = db.conversations.find((c) => c.id === id);
              const mode: RunMode = !conv || conv.projectId === "general-chat" ? "chat" : "project";
              next = startRun(next, id, mode, { notice: "백그라운드에서 진행 중입니다...", restored: true });
            }
            return next;
          });
        }
      } catch (err: any) {
        setErrorBanner(err?.message || String(err));
      }
    })();
  }, []);

  // 새 버전 확인과 설치 (설정을 읽은 뒤, 켜져 있으면 한 번 조용히 확인한다)
  const updater = useUpdater(settings.checkUpdates);

  // API 키를 입력한 지 며칠 됐는지 (발급 30일 뒤 만료되므로 가까워지면 알린다)
  const keyDays = settings.apiKeySet ? keyAgeDays(settings.apiKeySavedAt) : null;

  // 서버가 알려 준 호출 한도 (오늘 남은 호출 수 표시용)
  const [rateInfo, setRateInfo] = useState<RateInfo | null>(null);
  useEffect(() => {
    BackendAPI.getRateInfo()
      .then((info) => info && setRateInfo(info))
      .catch(() => {});
  }, []);

  // Safe single event listener registration
  useEffect(() => {
    let cancelled = false;
    let unlistenFn: (() => void) | null = null;

    BackendAPI.onExaoneStream((payload) => {
      if (cancelled) return;
      if (payload.rateInfo) setRateInfo(payload.rateInfo);
      setRuns((prev) => applyChunk(prev, payload));
      if (payload.toolRecord) {
        const r = payload.toolRecord;
        if (r.status === "completed" && ["write_file", "modify_file", "delete_path"].includes(r.toolName)) {
          setFileTreeVersion((v) => v + 1);
        }
      }
      if (payload.snapshot) {
        setSnapshots((prev) => [...prev, payload.snapshot!]);
      }
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlistenFn = fn;
      }
    });

    return () => {
      cancelled = true;
      if (unlistenFn) unlistenFn();
    };
  }, []);

  // 새로 고침 뒤 되살린 실행은 응답을 직접 받지 못하므로, 끝났는지 확인해 저장된 결과를 읽는다.
  const runsRef = useRef(runs);
  runsRef.current = runs;
  const hasRestoredRuns = Object.values(runs).some((r) => r.restored);
  useEffect(() => {
    if (!hasRestoredRuns) return;
    const timer = setInterval(async () => {
      try {
        const running = new Set(await BackendAPI.runningConversations());
        const finished = Object.values(runsRef.current)
          .filter((r) => r.restored && !running.has(r.conversationId))
          .map((r) => r.conversationId);
        if (finished.length === 0) return;
        const db = await BackendAPI.getInitialState();
        setConversations(db.conversations);
        setSnapshots(db.snapshots);
        finished.forEach((id) => finishRun(id));
      } catch {
        // 다음 확인 때 다시 시도
      }
    }, 1500);
    return () => clearInterval(timer);
  }, [hasRestoredRuns]);

  // AI가 DOCX/PPTX/PDF를 읽고 쓸 때 쓰는 문서 변환기 (웹뷰에서 실행)
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    startDocBridge().then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // 새 메시지가 생기면 맨 아래로 간다.
  useEffect(() => {
    chatEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [activeConversation?.messages]);

  // 답변을 받는 동안에는 맨 아래를 보고 있을 때만 따라 내려간다 (위쪽을 읽는 중이면 끌어내리지 않는다).
  useEffect(() => {
    const end = chatEndRef.current;
    const scroller = end?.closest(".overflow-y-auto");
    if (!end || !scroller) return;
    const distance = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight;
    if (distance < 240) end.scrollIntoView({ behavior: "auto" });
  }, [streamingReasoning, streamingContent, streamingTools]);

  async function handleSwitchMode(mode: "chat" | "project" | "code") {
    setActiveMode(mode);
    setShowProjectOverview(mode === "project");
    const targetScope =
      mode === "chat" ? "general-chat" : activeProject?.id || "";
    const firstConv = conversations.find((c) => c.projectId === targetScope);
    setActiveConvId(firstConv ? firstConv.id : "");
    // 화면에 맞는 지침은 메시지를 보낼 때 화면 이름(mode)으로 고른다.
  }

  /** 입력란에서 고른 추론 강도를 저장한다. 다음에 보내는 메시지부터 적용된다. */
  async function changeReasoningEffort(effort: AppSettings["reasoningEffort"]) {
    if (effort === settings.reasoningEffort) return;
    setSettings((prev) => ({ ...prev, reasoningEffort: effort }));
    try {
      // 키는 보내지 않는다 (비어 있으면 저장된 키를 그대로 둔다).
      await BackendAPI.saveSettings({ ...settings, apiKey: "", reasoningEffort: effort });
    } catch (err) {
      setErrorBanner(String(err));
    }
  }

  async function handleNewChat() {
    if (!currentScopeId) return;
    setShowProjectOverview(false);
    const conv = await BackendAPI.createConversation(
      currentScopeId,
      "새 대화"
    );
    setConversations((prev) => [conv, ...prev]);
    setActiveConvId(conv.id);
  }

  async function handleBrowseFolderForModal() {
    const picked = await BackendAPI.pickFolderDialog(editingProject.rootPath);
    if (picked) {
      const folderName =
        picked.split(/[\\/]/).filter(Boolean).pop() || "새 프로젝트";
      setEditingProject((prev) => ({
        ...prev,
        rootPath: picked,
        name: prev.name.trim() ? prev.name : folderName,
      }));
    }
  }

  async function handleBrowseFolderForActiveProject() {
    if (!activeProject) return;
    const picked = await BackendAPI.pickFolderDialog(activeProject.rootPath);
    if (picked) {
      const updated = await BackendAPI.createOrUpdateProject({
        id: activeProject.id,
        name: activeProject.name,
        rootPath: picked,
        instructions: activeProject.instructions,
      });
      setProjects((prev) =>
        prev.map((p) => (p.id === updated.id ? updated : p))
      );
    }
  }

  async function handlePickAttachments() {
    const files = await BackendAPI.pickFilesDialog(activeProject?.rootPath);
    if (files.length > 0) {
      setAttachedFiles((prev) => Array.from(new Set([...prev, ...files])));
    }
  }

  /** 프로젝트 안의 파일을 형식에 맞는 캔버스로 연다. */
  async function openFileInArtifact(filePath: string) {
    if (!activeProject) return;
    try {
      const kind = docKindOf(filePath);
      const b64 = await BackendAPI.readProjectFileBase64(activeProject.id, filePath);
      setActiveArtifact(
        canvasDocFromBase64(
          { title: filePath.split("/").pop() || filePath, kind, filePath },
          b64,
          isBinaryKind(kind)
        )
      );
    } catch (err: any) {
      setErrorBanner(String(err));
    }
  }

  /** 프로젝트 파일 열기: 옛 Office·미디어 형식처럼 캔버스로 못 여는 파일은 Windows 기본 앱으로 연다. */
  function openAnyProjectFile(filePath: string) {
    if (opensInExternalApp(filePath) && activeProject) {
      BackendAPI.openProjectFileExternal(activeProject.id, filePath).catch((err) => setErrorBanner(String(err)));
    } else {
      openFileInArtifact(filePath);
    }
  }

  /** 파일 열기 창에서 문서를 골라 캔버스로 연다 (프로젝트 밖 파일도 가능). */
  async function handleOpenDocument() {
    const root = activeProject?.rootPath;
    const files = await BackendAPI.pickFilesDialog(root);
    const picked = files[0];
    if (!picked) return;
    const rel = root && currentScopeId !== "general-chat" ? isInsideRoot(root, picked) : null;
    if (rel) {
      await openFileInArtifact(rel);
      return;
    }
    try {
      const kind = docKindOf(picked);
      const b64 = await BackendAPI.readAbsoluteFileBase64(picked);
      setActiveArtifact(
        canvasDocFromBase64(
          { title: picked.split(/[\\/]/).pop() || picked, kind, absolutePath: picked },
          b64,
          isBinaryKind(kind)
        )
      );
    } catch (err: any) {
      setErrorBanner(String(err));
    }
  }

  function openArtifactRef(art: ArtifactRef) {
    if (art.filePath) {
      openFileInArtifact(art.filePath);
    } else {
      setActiveArtifact({ ...art, id: `msg#${Date.now()}` });
    }
  }

  function handleCanvasSaved(info: CanvasSavedInfo) {
    bumpFileTree();
    if (info.snapshot) setSnapshots((prev) => [...prev, info.snapshot!]);
    if (info.replacesCurrent) {
      setActiveArtifact((prev) =>
        prev
          ? {
              ...prev,
              title: info.title,
              filePath: info.relativePath ?? undefined,
              absolutePath: info.relativePath ? undefined : info.absolutePath,
            }
          : prev
      );
    }
  }

  async function handleSend(overrideText?: string) {
    const text = (overrideText ?? inputPrompt).trim();
    if (!text || !currentScopeId || isStreaming) return;

    let convId: string | null = null;
    let failure: { error: string; text?: string } | undefined;
    try {
      setShowProjectOverview(false);
      setErrorBanner(null);
      let targetConv = activeConversation;
      if (!targetConv) {
        targetConv = await BackendAPI.createConversation(
          currentScopeId,
          text.slice(0, 28)
        );
        setConversations((prev) => [targetConv!, ...prev]);
        setActiveConvId(targetConv.id);
      }
      const id = targetConv.id;
      convId = id;

      const filesToSend = [...attachedFiles];
      if (!overrideText) {
        setInputPrompt("");
      }
      setAttachedFiles([]);
      setRunErrors((prev) => {
        if (!(id in prev)) return prev;
        const next = { ...prev };
        delete next[id];
        return next;
      });
      setRuns((prev) => startRun(prev, id, modeOfConversation(targetConv!), { question: text }));
      // 첫 질문이면 사이드바 제목을 바로 바꿔 동시에 진행 중인 대화를 구분할 수 있게 한다 (백엔드도 같은 제목을 저장).
      if (targetConv.messages.length === 0) {
        const title = text.slice(0, 28) || "새 대화";
        setConversations((prev) => prev.map((c) => (c.id === id ? { ...c, title } : c)));
      }

      const updatedConv = await BackendAPI.sendChatMessage(id, text, filesToSend, activeMode);

      setConversations((prev) => {
        const exists = prev.some((c) => c.id === updatedConv.id);
        if (!exists) return [updatedConv, ...prev];
        return prev.map((c) => (c.id === updatedConv.id ? updatedConv : c));
      });

      // 결과 문서는 그 대화를 보고 있을 때만 캔버스에 연다 (다른 작업을 가리지 않도록).
      const lastMsg = updatedConv.messages[updatedConv.messages.length - 1];
      if (lastMsg && lastMsg.role === "assistant" && shownConvIdRef.current === id) {
        const art = extractArtifactFromMessage(lastMsg);
        if (art) openArtifactRef(art);
      }
    } catch (err: any) {
      const msg = String(err);
      if (convId) {
        failure = { error: msg, text: overrideText ? undefined : text };
      } else {
        if (!overrideText) setInputPrompt(text);
        setErrorBanner(msg);
      }
      if (msg.includes("API 키")) {
        setShowSettingsModal(true);
      }
    } finally {
      if (convId) finishRun(convId, failure);
    }
  }

  async function handleEditAndResend(messageId: string, overrideText?: string) {
    const newText = (overrideText ?? editingMessageText).trim();
    if (!newText || !activeConversation || isStreaming) return;

    const convId = activeConversation.id;
    const previousMessages = [...activeConversation.messages];
    const msgIdx = previousMessages.findIndex((m) => m.id === messageId);
    if (msgIdx < 0) return;

    const truncated = [
      ...previousMessages.slice(0, msgIdx),
      {
        ...previousMessages[msgIdx],
        content: newText,
      },
    ];

    setConversations((prev) =>
      prev.map((c) => (c.id === convId ? { ...c, messages: truncated } : c))
    );

    setEditingMessageId(null);
    setEditingMessageText("");
    setErrorBanner(null);
    setRuns((prev) => startRun(prev, convId, modeOfConversation(activeConversation)));

    let failure: { error: string } | undefined;
    try {
      const updatedConv = await BackendAPI.editAndResendMessage(
        convId,
        messageId,
        newText
      );
      setConversations((prev) =>
        prev.map((c) => (c.id === updatedConv.id ? updatedConv : c))
      );
    } catch (err: any) {
      setConversations((prev) =>
        prev.map((c) =>
          c.id === convId ? { ...c, messages: previousMessages } : c
        )
      );
      failure = { error: String(err) };
    } finally {
      finishRun(convId, failure);
    }
  }

  async function handleRegenerateFromAssistantIndex(assistantIdx: number) {
    if (!activeConversation || isStreaming) return;
    for (let i = assistantIdx - 1; i >= 0; i--) {
      const m = activeConversation.messages[i];
      if (m.role === "user") {
        await handleEditAndResend(m.id, m.content);
        return;
      }
    }
  }

  /** 보고 있는 대화의 답변 생성만 멈춘다 (다른 대화는 계속된다). */
  async function handleStopGeneration() {
    if (shownConvId) await BackendAPI.cancelGeneration(shownConvId);
  }

  async function handleToolApproval(
    messageId: string,
    toolId: string,
    approve: boolean,
    always = false
  ) {
    if (!activeConversation || isStreaming) return;
    const convId = activeConversation.id;
    // "항상 허용"을 누른 도구가 프로젝트 단위로 허용되는 것(Python·명령 실행)이면 그 이름
    const approvedTool = activeConversation.messages
      .find((m) => m.id === messageId)
      ?.toolRecords.find((r) => r.id === toolId)?.toolName;
    const alwaysTool = AUTO_APPROVE_TOOLS.find((t) => t.tool === approvedTool)?.tool;

    // 승인/거부 직후 화면에 바로 반영하고, 마지막 대기 작업이면 백엔드가 에이전트를 이어서 실행한다.
    setConversations((prev) =>
      prev.map((c) =>
        c.id !== convId
          ? c
          : {
              ...c,
              messages: c.messages.map((m) =>
                m.id !== messageId
                  ? m
                  : {
                      ...m,
                      toolRecords: m.toolRecords.map((r) =>
                        r.id === toolId
                          ? {
                              ...r,
                              status: approve ? "running" : "rejected",
                              output: approve
                                ? "실행 중입니다..."
                                : "사용자가 실행을 거부했습니다.",
                            }
                          : r
                      ),
                    }
              ),
            }
      )
    );

    setErrorBanner(null);
    setRuns((prev) =>
      startRun(prev, convId, modeOfConversation(activeConversation), {
        notice: approve ? "승인된 작업을 실행하고 있습니다..." : "",
      })
    );

    let failure: { error: string } | undefined;
    try {
      const updated = await BackendAPI.resolveToolApproval(
        convId,
        messageId,
        toolId,
        approve,
        always
      );
      setConversations((prev) =>
        prev.map((c) => (c.id === updated.id ? updated : c))
      );
      if (approve && always && alwaysTool && activeProject) {
        const pid = activeProject.id;
        setProjects((prev) =>
          prev.map((p) =>
            p.id === pid ? { ...p, autoApprove: Array.from(new Set([...(p.autoApprove ?? []), alwaysTool])) } : p
          )
        );
      }
      const lastMsg = updated.messages[updated.messages.length - 1];
      if (approve && !waitsForApproval(updated) && lastMsg && lastMsg.role === "assistant" && shownConvIdRef.current === convId) {
        const art = extractArtifactFromMessage(lastMsg);
        if (art) openArtifactRef(art);
      }
    } catch (err: any) {
      failure = { error: String(err) };
      // 도구 결과는 저장됐지만 이어가기가 실패했을 수 있으므로 이 대화만 저장된 상태로 다시 맞춘다.
      try {
        const db = await BackendAPI.getInitialState();
        const saved = db.conversations.find((c) => c.id === convId);
        if (saved) setConversations((prev) => prev.map((c) => (c.id === convId ? saved : c)));
        setSnapshots(db.snapshots);
      } catch {
        // ignore
      }
    } finally {
      finishRun(convId, failure);
    }
  }

  async function handleUndoSnapshot(snapshotId: string): Promise<boolean> {
    try {
      await BackendAPI.undoFileSnapshot(snapshotId);
      bumpFileTree();
      if (activeArtifact?.filePath && activeProject) {
        await openFileInArtifact(activeArtifact.filePath);
      }
      return true;
    } catch (err: any) {
      setErrorBanner(String(err));
      return false;
    }
  }

  /** 도구 기록들 가운데 파일을 바꾼 기록(스냅샷)들로 변경 보기 창을 연다. */
  function openDiffView(records: ToolExecutionRecord[], initial?: string) {
    const ids = records.map((r) => r.snapshotId).filter((id): id is string => !!id);
    if (ids.length === 0) return;
    setDiffView({ ids, initial: initial ?? ids[0] });
  }

  useEffect(() => {
    const onResize = () => setWindowWidth(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  // 창 크기가 바뀌어도 채팅창이 너무 좁아지지 않도록 화면에 쓰는 폭만 줄인다 (저장된 폭은 그대로).
  const availWidth = availableWidth(windowWidth, sidebarCollapsed);
  const canvasMaxWidth = maxCanvasWidth(availWidth);
  const shownCanvasWidth = canvasExpanded
    ? wideCanvasWidth(windowWidth, sidebarCollapsed)
    : clampCanvasWidth(canvasWidth, availWidth);

  function changeCanvasWidth(width: number) {
    const w = clampCanvasWidth(width, availableWidth(window.innerWidth, sidebarCollapsed));
    setCanvasWidth(w);
    setCanvasExpanded(false);
    saveCanvasWidth(w);
  }

  /** 캔버스에 열린 파일의 변경 기록 (AI 도구가 바꾼 것과 캔버스에서 직접 저장한 것 모두, 최신순) */
  const activeFileHistory = useMemo(
    () =>
      activeArtifact?.filePath && activeProject && currentScopeId !== "general-chat"
        ? fileHistory(snapshots, activeProject.id, activeArtifact.filePath)
        : [],
    [snapshots, activeArtifact?.filePath, activeProject, currentScopeId]
  );

  function handleCopyText(id: string, text: string) {
    navigator.clipboard.writeText(text);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 1800);
  }

  /** 답변을 .md 또는 .docx 파일로 저장한다 (엑사원을 거치지 않고 앱이 직접 변환). */
  async function handleSaveAnswer(msgIndex: number, format: AnswerFormat) {
    setAnswerSaveMenu(null);
    const messages = activeConversation?.messages ?? [];
    const msg = messages[msgIndex];
    if (!msg?.content.trim() || savingAnswerId) return;
    const question = messages
      .slice(0, msgIndex)
      .reverse()
      .find((m) => m.role === "user")?.content;
    const dirKey = `exaone_answer_save_dir:${currentScopeId || "general-chat"}`;
    try {
      const { ANSWER_FORMATS, exportAnswer, parentDir, suggestAnswerFileName } = await import("./docs/answerExport");
      const spec = ANSWER_FORMATS[format];
      const baseName = suggestAnswerFileName(msg.content, question);
      const fileName = `${baseName}.${spec.ext}`;

      if (!isTauri()) {
        // 브라우저 미리보기: 내려받기로 대신한다.
        const bytes = await exportAnswer(msg.content, format, baseName);
        const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: spec.mime }));
        const a = document.createElement("a");
        a.href = url;
        a.download = fileName;
        a.click();
        setTimeout(() => URL.revokeObjectURL(url), 1000);
        setSavedAnswer({ name: fileName, absolutePath: fileName, relativePath: null });
        return;
      }

      let lastDir: string | null = null;
      try {
        lastDir = localStorage.getItem(dirKey);
      } catch {
        /* 저장소를 못 써도 저장은 된다 */
      }
      const projectRoot = currentScopeId !== "general-chat" ? activeProject?.rootPath : undefined;
      const picked = await BackendAPI.pickSavePath(lastDir || projectRoot, fileName, spec.filter);
      if (!picked) return;
      setSavingAnswerId(msg.id);
      const title = (picked.split(/[\\/]/).pop() || baseName).replace(/\.[^.]+$/, "");
      const bytes = await exportAnswer(msg.content, format, title);
      const projectId = currentScopeId !== "general-chat" ? activeProject?.id ?? null : null;
      const res = await BackendAPI.saveFileToPath(projectId, picked, bytesToBase64(bytes));
      if (res.snapshot) setSnapshots((prev) => [...prev, res.snapshot!]);
      if (res.relativePath) bumpFileTree();
      const dir = parentDir(res.absolutePath);
      try {
        if (dir) localStorage.setItem(dirKey, dir);
      } catch {
        /* 무시 */
      }
      setErrorBanner(null);
      setSavedAnswer({
        name: res.absolutePath.split(/[\\/]/).pop() || res.absolutePath,
        absolutePath: res.absolutePath,
        relativePath: res.relativePath ?? null,
      });
    } catch (err: any) {
      setErrorBanner(`답변을 저장하지 못했습니다: ${err?.message || String(err)}`);
    } finally {
      setSavingAnswerId(null);
    }
  }

  /** 지금 대화 전체를 Markdown 파일로 저장한다. */
  async function handleExportConversation() {
    const conv = activeConversation;
    if (!conv || conv.messages.length === 0) return;
    const text = conversationToMarkdown(conv);
    const fileName = `${(conv.title || "대화").replace(/[\\/:*?"<>|]/g, " ").trim().slice(0, 40) || "대화"}.md`;
    try {
      if (!isTauri()) {
        const url = URL.createObjectURL(new Blob([text], { type: "text/markdown" }));
        const a = document.createElement("a");
        a.href = url;
        a.download = fileName;
        a.click();
        setTimeout(() => URL.revokeObjectURL(url), 1000);
        return;
      }
      const projectRoot = currentScopeId !== "general-chat" ? activeProject?.rootPath : undefined;
      const picked = await BackendAPI.pickSavePath(projectRoot, fileName, "Markdown|md");
      if (!picked) return;
      const projectId = currentScopeId !== "general-chat" ? activeProject?.id ?? null : null;
      const res = await BackendAPI.saveFileToPath(projectId, picked, bytesToBase64(new TextEncoder().encode(text)));
      if (res.snapshot) setSnapshots((prev) => [...prev, res.snapshot!]);
      if (res.relativePath) bumpFileTree();
      setErrorBanner(null);
      setSavedAnswer({
        name: res.absolutePath.split(/[\\/]/).pop() || res.absolutePath,
        absolutePath: res.absolutePath,
        relativePath: res.relativePath ?? null,
      });
    } catch (err: any) {
      setErrorBanner(`대화를 저장하지 못했습니다: ${err?.message || String(err)}`);
    }
  }

  // 다른 프로젝트·대화로 옮기면 저장 알림을 닫는다. 그대로 두어도 15초 뒤 사라진다.
  useEffect(() => {
    setSavedAnswer(null);
    setAnswerSaveMenu(null);
  }, [currentScopeId, activeConversation?.id]);
  useEffect(() => {
    if (!savedAnswer) return;
    const t = setTimeout(() => setSavedAnswer(null), 15000);
    return () => clearTimeout(t);
  }, [savedAnswer]);

  function openSavedAnswer() {
    if (!savedAnswer || !isTauri()) return;
    if (savedAnswer.relativePath) {
      openFileInArtifact(savedAnswer.relativePath);
      return;
    }
    const path = savedAnswer.absolutePath;
    const kind = docKindOf(path);
    BackendAPI.readAbsoluteFileBase64(path)
      .then((b64) =>
        setActiveArtifact(
          canvasDocFromBase64({ title: savedAnswer.name, kind, absolutePath: path }, b64, isBinaryKind(kind))
        )
      )
      .catch((err) => setErrorBanner(String(err)));
  }

  function revealSavedAnswer() {
    if (!savedAnswer || !isTauri()) return;
    const p = savedAnswer.relativePath && activeProject
      ? BackendAPI.revealProjectPath(activeProject.id, savedAnswer.relativePath)
      : BackendAPI.revealSavedPath(savedAnswer.absolutePath);
    p.catch((err) => setErrorBanner(String(err)));
  }

  return (
    <div
      className={`h-screen w-screen flex overflow-hidden transition-colors duration-150 ${
        isDark
          ? "dark bg-[#1E1E1C] text-[#ECECEC]"
          : "bg-[#FAF9F5] text-[#1F1E1D]"
      }`}
    >
      {/* Left Sidebar (Claude Desktop Light & Dark) */}
      {!sidebarCollapsed && (
      <aside className="w-64 bg-[#F0EFEA] dark:bg-[#171715] border-r border-[#E2E0D8] dark:border-[#2C2C29] flex flex-col justify-between shrink-0 select-none transition-colors duration-150">
        <div className="p-3.5 space-y-4 flex-1 flex flex-col min-h-0">
          {/* Brand & Theme Toggle & New Chat */}
          <div className="space-y-3">
            <div className="flex items-center justify-between px-1">
              <div className="flex items-center gap-2">
                <span className="w-6 h-6 rounded-lg bg-[#A50034] text-white font-bold text-xs flex items-center justify-center">
                  EX
                </span>
                <span className="font-bold text-sm text-stone-900 dark:text-stone-100">
                  K-EXAONE
                </span>
                <span
                  title="Desktop App for LG K-EXAONE은 LG AI연구원이 만들거나 보증하지 않은 비공식 오픈소스 앱입니다."
                  className="text-[10px] px-1.5 py-0.5 rounded-full bg-stone-200/90 dark:bg-[#2A2A27] text-stone-600 dark:text-stone-300 font-medium cursor-help"
                >
                  비공식
                </span>
              </div>

              <div className="flex items-center gap-1">
                <button
                  type="button"
                  onClick={() => setIsDark(!isDark)}
                  className="p-1.5 rounded-lg bg-stone-200/70 dark:bg-[#262624] hover:bg-stone-300/70 dark:hover:bg-[#32322F] text-stone-700 dark:text-stone-200 transition cursor-pointer"
                  title={isDark ? "밝은 모드로 전환" : "다크 모드로 전환"}
                >
                  {isDark ? (
                    <Sun className="w-3.5 h-3.5 text-amber-400" />
                  ) : (
                    <Moon className="w-3.5 h-3.5 text-stone-600" />
                  )}
                </button>
                <button
                  type="button"
                  onClick={() => setSidebarCollapsed(true)}
                  className="p-1.5 rounded-lg bg-stone-200/70 dark:bg-[#262624] hover:bg-stone-300/70 dark:hover:bg-[#32322F] text-stone-700 dark:text-stone-200 transition cursor-pointer"
                  title="사이드바 접기 (Ctrl+B)"
                  aria-label="사이드바 접기"
                >
                  <PanelLeftClose className="w-3.5 h-3.5" />
                </button>
              </div>
            </div>

            <button
              onClick={handleNewChat}
              className="w-full py-2.5 px-3 rounded-xl bg-[#A50034] hover:bg-[#8A002B] text-white text-xs font-semibold flex items-center justify-center gap-1.5 shadow-2xs transition cursor-pointer"
            >
              <Plus className="w-4 h-4" /> 새 채팅
            </button>
          </div>

          {/* 3 Core Navigation Tabs: 채팅 / 프로젝트 / 코딩 */}
          <div className="grid grid-cols-3 gap-1 bg-stone-200/80 dark:bg-[#242421] p-1 rounded-xl text-xs">
            <button
              onClick={() => handleSwitchMode("chat")}
              className={`py-1.5 rounded-lg font-semibold flex items-center justify-center gap-1 transition cursor-pointer ${
                activeMode === "chat"
                  ? "bg-white dark:bg-[#32322E] text-stone-900 dark:text-stone-100 shadow-2xs"
                  : "text-stone-600 dark:text-stone-400 hover:text-stone-900 dark:hover:text-stone-200"
              }`}
            >
              <MessageSquare className="w-3.5 h-3.5" /> 채팅
              <RunBadge count={runCounts.chat} />
            </button>
            <button
              onClick={() => handleSwitchMode("project")}
              className={`py-1.5 rounded-lg font-semibold flex items-center justify-center gap-1 transition cursor-pointer ${
                activeMode === "project"
                  ? "bg-white dark:bg-[#32322E] text-stone-900 dark:text-stone-100 shadow-2xs"
                  : "text-stone-600 dark:text-stone-400 hover:text-stone-900 dark:hover:text-stone-200"
              }`}
            >
              <FolderOpen className="w-3.5 h-3.5" /> 프로젝트
              <RunBadge count={runCounts.project} />
            </button>
            <button
              onClick={() => handleSwitchMode("code")}
              className={`py-1.5 rounded-lg font-semibold flex items-center justify-center gap-1 transition cursor-pointer ${
                activeMode === "code"
                  ? "bg-white dark:bg-[#32322E] text-stone-900 dark:text-stone-100 shadow-2xs"
                  : "text-stone-600 dark:text-stone-400 hover:text-stone-900 dark:hover:text-stone-200"
              }`}
            >
              <Code2 className="w-3.5 h-3.5" /> 코딩
              <RunBadge count={runCounts.code} />
            </button>
          </div>

          {/* Projects Section (Hidden in General Chat Mode) */}
          {activeMode !== "chat" && (
            <div className="space-y-1.5">
              <div className="flex items-center justify-between px-1">
                <span className="text-xs font-bold text-stone-500 dark:text-stone-400">
                  프로젝트
                </span>
                <button
                  onClick={() => {
                    setEditingProject({
                      name: "",
                      rootPath: activeProject?.rootPath || "",
                      instructions: "",
                    });
                    setShowProjectModal(true);
                  }}
                  className="text-xs text-[#A50034] dark:text-[#FF5C8D] font-semibold hover:underline cursor-pointer"
                >
                  + 추가
                </button>
              </div>

              <div className="space-y-1 max-h-36 overflow-y-auto">
                {projects.map((p) => {
                  const isSelected = activeProject?.id === p.id;
                  return (
                    <div
                      key={p.id}
                      onClick={() => {
                        setActiveProjectId(p.id);
                        const firstConv = conversations.find(
                          (c) => c.projectId === p.id
                        );
                        setActiveConvId(firstConv ? firstConv.id : "");
                      }}
                      className={`group px-2.5 py-2 rounded-xl text-xs flex items-center justify-between cursor-pointer transition ${
                        isSelected
                          ? "bg-white dark:bg-[#282825] font-semibold text-stone-900 dark:text-stone-100 border border-stone-200/80 dark:border-[#3A3A36] shadow-2xs"
                          : "text-stone-700 dark:text-stone-300 hover:bg-stone-200/60 dark:hover:bg-[#232320]"
                      }`}
                    >
                      <span className="truncate flex items-center gap-1.5">
                        <FolderOpen className="w-3.5 h-3.5 text-[#A50034] dark:text-[#FF5C8D] shrink-0" />
                        <span className="truncate">{p.name}</span>
                        {(runningByProject[p.id] ?? 0) > 0 && (
                          <span
                            className="shrink-0 flex items-center gap-0.5 text-[10px] font-semibold text-[#A50034] dark:text-[#FF5C8D]"
                            title={`진행 중인 대화 ${runningByProject[p.id]}개`}
                          >
                            <RefreshCw className="w-2.5 h-2.5 animate-spin" />
                            {runningByProject[p.id]}
                          </span>
                        )}
                      </span>
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          setEditingProject({
                            id: p.id,
                            name: p.name,
                            rootPath: p.rootPath,
                            instructions: p.instructions,
                          });
                          setShowProjectModal(true);
                        }}
                        className="opacity-0 group-hover:opacity-100 text-stone-400 hover:text-stone-800 dark:hover:text-stone-100 p-0.5"
                        title="프로젝트 설정"
                      >
                        <Settings className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {/* Recent Conversations with Search / Project File Explorer */}
          <div className="flex-1 flex flex-col min-h-0 space-y-2">
            {explorerApi ? (
              <div className="grid grid-cols-2 gap-1 bg-stone-200/80 dark:bg-[#242421] p-1 rounded-xl text-xs">
                {(["chats", "files"] as const).map((tab) => (
                  <button
                    key={tab}
                    onClick={() => setSidebarTab(tab)}
                    className={`py-1 rounded-lg font-semibold cursor-pointer transition ${
                      sidebarTab === tab
                        ? "bg-white dark:bg-[#32322E] text-stone-900 dark:text-stone-100 shadow-2xs"
                        : "text-stone-600 dark:text-stone-400 hover:text-stone-900 dark:hover:text-stone-200"
                    }`}
                  >
                    {tab === "chats" ? "대화" : "파일"}
                  </button>
                ))}
              </div>
            ) : (
              <div className="flex items-center justify-between px-1">
                <span className="text-xs font-bold text-stone-500 dark:text-stone-400">
                  최근 대화
                </span>
              </div>
            )}

            {explorerApi && sidebarTab === "files" ? (
              <FileExplorer
                api={explorerApi}
                rootName={activeProject?.name ?? "프로젝트"}
                refreshToken={fileTreeVersion}
                activePath={activeArtifact?.filePath}
                onOpen={openAnyProjectFile}
                onRunPython={(rel) => setPythonRun(rel)}
                onOpenExternal={(rel) =>
                  activeProject &&
                  BackendAPI.openProjectFileExternal(activeProject.id, rel).catch((err) => setErrorBanner(String(err)))
                }
                onAttach={attachFile}
                onRenamed={handleExplorerRenamed}
                onDeleted={handleExplorerDeleted}
                onError={(msg) => setErrorBanner(msg)}
              />
            ) : (
            <>
            <div className="relative">
              <Search className="w-3.5 h-3.5 text-stone-400 absolute left-2.5 top-2" />
              <input
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="대화 제목·내용 검색..."
                className="w-full bg-white/80 dark:bg-[#232320] border border-stone-200/90 dark:border-[#343430] rounded-lg pl-8 pr-2.5 py-1 text-xs text-stone-800 dark:text-stone-200 placeholder:text-stone-400 focus:outline-none focus:border-stone-400 dark:focus:border-stone-500"
              />
            </div>

            <div className="flex-1 overflow-y-auto space-y-1">
              {filteredConversations.map((c) => (
                <div
                  key={c.id}
                  onClick={() => {
                    setActiveConvId(c.id);
                    setShowProjectOverview(false);
                  }}
                  className={`group px-2.5 py-2 rounded-xl text-xs flex items-center justify-between cursor-pointer transition ${
                    activeConversation?.id === c.id && !showProjectOverview
                      ? "bg-white dark:bg-[#282825] font-semibold text-stone-900 dark:text-stone-100 border border-stone-200/80 dark:border-[#3A3A36] shadow-2xs"
                      : "text-stone-700 dark:text-stone-300 hover:bg-stone-200/60 dark:hover:bg-[#232320]"
                  }`}
                >
                  <span className="truncate pr-2 flex items-center gap-1.5 min-w-0">
                    {runs[c.id] ? (
                      <RefreshCw
                        className="w-3 h-3 shrink-0 animate-spin text-[#A50034] dark:text-[#FF5C8D]"
                        aria-label="답변 작성 중"
                      />
                    ) : waitsForApproval(c) ? (
                      <span
                        className="w-1.5 h-1.5 rounded-full shrink-0 bg-amber-500"
                        title="승인을 기다리는 작업이 있습니다"
                      />
                    ) : unseenDone[c.id] ? (
                      <span
                        className={`w-1.5 h-1.5 rounded-full shrink-0 ${runErrors[c.id] ? "bg-red-500" : "bg-emerald-500"}`}
                        title={runErrors[c.id] ? "오류로 멈춤" : "새 답변"}
                      />
                    ) : null}
                    <span className="truncate">{c.title}</span>
                  </span>
                  <button
                    onClick={async (e) => {
                      e.stopPropagation();
                      await BackendAPI.deleteConversation(c.id);
                      setConversations((prev) =>
                        prev.filter((item) => item.id !== c.id)
                      );
                    }}
                    className="opacity-0 group-hover:opacity-100 text-stone-400 hover:text-red-600 p-0.5"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
            </>
            )}
          </div>
        </div>

        {/* Bottom Settings Button */}
        <div className="p-3 border-t border-[#E2E0D8] dark:border-[#2C2C29] space-y-1.5">
          <UpdateBanner updater={updater} runningCount={Object.keys(runs).length} />
          <button
            onClick={() => setShowSkills(true)}
            className="w-full py-2 px-3 rounded-xl bg-white dark:bg-[#242421] hover:bg-stone-100 dark:hover:bg-[#2E2E2A] border border-stone-300/80 dark:border-[#383834] text-xs font-medium text-stone-700 dark:text-stone-200 flex items-center gap-1.5 cursor-pointer transition"
          >
            <Sparkles className="w-3.5 h-3.5 text-stone-500 dark:text-stone-400" />
            Skill
          </button>
          <button
            onClick={() => setShowMcp(true)}
            className="w-full py-2 px-3 rounded-xl bg-white dark:bg-[#242421] hover:bg-stone-100 dark:hover:bg-[#2E2E2A] border border-stone-300/80 dark:border-[#383834] text-xs font-medium text-stone-700 dark:text-stone-200 flex items-center gap-1.5 cursor-pointer transition"
          >
            <Plug className="w-3.5 h-3.5 text-stone-500 dark:text-stone-400" />
            MCP 서버
          </button>
          <button
            onClick={() => setShowSettingsModal(true)}
            className="w-full py-2 px-3 rounded-xl bg-white dark:bg-[#242421] hover:bg-stone-100 dark:hover:bg-[#2E2E2A] border border-stone-300/80 dark:border-[#383834] text-xs font-medium text-stone-700 dark:text-stone-200 flex items-center justify-between cursor-pointer transition"
          >
            <span className="flex items-center gap-1.5">
              <Settings className="w-3.5 h-3.5 text-stone-500 dark:text-stone-400" />
              API 및 모델 설정
            </span>
            <span
              className={`text-xs font-semibold ${
                settings.apiKeySet && !(keyDays !== null && keyDays >= KEY_VALID_DAYS - 5)
                  ? "text-emerald-700 dark:text-emerald-400"
                  : "text-amber-700 dark:text-amber-400"
              }`}
            >
              {!settings.apiKeySet
                ? "키 입력 필요"
                : keyDays !== null && keyDays >= KEY_VALID_DAYS
                  ? "키 만료됐을 수 있음"
                  : keyDays !== null && keyDays >= KEY_VALID_DAYS - 5
                    ? `키 만료 ${KEY_VALID_DAYS - keyDays}일 전`
                    : "연결됨"}
            </span>
          </button>
        </div>
      </aside>
      )}

      {/* Center Main Area */}
      <main className="flex-1 flex flex-col min-w-0 bg-[#FAF9F5] dark:bg-[#1E1E1C] transition-colors duration-150">
        {/* Clean Header */}
        <header className="h-13 border-b border-[#E5E3DB] dark:border-[#2E2E2A] px-6 flex items-center justify-between gap-3 shrink-0 bg-[#FAF9F5]/90 dark:bg-[#1E1E1C]/90 backdrop-blur-xs">
          <div className="flex items-center gap-2.5 text-xs min-w-0">
            {sidebarCollapsed && (() => {
              const hasOtherRunning = Object.keys(runs).some((id) => id !== shownConvId);
              const hasUnseenDone = Object.keys(unseenDone).length > 0;
              return (
                <div className="flex items-center gap-1 shrink-0">
                  <button
                    type="button"
                    onClick={() => setSidebarCollapsed(false)}
                    className="relative p-1.5 rounded-lg bg-stone-200/70 dark:bg-[#262624] hover:bg-stone-300/70 dark:hover:bg-[#32322F] text-stone-700 dark:text-stone-200 transition cursor-pointer"
                    title="사이드바 펼치기 (Ctrl+B)"
                    aria-label="사이드바 펼치기"
                  >
                    <PanelLeftOpen className="w-4 h-4" />
                    {(hasOtherRunning || hasUnseenDone) && (
                      <span
                        className={`absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full ring-2 ring-[#FAF9F5] dark:ring-[#1E1E1C] ${
                          hasOtherRunning ? "bg-[#A50034] dark:bg-[#FF5C8D] animate-pulse" : "bg-emerald-500"
                        }`}
                        title={hasOtherRunning ? "다른 대화에서 답변 생성 중" : "다른 대화에 새 답변이 있습니다"}
                      />
                    )}
                  </button>
                  <button
                    type="button"
                    onClick={handleNewChat}
                    className="p-1.5 rounded-lg bg-stone-200/70 dark:bg-[#262624] hover:bg-stone-300/70 dark:hover:bg-[#32322F] text-stone-700 dark:text-stone-200 transition cursor-pointer"
                    title="새 채팅"
                    aria-label="새 채팅"
                  >
                    <Plus className="w-4 h-4" />
                  </button>
                </div>
              );
            })()}
            <span className="font-bold text-sm text-stone-900 dark:text-stone-100 truncate">
              {activeMode === "chat" &&
                (activeConversation ? activeConversation.title : "새 채팅")}
              {activeMode === "project" && "프로젝트"}
              {activeMode === "code" && "코딩"}
            </span>
            {activeMode !== "chat" && activeProject && (
              <>
                <span className="text-stone-300 dark:text-stone-600 shrink-0">/</span>
                <span className="font-semibold text-stone-700 dark:text-stone-300 truncate">
                  {activeProject.name}
                </span>
              </>
            )}
          </div>

          {/* 채팅창이 좁아지면 작업 폴더 경로부터 줄인다. 버튼 글자는 줄바꿈하지 않는다. */}
          <div className="flex items-center gap-2 min-w-0">
            {(activeMode === "project" || activeMode === "code") &&
              activeProject && (
                <button
                  onClick={handleBrowseFolderForActiveProject}
                  className="px-3 py-1.5 rounded-lg bg-white dark:bg-[#262624] hover:bg-stone-100 dark:hover:bg-[#30302C] border border-stone-300 dark:border-[#3A3A36] text-xs font-medium text-stone-700 dark:text-stone-200 flex items-center gap-1.5 cursor-pointer transition min-w-0"
                  title={`작업 폴더 변경 (${activeProject.rootPath})`}
                >
                  <FolderOpen className="w-3.5 h-3.5 shrink-0 text-[#A50034] dark:text-[#FF5C8D]" />
                  <span className="truncate max-w-[240px] min-w-0">
                    {activeProject.rootPath}
                  </span>
                </button>
              )}

            {activeMode === "project" && (
              <button
                onClick={() => setShowProjectOverview(!showProjectOverview)}
                className="px-3 py-1.5 rounded-lg bg-white dark:bg-[#262624] hover:bg-stone-100 dark:hover:bg-[#30302C] border border-stone-300 dark:border-[#3A3A36] text-xs font-semibold text-stone-700 dark:text-stone-200 cursor-pointer transition whitespace-nowrap shrink-0"
              >
                {showProjectOverview ? "대화로 돌아가기" : "프로젝트 관리"}
              </button>
            )}

            {activeConversation && activeConversation.messages.length > 0 && !showProjectOverview && (
              <button
                onClick={handleExportConversation}
                className="px-2.5 py-1.5 rounded-lg bg-stone-200/80 dark:bg-[#2C2C28] hover:bg-stone-300 dark:hover:bg-[#383833] text-xs font-semibold text-stone-700 dark:text-stone-200 flex items-center gap-1 cursor-pointer whitespace-nowrap shrink-0"
                title="이 대화 전체를 Markdown 파일로 저장합니다"
              >
                <Download className="w-3.5 h-3.5" />
                <span>대화 저장</span>
              </button>
            )}
            <button
              onClick={handleOpenDocument}
              className="px-2.5 py-1.5 rounded-lg bg-stone-200/80 dark:bg-[#2C2C28] hover:bg-stone-300 dark:hover:bg-[#383833] text-xs font-semibold text-stone-700 dark:text-stone-200 flex items-center gap-1 cursor-pointer whitespace-nowrap shrink-0"
              title="Markdown, Word, PowerPoint, PDF, HTML, 이미지, 텍스트 파일을 문서 캔버스에서 엽니다"
            >
              <FileText className="w-3.5 h-3.5" />
              <span>문서 열기</span>
            </button>
          </div>
        </header>

        {savedAnswer && (
          <div className="bg-emerald-50 dark:bg-emerald-950/50 border-b border-emerald-200 dark:border-emerald-900 px-6 py-2 text-xs text-emerald-800 dark:text-emerald-200 flex items-center gap-3">
            <Check className="w-3.5 h-3.5 shrink-0" />
            <span className="min-w-0 truncate" title={savedAnswer.absolutePath}>
              답변을 저장했습니다: <span className="font-semibold">{savedAnswer.name}</span>
            </span>
            {isTauri() && (
              <>
                <button
                  onClick={openSavedAnswer}
                  className="shrink-0 font-semibold hover:underline cursor-pointer"
                >
                  캔버스에서 열기
                </button>
                <button
                  onClick={revealSavedAnswer}
                  className="shrink-0 font-semibold hover:underline cursor-pointer"
                >
                  폴더에서 보기
                </button>
              </>
            )}
            <button onClick={() => setSavedAnswer(null)} className="ml-auto shrink-0 cursor-pointer" aria-label="닫기">
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
        )}

        {shownConvId && runErrors[shownConvId] && !isStreaming && (
          <div className="bg-red-50 dark:bg-red-950/60 border-b border-red-200 dark:border-red-900 px-6 py-2 text-xs text-red-700 dark:text-red-300 flex items-center justify-between gap-3">
            <span className="min-w-0 break-words">{runErrors[shownConvId]}</span>
            <button
              onClick={() =>
                setRunErrors((prev) => {
                  const next = { ...prev };
                  delete next[shownConvId];
                  return next;
                })
              }
              className="shrink-0 cursor-pointer"
              aria-label="닫기"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
        )}

        {errorBanner && (
          <div className="bg-red-50 dark:bg-red-950/60 border-b border-red-200 dark:border-red-900 px-6 py-2 text-xs text-red-700 dark:text-red-300 flex items-center justify-between">
            <span>{errorBanner}</span>
            <button onClick={() => setErrorBanner(null)}>
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
        )}

        {/* Mode 1: Claude-Style Project Dashboard View */}
        {showProjectOverview && activeProject ? (
          <div className="flex-1 overflow-y-auto p-8">
            <div className="max-w-3xl mx-auto space-y-6">
              <div className="bg-white dark:bg-[#262624] border border-[#E5E3DB] dark:border-[#363632] rounded-2xl p-6 shadow-2xs space-y-5">
                <div className="flex items-start justify-between">
                  <div>
                    <h2 className="text-lg font-bold text-stone-900 dark:text-stone-100">
                      {activeProject.name}
                    </h2>
                    <p className="text-xs text-stone-500 dark:text-stone-400 mt-1">
                      프로젝트마다 작업 폴더와 지침을 지정하여 일관된 문서 작성과 코딩을 수행합니다.
                    </p>
                  </div>
                  <button
                    onClick={() => {
                      setEditingProject({
                        id: activeProject.id,
                        name: activeProject.name,
                        rootPath: activeProject.rootPath,
                        instructions: activeProject.instructions,
                      });
                      setShowProjectModal(true);
                    }}
                    className="px-3.5 py-1.5 rounded-xl bg-stone-100 dark:bg-[#32322F] hover:bg-stone-200 dark:hover:bg-[#3C3C38] text-xs font-semibold text-stone-800 dark:text-stone-200 cursor-pointer"
                  >
                    프로젝트 수정
                  </button>
                </div>

                <div className="grid grid-cols-1 md:grid-cols-2 gap-4 pt-2 border-t border-stone-100 dark:border-[#343430]">
                  <div className="p-4 rounded-xl bg-[#FAF9F5] dark:bg-[#1E1E1C] border border-stone-200/80 dark:border-[#343430] space-y-2">
                    <div className="flex items-center justify-between">
                      <span className="text-xs font-bold text-stone-700 dark:text-stone-300">
                        연결된 폴더
                      </span>
                      <button
                        onClick={handleBrowseFolderForActiveProject}
                        className="text-xs text-[#A50034] dark:text-[#FF5C8D] font-semibold hover:underline flex items-center gap-1 cursor-pointer"
                      >
                        <FolderOpen className="w-3.5 h-3.5" /> 폴더 찾아보기
                      </button>
                    </div>
                    <div className="text-xs font-mono text-stone-600 dark:text-stone-400 break-all">
                      {activeProject.rootPath}
                    </div>
                  </div>

                  <div className="p-4 rounded-xl bg-[#FAF9F5] dark:bg-[#1E1E1C] border border-stone-200/80 dark:border-[#343430] space-y-2">
                    <div className="flex items-center justify-between">
                      <span className="text-xs font-bold text-stone-700 dark:text-stone-300">
                        프로젝트 지침
                      </span>
                      <button
                        onClick={() => {
                          setEditingProject({
                            id: activeProject.id,
                            name: activeProject.name,
                            rootPath: activeProject.rootPath,
                            instructions: activeProject.instructions,
                          });
                          setShowProjectModal(true);
                        }}
                        className="text-xs text-[#A50034] dark:text-[#FF5C8D] font-semibold hover:underline cursor-pointer"
                      >
                        편집
                      </button>
                    </div>
                    <p className="text-xs text-stone-600 dark:text-stone-400 line-clamp-3 leading-relaxed">
                      {activeProject.instructions ||
                        "설정된 프로젝트 지침이 없습니다."}
                    </p>
                  </div>
                </div>

                <div className="p-4 rounded-xl bg-[#FAF9F5] dark:bg-[#1E1E1C] border border-stone-200/80 dark:border-[#343430] space-y-2">
                  <div className="text-xs font-bold text-stone-700 dark:text-stone-300">묻지 않고 실행</div>
                  <p className="text-xs text-stone-500 dark:text-stone-400 leading-relaxed">
                    켜 두면 이 프로젝트에서는 엑사원이 만든 스크립트나 명령을 승인 없이 바로 실행합니다. 호출과 시간을 아끼지만,
                    실행 전에 내용을 확인할 수 없습니다. 파일 삭제는 항상 묻습니다.
                  </p>
                  {AUTO_APPROVE_TOOLS.map((t) => (
                    <label key={t.tool} className="flex items-center gap-2 text-xs text-stone-700 dark:text-stone-300 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={(activeProject.autoApprove ?? []).includes(t.tool)}
                        onChange={async (e) => {
                          try {
                            const updated = await BackendAPI.setProjectAutoApprove(activeProject.id, t.tool, e.target.checked);
                            setProjects((prev) => prev.map((p) => (p.id === updated.id ? updated : p)));
                          } catch (err) {
                            setErrorBanner(String(err));
                          }
                        }}
                      />
                      {t.label}
                    </label>
                  ))}
                </div>

                <div className="pt-2 flex justify-end">
                  <button
                    onClick={handleNewChat}
                    className="px-4 py-2 rounded-xl bg-[#A50034] hover:bg-[#8A002B] text-white text-xs font-semibold flex items-center gap-1.5 cursor-pointer"
                  >
                    <Plus className="w-4 h-4" /> 이 프로젝트에서 새 대화 시작
                  </button>
                </div>
              </div>

              {/* Project Conversations List */}
              <div className="space-y-3">
                <h3 className="text-sm font-bold text-stone-800 dark:text-stone-200 px-1">
                  프로젝트 대화 내역
                </h3>
                {filteredConversations.length === 0 ? (
                  <div className="bg-white dark:bg-[#262624] border border-[#E5E3DB] dark:border-[#363632] rounded-2xl p-8 text-center text-xs text-stone-400">
                    이 프로젝트에 생성된 대화가 없습니다.
                  </div>
                ) : (
                  <div className="grid grid-cols-1 gap-2">
                    {filteredConversations.map((c) => (
                      <div
                        key={c.id}
                        onClick={() => {
                          setActiveConvId(c.id);
                          setShowProjectOverview(false);
                        }}
                        className="bg-white dark:bg-[#262624] hover:bg-stone-50 dark:hover:bg-[#2E2E2B] border border-[#E5E3DB] dark:border-[#363632] rounded-xl p-4 flex items-center justify-between cursor-pointer transition"
                      >
                        <div className="space-y-0.5">
                          <div className="text-sm font-semibold text-stone-900 dark:text-stone-100">
                            {c.title}
                          </div>
                          <div className="text-xs text-stone-400">
                            메시지 {c.messages.length}개 ·{" "}
                            {new Date(c.updatedAt).toLocaleString()}
                          </div>
                        </div>
                        <ChevronRight className="w-4 h-4 text-stone-400" />
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          </div>
        ) : (
          /* Mode 2: Chat & Coding Conversation Stream */
          <>
            <div className="flex-1 overflow-y-auto px-6 py-6">
              <div className="max-w-3xl mx-auto space-y-6">
                {(!activeConversation ||
                  activeConversation.messages.length === 0) &&
                  !isStreaming && (
                    <div className="py-14 space-y-6">
                      <div className="text-center space-y-2">
                        <h2 className="text-2xl font-bold text-stone-800 dark:text-stone-100 tracking-tight">
                          무엇을 도와드릴까요?
                        </h2>
                        <p className="text-xs text-stone-500 dark:text-stone-400">
                          {activeMode === "chat" &&
                            "글쓰기, 보고서 작성, 문서 요약 및 질의응답을 도와드립니다."}
                          {activeMode === "project" &&
                            `${activeProject?.name} 폴더의 문서와 파일을 함께 읽고 작성합니다.`}
                          {activeMode === "code" &&
                            "로컬 폴더의 코드를 분석·작성·수정하고 실행합니다."}
                        </p>
                      </div>

                      <div className="grid grid-cols-2 gap-2.5 max-w-xl mx-auto">
                        {activeMode === "code" ? (
                          <>
                            <button
                              onClick={() =>
                                handleSend(
                                  "현재 폴더의 파일 구조를 확인하고 주요 코드 파일들을 요약해 줘."
                                )
                              }
                              className="p-3.5 rounded-xl bg-white dark:bg-[#262624] hover:bg-stone-50 dark:hover:bg-[#2E2E2B] border border-[#E5E3DB] dark:border-[#363632] text-left transition cursor-pointer"
                            >
                              <div className="text-xs font-bold text-stone-800 dark:text-stone-200">
                                프로젝트 코드 구조 분석
                              </div>
                              <div className="text-xs text-stone-500 dark:text-stone-400 mt-0.5">
                                폴더 내 파일 탐색 및 아키텍처 요약
                              </div>
                            </button>
                            <button
                              onClick={() =>
                                handleSend(
                                  "현재 폴더의 코드에서 개선할 점이나 버그 가능성이 있는 부분을 찾아서 알려줘."
                                )
                              }
                              className="p-3.5 rounded-xl bg-white dark:bg-[#262624] hover:bg-stone-50 dark:hover:bg-[#2E2E2B] border border-[#E5E3DB] dark:border-[#363632] text-left transition cursor-pointer"
                            >
                              <div className="text-xs font-bold text-stone-800 dark:text-stone-200">
                                코드 리뷰 및 리팩토링 제안
                              </div>
                              <div className="text-xs text-stone-500 dark:text-stone-400 mt-0.5">
                                주요 파일 점검 및 개선안 도출
                              </div>
                            </button>
                          </>
                        ) : (
                          <>
                            <button
                              onClick={() =>
                                handleSend(
                                  "신규 AI 서비스 도입을 위한 경영진 보고서 초안을 체계적인 목차와 표를 포함해 작성해 줘."
                                )
                              }
                              className="p-3.5 rounded-xl bg-white dark:bg-[#262624] hover:bg-stone-50 dark:hover:bg-[#2E2E2B] border border-[#E5E3DB] dark:border-[#363632] text-left transition cursor-pointer"
                            >
                              <div className="text-xs font-bold text-stone-800 dark:text-stone-200">
                                기획 보고서 초안 작성
                              </div>
                              <div className="text-xs text-stone-500 dark:text-stone-400 mt-0.5">
                                요약, 기대 효과 표, 추진 일정 포함
                              </div>
                            </button>
                            <button
                              onClick={() =>
                                handleSend(
                                  "비즈니스 이메일이나 공식 문서를 더 설득력 있고 정중한 표현으로 다듬어 줘."
                                )
                              }
                              className="p-3.5 rounded-xl bg-white dark:bg-[#262624] hover:bg-stone-50 dark:hover:bg-[#2E2E2B] border border-[#E5E3DB] dark:border-[#363632] text-left transition cursor-pointer"
                            >
                              <div className="text-xs font-bold text-stone-800 dark:text-stone-200">
                                문서 및 글쓰기 교정
                              </div>
                              <div className="text-xs text-stone-500 dark:text-stone-400 mt-0.5">
                                문맥 흐름 개선 및 전문적인 문체 적용
                              </div>
                            </button>
                          </>
                        )}
                      </div>
                    </div>
                  )}

                {activeConversation?.messages.map((msg, idx) => {
                  const msgArtifact =
                    msg.role === "assistant"
                      ? extractArtifactFromMessage(msg)
                      : null;

                  return (
                    <div key={msg.id} className="space-y-2">
                      {msg.role === "user" ? (
                        <div className="flex flex-col items-end">
                          {editingMessageId === msg.id ? (
                            <div className="w-full max-w-2xl bg-white dark:bg-[#262624] border-2 border-[#A50034] rounded-2xl p-3.5 space-y-2.5 shadow-sm">
                              <textarea
                                rows={3}
                                value={editingMessageText}
                                onChange={(e) =>
                                  setEditingMessageText(e.target.value)
                                }
                                onKeyDown={(e) => {
                                  if (e.key === "Enter" && !e.shiftKey) {
                                    e.preventDefault();
                                    handleEditAndResend(msg.id);
                                  } else if (e.key === "Escape") {
                                    setEditingMessageId(null);
                                  }
                                }}
                                className="w-full text-sm text-stone-900 dark:text-stone-100 bg-transparent resize-none focus:outline-none leading-relaxed"
                                autoFocus
                              />
                              <div className="flex items-center justify-end gap-1.5 pt-1 border-t border-stone-100 dark:border-[#363632]">
                                <button
                                  onClick={() => setEditingMessageId(null)}
                                  className="px-3 py-1.5 rounded-lg bg-stone-100 dark:bg-[#32322F] hover:bg-stone-200 dark:hover:bg-[#3E3E3A] text-stone-700 dark:text-stone-200 text-xs font-semibold cursor-pointer"
                                >
                                  취소
                                </button>
                                <button
                                  onClick={() => handleEditAndResend(msg.id)}
                                  disabled={!editingMessageText.trim()}
                                  className="px-3.5 py-1.5 rounded-lg bg-[#A50034] hover:bg-[#8A002B] disabled:opacity-40 text-white text-xs font-semibold cursor-pointer"
                                >
                                  수정 후 재전송
                                </button>
                              </div>
                            </div>
                          ) : (
                            <div className="group flex flex-col items-end gap-1 max-w-2xl">
                              <div className="bg-[#EFECE6] dark:bg-[#2D2D2A] text-stone-900 dark:text-stone-100 rounded-2xl px-4 py-3 text-sm whitespace-pre-wrap leading-relaxed">
                                {msg.content}
                              </div>
                              {!isStreaming && (
                                <div className="opacity-0 group-hover:opacity-100 transition flex items-center gap-1">
                                  <button
                                    onClick={() =>
                                      handleCopyText(msg.id, msg.content)
                                    }
                                    className="text-xs text-stone-500 dark:text-stone-400 hover:text-stone-900 dark:hover:text-stone-100 flex items-center gap-1 px-2 py-0.5 rounded hover:bg-stone-200/60 dark:hover:bg-[#2C2C29] cursor-pointer"
                                  >
                                    {copiedId === msg.id ? (
                                      <Check className="w-3 h-3 text-emerald-600" />
                                    ) : (
                                      <Copy className="w-3 h-3" />
                                    )}
                                    복사
                                  </button>
                                  <button
                                    onClick={() => {
                                      setEditingMessageId(msg.id);
                                      setEditingMessageText(msg.content);
                                    }}
                                    className="text-xs text-stone-500 dark:text-stone-400 hover:text-[#A50034] dark:hover:text-[#FF5C8D] flex items-center gap-1 px-2 py-0.5 rounded hover:bg-stone-200/60 dark:hover:bg-[#2C2C29] cursor-pointer"
                                  >
                                    <Edit2 className="w-3 h-3" /> 수정
                                  </button>
                                </div>
                              )}
                            </div>
                          )}
                        </div>
                      ) : (
                        <div className="group bg-white dark:bg-[#262624] border border-[#E5E3DB] dark:border-[#363632] rounded-2xl p-5 space-y-3 shadow-2xs">
                          {/* Collapsible Thinking / Reasoning */}
                          {msg.reasoningContent && (
                            <div className="border border-stone-200/90 dark:border-[#383834] bg-[#FAF9F5] dark:bg-[#1E1E1C] rounded-xl overflow-hidden">
                              <button
                                onClick={() =>
                                  setExpandedReasoning((prev) => ({
                                    ...prev,
                                    [msg.id]: !prev[msg.id],
                                  }))
                                }
                                className="w-full px-3 py-2 flex items-center justify-between text-xs font-medium text-stone-500 dark:text-stone-400 hover:bg-stone-100 dark:hover:bg-[#2A2A27] cursor-pointer"
                              >
                                <span>생각 과정 보기</span>
                                {expandedReasoning[msg.id] ? (
                                  <ChevronDown className="w-3.5 h-3.5" />
                                ) : (
                                  <ChevronRight className="w-3.5 h-3.5" />
                                )}
                              </button>
                              {expandedReasoning[msg.id] && (
                                <div className="px-3.5 pb-3 pt-1.5 text-xs text-stone-500 dark:text-stone-400 border-t border-stone-200 dark:border-[#383834] whitespace-pre-wrap leading-relaxed">
                                  {msg.reasoningContent}
                                </div>
                              )}
                            </div>
                          )}

                          {/* Tool Calls */}
                          {msg.toolRecords && msg.toolRecords.length > 0 && (
                            <div className="space-y-1.5">
                              {msg.toolRecords.map((tr) => (
                                <ToolCallBox
                                  key={tr.id}
                                  record={tr}
                                  onOpenFile={openFileInArtifact}
                                  onUndo={handleUndoSnapshot}
                                  onViewDiff={(id) => openDiffView(msg.toolRecords, id)}
                                  onRunPython={(rel) => setPythonRun(rel)}
                                  onOpenProduced={openAnyProjectFile}
                                  onApprove={(approve, always) =>
                                    handleToolApproval(msg.id, tr.id, approve, always)
                                  }
                                  approvalDisabled={isStreaming}
                                />
                              ))}
                              {msg.toolRecords.filter((r) => r.snapshotId).length > 1 && (
                                <button
                                  onClick={() => openDiffView(msg.toolRecords)}
                                  className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-medium text-[#A50034] dark:text-[#FF5C8D] hover:bg-[#A50034]/5 dark:hover:bg-[#FF5C8D]/10 cursor-pointer"
                                >
                                  <FileDiff className="w-3.5 h-3.5" />
                                  이 답변의 변경 사항 {msg.toolRecords.filter((r) => r.snapshotId).length}개 모두 보기
                                </button>
                              )}
                            </div>
                          )}

                          {/* Main Markdown Response */}
                          {msg.content && <MarkdownBody text={msg.content} onClick={handleMarkdownLinkClick} />}
                          <TurnStatsLine stats={msg.stats} />

                          {/* Claude-Style Artifact Card */}
                          {msgArtifact && (
                            <div
                              onClick={() => openArtifactRef(msgArtifact)}
                              className="p-3 rounded-xl bg-[#FAF9F5] dark:bg-[#1E1E1C] hover:bg-[#F2EFE9] dark:hover:bg-[#2C2C29] border border-stone-200 dark:border-[#383834] flex items-center justify-between cursor-pointer transition"
                            >
                              <div className="flex items-center gap-2.5">
                                <div className="w-8 h-8 rounded-lg bg-[#A50034]/10 dark:bg-[#A50034]/25 text-[#A50034] dark:text-[#FF5C8D] flex items-center justify-center">
                                  <FileText className="w-4 h-4" />
                                </div>
                                <div>
                                  <div className="text-xs font-bold text-stone-900 dark:text-stone-100">
                                    {msgArtifact.title}
                                  </div>
                                  <div className="text-xs text-stone-500 dark:text-stone-400">
                                    클릭하여 우측 문서 패널에서 열기
                                  </div>
                                </div>
                              </div>
                              <PanelRightOpen className="w-4 h-4 text-stone-500 dark:text-stone-400" />
                            </div>
                          )}

                          {/* Assistant Action Bar (Copy, Save, Regenerate) */}
                          {!isStreaming && (
                            <div
                              className={`pt-1 flex items-center gap-2 transition ${
                                answerSaveMenu === msg.id || savingAnswerId === msg.id
                                  ? "opacity-100"
                                  : "opacity-0 group-hover:opacity-100 focus-within:opacity-100"
                              }`}
                            >
                              <button
                                onClick={() =>
                                  handleCopyText(msg.id, msg.content)
                                }
                                className="text-xs text-stone-500 dark:text-stone-400 hover:text-stone-900 dark:hover:text-stone-100 flex items-center gap-1 px-2 py-1 rounded-lg hover:bg-stone-100 dark:hover:bg-[#32322F] cursor-pointer"
                              >
                                {copiedId === msg.id ? (
                                  <Check className="w-3.5 h-3.5 text-emerald-600" />
                                ) : (
                                  <Copy className="w-3.5 h-3.5" />
                                )}
                                <span>복사</span>
                              </button>
                              {msg.content.trim() && (
                                <div className="relative">
                                  <button
                                    onClick={(e) => {
                                      if (answerSaveMenu === msg.id) {
                                        setAnswerSaveMenu(null);
                                        return;
                                      }
                                      const btn = e.currentTarget.getBoundingClientRect();
                                      const scroller = e.currentTarget.closest(".overflow-y-auto");
                                      const bottom = scroller ? scroller.getBoundingClientRect().bottom : window.innerHeight;
                                      setAnswerSaveMenuUp(bottom - btn.bottom < 120);
                                      setAnswerSaveMenu(msg.id);
                                    }}
                                    disabled={savingAnswerId !== null}
                                    aria-haspopup="menu"
                                    aria-expanded={answerSaveMenu === msg.id}
                                    title="이 답변을 파일로 저장"
                                    className="text-xs text-stone-500 dark:text-stone-400 hover:text-stone-900 dark:hover:text-stone-100 disabled:opacity-50 flex items-center gap-1 px-2 py-1 rounded-lg hover:bg-stone-100 dark:hover:bg-[#32322F] cursor-pointer"
                                  >
                                    {savingAnswerId === msg.id ? (
                                      <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                                    ) : (
                                      <Download className="w-3.5 h-3.5" />
                                    )}
                                    <span>{savingAnswerId === msg.id ? "저장 중..." : "저장"}</span>
                                    <ChevronDown className="w-3 h-3" />
                                  </button>
                                  {answerSaveMenu === msg.id && (
                                    <>
                                      <div
                                        className="fixed inset-0 z-30"
                                        onClick={() => setAnswerSaveMenu(null)}
                                      />
                                      <div
                                        role="menu"
                                        onKeyDown={(e) => {
                                          if (e.key === "Escape") setAnswerSaveMenu(null);
                                        }}
                                        className={`absolute left-0 z-40 w-52 ${answerSaveMenuUp ? "bottom-full mb-1" : "top-full mt-1"} py-1 rounded-xl bg-white dark:bg-[#2A2A27] border border-stone-200 dark:border-[#3A3A36] shadow-lg`}
                                      >
                                        {([
                                          ["markdown", "Markdown (.md)", "서식 기호 그대로 텍스트로"],
                                          ["docx", "Word 문서 (.docx)", "제목·표·목록을 Word 서식으로"],
                                          ["hwpx", "한글 문서 (.hwpx)", "제목·표·목록을 한글 서식으로"],
                                        ] as const).map(([fmt, label, hint], i) => (
                                          <button
                                            key={fmt}
                                            role="menuitem"
                                            autoFocus={i === 0}
                                            onClick={() => handleSaveAnswer(idx, fmt)}
                                            className="w-full text-left px-3 py-1.5 hover:bg-stone-100 dark:hover:bg-[#34342F] focus:bg-stone-100 dark:focus:bg-[#34342F] focus:outline-none cursor-pointer"
                                          >
                                            <div className="text-xs font-semibold text-stone-800 dark:text-stone-100">{label}</div>
                                            <div className="text-[11px] text-stone-500 dark:text-stone-400">{hint}</div>
                                          </button>
                                        ))}
                                      </div>
                                    </>
                                  )}
                                </div>
                              )}
                              <button
                                onClick={() =>
                                  handleRegenerateFromAssistantIndex(idx)
                                }
                                className="text-xs text-stone-500 dark:text-stone-400 hover:text-stone-900 dark:hover:text-stone-100 flex items-center gap-1 px-2 py-1 rounded-lg hover:bg-stone-100 dark:hover:bg-[#32322F] cursor-pointer"
                              >
                                <RefreshCw className="w-3.5 h-3.5" />
                                <span>다시 생성</span>
                              </button>
                            </div>
                          )}
                        </div>
                      )}
                    </div>
                  );
                })}

                {/* 답변을 기다리는 질문 (끝나면 대화 기록으로 옮겨진다) */}
                {activeRun?.question && (
                  <div className="flex flex-col items-end">
                    <div className="max-w-2xl bg-[#EFECE6] dark:bg-[#2D2D2A] text-stone-900 dark:text-stone-100 rounded-2xl px-4 py-3 text-sm whitespace-pre-wrap leading-relaxed">
                      {activeRun.question}
                    </div>
                  </div>
                )}

                {/* Active Streaming Message */}
                {isStreaming && (
                  <div className="bg-white dark:bg-[#262624] border border-[#E5E3DB] dark:border-[#363632] rounded-2xl p-5 space-y-3 shadow-2xs">
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-2 text-xs text-[#A50034] dark:text-[#FF5C8D] font-semibold">
                        <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                        <span>
                          {retryNotice || "답변을 작성하고 있습니다..."}
                        </span>
                      </div>
                      <button
                        onClick={handleStopGeneration}
                        className="px-2.5 py-1 rounded-lg bg-stone-100 dark:bg-[#32322F] hover:bg-stone-200 dark:hover:bg-[#3C3C38] text-xs font-semibold text-stone-700 dark:text-stone-200 flex items-center gap-1 cursor-pointer"
                      >
                        <Square className="w-3 h-3 fill-current" /> 중지
                      </button>
                    </div>

                    {streamingReasoning && (
                      <div className="p-3 rounded-xl bg-[#FAF9F5] dark:bg-[#1E1E1C] border border-stone-200 dark:border-[#383834] text-xs text-stone-500 dark:text-stone-400 whitespace-pre-wrap max-h-40 overflow-y-auto leading-relaxed">
                        {streamingReasoning}
                      </div>
                    )}

                    {streamingTools.map((tr) => (
                      <ToolCallBox
                        key={tr.id}
                        record={tr}
                        onOpenFile={openFileInArtifact}
                        onUndo={handleUndoSnapshot}
                        onViewDiff={(id) => openDiffView(streamingTools, id)}
                        onOpenProduced={openAnyProjectFile}
                        onApprove={() => {}}
                        approvalDisabled
                      />
                    ))}

                    {streamingContent && <StreamingMarkdown text={streamingContent} onClick={handleMarkdownLinkClick} />}
                  </div>
                )}

                <div ref={chatEndRef} />
              </div>
            </div>

            {/* Bottom Input Composer */}
            <div className="px-6 pb-5 pt-2">
              <div
                className="relative max-w-3xl mx-auto bg-white dark:bg-[#262624] border border-stone-300 dark:border-[#3A3A36] rounded-2xl p-3 shadow-xs focus-within:border-[#A50034] dark:focus-within:border-[#FF5C8D] space-y-2"
                onDragOver={(e) => {
                  if (e.dataTransfer.types.includes(DRAG_MIME)) {
                    e.preventDefault();
                    e.dataTransfer.dropEffect = "copy";
                  }
                }}
                onDrop={(e) => {
                  const rel = e.dataTransfer.getData(DRAG_MIME);
                  if (rel) {
                    e.preventDefault();
                    attachFile(rel);
                  }
                }}
              >
                {mention.popup}
                {attachedFiles.length > 0 && (
                  <div className="flex flex-wrap gap-1.5 pb-1.5 border-b border-stone-100 dark:border-[#363632]">
                    {attachedFiles.map((f) => {
                      const fileName = f.split(/[\\/]/).pop() || f;
                      return (
                        <span
                          key={f}
                          className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-[#FAF9F5] dark:bg-[#1E1E1C] border border-stone-200 dark:border-[#383834] text-xs font-medium text-stone-700 dark:text-stone-200"
                        >
                          <Paperclip className="w-3 h-3 text-[#A50034] dark:text-[#FF5C8D]" />
                          <span className="truncate max-w-[180px]">
                            {fileName}
                          </span>
                          <button
                            onClick={() =>
                              setAttachedFiles((prev) =>
                                prev.filter((item) => item !== f)
                              )
                            }
                            className="text-stone-400 hover:text-stone-800 dark:hover:text-stone-100"
                          >
                            <X className="w-3 h-3" />
                          </button>
                        </span>
                      );
                    })}
                  </div>
                )}

                <textarea
                  ref={composerRef}
                  rows={3}
                  value={inputPrompt}
                  onChange={(e) => {
                    setInputPrompt(e.target.value);
                    mention.sync();
                  }}
                  onKeyDown={(e) => {
                    if (mention.handleKeyDown(e)) return;
                    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                      e.preventDefault();
                      handleSend();
                    }
                  }}
                  onKeyUp={(e) => {
                    if (["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) mention.sync();
                  }}
                  onClick={() => mention.sync()}
                  onBlur={() => mention.close()}
                  placeholder={
                    isStreaming
                      ? "이 대화는 답변을 작성하고 있습니다. 다른 작업은 [새 채팅]이나 다른 대화에서 동시에 요청할 수 있습니다."
                      : explorerApi
                        ? "메시지를 입력하세요... (@로 프로젝트 파일 첨부)"
                        : "메시지를 입력하세요..."
                  }
                  className="w-full text-sm text-stone-800 dark:text-stone-100 bg-transparent placeholder:text-stone-400 dark:placeholder:text-stone-500 resize-none focus:outline-none leading-relaxed"
                />

                <LowBudgetNotice info={rateInfo} />
                <div className="flex justify-between items-center pt-1">
                  <div className="flex items-center gap-1.5">
                    <button
                      onClick={handlePickAttachments}
                      className="px-2.5 py-1.5 rounded-lg hover:bg-stone-100 dark:hover:bg-[#32322F] text-xs font-medium text-stone-600 dark:text-stone-300 flex items-center gap-1.5 cursor-pointer transition"
                      title="탐색기에서 파일 첨부"
                    >
                      <Paperclip className="w-3.5 h-3.5" />
                      <span>파일 첨부</span>
                    </button>
                    <div
                      role="radiogroup"
                      aria-label="추론 강도"
                      className="flex items-center gap-0.5 pl-1.5 ml-0.5 border-l border-stone-200 dark:border-[#383834]"
                    >
                      <span className="px-1 text-[11px] text-stone-400 dark:text-stone-500 select-none">추론</span>
                      {REASONING_EFFORTS.map((opt) => {
                        const selected = settings.reasoningEffort === opt.value;
                        return (
                          <button
                            key={opt.value}
                            type="button"
                            role="radio"
                            aria-checked={selected}
                            title={opt.hint}
                            onClick={() => changeReasoningEffort(opt.value)}
                            className={`px-2 py-1 rounded-md text-xs font-medium cursor-pointer transition ${
                              selected
                                ? "bg-[#A50034]/10 text-[#A50034] dark:bg-[#FF5C8D]/15 dark:text-[#FF5C8D]"
                                : "text-stone-500 dark:text-stone-400 hover:bg-stone-100 dark:hover:bg-[#32322F]"
                            }`}
                          >
                            {opt.label}
                          </button>
                        );
                      })}
                    </div>
                  </div>

                  <div className="flex items-center gap-2.5">
                    <RateBadge info={rateInfo} />
                  {isStreaming ? (
                    <button
                      onClick={handleStopGeneration}
                      className="px-4 py-1.5 rounded-xl bg-stone-800 dark:bg-stone-700 hover:bg-stone-900 dark:hover:bg-stone-600 text-white text-xs font-semibold flex items-center gap-1.5 cursor-pointer"
                    >
                      <Square className="w-3 h-3 fill-current" /> 중지
                    </button>
                  ) : (
                    <button
                      onClick={() => handleSend()}
                      disabled={!inputPrompt.trim()}
                      className="px-4 py-1.5 rounded-xl bg-[#A50034] hover:bg-[#8A002B] disabled:opacity-40 text-white text-xs font-semibold flex items-center gap-1.5 cursor-pointer transition"
                    >
                      <Send className="w-3.5 h-3.5" /> 전송
                    </button>
                  )}
                  </div>
                </div>
              </div>
            </div>
          </>
        )}
      </main>

      {/* 문서 캔버스: 형식(Markdown·Word·PowerPoint·PDF·HTML·텍스트)에 맞춘 편집기 */}
      {activeArtifact && (
        <aside
          ref={canvasRef}
          style={{ width: shownCanvasWidth }}
          className="relative bg-white dark:bg-[#232321] border-l border-[#E2E0D8] dark:border-[#2E2E2A] flex flex-col shrink-0 shadow-lg transition-[width] duration-150 min-w-0"
        >
          <PanelResizer
            target={canvasRef}
            width={shownCanvasWidth}
            min={CANVAS_MIN_WIDTH}
            max={canvasMaxWidth}
            onChange={changeCanvasWidth}
            onReset={() => changeCanvasWidth(CANVAS_DEFAULT_WIDTH)}
          />
          <React.Suspense
            fallback={<div className="p-6 text-xs text-stone-500">문서 편집기를 불러오는 중...</div>}
          >
          <DocumentCanvas
            doc={activeArtifact}
            projectId={currentScopeId === "general-chat" ? null : activeProject?.id ?? null}
            projectRoot={currentScopeId === "general-chat" ? undefined : activeProject?.rootPath}
            expanded={canvasExpanded}
            onToggleExpand={() => setCanvasExpanded((v) => !v)}
            onClose={() => setActiveArtifact(null)}
            onSaved={handleCanvasSaved}
            onAttach={(abs) =>
              setAttachedFiles((prev) => Array.from(new Set([...prev, abs])))
            }
            onError={(msg) => setErrorBanner(msg)}
            onRunPython={
              activeArtifact.filePath && isPythonScript(activeArtifact.filePath) && activeProject && currentScopeId !== "general-chat"
                ? () => setPythonRun(activeArtifact.filePath!)
                : undefined
            }
            onOpenExternal={
              activeArtifact.filePath && activeProject && currentScopeId !== "general-chat"
                ? () =>
                    BackendAPI.openProjectFileExternal(activeProject.id, activeArtifact.filePath!).catch((err) =>
                      setErrorBanner(String(err))
                    )
                : undefined
            }
            historyCount={activeFileHistory.length}
            onShowHistory={() =>
              activeFileHistory.length > 0 &&
              setDiffView({
                ids: activeFileHistory.map((s) => s.id),
                initial: activeFileHistory[0].id,
                variant: "history",
              })
            }
          />
          </React.Suspense>
        </aside>
      )}

      {pythonRun && activeProject && (
        <React.Suspense fallback={null}>
          <PythonRunDialog
            projectId={activeProject.id}
            projectRoot={activeProject.rootPath}
            relativePath={pythonRun}
            onClose={() => setPythonRun(null)}
            onOpenFile={(rel) => {
              if (!opensInExternalApp(rel)) setPythonRun(null);
              openAnyProjectFile(rel);
            }}
            onReveal={(rel) =>
              BackendAPI.revealProjectPath(activeProject.id, rel).catch((err) => setErrorBanner(String(err)))
            }
            onViewScript={(rel) => {
              setPythonRun(null);
              openFileInArtifact(rel);
            }}
            onFilesChanged={bumpFileTree}
            onAskFix={(prompt) => {
              // 바로 보내지 않고 입력창에 넣어, 사용자가 확인·보완한 뒤 보내게 한다.
              setPythonRun(null);
              setShowProjectOverview(false);
              setInputPrompt(prompt);
              requestAnimationFrame(() => {
                const el = composerRef.current;
                if (!el) return;
                el.focus();
                el.setSelectionRange(el.value.length, el.value.length);
              });
            }}
          />
        </React.Suspense>
      )}

      {diffView && (
        <React.Suspense fallback={null}>
          <DiffViewer
            snapshotIds={diffView.ids}
            initialId={diffView.initial}
            onClose={() => setDiffView(null)}
            onUndo={handleUndoSnapshot}
            onOpenFile={openFileInArtifact}
            variant={diffView.variant}
          />
        </React.Suspense>
      )}

      {showSkills && (
        <SkillsDialog
          projectId={activeMode !== "chat" ? activeProject?.id : undefined}
          projectName={activeMode !== "chat" ? activeProject?.name : undefined}
          onClose={() => setShowSkills(false)}
        />
      )}

      {showMcp && <McpDialog onClose={() => setShowMcp(false)} />}

      {/* Modal 1: API & Model Settings */}
      {showSettingsModal && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-white dark:bg-[#262624] border border-stone-200 dark:border-[#3A3A36] rounded-2xl w-full max-w-lg max-h-[90vh] overflow-y-auto p-6 shadow-xl space-y-4">
            <div className="flex items-center justify-between">
              <h3 className="text-sm font-bold text-stone-900 dark:text-stone-100">
                API 및 모델 설정
              </h3>
              <button onClick={closeSettingsModal}>
                <X className="w-4 h-4 text-stone-400" />
              </button>
            </div>

            <div className="space-y-3 text-xs">
              <div>
                <label className="font-semibold text-stone-700 dark:text-stone-300 block mb-1">
                  API Key
                </label>
                <input
                  type="password"
                  autoComplete="off"
                  value={settings.apiKey ?? ""}
                  onChange={(e) =>
                    setSettings({ ...settings, apiKey: e.target.value })
                  }
                  placeholder={
                    settings.apiKeySet
                      ? "저장된 키 사용 중 — 바꾸려면 새 키 입력"
                      : "발급받은 EXAONE API Key 입력"
                  }
                  className="w-full bg-white dark:bg-[#1E1E1C] border border-stone-300 dark:border-[#3A3A36] rounded-xl px-3 py-2 text-stone-900 dark:text-stone-100 focus:outline-none focus:border-[#A50034]"
                />
                {keyDays !== null && (
                  <div
                    className={`mt-1 text-[11px] ${
                      keyDays >= KEY_VALID_DAYS - 5
                        ? "text-amber-700 dark:text-amber-400 font-semibold"
                        : "text-stone-500 dark:text-stone-400"
                    }`}
                  >
                    이 키를 입력한 지 {keyDays}일 지났습니다. K-EXAONE API 키는 발급한 날부터 30일 뒤 만료되니,
                    {keyDays >= KEY_VALID_DAYS - 5 ? " 새 키를 발급받아 바꿔 주세요." : " 그 전에 새 키로 바꿔 주세요."}
                  </div>
                )}
                <div className="mt-1 flex items-center justify-between text-[11px] text-stone-500 dark:text-stone-400">
                  <span>키는 Windows 자격 증명 관리자에 암호화되어 저장됩니다.</span>
                  {settings.apiKeySet && (
                    <button
                      type="button"
                      onClick={async () => {
                        if (!window.confirm("저장된 API 키를 삭제할까요?")) return;
                        try {
                          const saved = await BackendAPI.clearApiKey();
                          setSettings({ ...saved, apiKey: "" });
                          setSettingsError(null);
                        } catch (err) {
                          setSettingsError(String(err));
                        }
                      }}
                      className="text-[#A50034] dark:text-[#FF5C8D] font-semibold hover:underline cursor-pointer"
                    >
                      저장된 키 삭제
                    </button>
                  )}
                </div>
              </div>
              <div>
                <label className="font-semibold text-stone-700 dark:text-stone-300 block mb-1">
                  엔드포인트 주소
                </label>
                <input
                  type="text"
                  value={settings.baseUrl}
                  onChange={(e) =>
                    setSettings({ ...settings, baseUrl: e.target.value })
                  }
                  className="w-full bg-white dark:bg-[#1E1E1C] border border-stone-300 dark:border-[#3A3A36] rounded-xl px-3 py-2 font-mono text-stone-900 dark:text-stone-100 focus:outline-none focus:border-[#A50034]"
                />
              </div>
              <div>
                <label className="font-semibold text-stone-700 dark:text-stone-300 block mb-1">
                  모델 이름
                </label>
                <input
                  type="text"
                  value={settings.model}
                  onChange={(e) =>
                    setSettings({ ...settings, model: e.target.value })
                  }
                  className="w-full bg-white dark:bg-[#1E1E1C] border border-stone-300 dark:border-[#3A3A36] rounded-xl px-3 py-2 font-mono text-stone-900 dark:text-stone-100 focus:outline-none focus:border-[#A50034]"
                />
              </div>
            </div>

            <div className="pt-3 border-t border-stone-200 dark:border-[#34342F] text-xs space-y-2">
              <h4 className="font-bold text-stone-900 dark:text-stone-100">실행 환경</h4>
              <RuntimePanel />
            </div>

            <div className="pt-3 border-t border-stone-200 dark:border-[#34342F] text-xs space-y-2">
              <h4 className="font-bold text-stone-900 dark:text-stone-100">업데이트</h4>
              <UpdateSettings
                updater={updater}
                runningCount={Object.keys(runs).length}
                autoCheck={settings.checkUpdates !== false}
                onAutoCheckChange={async (value) => {
                  setSettings((prev) => ({ ...prev, checkUpdates: value }));
                  try {
                    // 키는 보내지 않는다 (비어 있으면 저장된 키를 그대로 둔다).
                    await BackendAPI.saveSettings({ ...settings, apiKey: "", checkUpdates: value });
                  } catch (err) {
                    setSettingsError(String(err));
                  }
                }}
              />
            </div>

            {settingsError && (
              <div className="rounded-xl border border-red-200 dark:border-red-900/60 bg-red-50 dark:bg-red-950/40 px-3 py-2 text-xs text-red-700 dark:text-red-300">
                {settingsError}
              </div>
            )}
            <div className="flex justify-end gap-2 pt-2">
              <button
                onClick={closeSettingsModal}
                className="px-4 py-2 rounded-xl bg-stone-100 dark:bg-[#32322F] text-stone-700 dark:text-stone-200 text-xs font-semibold cursor-pointer"
              >
                취소
              </button>
              <button
                onClick={async () => {
                  try {
                    const saved = await BackendAPI.saveSettings(settings);
                    setSettings({ ...saved, apiKey: "" });
                    setSettingsError(null);
                    setShowSettingsModal(false);
                  } catch (err) {
                    setSettingsError(String(err));
                  }
                }}
                className="px-4 py-2 rounded-xl bg-[#A50034] text-white text-xs font-bold cursor-pointer"
              >
                저장
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Modal 2: Project Configuration with Native Folder Explorer Picker */}
      {showProjectModal && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-white dark:bg-[#262624] border border-stone-200 dark:border-[#3A3A36] rounded-2xl w-full max-w-md p-6 shadow-xl space-y-4">
            <div className="flex items-center justify-between">
              <h3 className="text-sm font-bold text-stone-900 dark:text-stone-100">
                프로젝트 설정
              </h3>
              <button onClick={() => setShowProjectModal(false)}>
                <X className="w-4 h-4 text-stone-400" />
              </button>
            </div>

            <div className="space-y-3.5 text-xs">
              <div>
                <label className="font-semibold text-stone-700 dark:text-stone-300 block mb-1">
                  프로젝트 이름
                </label>
                <input
                  type="text"
                  value={editingProject.name}
                  onChange={(e) =>
                    setEditingProject({
                      ...editingProject,
                      name: e.target.value,
                    })
                  }
                  placeholder="예: 신사업 기획 프로젝트"
                  className="w-full bg-white dark:bg-[#1E1E1C] border border-stone-300 dark:border-[#3A3A36] rounded-xl px-3 py-2 text-stone-900 dark:text-stone-100 focus:outline-none focus:border-[#A50034]"
                />
              </div>

              <div>
                <label className="font-semibold text-stone-700 dark:text-stone-300 block mb-1">
                  폴더 경로
                </label>
                <div className="flex gap-1.5">
                  <input
                    type="text"
                    value={editingProject.rootPath}
                    onChange={(e) =>
                      setEditingProject({
                        ...editingProject,
                        rootPath: e.target.value,
                      })
                    }
                    placeholder="폴더를 선택하세요"
                    className="flex-1 bg-white dark:bg-[#1E1E1C] border border-stone-300 dark:border-[#3A3A36] rounded-xl px-3 py-2 font-mono text-stone-900 dark:text-stone-100 focus:outline-none focus:border-[#A50034]"
                  />
                  <button
                    type="button"
                    onClick={handleBrowseFolderForModal}
                    className="px-3 py-2 rounded-xl bg-stone-100 dark:bg-[#32322F] hover:bg-stone-200 dark:hover:bg-[#3C3C38] border border-stone-300 dark:border-[#42423E] text-stone-800 dark:text-stone-200 font-semibold flex items-center gap-1 shrink-0 cursor-pointer transition"
                  >
                    <FolderOpen className="w-3.5 h-3.5 text-[#A50034] dark:text-[#FF5C8D]" />
                    <span>폴더 선택</span>
                  </button>
                </div>
              </div>

              <div>
                <label className="font-semibold text-stone-700 dark:text-stone-300 block mb-1">
                  프로젝트 지침
                </label>
                <textarea
                  rows={3}
                  value={editingProject.instructions}
                  onChange={(e) =>
                    setEditingProject({
                      ...editingProject,
                      instructions: e.target.value,
                    })
                  }
                  placeholder="이 프로젝트에서 EXAONE이 참고할 지침이나 역할을 입력하세요."
                  className="w-full bg-white dark:bg-[#1E1E1C] border border-stone-300 dark:border-[#3A3A36] rounded-xl p-3 text-stone-900 dark:text-stone-100 focus:outline-none focus:border-[#A50034]"
                />
              </div>
            </div>

            <div className="flex justify-between items-center pt-2">
              {editingProject.id && projects.length > 1 ? (
                <button
                  onClick={async () => {
                    await BackendAPI.deleteProject(editingProject.id!);
                    const remaining = projects.filter(
                      (p) => p.id !== editingProject.id
                    );
                    setProjects(remaining);
                    setActiveProjectId(remaining[0].id);
                    setShowProjectModal(false);
                  }}
                  className="text-xs text-red-600 dark:text-red-400 font-semibold hover:underline cursor-pointer"
                >
                  삭제
                </button>
              ) : (
                <div />
              )}
              <div className="flex gap-2">
                <button
                  onClick={() => setShowProjectModal(false)}
                  className="px-4 py-2 rounded-xl bg-stone-100 dark:bg-[#32322F] text-stone-700 dark:text-stone-200 text-xs font-semibold cursor-pointer"
                >
                  취소
                </button>
                <button
                  onClick={async () => {
                    if (!editingProject.name.trim()) return;
                    const saved = await BackendAPI.createOrUpdateProject(
                      editingProject
                    );
                    setProjects((prev) => {
                      const exists = prev.some((p) => p.id === saved.id);
                      return exists
                        ? prev.map((p) => (p.id === saved.id ? saved : p))
                        : [...prev, saved];
                    });
                    setActiveProjectId(saved.id);
                    setShowProjectModal(false);
                  }}
                  className="px-4 py-2 rounded-xl bg-[#A50034] text-white text-xs font-bold cursor-pointer"
                >
                  저장
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/** 승인 카드에 보여 줄 내용: 명령은 그대로, 파일 쓰기는 경로와 내용을 보여 준다. */
function approvalPreview(record: ToolExecutionRecord): string {
  let args: Record<string, unknown> = {};
  try {
    args = JSON.parse(record.argumentsJson || "{}");
  } catch {
    return record.argumentsJson;
  }
  const str = (k: string) => (typeof args[k] === "string" ? (args[k] as string) : "");
  switch (record.toolName) {
    case "run_command":
      return `> ${str("command")}`;
    case "delete_path":
      return `삭제: ${str("relative_path")}`;
    case "run_python":
      if (str("skill"))
        return `Skill '${str("skill")}'의 Python 스크립트 실행: ${str("relative_path")}\n(실행 위치: 프로젝트 폴더, 최대 3분. 믿을 수 있는 Skill인지 확인하세요.)`;
      // 스크립트 내용을 함께 받은 경우(방금 저장됨)에는 실행할 내용을 바로 보여 준다.
      if (str("code"))
        return `Python 스크립트 실행: ${str("relative_path")}\n(실행 위치: 프로젝트 폴더, 최대 3분. 아래 내용으로 방금 저장했습니다.)\n\n${str("code")}`;
      return `Python 스크립트 실행: ${str("relative_path")}\n(실행 위치: 프로젝트 폴더, 최대 3분. 실행 전에 파일 이름을 눌러 내용을 확인할 수 있습니다.)`;
    case "write_file":
      return `파일: ${str("relative_path")}\n\n${str("content")}`;
    case "modify_file":
      return `파일: ${str("relative_path")}\n\n- 바꿀 원문:\n${str("target")}\n\n+ 새 내용:\n${str("replacement")}`;
    default:
      if (record.toolName.startsWith("mcp__")) return "보낼 값:\n" + JSON.stringify(args, null, 2);
      return JSON.stringify(args, null, 2);
  }
}

function ToolCallBox({
  record,
  onOpenFile,
  onUndo,
  onViewDiff,
  onRunPython,
  onOpenProduced,
  onApprove,
  approvalDisabled = false,
}: {
  record: ToolExecutionRecord;
  onOpenFile: (rel: string) => void;
  onUndo: (snapId: string) => void;
  onViewDiff?: (snapId: string) => void;
  /** .py 스크립트를 실행 창으로 */
  onRunPython?: (rel: string) => void;
  /** run_python이 만든 파일 열기 */
  onOpenProduced?: (rel: string) => void;
  /** always: MCP 도구를 앞으로 승인 없이 실행 */
  onApprove: (approve: boolean, always?: boolean) => void;
  approvalDisabled?: boolean;
}) {
  const isMcp = record.toolName.startsWith("mcp__");
  // 프로젝트 단위로 "항상 허용"할 수 있는 도구 (파일 삭제와 보호 폴더 쓰기는 매번 묻는다)
  const projectAlways = AUTO_APPROVE_TOOLS.some((t) => t.tool === record.toolName);
  const isPending = record.status === "pending_approval";
  const isRunning = record.status === "running";
  // AI가 작성했거나 실행한 Python 스크립트는 카드에서 바로 (다시) 실행할 수 있다.
  const runnableScript =
    onRunPython &&
    record.filePath &&
    isPythonScript(record.filePath) &&
    ((record.toolName === "run_python" && (record.status === "completed" || record.status === "error")) ||
      ((record.toolName === "write_file" || record.toolName === "modify_file") && record.status === "completed"))
      ? record.filePath
      : null;

  return (
    <div className="rounded-xl border border-stone-200 dark:border-[#383834] bg-[#FAF9F5] dark:bg-[#1E1E1C] px-3 py-2 text-xs space-y-1">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2 font-mono">
          <span className="px-1.5 py-0.5 rounded bg-stone-800 dark:bg-stone-700 text-white">
            {record.toolName}
          </span>
          {record.filePath && (
            <button
              onClick={() => onOpenFile(record.filePath!)}
              className="text-[#A50034] dark:text-[#FF5C8D] font-semibold hover:underline cursor-pointer"
            >
              {record.filePath}
            </button>
          )}
        </div>
        <div className="flex items-center gap-2">
          {runnableScript && (
            <button
              onClick={() => onRunPython!(runnableScript)}
              title="이 스크립트를 앱 안에서 실행해 결과 파일을 만듭니다"
              className="px-2 py-0.5 rounded bg-[#A50034] hover:bg-[#8A002B] text-white font-medium flex items-center gap-1 cursor-pointer"
            >
              <Play className="w-3 h-3" /> {record.toolName === "run_python" ? "다시 실행" : "실행"}
            </button>
          )}
          {record.snapshotId && onViewDiff && (
            <button
              onClick={() => onViewDiff(record.snapshotId!)}
              className="px-2 py-0.5 rounded bg-[#A50034]/10 dark:bg-[#FF5C8D]/15 hover:bg-[#A50034]/15 dark:hover:bg-[#FF5C8D]/25 text-[#A50034] dark:text-[#FF5C8D] font-medium flex items-center gap-1 cursor-pointer"
            >
              <FileDiff className="w-3 h-3" /> 변경 보기
            </button>
          )}
          {record.snapshotId && (
            <button
              onClick={() => onUndo(record.snapshotId!)}
              className="px-2 py-0.5 rounded bg-stone-200 dark:bg-[#32322F] hover:bg-stone-300 dark:hover:bg-[#3E3E3A] text-stone-700 dark:text-stone-200 font-medium flex items-center gap-1 cursor-pointer"
            >
              <RotateCcw className="w-3 h-3" /> 되돌리기
            </button>
          )}
          {record.status === "completed" && (
            <span className="text-emerald-700 dark:text-emerald-400 font-semibold flex items-center gap-1">
              <Check className="w-3.5 h-3.5" /> 완료
            </span>
          )}
          {isRunning && (
            <span className="text-amber-700 dark:text-amber-300 font-semibold flex items-center gap-1">
              <RefreshCw className="w-3.5 h-3.5 animate-spin" /> 실행 중
            </span>
          )}
        </div>
      </div>

      {isPending ? (
        <div className="pt-1 space-y-1.5">
          <div className="text-amber-800 dark:text-amber-300">
            {record.output && record.output !== "사용자 승인을 기다리고 있습니다."
              ? record.output
              : "실행 승인이 필요합니다. 내용을 확인하세요."}
          </div>
          <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-all rounded-lg bg-stone-900 text-stone-100 px-2.5 py-2 font-mono text-[11px]">
            {approvalPreview(record)}
          </pre>
          <div className="flex justify-end gap-1.5">
            <button
              onClick={() => onApprove(true)}
              disabled={approvalDisabled}
              className="px-2.5 py-1 rounded bg-emerald-600 text-white font-semibold cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
            >
              승인
            </button>
            {(isMcp || projectAlways) && (
              <button
                onClick={() => onApprove(true, true)}
                disabled={approvalDisabled}
                title={
                  isMcp
                    ? "이 도구는 앞으로 묻지 않고 실행합니다 (MCP 서버 설정에서 되돌릴 수 있음)"
                    : "이 프로젝트에서는 앞으로 묻지 않고 실행합니다 ([프로젝트 관리]에서 되돌릴 수 있음)"
                }
                className="px-2.5 py-1 rounded bg-emerald-600/15 text-emerald-800 dark:text-emerald-300 font-semibold cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
              >
                항상 허용
              </button>
            )}
            <button
              onClick={() => onApprove(false)}
              disabled={approvalDisabled}
              className="px-2.5 py-1 rounded bg-stone-300 dark:bg-[#383834] text-stone-800 dark:text-stone-200 font-semibold cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
            >
              거부
            </button>
          </div>
        </div>
      ) : (
        <div className="text-stone-500 dark:text-stone-400 font-mono truncate" title={record.output}>
          {record.output}
        </div>
      )}

      {record.producedFiles && record.producedFiles.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5 pt-0.5">
          <span className="text-stone-500 dark:text-stone-400">만든 파일:</span>
          {record.producedFiles.map((f) => (
            <button
              key={f}
              onClick={() => (onOpenProduced ?? onOpenFile)(f)}
              title={`${f} 열기`}
              className="px-2 py-0.5 rounded-md border border-[#A50034]/30 dark:border-[#FF5C8D]/30 bg-white dark:bg-[#262624] hover:bg-[#A50034]/5 dark:hover:bg-[#FF5C8D]/10 text-[#A50034] dark:text-[#FF5C8D] font-mono font-semibold cursor-pointer"
            >
              {f}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
