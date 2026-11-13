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
  status: "completed" | "pending_approval" | "rejected" | "error";
  output: string;
  filePath?: string | null;
  snapshotId?: string | null;
}

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system" | "tool";
  content: string;
  reasoningContent?: string | null;
  toolRecords: ToolExecutionRecord[];
  createdAt: string;
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
