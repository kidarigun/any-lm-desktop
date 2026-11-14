import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import {
  AppDatabase,
  AppSettings,
  Conversation,
  FileNode,
  FileSnapshot,
  Project,
  SaveResult,
  SnapshotDiff,
  PythonInfo,
  PythonRunOutcome,
  McpServerConfig,
  McpServerView,
  RuntimeInfo,
  RuntimeKind,
  SkillInfo,
  StreamChunkPayload,
} from "./types";

export const isTauri = () =>
  typeof window !== "undefined" &&
  ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

const FALLBACK_STORAGE_KEY = "exaone_desktop_fallback_db_v1";

function loadFallbackDb(): AppDatabase {
  const raw = localStorage.getItem(FALLBACK_STORAGE_KEY);
  if (raw) {
    try {
      return JSON.parse(raw);
    } catch {
      // ignore
    }
  }
  const initial: AppDatabase = {
    settings: {
      apiKey: "",
      baseUrl: "https://api.exaone.ai/v1/chat/completions",
      model: "k-exaone_v2.5",
      reasoningEffort: "high",
      writingStyle: "executive_report",
    },
    projects: [
      {
        id: "proj-default",
        name: "EXAONE 기본 워크스페이스",
        rootPath: "c:\\DOCS\\EXAONE",
        instructions:
          "당신은 LG AI연구원의 K-EXAONE 2.5 모델을 기반으로 동작하는 한국어 글쓰기·보고서·문서 작성 및 로컬 코딩 에이전트입니다.",
        createdAt: new Date().toISOString(),
      },
    ],
    conversations: [],
    snapshots: [],
  };
  localStorage.setItem(FALLBACK_STORAGE_KEY, JSON.stringify(initial));
  return initial;
}

export const BackendAPI = {
  async getInitialState(): Promise<AppDatabase> {
    if (isTauri()) {
      return await invoke<AppDatabase>("get_initial_state");
    }
    return loadFallbackDb();
  },

  /** 저장된 API 키를 Windows 자격 증명 관리자에서 지운다. */
  async clearApiKey(): Promise<AppSettings> {
    if (isTauri()) {
      return await invoke<AppSettings>("clear_api_key");
    }
    const db = loadFallbackDb();
    db.settings = { ...db.settings, apiKey: "", apiKeySet: false };
    localStorage.setItem(FALLBACK_STORAGE_KEY, JSON.stringify(db));
    return db.settings;
  },

  /** http(s)·mailto 링크를 기본 브라우저로 연다. */
  async openExternalUrl(url: string): Promise<void> {
    if (isTauri()) {
      await invoke("open_external_url", { url });
      return;
    }
    window.open(url, "_blank", "noopener,noreferrer");
  },

  async saveSettings(settings: AppSettings): Promise<AppSettings> {
    if (isTauri()) {
      return await invoke<AppSettings>("save_settings", { settings });
    }
    const db = loadFallbackDb();
    // 브라우저 미리보기 모드: 키를 입력하지 않았으면 기존 키를 유지한다.
    const apiKey = settings.apiKey?.trim() || db.settings.apiKey || "";
    db.settings = { ...settings, apiKey, apiKeySet: !!apiKey };
    localStorage.setItem(FALLBACK_STORAGE_KEY, JSON.stringify(db));
    return { ...db.settings, apiKey: "" };
  },

  async pickFolderDialog(initialDir?: string): Promise<string | null> {
    if (isTauri()) {
      return await invoke<string | null>("pick_folder_dialog", {
        initialDir: initialDir ?? null,
      });
    }
    return null;
  },

  async pickFilesDialog(initialDir?: string): Promise<string[]> {
    if (isTauri()) {
      return await invoke<string[]>("pick_files_dialog", {
        initialDir: initialDir ?? null,
      });
    }
    return [];
  },

  /** 한 대화의 답변 생성을 멈춘다 (지정하지 않으면 모두). */
  async cancelGeneration(conversationId?: string): Promise<void> {
    if (isTauri()) {
      await invoke("cancel_generation", { conversationId: conversationId ?? null });
    }
  },

  /** 백엔드에서 지금 답변을 만들고 있는 대화 ID들 */
  async runningConversations(): Promise<string[]> {
    if (isTauri()) {
      return await invoke<string[]>("running_conversations");
    }
    return [];
  },

  async createOrUpdateProject(params: {
    id?: string;
    name: string;
    rootPath: string;
    instructions: string;
  }): Promise<Project> {
    if (isTauri()) {
      return await invoke<Project>("create_or_update_project", {
        id: params.id ?? null,
        name: params.name,
        rootPath: params.rootPath,
        instructions: params.instructions,
      });
    }
    const db = loadFallbackDb();
    if (params.id) {
      const idx = db.projects.findIndex((p) => p.id === params.id);
      if (idx >= 0) {
        db.projects[idx] = { ...db.projects[idx], ...params };
        localStorage.setItem(FALLBACK_STORAGE_KEY, JSON.stringify(db));
        return db.projects[idx];
      }
    }
    const proj: Project = {
      id: "proj-" + Date.now(),
      name: params.name,
      rootPath: params.rootPath,
      instructions: params.instructions,
      createdAt: new Date().toISOString(),
    };
    db.projects.push(proj);
    localStorage.setItem(FALLBACK_STORAGE_KEY, JSON.stringify(db));
    return proj;
  },

  async deleteProject(projectId: string): Promise<void> {
    if (isTauri()) {
      await invoke("delete_project", { projectId });
      return;
    }
    const db = loadFallbackDb();
    db.projects = db.projects.filter((p) => p.id !== projectId);
    localStorage.setItem(FALLBACK_STORAGE_KEY, JSON.stringify(db));
  },

  async listProjectFiles(projectId: string): Promise<FileNode[]> {
    if (isTauri()) {
      return await invoke<FileNode[]>("list_project_files", { projectId });
    }
    return [
      {
        name: "docs",
        relativePath: "docs",
        isDir: true,
        size: 0,
        children: [
          {
            name: "2026_AI_도입_기획보고서.md",
            relativePath: "docs/2026_AI_도입_기획보고서.md",
            isDir: false,
            size: 1420,
          },
        ],
      },
      {
        name: "README.md",
        relativePath: "README.md",
        isDir: false,
        size: 512,
      },
    ];
  },

  async readProjectFile(
    projectId: string,
    relativePath: string
  ): Promise<string> {
    if (isTauri()) {
      return await invoke<string>("read_project_file", {
        projectId,
        relativePath,
      });
    }
    return `# ${relativePath}\n\nTauri 데스크톱 모드에서 실제 로컬 파일을 읽고 수정합니다.`;
  },

  async writeProjectFile(
    projectId: string,
    relativePath: string,
    content: string
  ): Promise<FileSnapshot> {
    if (isTauri()) {
      return await invoke<FileSnapshot>("write_project_file", {
        projectId,
        relativePath,
        content,
      });
    }
    return {
      id: "snap-" + Date.now(),
      projectId,
      relativePath,
      existedBefore: true,
      previousContent: "",
      newContent: content,
      action: "write",
      createdAt: new Date().toISOString(),
    };
  },

  async modifyProjectFile(
    projectId: string,
    relativePath: string,
    target: string,
    replacement: string
  ): Promise<FileSnapshot> {
    if (isTauri()) {
      return await invoke<FileSnapshot>("modify_project_file", {
        projectId,
        relativePath,
        target,
        replacement,
      });
    }
    throw new Error("Tauri 환경에서만 지원됩니다.");
  },

  async createProjectFolder(
    projectId: string,
    relativePath: string
  ): Promise<string> {
    if (isTauri()) {
      return await invoke<string>("create_project_folder", {
        projectId,
        relativePath,
      });
    }
    return `폴더 생성됨: ${relativePath}`;
  },

  async deleteProjectPath(
    projectId: string,
    relativePath: string
  ): Promise<FileSnapshot> {
    if (isTauri()) {
      return await invoke<FileSnapshot>("delete_project_path", {
        projectId,
        relativePath,
      });
    }
    return {
      id: "snap-" + Date.now(),
      projectId,
      relativePath,
      existedBefore: true,
      previousContent: "",
      newContent: "",
      action: "delete",
      createdAt: new Date().toISOString(),
    };
  },

  async readProjectFileBase64(projectId: string, relativePath: string): Promise<string> {
    if (isTauri()) {
      return await invoke<string>("read_project_file_base64", { projectId, relativePath });
    }
    throw new Error("Tauri 데스크톱 창에서만 문서 파일을 열 수 있습니다.");
  },

  async listProjectDir(projectId: string, relativePath: string): Promise<FileNode[]> {
    if (isTauri()) {
      return await invoke<FileNode[]>("list_project_dir", { projectId, relativePath });
    }
    return [];
  },

  async createProjectFile(projectId: string, relativePath: string): Promise<string> {
    return await invoke<string>("create_project_file", { projectId, relativePath });
  },

  async renameProjectPath(projectId: string, from: string, to: string): Promise<string> {
    return await invoke<string>("rename_project_path", { projectId, from, to });
  },

  async trashProjectPath(projectId: string, relativePath: string): Promise<void> {
    await invoke("trash_project_path", { projectId, relativePath });
  },

  async findProjectFiles(projectId: string, query: string): Promise<string[]> {
    if (isTauri()) {
      return await invoke<string[]>("find_project_files", { projectId, query });
    }
    return [];
  },

  async revealProjectPath(projectId: string, relativePath: string): Promise<void> {
    await invoke("reveal_project_path", { projectId, relativePath });
  },

  async readAbsoluteFileBase64(path: string): Promise<string> {
    if (isTauri()) {
      return await invoke<string>("read_absolute_file_base64", { path });
    }
    throw new Error("Tauri 데스크톱 창에서만 파일을 열 수 있습니다.");
  },

  async writeProjectFileBase64(
    projectId: string,
    relativePath: string,
    dataBase64: string
  ): Promise<FileSnapshot> {
    if (isTauri()) {
      return await invoke<FileSnapshot>("write_project_file_base64", {
        projectId,
        relativePath,
        dataBase64,
      });
    }
    throw new Error("Tauri 데스크톱 창에서만 저장할 수 있습니다.");
  },

  async pickSavePath(initialDir: string | undefined, defaultName: string, filter: string): Promise<string | null> {
    if (isTauri()) {
      return await invoke<string | null>("pick_save_path_dialog", {
        initialDir: initialDir ?? null,
        defaultName,
        filter,
      });
    }
    return null;
  },

  async saveFileToPath(projectId: string | null, absolutePath: string, dataBase64: string): Promise<SaveResult> {
    if (isTauri()) {
      return await invoke<SaveResult>("save_file_to_path", { projectId, absolutePath, dataBase64 });
    }
    throw new Error("Tauri 데스크톱 창에서만 저장할 수 있습니다.");
  },

  /** 저장 창에서 고른 파일을 탐색기에서 보여 준다. */
  async revealSavedPath(path: string): Promise<void> {
    if (isTauri()) {
      await invoke("reveal_saved_path", { path });
    }
  },

  /** 이 프로젝트에서 쓸 Python (없으면 null) */
  async pythonStatus(projectId: string): Promise<PythonInfo | null> {
    if (isTauri()) {
      return await invoke<PythonInfo | null>("python_status", { projectId });
    }
    return null;
  },

  /** 프로젝트 안의 .py 스크립트를 프로젝트 루트에서 실행한다. */
  async runPythonScript(projectId: string, relativePath: string): Promise<PythonRunOutcome> {
    if (isTauri()) {
      return await invoke<PythonRunOutcome>("run_python_script", { projectId, relativePath });
    }
    throw new Error("Tauri 데스크톱 창에서만 Python을 실행할 수 있습니다.");
  },

  async installPythonPackage(projectId: string, pkg: string): Promise<string> {
    if (isTauri()) {
      return await invoke<string>("install_python_package", { projectId, package: pkg });
    }
    throw new Error("Tauri 데스크톱 창에서만 패키지를 설치할 수 있습니다.");
  },

  // ---- 실행 환경 (Python·Node.js·uv) ----

  async runtimeStatus(): Promise<RuntimeInfo[]> {
    if (isTauri()) return await invoke<RuntimeInfo[]>("runtime_status");
    return [];
  },

  /** 공식 배포본을 내려받아 앱 전용으로 설치한다. 끝날 때까지 기다린다. */
  async runtimeInstall(kind: RuntimeKind): Promise<void> {
    if (isTauri()) return await invoke("runtime_install", { kind });
    throw new Error("Tauri 데스크톱 창에서만 설치할 수 있습니다.");
  },

  async runtimeRemove(kind: RuntimeKind): Promise<void> {
    if (isTauri()) await invoke("runtime_remove", { kind });
  },

  // ---- 찾아보기 목록 ----

  /** 목록을 불러온다 (주소가 없으면 기본 목록). 인터넷이 안 되면 마지막 저장본. */
  async catalogLoad(url?: string): Promise<{ url: string; catalog: unknown; fromCache: boolean; error?: string | null }> {
    if (isTauri()) return await invoke("catalog_load", { url: url ?? null });
    throw new Error("Tauri 데스크톱 창에서만 목록을 불러올 수 있습니다.");
  },

  async catalogDefaultUrl(): Promise<string> {
    if (isTauri()) return await invoke<string>("catalog_default_url");
    return "https://raw.githubusercontent.com/kidarigun/exaone-catalog/main/catalog.json";
  },

  /** 목록의 Skill zip을 받아 sha256 확인 후 설치. 같은 Skill이 있으면 ALREADY_EXISTS:<이름> 오류. */
  async catalogInstallSkill(url: string, sha256: string, overwrite = false): Promise<SkillInfo> {
    return await invoke<SkillInfo>("catalog_install_skill", { url, sha256, overwrite });
  },

  async catalogRuntimes(): Promise<{ node: boolean; uv: boolean; python: boolean; docker: boolean } | null> {
    if (isTauri()) return await invoke("catalog_runtimes");
    return null;
  },

  // ---- MCP ----

  async mcpListServers(): Promise<McpServerView[]> {
    if (isTauri()) return await invoke<McpServerView[]>("mcp_list_servers");
    return [];
  },

  /** 서버 추가·수정. env·headers의 빈 값은 저장된 값을 유지한다. */
  async mcpSaveServer(
    config: McpServerConfig,
    env: Record<string, string>,
    headers: Record<string, string>,
    /** OAuth 클라이언트 보안 비밀번호 (비우면 저장된 값 유지) */
    oauthClientSecret?: string
  ): Promise<McpServerConfig> {
    if (isTauri())
      return await invoke<McpServerConfig>("mcp_save_server", { config, env, headers, oauthClientSecret: oauthClientSecret || null });
    throw new Error("Tauri 데스크톱 창에서만 MCP 서버를 추가할 수 있습니다.");
  },

  /** 브라우저로 로그인한다. 로그인이 끝나거나 실패할 때까지(최대 5분) 기다린다. */
  async mcpOauthLogin(id: string): Promise<void> {
    if (isTauri()) return await invoke("mcp_oauth_login", { id });
    throw new Error("Tauri 데스크톱 창에서만 로그인할 수 있습니다.");
  },

  async mcpOauthCancel(): Promise<void> {
    if (isTauri()) await invoke("mcp_oauth_cancel");
  },

  async mcpOauthLogout(id: string): Promise<void> {
    if (isTauri()) await invoke("mcp_oauth_logout", { id });
  },

  async mcpDeleteServer(id: string): Promise<void> {
    if (isTauri()) await invoke("mcp_delete_server", { id });
  },

  async mcpSetServerEnabled(id: string, enabled: boolean): Promise<void> {
    if (isTauri()) await invoke("mcp_set_server_enabled", { id, enabled });
  },

  async mcpRestartServer(id: string): Promise<void> {
    if (isTauri()) await invoke("mcp_restart_server", { id });
  },

  async mcpSetTool(id: string, tool: string, opts: { enabled?: boolean; autoApprove?: boolean }): Promise<void> {
    if (isTauri())
      await invoke("mcp_set_tool", { id, tool, enabled: opts.enabled ?? null, autoApprove: opts.autoApprove ?? null });
  },

  // ---- Skill ----

  /** 설치된 Skill 목록 (프로젝트를 주면 그 프로젝트의 Skill도) */
  async listSkills(projectId?: string): Promise<SkillInfo[]> {
    if (isTauri()) {
      return await invoke<SkillInfo[]>("list_skills", { projectId: projectId ?? null });
    }
    return [];
  },

  async setSkillEnabled(id: string, enabled: boolean): Promise<void> {
    if (isTauri()) await invoke("set_skill_enabled", { id, enabled });
  },

  /** 설치할 Skill 폴더 또는 .zip/.skill 파일을 고른다 (취소하면 null) */
  async pickSkillSource(kind: "folder" | "zip"): Promise<string | null> {
    if (isTauri()) return await invoke<string | null>("pick_skill_source", { kind });
    throw new Error("Tauri 데스크톱 창에서만 Skill을 설치할 수 있습니다.");
  },

  /** 같은 Skill이 있으면 `ALREADY_EXISTS:<이름>` 오류를 낸다. */
  async installSkill(sourcePath: string, overwrite = false): Promise<SkillInfo> {
    return await invoke<SkillInfo>("install_skill", { sourcePath, overwrite });
  },

  async deleteSkill(id: string): Promise<void> {
    await invoke("delete_skill", { id });
  },

  async createSkill(name: string): Promise<SkillInfo> {
    if (isTauri()) return await invoke<SkillInfo>("create_skill", { name });
    throw new Error("Tauri 데스크톱 창에서만 Skill을 만들 수 있습니다.");
  },

  async skillText(id: string, projectId?: string): Promise<string> {
    if (isTauri()) return await invoke<string>("skill_text", { id, projectId: projectId ?? null });
    return "";
  },

  /** Skill 폴더를 탐색기로(folder), SKILL.md를 메모장으로(edit) 연다. id가 없으면 Skill 폴더 전체를 연다. */
  async openSkillLocation(target: "folder" | "edit", id?: string, projectId?: string): Promise<void> {
    if (isTauri()) {
      await invoke("open_skill_location", { id: id ?? null, projectId: projectId ?? null, target });
      return;
    }
    throw new Error("Tauri 데스크톱 창에서만 폴더를 열 수 있습니다.");
  },

  /** 엑셀·이미지 등 캔버스로 열 수 없는 파일을 Windows 기본 앱으로 연다. */
  async openProjectFileExternal(projectId: string, relativePath: string): Promise<void> {
    if (isTauri()) {
      await invoke("open_project_file_external", { projectId, relativePath });
      return;
    }
    throw new Error("Tauri 데스크톱 창에서만 기본 앱으로 열 수 있습니다.");
  },

  /** 변경 보기용: 스냅샷의 이전·이후 내용 (docx·pptx는 Markdown으로 변환됨) */
  async getSnapshotDiff(snapshotId: string): Promise<SnapshotDiff> {
    if (isTauri()) {
      return await invoke<SnapshotDiff>("get_snapshot_diff", { snapshotId });
    }
    // 브라우저 미리보기 모드: 저장된 텍스트 스냅샷으로 만든다.
    const db = loadFallbackDb();
    const idx = db.snapshots.findIndex((s) => s.id === snapshotId);
    if (idx < 0) throw new Error("변경 기록을 찾을 수 없습니다.");
    const s = db.snapshots[idx];
    const superseded = db.snapshots
      .slice(idx + 1)
      .some((n) => n.projectId === s.projectId && n.relativePath === s.relativePath);
    return {
      snapshotId: s.id,
      relativePath: s.relativePath,
      action: s.action,
      createdAt: s.createdAt,
      before: s.existedBefore ? s.previousContent : null,
      after: s.action === "delete" ? null : s.newContent,
      beforeExists: s.existedBefore,
      afterExists: s.action !== "delete",
      kind: "text",
      notes: [],
      superseded,
      source: s.source ?? null,
    };
  },

  async undoFileSnapshot(snapshotId: string): Promise<string> {
    if (isTauri()) {
      return await invoke<string>("undo_file_snapshot", { snapshotId });
    }
    return "스냅샷 복원 완료";
  },

  async createConversation(
    projectId: string,
    title: string
  ): Promise<Conversation> {
    if (isTauri()) {
      return await invoke<Conversation>("create_conversation", {
        projectId,
        title,
      });
    }
    const db = loadFallbackDb();
    const conv: Conversation = {
      id: "conv-" + Date.now(),
      projectId,
      title,
      messages: [],
      updatedAt: new Date().toISOString(),
    };
    db.conversations.unshift(conv);
    localStorage.setItem(FALLBACK_STORAGE_KEY, JSON.stringify(db));
    return conv;
  },

  async deleteConversation(conversationId: string): Promise<void> {
    if (isTauri()) {
      await invoke("delete_conversation", { conversationId });
      return;
    }
    const db = loadFallbackDb();
    db.conversations = db.conversations.filter((c) => c.id !== conversationId);
    localStorage.setItem(FALLBACK_STORAGE_KEY, JSON.stringify(db));
  },

  async sendChatMessage(
    conversationId: string,
    userContent: string,
    attachedFiles: string[]
  ): Promise<Conversation> {
    if (isTauri()) {
      return await invoke<Conversation>("send_chat_message", {
        conversationId,
        userContent,
        attachedFiles,
      });
    }
    throw new Error("Tauri 데스크톱 창에서 실행해 주세요.");
  },

  async editAndResendMessage(
    conversationId: string,
    messageId: string,
    newContent: string
  ): Promise<Conversation> {
    if (isTauri()) {
      return await invoke<Conversation>("edit_and_resend_message", {
        conversationId,
        messageId,
        newContent,
      });
    }
    throw new Error("Tauri 데스크톱 창에서 실행해 주세요.");
  },

  async resolveToolApproval(
    conversationId: string,
    messageId: string,
    toolId: string,
    approve: boolean,
    /** MCP 도구: 앞으로 이 도구는 승인 없이 실행 */
    always = false
  ): Promise<Conversation> {
    if (isTauri()) {
      return await invoke<Conversation>("resolve_tool_approval", {
        conversationId,
        messageId,
        toolId,
        approve,
        always,
      });
    }
    throw new Error("Tauri 환경에서만 지원됩니다.");
  },

  async runManualTerminalCommand(
    projectId: string,
    command: string
  ): Promise<string> {
    if (isTauri()) {
      return await invoke<string>("run_manual_terminal_command", {
        projectId,
        command,
      });
    }
    return "$ " + command + "\n(Tauri 네이티브 모드에서 실행됩니다.)";
  },

  async onExaoneStream(
    handler: (payload: StreamChunkPayload) => void
  ): Promise<UnlistenFn> {
    if (isTauri()) {
      return await listen<StreamChunkPayload>("exaone-stream", (event) => {
        handler(event.payload);
      });
    }
    return () => {};
  },
};
