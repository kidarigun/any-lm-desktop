import type { McpServerConfig } from "../types";

/** 붙여 넣은 설정에서 읽은 서버 하나 (비밀 값은 따로) */
export interface ImportedServer {
  config: McpServerConfig;
  env: Record<string, string>;
  headers: Record<string, string>;
}

export function emptyConfig(): McpServerConfig {
  return {
    id: "",
    name: "",
    enabled: true,
    transport: "stdio",
    command: "",
    args: [],
    cwd: "",
    url: "",
    envKeys: [],
    headerKeys: [],
    disabledTools: [],
    autoApprove: [],
    catalogId: "",
    oauth: null,
  };
}

type Json = Record<string, unknown>;

const isObject = (v: unknown): v is Json => !!v && typeof v === "object" && !Array.isArray(v);

function stringMap(v: unknown): Record<string, string> {
  if (!isObject(v)) return {};
  const out: Record<string, string> = {};
  for (const [k, val] of Object.entries(v)) {
    if (val === null || val === undefined) continue;
    out[k] = typeof val === "string" ? val : String(val);
  }
  return out;
}

function looksLikeServer(v: unknown): boolean {
  return isObject(v) && (typeof v.command === "string" || typeof v.url === "string" || typeof v.serverUrl === "string");
}

function toServer(name: string, raw: Json): ImportedServer {
  const config = emptyConfig();
  config.name = name;
  config.enabled = raw.disabled !== true;
  const url = typeof raw.url === "string" ? raw.url : typeof raw.serverUrl === "string" ? raw.serverUrl : "";
  const env = stringMap(raw.env);
  const headers = stringMap(raw.headers);
  if (url && typeof raw.command !== "string") {
    config.transport = "http";
    config.url = url.trim();
  } else {
    config.transport = "stdio";
    config.command = String(raw.command ?? "").trim();
    config.args = Array.isArray(raw.args) ? raw.args.map((a) => String(a)) : [];
    config.cwd = typeof raw.cwd === "string" ? raw.cwd : "";
  }
  config.envKeys = config.transport === "stdio" ? Object.keys(env) : [];
  config.headerKeys = config.transport === "http" ? Object.keys(headers) : [];
  return {
    config,
    env: config.transport === "stdio" ? env : {},
    headers: config.transport === "http" ? headers : {},
  };
}

/**
 * Claude Desktop(`mcpServers`), VS Code(`servers`) 형식의 MCP 설정 JSON을 읽는다.
 * 서버 이름 → 설정 묶음이나, 서버 설정 하나만 붙여 넣어도 된다.
 */
export function parseMcpJson(text: string): ImportedServer[] {
  let data: unknown;
  try {
    // 붙여 넣을 때 따라온 앞뒤 쉼표·중괄호 없는 조각도 받아 준다.
    const t = text.trim().replace(/,\s*$/, "");
    data = JSON.parse(t.startsWith("{") ? t : `{${t}}`);
  } catch {
    throw new Error("JSON 형식이 올바르지 않습니다. 설정 파일 내용을 그대로 붙여 넣으세요.");
  }
  if (!isObject(data)) throw new Error("MCP 서버 설정을 찾지 못했습니다.");

  if (looksLikeServer(data)) return [toServer("MCP 서버", data as Json)];
  const map = isObject(data.mcpServers) ? data.mcpServers : isObject(data.servers) ? data.servers : data;
  const servers = Object.entries(map)
    .filter(([, v]) => looksLikeServer(v))
    .map(([name, v]) => toServer(name, v as Json));
  if (servers.length === 0) {
    throw new Error("MCP 서버 설정을 찾지 못했습니다. \"mcpServers\" 안에 command 또는 url이 있는 항목이 있어야 합니다.");
  }
  return servers;
}

/** 명령줄 인자 한 줄을 나눈다. 따옴표로 감싼 부분은 하나로 본다. */
export function splitArgs(line: string): string[] {
  const out: string[] = [];
  let cur = "";
  let quote: string | null = null;
  let has = false;
  for (const ch of line) {
    if (quote) {
      if (ch === quote) quote = null;
      else cur += ch;
    } else if (ch === '"' || ch === "'") {
      quote = ch;
      has = true;
    } else if (/\s/.test(ch)) {
      if (has || cur) out.push(cur);
      cur = "";
      has = false;
    } else {
      cur += ch;
      has = true;
    }
  }
  if (has || cur) out.push(cur);
  return out;
}

/** 인자 목록을 한 줄로 (공백이 있으면 따옴표로 감싼다) */
export function joinArgs(args: string[]): string {
  return args.map((a) => (a === "" || /\s|"/.test(a) ? (a.includes('"') ? `'${a}'` : `"${a}"`) : a)).join(" ");
}
