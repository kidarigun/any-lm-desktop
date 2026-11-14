export interface AppSettings {
  /** 화면에서 새 키를 보낼 때만 쓴다. 저장된 키는 화면으로 돌아오지 않는다 (비어 있으면 기존 키 유지). */
  apiKey: string;
  /** 저장된 키가 있는지 */
  apiKeySet?: boolean;
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
  /** 바이너리 파일의 바뀐 뒤 내용 백업 이름 */
  afterBlob?: string | null;
  /** 누가 바꿨는지: agent(AI 도구) | user(캔버스 저장 등). 예전 기록에는 없다. */
  source?: "agent" | "user" | null;
}

/** 변경 보기: 스냅샷 하나의 이전·이후 내용 */
export interface SnapshotDiff {
  snapshotId: string;
  relativePath: string;
  action: "write" | "modify" | "delete" | string;
  createdAt: string;
  /** null: 파일이 없었음(beforeExists=false) 또는 텍스트로 볼 수 없음 */
  before: string | null;
  after: string | null;
  beforeExists: boolean;
  afterExists: boolean;
  /** text | document(문서를 Markdown으로 바꿔 비교) | binary(비교 불가) */
  kind: "text" | "document" | "binary";
  notes: string[];
  /** 이후에 같은 파일을 또 바꾼 기록이 있음 */
  superseded: boolean;
  source?: "agent" | "user" | null;
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
