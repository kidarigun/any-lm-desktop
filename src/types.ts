export interface ModelPref {
  contextLength?: number;
  reasoning?: "on" | "off";
}

export interface AppSettings {
  /** LM Studio API 토큰(선택). 화면에서 새 토큰을 보낼 때만 쓴다. 저장된 토큰은 화면으로 돌아오지 않는다 (비어 있으면 기존 토큰 유지). */
  apiKey: string;
  /** 저장된 토큰이 있는지 */
  apiKeySet?: boolean;
  /** LM Studio 서버 주소 (localhost / 127.0.0.1 / [::1] 만 허용) */
  serverUrl: string;
  /** 기본 모델 key. 비어 있으면 아직 고르지 않은 것 */
  model: string;
  writingStyle:
    | "executive_report"
    | "technical_doc"
    | "creative_writing"
    | "code_architect"
    | "default";
  /** 컨텍스트가 차면 자동으로 요약 압축한다 */
  autoCompact: boolean;
  modelPrefs: Record<string, ModelPref>;
  /** 요청에 맞는 Skill을 앱이 골라 미리 넣는다. 없으면 켜짐 */
  skillAutoLoad?: boolean;
  /** 작업 패널 터미널의 셸: "powershell"(기본) | "pwsh" | "cmd" */
  terminalShell?: string;
}

export interface Project {
  id: string;
  name: string;
  rootPath: string;
  instructions: string;
  createdAt: string;
  /** 이 프로젝트에서 묻지 않고 실행하도록 허락한 도구 ("run_python" | "run_command") */
  autoApprove?: string[];
  /** 프로젝트 메모리 (오래된 항목이 앞) */
  memory?: MemoryItem[];
  /** 지식 폴더(절대 경로). 앱은 읽기만 한다. */
  knowledgeFolders?: string[];
  devServers?: import("./workpanel/devServerApi").DevServerConfig[];
}

/** 지식 폴더 하나의 색인 상태 */
export interface KnowledgeFolderStatus {
  path: string;
  files: number;
  chunks: number;
  skipped: number;
  missing: boolean;
}

export interface KnowledgeSkipped {
  path: string;
  reason: string;
}

/** 프로젝트 지식 폴더 색인 상태 (knowledge_status 명령, anylm-knowledge-status 이벤트) */
export interface KnowledgeStatus {
  indexing: boolean;
  /** 색인 중: 추출한 파일 수 / 추출할 파일 수 */
  done: number;
  total: number;
  files: number;
  chunks: number;
  skipped: number;
  /** 시간이 모자라 다음 갱신으로 미룬 파일 수 */
  deferred: number;
  skippedSamples: KnowledgeSkipped[];
  lastIndexed: string | null;
  folders: KnowledgeFolderStatus[];
  error: string | null;
  /** 한도에 닿아 나머지는 색인하지 않았는지 */
  overLimit: boolean;
  /** 그 한도가 훑기 항목(폴더·파일 200,000개) 상한이었는지(아니면 지원 파일 3,000개) */
  overEntryLimit: boolean;
}

export interface MemoryItem {
  id: string;
  text: string;
  createdAt: string;
  updatedAt: string;
  /** "model": remember 도구로 모델이 저장, "user": 사용자가 직접 */
  source: "model" | "user";
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
  /** run_python 실행으로 새로 생기거나 바뀐 파일 */
  producedFiles?: string[];
  /** 모델이 아니라 앱이 한 일(Skill 자동 불러오기)의 기록 */
  automatic?: boolean;
}

/** MCP 서버 설정 (환경 변수·헤더 값은 Windows 자격 증명 관리자에 따로 저장) */
export interface McpServerConfig {
  id: string;
  name: string;
  enabled: boolean;
  /** stdio = 내 PC에서 실행, http = 원격 URL */
  transport: "stdio" | "http";
  command: string;
  args: string[];
  cwd: string;
  url: string;
  envKeys: string[];
  headerKeys: string[];
  disabledTools: string[];
  autoApprove: string[];
  /** 찾아보기 목록에서 설치했으면 그 항목 ID */
  catalogId?: string;
  /** 원격 서버의 OAuth 로그인 (클라이언트 보안 비밀번호는 따로 저장) */
  oauth?: McpOAuthSettings | null;
}

export interface McpOAuthSettings {
  /** 비우면 서버에 자동 등록을 시도 */
  clientId: string;
  scopes: string[];
  /** 비우면 서버에서 찾는다 */
  authorizationEndpoint: string;
  tokenEndpoint: string;
  extraParams: Record<string, string>;
  /** 같은 묶음·같은 클라이언트 ID인 서버끼리 로그인을 함께 쓴다 */
  group: string;
}

export interface McpRemoteTool {
  name: string;
  description: string;
  inputSchema: unknown;
}

export interface McpServerState {
  status: "off" | "connecting" | "connected" | "error" | "needs_login";
  error?: string | null;
  serverInfo: string;
  tools: McpRemoteTool[];
  /** 로컬 서버가 남긴 마지막 출력 */
  log: string;
  /** 로그인(OAuth)을 쓰는 서버에서 로그인되어 있는지 */
  signedIn?: boolean;
  /** 브라우저 로그인을 기다리는 중인지 */
  signingIn?: boolean;
}

export interface McpServerView {
  config: McpServerConfig;
  state: McpServerState;
}

/** 앱이 설치·관리하는 실행 환경 */
export type RuntimeKind = "python" | "node" | "uv";

export interface RuntimeInfo {
  id: RuntimeKind;
  label: string;
  purpose: string;
  /** 앱이 설치한 것이 있는지 */
  appInstalled: boolean;
  /** PC에 따로 설치된 것이 있는지 */
  system: boolean;
  version: string;
  sizeHint: string;
  job: { running: boolean; message: string; percent?: number | null; error?: string | null };
}

/** 설치된 Skill (작업 지침 묶음) */
export interface SkillInfo {
  /** 켜고 끄기·삭제에 쓰는 ID (`user:<폴더>` 또는 `project:<프로젝트ID>:<경로>`) */
  id: string;
  name: string;
  description: string;
  /** user = 모든 대화, project = 이 프로젝트 */
  scope: "user" | "project";
  /** 표시용 위치 */
  location: string;
  enabled: boolean;
  /** SKILL.md에 문제가 있으면 그 내용 (모델에게 알려 주지 않음) */
  error?: string | null;
  /** 같은 이름의 프로젝트 Skill에 가려졌는지 */
  shadowed: boolean;
  /** SKILL.md 말고 함께 든 파일 */
  files: string[];
}

/** 이 프로젝트에서 쓸 Python */
export interface PythonInfo {
  path: string;
  version: string;
  inVenv: boolean;
  /** 프로젝트 폴더의 .venv / venv */
  projectVenv: boolean;
}

/** Python 스크립트 실행 결과 */
export interface PythonRunOutcome {
  script: string;
  python: PythonInfo;
  /** 시간 초과로 중단되면 null */
  exitCode: number | null;
  timedOut: boolean;
  durationMs: number;
  stdout: string;
  stderr: string;
  created: string[];
  modified: string[];
  missingModule: string | null;
  suggestedPackage: string | null;
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
  /** 이 답변을 만드는 데 든 호출·토큰·시간 */
  stats?: TurnStats | null;
  /** 이 답변을 만든 모델 key */
  modelKey?: string | null;
}

export interface TurnStats {
  /** 서버에 보낸 요청 수 (하루 호출 한도에서 빠지는 수) */
  apiCalls: number;
  promptTokens: number;
  completionTokens: number;
  reasoningTokens: number;
  elapsedMs: number;
}

export interface ContextCompaction {
  coveredMessages: number;
  createdAt: string;
  summary: string;
  focus?: string | null;
  sourceChars?: number | null;
}

export interface Conversation {
  id: string;
  projectId: string;
  title: string;
  messages: ChatMessage[];
  updatedAt: string;
  /** 마지막으로 메시지를 보낸 화면 */
  mode?: "chat" | "project" | "code" | null;
  /** 적용된 컨텍스트 압축 요약 */
  compaction?: ContextCompaction | null;
  /** 이 대화에서 쓸 모델 key. 없으면 기본 모델 */
  modelKey?: string | null;
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

export interface LoadedInstance {
  id: string;
  contextLength: number;
}

/** LM Studio에 설치된 LLM 한 개 */
export interface LmModel {
  key: string;
  displayName: string;
  params?: string | null;
  quantization?: string | null;
  sizeBytes: number;
  maxContextLength: number;
  loaded: LoadedInstance[];
  vision: boolean;
  toolUse: boolean;
  reasoningOptions: string[];
}

/** 요청 전에 모델을 쓸 수 있는지에 대한 판단 */
export type LoadPlan =
  | { kind: "ready"; instanceId: string; contextLength: number }
  | { kind: "smallerLoaded"; instanceId: string; loaded: number; wanted: number }
  | { kind: "notLoaded"; others: string[] }
  | { kind: "missing" };

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
