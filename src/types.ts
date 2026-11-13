export interface AppSettings {
  apiKey: string;
  baseUrl: string;
  model: string;
  reasoningEffort: "low" | "high" | "max";
  writingStyle:
    | "executive_report"
    | "technical_doc"
    | "creative_writing"
    | "code_architect"
    | "default";
}

export interface Project {
  id: string;
  name: string;
  rootPath: string;
  instructions: string;
  createdAt: string;
}

export interface ToolExecutionRecord {
  id: string;
  toolName: string;
  argumentsJson: string;
  status: "completed" | "pending_approval" | "running" | "rejected" | "error";
  output: string;
  filePath?: string | null;
  snapshotId?: string | null;
  /** 에이전트 루프에서 호출된 라운드 번호 */
  round?: number;
}

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system" | "tool";
  content: string;
  reasoningContent?: string | null;
  toolRecords: ToolExecutionRecord[];
  createdAt: string;
  /** 라운드별 어시스턴트 텍스트 (content는 이를 합친 표시용 문자열) */
  roundContents?: string[];
}

export interface Conversation {
  id: string;
  projectId: string;
  title: string;
  messages: ChatMessage[];
  updatedAt: string;
}

export interface FileSnapshot {
  id: string;
  projectId: string;
  relativePath: string;
  existedBefore: boolean;
  previousContent: string;
  newContent: string;
  action: "write" | "modify" | "delete";
  createdAt: string;
  /** 바이너리 파일의 이전 내용 백업 이름 */
  backupBlob?: string | null;
  isBinary?: boolean;
}

export interface SaveResult {
  absolutePath: string;
  relativePath?: string | null;
  snapshot?: FileSnapshot | null;
}

export interface FileNode {
  name: string;
  relativePath: string;
  isDir: boolean;
  size: number;
  children?: FileNode[] | null;
}

export interface AppDatabase {
  settings: AppSettings;
  projects: Project[];
  conversations: Conversation[];
  snapshots: FileSnapshot[];
}

export interface StreamChunkPayload {
  conversationId: string;
  messageId: string;
  deltaReasoning?: string | null;
  deltaContent?: string | null;
  toolRecord?: ToolExecutionRecord | null;
  snapshot?: FileSnapshot | null;
  statusNotice?: string | null;
  done: boolean;
  error?: string | null;
}
