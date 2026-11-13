import React, { useEffect, useMemo, useRef, useState } from "react";
import { marked } from "marked";
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
  PanelRightClose,
  PanelRightOpen,
  Plus,
  RefreshCw,
  RotateCcw,
  Save,
  Search,
  Send,
  Settings,
  Square,
  Sun,
  Trash2,
  X,
} from "lucide-react";
import { BackendAPI } from "./api";
import {
  canvasDocFromBase64,
  type CanvasDoc,
  type CanvasSavedInfo,
} from "./canvas/canvasDoc";

// 문서 편집 라이브러리(TipTap, docx, pptxgenjs, pdf.js)는 캔버스를 처음 열 때 불러온다.
const DocumentCanvas = React.lazy(() => import("./canvas/DocumentCanvas"));
import { startDocBridge } from "./docs/bridge";
import { DRAG_MIME, FileExplorer, type ExplorerApi } from "./explorer/FileExplorer";
import { useFileMention } from "./explorer/FileMention";
import { docKindOf, docKindOfLanguage, isBinaryKind } from "./docs/formats";
import {
  AppSettings,
  ChatMessage,
  Conversation,
  FileSnapshot,
  Project,
  ToolExecutionRecord,
} from "./types";

/** 메시지에서 캔버스로 열 문서: 파일(경로) 또는 코드 블록(본문) */
type ArtifactRef = Omit<CanvasDoc, "id">;

function extractArtifactFromMessage(msg: ChatMessage): ArtifactRef | null {
  // 가장 마지막에 작성/수정한 파일을 연다.
  const fileRecords = (msg.toolRecords || []).filter(
    (tr) =>
      (tr.toolName === "write_file" || tr.toolName === "modify_file") &&
      tr.status === "completed" &&
      tr.filePath
  );
  const last = fileRecords[fileRecords.length - 1];
  if (last?.filePath) {
    return {
      title: last.filePath.split("/").pop() || last.filePath,
      kind: docKindOf(last.filePath),
      filePath: last.filePath,
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

function isInsideRoot(root: string, absolute: string): string | null {
  const norm = (p: string) => p.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();
  const r = norm(root);
  const a = absolute.replace(/\//g, "\\");
  if (!norm(a).startsWith(r + "\\")) return null;
  return a.slice(r.length + 1).replace(/\\/g, "/");
}

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
  const [isStreaming, setIsStreaming] = useState<boolean>(false);
  const [streamingReasoning, setStreamingReasoning] = useState<string>("");
  const [streamingContent, setStreamingContent] = useState<string>("");
  const [streamingTools, setStreamingTools] = useState<ToolExecutionRecord[]>(
    []
  );
  const [retryNotice, setRetryNotice] = useState<string>("");
  const [errorBanner, setErrorBanner] = useState<string | null>(null);

  // Inline prompt editing
  const [editingMessageId, setEditingMessageId] = useState<string | null>(null);
  const [editingMessageText, setEditingMessageText] = useState<string>("");
  const [copiedId, setCopiedId] = useState<string | null>(null);

  // Collapsible Claude-style Right Artifact Drawer
  const [canvasExpanded, setCanvasExpanded] = useState<boolean>(false);
  // 사이드바 하단: 대화 목록 / 프로젝트 파일 탐색기
  const [sidebarTab, setSidebarTab] = useState<"chats" | "files">("chats");
  const [fileTreeVersion, setFileTreeVersion] = useState(0);
  const bumpFileTree = () => setFileTreeVersion((v) => v + 1);
  const composerRef = useRef<HTMLTextAreaElement | null>(null);
  const [activeArtifact, setActiveArtifact] = useState<CanvasDoc | null>(
    null
  );
  // Modals
  const [showSettingsModal, setShowSettingsModal] = useState<boolean>(false);
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
    return list.filter((c) => c.title.toLowerCase().includes(q));
  }, [conversations, activeMode, activeProject, searchQuery]);

  const activeConversation = useMemo(
    () =>
      filteredConversations.find((c) => c.id === activeConvId) ||
      filteredConversations[0] ||
      null,
    [filteredConversations, activeConvId]
  );

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
        setSettings(db.settings);
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
      } catch (err: any) {
        setErrorBanner(err?.message || String(err));
      }
    })();
  }, []);

  // Safe single event listener registration
  useEffect(() => {
    let cancelled = false;
    let unlistenFn: (() => void) | null = null;

    BackendAPI.onExaoneStream((payload) => {
      if (cancelled) return;
      if (payload.deltaReasoning) {
        setStreamingReasoning((prev) => prev + payload.deltaReasoning);
      }
      if (payload.deltaContent) {
        setStreamingContent((prev) => prev + payload.deltaContent);
      }
      if (payload.toolRecord) {
        setStreamingTools((prev) => [...prev, payload.toolRecord!]);
        const r = payload.toolRecord;
        if (r.status === "completed" && ["write_file", "modify_file", "delete_path"].includes(r.toolName)) {
          setFileTreeVersion((v) => v + 1);
        }
      }
      if (payload.snapshot) {
        setSnapshots((prev) => [...prev, payload.snapshot!]);
      }
      if (payload.statusNotice !== undefined && payload.statusNotice !== null) {
        setRetryNotice(payload.statusNotice);
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

  useEffect(() => {
    chatEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [
    activeConversation?.messages,
    streamingReasoning,
    streamingContent,
    streamingTools,
  ]);

  async function handleSwitchMode(mode: "chat" | "project" | "code") {
    setActiveMode(mode);
    setShowProjectOverview(mode === "project");
    const targetScope =
      mode === "chat" ? "general-chat" : activeProject?.id || "";
    const firstConv = conversations.find((c) => c.projectId === targetScope);
    setActiveConvId(firstConv ? firstConv.id : "");

    const style =
      mode === "code"
        ? "code_architect"
        : mode === "project"
        ? "project"
        : "default";
    const nextSettings = { ...settings, writingStyle: style as any };
    setSettings(nextSettings);
    try {
      await BackendAPI.saveSettings(nextSettings);
    } catch {
      // ignore
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

      const filesToSend = [...attachedFiles];
      if (!overrideText) {
        setInputPrompt("");
      }
      setAttachedFiles([]);
      setIsStreaming(true);
      setRetryNotice("");
      setStreamingReasoning("");
      setStreamingContent("");
      setStreamingTools([]);

      const updatedConv = await BackendAPI.sendChatMessage(
        targetConv.id,
        text,
        filesToSend
      );

      setConversations((prev) => {
        const exists = prev.some((c) => c.id === updatedConv.id);
        if (!exists) return [updatedConv, ...prev];
        return prev.map((c) => (c.id === updatedConv.id ? updatedConv : c));
      });

      const lastMsg = updatedConv.messages[updatedConv.messages.length - 1];
      if (lastMsg && lastMsg.role === "assistant") {
        const art = extractArtifactFromMessage(lastMsg);
        if (art) openArtifactRef(art);
      }
    } catch (err: any) {
      const msg = String(err);
      if (!overrideText) {
        setInputPrompt(text);
      }
      setErrorBanner(msg);
      if (msg.includes("API 키")) {
        setShowSettingsModal(true);
      }
    } finally {
      bumpFileTree();
      setIsStreaming(false);
      setRetryNotice("");
      setStreamingReasoning("");
      setStreamingContent("");
      setStreamingTools([]);
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
    setIsStreaming(true);
    setRetryNotice("");
    setStreamingReasoning("");
    setStreamingContent("");
    setStreamingTools([]);

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
      setErrorBanner(String(err));
    } finally {
      bumpFileTree();
      setIsStreaming(false);
      setRetryNotice("");
      setStreamingReasoning("");
      setStreamingContent("");
      setStreamingTools([]);
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

  async function handleStopGeneration() {
    await BackendAPI.cancelGeneration();
  }

  async function handleToolApproval(
    messageId: string,
    toolId: string,
    approve: boolean
  ) {
    if (!activeConversation || isStreaming) return;
    const convId = activeConversation.id;

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
    setIsStreaming(true);
    setRetryNotice(approve ? "승인된 작업을 실행하고 있습니다..." : "");
    setStreamingReasoning("");
    setStreamingContent("");
    setStreamingTools([]);

    try {
      const updated = await BackendAPI.resolveToolApproval(
        convId,
        messageId,
        toolId,
        approve
      );
      setConversations((prev) =>
        prev.map((c) => (c.id === updated.id ? updated : c))
      );
    } catch (err: any) {
      setErrorBanner(String(err));
      // 도구 결과는 저장됐지만 이어가기가 실패했을 수 있으므로 저장된 상태로 다시 맞춘다.
      try {
        const db = await BackendAPI.getInitialState();
        setConversations(db.conversations);
        setSnapshots(db.snapshots);
      } catch {
        // ignore
      }
    } finally {
      bumpFileTree();
      setIsStreaming(false);
      setRetryNotice("");
      setStreamingReasoning("");
      setStreamingContent("");
      setStreamingTools([]);
    }
  }

  async function handleUndoSnapshot(snapshotId: string) {
    try {
      await BackendAPI.undoFileSnapshot(snapshotId);
      bumpFileTree();
      if (activeArtifact?.filePath && activeProject) {
        await openFileInArtifact(activeArtifact.filePath);
      }
    } catch (err: any) {
      setErrorBanner(String(err));
    }
  }

  function handleCopyText(id: string, text: string) {
    navigator.clipboard.writeText(text);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 1800);
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
                  EXAONE
                </span>
                <span className="text-xs px-2 py-0.5 rounded-full bg-stone-200/90 dark:bg-[#2A2A27] text-stone-600 dark:text-stone-300 font-medium">
                  2.5
                </span>
              </div>

              <button
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
                      rootPath: activeProject?.rootPath || "c:\\DOCS\\EXAONE",
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
                onOpen={openFileInArtifact}
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
                placeholder="대화 검색..."
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
                  <span className="truncate pr-2">{c.title}</span>
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
        <div className="p-3 border-t border-[#E2E0D8] dark:border-[#2C2C29]">
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
                settings.apiKey
                  ? "text-emerald-700 dark:text-emerald-400"
                  : "text-amber-700 dark:text-amber-400"
              }`}
            >
              {settings.apiKey ? "연결됨" : "키 입력 필요"}
            </span>
          </button>
        </div>
      </aside>

      {/* Center Main Area */}
      <main className="flex-1 flex flex-col min-w-0 bg-[#FAF9F5] dark:bg-[#1E1E1C] transition-colors duration-150">
        {/* Clean Header */}
        <header className="h-13 border-b border-[#E5E3DB] dark:border-[#2E2E2A] px-6 flex items-center justify-between shrink-0 bg-[#FAF9F5]/90 dark:bg-[#1E1E1C]/90 backdrop-blur-xs">
          <div className="flex items-center gap-2.5 text-xs">
            <span className="font-bold text-sm text-stone-900 dark:text-stone-100">
              {activeMode === "chat" &&
                (activeConversation ? activeConversation.title : "새 채팅")}
              {activeMode === "project" && "프로젝트"}
              {activeMode === "code" && "코딩"}
            </span>
            {activeMode !== "chat" && activeProject && (
              <>
                <span className="text-stone-300 dark:text-stone-600">/</span>
                <span className="font-semibold text-stone-700 dark:text-stone-300">
                  {activeProject.name}
                </span>
              </>
            )}
          </div>

          <div className="flex items-center gap-2">
            {(activeMode === "project" || activeMode === "code") &&
              activeProject && (
                <button
                  onClick={handleBrowseFolderForActiveProject}
                  className="px-3 py-1.5 rounded-lg bg-white dark:bg-[#262624] hover:bg-stone-100 dark:hover:bg-[#30302C] border border-stone-300 dark:border-[#3A3A36] text-xs font-medium text-stone-700 dark:text-stone-200 flex items-center gap-1.5 cursor-pointer transition"
                  title="작업 폴더 변경"
                >
                  <FolderOpen className="w-3.5 h-3.5 text-[#A50034] dark:text-[#FF5C8D]" />
                  <span className="truncate max-w-[240px]">
                    {activeProject.rootPath}
                  </span>
                </button>
              )}

            {activeMode === "project" && (
              <button
                onClick={() => setShowProjectOverview(!showProjectOverview)}
                className="px-3 py-1.5 rounded-lg bg-white dark:bg-[#262624] hover:bg-stone-100 dark:hover:bg-[#30302C] border border-stone-300 dark:border-[#3A3A36] text-xs font-semibold text-stone-700 dark:text-stone-200 cursor-pointer transition"
              >
                {showProjectOverview ? "대화로 돌아가기" : "프로젝트 관리"}
              </button>
            )}

            <button
              onClick={handleOpenDocument}
              className="px-2.5 py-1.5 rounded-lg bg-stone-200/80 dark:bg-[#2C2C28] hover:bg-stone-300 dark:hover:bg-[#383833] text-xs font-semibold text-stone-700 dark:text-stone-200 flex items-center gap-1 cursor-pointer"
              title="Markdown, Word, PowerPoint, PDF, HTML, 텍스트 파일을 문서 캔버스에서 엽니다"
            >
              <FileText className="w-3.5 h-3.5" />
              <span>문서 열기</span>
            </button>
          </div>
        </header>

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
                                  onApprove={(approve) =>
                                    handleToolApproval(msg.id, tr.id, approve)
                                  }
                                  approvalDisabled={isStreaming}
                                />
                              ))}
                            </div>
                          )}

                          {/* Main Markdown Response */}
                          {msg.content && (
                            <div
                              className="markdown-body text-sm text-stone-800 dark:text-stone-100 select-text"
                              dangerouslySetInnerHTML={{
                                __html: marked.parse(msg.content) as string,
                              }}
                            />
                          )}

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

                          {/* Assistant Action Bar (Copy, Regenerate) */}
                          {!isStreaming && (
                            <div className="pt-1 flex items-center gap-2 opacity-0 group-hover:opacity-100 transition">
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
                        onApprove={() => {}}
                        approvalDisabled
                      />
                    ))}

                    {streamingContent && (
                      <div
                        className="markdown-body text-sm text-stone-800 dark:text-stone-100 select-text"
                        dangerouslySetInnerHTML={{
                          __html: marked.parse(streamingContent) as string,
                        }}
                      />
                    )}
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
                    explorerApi
                      ? "메시지를 입력하세요... (@로 프로젝트 파일 첨부)"
                      : "메시지를 입력하세요..."
                  }
                  className="w-full text-sm text-stone-800 dark:text-stone-100 bg-transparent placeholder:text-stone-400 dark:placeholder:text-stone-500 resize-none focus:outline-none leading-relaxed"
                />

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
                  </div>

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
          </>
        )}
      </main>

      {/* 문서 캔버스: 형식(Markdown·Word·PowerPoint·PDF·HTML·텍스트)에 맞춘 편집기 */}
      {activeArtifact && (
        <aside
          className={`${
            canvasExpanded ? "w-[min(64vw,1180px)]" : "w-[520px]"
          } bg-white dark:bg-[#232321] border-l border-[#E2E0D8] dark:border-[#2E2E2A] flex flex-col shrink-0 shadow-lg transition-[width] duration-150 min-w-0`}
        >
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
          />
          </React.Suspense>
        </aside>
      )}

      {/* Modal 1: API & Model Settings */}
      {showSettingsModal && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-white dark:bg-[#262624] border border-stone-200 dark:border-[#3A3A36] rounded-2xl w-full max-w-md p-6 shadow-xl space-y-4">
            <div className="flex items-center justify-between">
              <h3 className="text-sm font-bold text-stone-900 dark:text-stone-100">
                API 설정
              </h3>
              <button onClick={() => setShowSettingsModal(false)}>
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
                  value={settings.apiKey}
                  onChange={(e) =>
                    setSettings({ ...settings, apiKey: e.target.value })
                  }
                  placeholder="발급받은 EXAONE API Key 입력"
                  className="w-full bg-white dark:bg-[#1E1E1C] border border-stone-300 dark:border-[#3A3A36] rounded-xl px-3 py-2 text-stone-900 dark:text-stone-100 focus:outline-none focus:border-[#A50034]"
                />
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

            <div className="flex justify-end gap-2 pt-2">
              <button
                onClick={() => setShowSettingsModal(false)}
                className="px-4 py-2 rounded-xl bg-stone-100 dark:bg-[#32322F] text-stone-700 dark:text-stone-200 text-xs font-semibold cursor-pointer"
              >
                취소
              </button>
              <button
                onClick={async () => {
                  await BackendAPI.saveSettings(settings);
                  setShowSettingsModal(false);
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

function ToolCallBox({
  record,
  onOpenFile,
  onUndo,
  onApprove,
  approvalDisabled = false,
}: {
  record: ToolExecutionRecord;
  onOpenFile: (rel: string) => void;
  onUndo: (snapId: string) => void;
  onApprove: (approve: boolean) => void;
  approvalDisabled?: boolean;
}) {
  const isPending = record.status === "pending_approval";
  const isRunning = record.status === "running";

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
        <div className="pt-1 flex items-center justify-between">
          <span className="text-amber-800 dark:text-amber-300">
            실행 승인이 필요합니다: <code>{record.argumentsJson}</code>
          </span>
          <div className="flex gap-1.5">
            <button
              onClick={() => onApprove(true)}
              disabled={approvalDisabled}
              className="px-2.5 py-1 rounded bg-emerald-600 text-white font-semibold cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
            >
              승인
            </button>
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
        <div className="text-stone-500 dark:text-stone-400 font-mono truncate">
          {record.output}
        </div>
      )}
    </div>
  );
}
