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
  StreamChunkPayload,
} from "./types";

const isTauri = () =>
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

  async saveSettings(settings: AppSettings): Promise<AppSettings> {
    if (isTauri()) {
      return await invoke<AppSettings>("save_settings", { settings });
    }
    const db = loadFallbackDb();
    db.settings = settings;
    localStorage.setItem(FALLBACK_STORAGE_KEY, JSON.stringify(db));
    return settings;
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

  async cancelGeneration(): Promise<void> {
    if (isTauri()) {
      await invoke("cancel_generation");
    }
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
    approve: boolean
  ): Promise<Conversation> {
    if (isTauri()) {
      return await invoke<Conversation>("resolve_tool_approval", {
        conversationId,
        messageId,
        toolId,
        approve,
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
