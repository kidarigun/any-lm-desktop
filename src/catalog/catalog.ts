import type { McpOAuthSettings, McpServerConfig } from "../types";
import { emptyConfig } from "../mcp/importConfig";

/** 실행에 필요한 프로그램 */
export type Runtime = "node" | "uv" | "python" | "docker";

export const RUNTIME_INFO: Record<Runtime, { label: string; url: string }> = {
  node: { label: "Node.js", url: "https://nodejs.org/ko/download" },
  uv: { label: "uv", url: "https://docs.astral.sh/uv/getting-started/installation/" },
  python: { label: "Python", url: "https://www.python.org/downloads/windows/" },
  docker: { label: "Docker Desktop", url: "https://www.docker.com/products/docker-desktop/" },
};

export type Runtimes = Record<Runtime, boolean>;

export interface CatalogInput {
  key: string;
  label: string;
  type: "text" | "secret" | "folder";
  required: boolean;
  placeholder?: string;
  help?: string;
  helpUrl?: string;
}

interface CatalogBase {
  id: string;
  name: string;
  summary: string;
  description?: string;
  category: string;
  publisher?: string;
  homepage?: string;
  requires: Runtime[];
  /** 라이선스 (예: Apache-2.0) */
  license?: string;
  /** 이 앱에서 쓸 때 알아 둘 점 */
  note?: string;
  /** 처음 설정 방법을 설명하는 페이지 */
  guide?: string;
}

export interface CatalogMcp extends CatalogBase {
  install: {
    transport: "stdio" | "http";
    command?: string;
    args?: string[];
    env?: Record<string, string>;
    url?: string;
    headers?: Record<string, string>;
    /** 원격 서버 로그인. clientId·clientSecret에 {key}를 쓸 수 있다. */
    oauth?: {
      clientId: string;
      clientSecret: string;
      scopes: string[];
      authorizationEndpoint: string;
      tokenEndpoint: string;
      extraParams: Record<string, string>;
      group: string;
    };
  };
  inputs: CatalogInput[];
  safeTools: string[];
}

export interface CatalogSkill extends CatalogBase {
  /** 절대 주소로 바꾼 zip 주소 */
  download: string;
  sha256: string;
  size?: number;
  files: string[];
  /** 파일이 많으면 전체 개수 (files에는 앞부분만) */
  fileCount?: number;
  /** SKILL.md의 name (설치 여부 확인용) */
  skillName: string;
}

export interface Catalog {
  name: string;
  homepage?: string;
  updated?: string;
  mcp: CatalogMcp[];
  skills: CatalogSkill[];
}

type Json = Record<string, unknown>;
const isObj = (v: unknown): v is Json => !!v && typeof v === "object" && !Array.isArray(v);
const str = (v: unknown) => (typeof v === "string" ? v.trim() : "");
const strList = (v: unknown) => (Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : []);
const strMap = (v: unknown): Record<string, string> =>
  isObj(v) ? Object.fromEntries(Object.entries(v).filter(([, x]) => typeof x === "string")) as Record<string, string> : {};
const RUNTIMES: Runtime[] = ["node", "uv", "python", "docker"];
const runtimeList = (v: unknown) => strList(v).filter((r): r is Runtime => RUNTIMES.includes(r as Runtime));
const ID = /^[a-z0-9][a-z0-9._-]{0,63}$/;

/** 목록 안의 http(s) 링크만 남긴다. */
function link(v: unknown): string | undefined {
  const s = str(v);
  return /^https?:\/\//i.test(s) ? s : undefined;
}

function base(raw: Json): CatalogBase | null {
  const id = str(raw.id);
  const name = str(raw.name);
  if (!ID.test(id) || !name) return null;
  return {
    id,
    name,
    summary: str(raw.summary),
    description: str(raw.description) || undefined,
    category: str(raw.category) || "기타",
    publisher: str(raw.publisher) || undefined,
    homepage: link(raw.homepage),
    requires: runtimeList(raw.requires),
    license: str(raw.license) || undefined,
    note: str(raw.note) || undefined,
    guide: link(raw.guide),
  };
}

function inputs(v: unknown): CatalogInput[] {
  if (!Array.isArray(v)) return [];
  return v.filter(isObj).flatMap((i) => {
    const key = str(i.key);
    if (!/^[A-Za-z0-9_]+$/.test(key)) return [];
    const type = i.type === "secret" || i.type === "folder" ? i.type : "text";
    return [{
      key,
      label: str(i.label) || key,
      type,
      required: i.required !== false,
      placeholder: str(i.placeholder) || undefined,
      help: str(i.help) || undefined,
      helpUrl: link(i.helpUrl),
    } as CatalogInput];
  });
}

/** 목록 JSON을 검사해 쓸 수 있는 항목만 남긴다. 상대 주소는 목록 주소 기준으로 바꾼다. */
export function normalizeCatalog(raw: unknown, catalogUrl: string): Catalog {
  const data = isObj(raw) ? raw : {};
  const seen = new Set<string>();
  const unique = <T extends { id: string }>(x: T | null): x is T => !!x && !seen.has(x.id) && !!seen.add(x.id);

  const mcp = (Array.isArray(data.mcp) ? data.mcp : []).filter(isObj).map((m): CatalogMcp | null => {
    const b = base(m);
    const ins = isObj(m.install) ? m.install : null;
    if (!b || !ins) return null;
    const transport = ins.transport === "http" ? "http" : ins.transport === "stdio" ? "stdio" : null;
    if (!transport) return null;
    if (transport === "stdio" && !str(ins.command)) return null;
    if (transport === "http" && !/^https:\/\//i.test(str(ins.url))) return null;
    return {
      ...b,
      install: {
        transport,
        command: str(ins.command) || undefined,
        args: strList(ins.args),
        env: strMap(ins.env),
        url: str(ins.url) || undefined,
        headers: strMap(ins.headers),
        oauth:
          transport === "http" && isObj(ins.oauth)
            ? {
                clientId: str(ins.oauth.clientId),
                clientSecret: str(ins.oauth.clientSecret),
                scopes: strList(ins.oauth.scopes),
                authorizationEndpoint: link(ins.oauth.authorizationEndpoint) ?? "",
                tokenEndpoint: link(ins.oauth.tokenEndpoint) ?? "",
                extraParams: strMap(ins.oauth.extraParams),
                group: str(ins.oauth.group),
              }
            : undefined,
      },
      inputs: inputs(m.inputs),
      safeTools: strList(m.safeTools),
    };
  });
  seen.clear();
  const filteredMcp = mcp.filter(unique);

  seen.clear();
  const skills = (Array.isArray(data.skills) ? data.skills : []).filter(isObj).map((s): CatalogSkill | null => {
    const b = base(s);
    const sha256 = str(s.sha256).toLowerCase();
    if (!b || !str(s.download) || !/^[0-9a-f]{64}$/.test(sha256)) return null;
    let download: string;
    try {
      download = new URL(str(s.download), catalogUrl).toString();
    } catch {
      return null;
    }
    if (!download.startsWith("https://")) return null;
    return {
      ...b,
      download,
      sha256,
      size: typeof s.size === "number" ? s.size : undefined,
      files: strList(s.files),
      fileCount: typeof s.fileCount === "number" ? s.fileCount : undefined,
      skillName: str(s.skillName) || b.id,
    };
  });

  return {
    name: str(data.name) || "찾아보기 목록",
    homepage: link(data.homepage),
    updated: str(data.updated) || undefined,
    mcp: filteredMcp,
    skills: skills.filter(unique),
  };
}

/**
 * `{key}`를 입력값으로 바꾼다. 틀에 자리표시가 있었는데 모두 비어 있으면 null (그 항목은 뺀다).
 */
export function fillTemplate(template: string, values: Record<string, string>): string | null {
  let placeholders = 0;
  let filled = 0;
  const out = template.replace(/\{([A-Za-z0-9_]+)\}/g, (m, key: string) => {
    if (!(key in values)) return m;
    placeholders++;
    const v = values[key] ?? "";
    if (v.trim()) filled++;
    return v.trim();
  });
  return placeholders > 0 && filled === 0 ? null : out;
}

export function missingRuntimes(requires: Runtime[], runtimes: Runtimes | null): Runtime[] {
  if (!runtimes) return [];
  return requires.filter((r) => !runtimes[r]);
}

/** 목록 항목과 입력값으로 MCP 서버 설정을 만든다. 필수 값이 비어 있으면 오류. */
export function buildMcpInstall(
  entry: CatalogMcp,
  values: Record<string, string>,
  /** 이미 저장된 값을 쓰므로 비워도 되는 입력 */
  optional: Set<string> = new Set()
): { config: McpServerConfig; env: Record<string, string>; headers: Record<string, string>; oauthClientSecret: string } {
  for (const i of entry.inputs) {
    if (i.required && !optional.has(i.key) && !(values[i.key] ?? "").trim()) throw new Error(`'${i.label}'을(를) 입력하세요.`);
  }
  const all = Object.fromEntries(entry.inputs.map((i) => [i.key, values[i.key] ?? ""]));
  const fillMap = (m: Record<string, string> = {}) =>
    Object.fromEntries(
      Object.entries(m).flatMap(([k, t]) => {
        const v = fillTemplate(t, all);
        return v === null || v === "" ? [] : [[k, v]];
      })
    ) as Record<string, string>;

  const config = emptyConfig();
  config.name = entry.name;
  config.catalogId = entry.id;
  config.transport = entry.install.transport;
  config.autoApprove = [...entry.safeTools];
  const env = entry.install.transport === "stdio" ? fillMap(entry.install.env) : {};
  const headers = entry.install.transport === "http" ? fillMap(entry.install.headers) : {};
  if (entry.install.transport === "stdio") {
    config.command = entry.install.command ?? "";
    config.args = (entry.install.args ?? []).flatMap((a) => {
      const v = fillTemplate(a, all);
      return v === null ? [] : [v];
    });
    config.envKeys = Object.keys(env);
  } else {
    config.url = fillTemplate(entry.install.url ?? "", all) ?? "";
    config.headerKeys = Object.keys(headers);
  }
  let oauthClientSecret = "";
  const o = entry.install.oauth;
  if (o) {
    const oauth: McpOAuthSettings = {
      clientId: fillTemplate(o.clientId, all) ?? "",
      scopes: [...o.scopes],
      authorizationEndpoint: o.authorizationEndpoint,
      tokenEndpoint: o.tokenEndpoint,
      extraParams: { ...o.extraParams },
      group: o.group,
    };
    config.oauth = oauth;
    oauthClientSecret = fillTemplate(o.clientSecret, all) ?? "";
  }
  return { config, env, headers, oauthClientSecret };
}

/** 이 항목의 클라이언트 ID·보안 비밀번호를 채우는 입력 키 */
export function oauthInputKeys(entry: CatalogMcp): { clientId: string[]; clientSecret: string[] } {
  const keys = (t = "") => [...t.matchAll(/\{([A-Za-z0-9_]+)\}/g)].map((m) => m[1]);
  return { clientId: keys(entry.install.oauth?.clientId), clientSecret: keys(entry.install.oauth?.clientSecret) };
}

/** 검색어·분류로 거른다. */
export function filterEntries<T extends CatalogBase>(items: T[], query: string, category: string): T[] {
  const q = query.trim().toLowerCase();
  return items.filter(
    (i) =>
      (!category || i.category === category) &&
      (!q || [i.name, i.summary, i.description ?? "", i.category, i.publisher ?? "", i.id].some((t) => t.toLowerCase().includes(q)))
  );
}

export function categoriesOf(items: CatalogBase[]): string[] {
  return [...new Set(items.map((i) => i.category))];
}
