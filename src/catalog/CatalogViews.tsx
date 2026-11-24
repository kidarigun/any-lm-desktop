import { useCallback, useEffect, useMemo, useState } from "react";
import { AlertTriangle, Check, Download, ExternalLink, FolderOpen, Globe, Laptop, Plug, RefreshCw, Search } from "lucide-react";
import { BackendAPI } from "../api";
import type { McpServerView, SkillInfo } from "../types";
import {
  buildMcpInstall,
  categoriesOf,
  filterEntries,
  mergeCatalogs,
  missingRuntimes,
  normalizeCatalog,
  oauthInputKeys,
  RUNTIME_INFO,
  type Catalog,
  type CatalogMcp,
  type CatalogSkill,
  type Runtimes,
} from "./catalog";
import { builtinCatalog } from "./builtin";
import { joinArgs } from "../mcp/importConfig";
import { RuntimeInstallButton } from "../runtime/RuntimePanel";

const URL_KEY = "anylm_catalog_url";
const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));

function savedUrl(): string {
  try {
    return localStorage.getItem(URL_KEY) ?? "";
  } catch {
    return "";
  }
}

interface Loaded {
  url: string;
  catalog: Catalog;
  fromCache: boolean;
  error?: string | null;
}

// 앱이 켜져 있는 동안은 한 번 받은 목록을 다시 쓴다 ([새로 고침]으로 다시 받는다).
let memo: Loaded | null = null;
let runtimesMemo: Runtimes | null = null;

/** 찾아보기 목록과 이 PC의 실행 환경 */
export function useCatalog() {
  const [data, setData] = useState<Loaded | null>(memo);
  const [runtimes, setRuntimes] = useState<Runtimes | null>(runtimesMemo);
  const [loading, setLoading] = useState(!memo);
  const [error, setError] = useState("");

  const load = useCallback(async (force: boolean) => {
    const url = savedUrl();
    if (!force && memo && memo.url === url) {
      setData(memo);
      return;
    }
    setLoading(true);
    setError("");
    try {
      const [r, rt] = await Promise.all([BackendAPI.catalogLoad(url || undefined), BackendAPI.catalogRuntimes()]);
      memo = { url: r.url, catalog: mergeCatalogs(builtinCatalog(), normalizeCatalog(r.catalog, r.url)), fromCache: r.fromCache, error: r.error };
      runtimesMemo = rt;
      setData(memo);
      setRuntimes(rt);
    } catch (e) {
      // 목록을 받지 못했고 저장본도 없으면 내장 목록만 보여 준다 (기억해 두지 않아 다시 시도할 수 있다).
      setData({ url, catalog: mergeCatalogs(builtinCatalog(), null), fromCache: false, error: null });
      setError(errorText(e));
      const rt = await BackendAPI.catalogRuntimes().catch(() => null);
      if (rt) {
        runtimesMemo = rt;
        setRuntimes(rt);
      }
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load(false);
  }, [load]);

  const setUrl = (url: string) => {
    try {
      if (url.trim()) localStorage.setItem(URL_KEY, url.trim());
      else localStorage.removeItem(URL_KEY);
    } catch {
      // 저장하지 못해도 이번에는 기본 목록을 쓴다
    }
    void load(true);
  };

  /** 실행 환경을 설치한 뒤: 목록은 그대로 두고 설치 여부만 다시 확인한다 */
  const refreshRuntimes = async () => {
    const rt = await BackendAPI.catalogRuntimes().catch(() => null);
    runtimesMemo = rt;
    setRuntimes(rt);
  };

  return { data, runtimes, loading, error, reload: () => load(true), setUrl, refreshRuntimes };
}

const chip = (active: boolean) =>
  `px-2 py-0.5 rounded-full border text-[11px] cursor-pointer ${
    active
      ? "border-[#A50034] bg-[#A50034] text-white"
      : "border-stone-300 dark:border-[#3A3A36] text-stone-600 dark:text-stone-300 hover:bg-stone-100 dark:hover:bg-[#2C2C29]"
  }`;
const inputCls =
  "w-full bg-white dark:bg-[#1E1E1C] border border-stone-300 dark:border-[#3A3A36] rounded-lg px-2.5 py-1.5 text-stone-900 dark:text-stone-100 focus:outline-none focus:border-[#A50034]";
const primary = "px-3 py-1.5 rounded-lg bg-[#A50034] hover:bg-[#8A002B] text-white font-semibold cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed flex items-center gap-1";
const linkCls = "text-[#A50034] dark:text-[#FF5C8D] hover:underline cursor-pointer";

const openUrl = (url: string) => void BackendAPI.openExternalUrl(url).catch(() => {});

function Toolbar({
  query,
  setQuery,
  categories,
  category,
  setCategory,
  onReload,
  loading,
}: {
  query: string;
  setQuery: (q: string) => void;
  categories: string[];
  category: string;
  setCategory: (c: string) => void;
  onReload: () => void;
  loading: boolean;
}) {
  return (
    <div className="space-y-2 mb-3">
      <div className="flex items-center gap-1.5">
        <div className="relative flex-1">
          <Search className="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-stone-400" />
          <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="검색" className={`${inputCls} pl-8`} />
        </div>
        <button onClick={onReload} disabled={loading} title="목록 새로 고침" className="p-1.5 rounded-lg hover:bg-stone-200/60 dark:hover:bg-[#2C2C29] text-stone-500 cursor-pointer disabled:opacity-50">
          <RefreshCw className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`} />
        </button>
      </div>
      {categories.length > 1 && (
        <div className="flex flex-wrap gap-1">
          <button onClick={() => setCategory("")} className={chip(!category)}>
            전체
          </button>
          {categories.map((c) => (
            <button key={c} onClick={() => setCategory(c === category ? "" : c)} className={chip(c === category)}>
              {c}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/** 목록 출처와 주소 바꾸기 */
function SourceFooter({ data, setUrl }: { data: Loaded | null; setUrl: (u: string) => void }) {
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState(savedUrl());
  return (
    <div className="mt-4 pt-3 border-t border-stone-200 dark:border-[#34342F] text-[11px] text-stone-500 dark:text-stone-400 space-y-1.5">
      <div className="flex items-center gap-1.5 flex-wrap">
        <span>
          목록: {data?.catalog.name ?? "—"}
          {data?.catalog.updated && ` · ${data.catalog.updated} 갱신`}
        </span>
        {data?.catalog.homepage && (
          <button onClick={() => openUrl(data.catalog.homepage!)} className={linkCls}>
            목록 사이트
          </button>
        )}
        <button onClick={() => setEditing(!editing)} className={linkCls}>
          목록 주소 바꾸기
        </button>
      </div>
      {editing && (
        <form
          className="flex items-center gap-1.5"
          onSubmit={(e) => {
            e.preventDefault();
            setUrl(value);
            setEditing(false);
          }}
        >
          <input value={value} onChange={(e) => setValue(e.target.value)} placeholder="선택 사항. 비워 두면 내장 목록 사용" className={`${inputCls} font-mono text-[11px]`} spellCheck={false} />
          <button type="submit" className={primary}>
            적용
          </button>
        </form>
      )}
    </div>
  );
}

/** 필요한 프로그램이 없을 때: 앱이 바로 설치할 수 있으면 [자동 설치], 아니면 설치 안내 링크 */
function RuntimeNotice({ missing, onInstalled }: { missing: string[]; onInstalled: () => void }) {
  if (missing.length === 0) return null;
  return (
    <div className="space-y-1">
      <div className="flex flex-wrap items-center gap-1 text-[11px] text-amber-800 dark:text-amber-300">
        <AlertTriangle className="w-3 h-3" />
        <span>{missing.map((r) => RUNTIME_INFO[r as keyof typeof RUNTIME_INFO].label).join(", ")}이(가) 필요합니다.</span>
      </div>
      <div className="flex flex-wrap items-start gap-2">
        {missing.map((r) =>
          r === "python" || r === "node" || r === "uv" ? (
            <RuntimeInstallButton key={r} kind={r} onDone={onInstalled} />
          ) : (
            <button key={r} onClick={() => openUrl(RUNTIME_INFO[r as keyof typeof RUNTIME_INFO].url)} className="text-[11px] underline text-amber-800 dark:text-amber-300 cursor-pointer">
              {RUNTIME_INFO[r as keyof typeof RUNTIME_INFO].label} 설치 안내
            </button>
          )
        )}
      </div>
    </div>
  );
}

function Meta({ e }: { e: { category: string; publisher?: string; homepage?: string; license?: string; note?: string; guide?: string; official?: boolean } }) {
  return (
    <>
    <div className="flex items-center gap-1.5 flex-wrap text-[10px] text-stone-400 dark:text-stone-500">
      <span className="px-1.5 py-px rounded bg-stone-100 dark:bg-[#2C2C29] text-stone-500 dark:text-stone-400">{e.category}</span>
      {e.official && <span className="px-1.5 py-px rounded bg-emerald-50 dark:bg-emerald-900/30 text-emerald-700 dark:text-emerald-300 font-semibold">공식</span>}
      {e.publisher && <span>{e.publisher}</span>}
      {e.license && <span>{e.license}</span>}
      {e.guide && (
        <button onClick={() => openUrl(e.guide!)} className="flex items-center gap-0.5 font-semibold text-[#A50034] dark:text-[#FF5C8D] hover:underline cursor-pointer">
          설정 안내 <ExternalLink className="w-2.5 h-2.5" />
        </button>
      )}
      {e.homepage && (
        <button onClick={() => openUrl(e.homepage!)} className="flex items-center gap-0.5 hover:underline cursor-pointer">
          안내 <ExternalLink className="w-2.5 h-2.5" />
        </button>
      )}
    </div>
    {e.note && <p className="text-[11px] text-amber-800 dark:text-amber-300/90">{e.note}</p>}
    </>
  );
}

function StatusLine({ loading, error, data }: { loading: boolean; error: string; data: Loaded | null }) {
  if (error && !data)
    return (
      <div className="py-6 text-center text-red-700 dark:text-red-400 space-y-1">
        <p>목록을 불러오지 못했습니다.</p>
        <p className="text-[11px]">{error}</p>
      </div>
    );
  if (error && data)
    return (
      <div className="mb-2 px-3 py-1.5 rounded-lg bg-amber-50 dark:bg-amber-900/20 text-[11px] text-amber-800 dark:text-amber-300">
        인터넷에서 목록을 받지 못했습니다. 앱에 들어 있거나 이미 받아 둔 항목만 보여 줍니다. ({error})
      </div>
    );
  if (loading && !data) return <div className="py-8 text-center text-stone-500">목록을 불러오는 중...</div>;
  if (data?.fromCache)
    return (
      <div className="mb-2 px-3 py-1.5 rounded-lg bg-amber-50 dark:bg-amber-900/20 text-[11px] text-amber-800 dark:text-amber-300">
        인터넷에서 받지 못해 마지막으로 받은 목록을 보여 줍니다. ({data.error})
      </div>
    );
  return null;
}

// ---------------------------------------------------------------------------
// MCP
// ---------------------------------------------------------------------------

function McpInstallPanel({
  entry,
  missing,
  installed,
  onCancel,
  onDone,
}: {
  entry: CatalogMcp;
  missing: string[];
  installed: McpServerView[];
  onCancel: () => void;
  onDone: (id: string, needsLogin: boolean) => void;
}) {
  // 같은 로그인 묶음(예: Google)의 서버가 이미 있으면 그 클라이언트 정보를 그대로 쓴다.
  const group = entry.install.oauth?.group;
  const sibling = group ? installed.find((s) => s.config.oauth?.group === group && s.config.oauth.clientId) : undefined;
  const keys = oauthInputKeys(entry);
  const reused = new Set(sibling ? [...keys.clientId, ...keys.clientSecret] : []);
  const [values, setValues] = useState<Record<string, string>>(() =>
    sibling ? Object.fromEntries(keys.clientId.map((k) => [k, sibling.config.oauth!.clientId])) : {}
  );
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  let preview = "";
  try {
    const { config } = buildMcpInstall(entry, Object.fromEntries(entry.inputs.map((i) => [i.key, values[i.key] || (i.required ? `<${i.label}>` : "")])));
    preview = config.transport === "http" ? config.url : `${config.command} ${joinArgs(config.args)}`;
  } catch {
    preview = "";
  }

  const install = async () => {
    setBusy(true);
    setError("");
    try {
      const { config, env, headers, oauthClientSecret } = buildMcpInstall(entry, values, reused);
      const saved = await BackendAPI.mcpSaveServer(config, env, headers, oauthClientSecret);
      onDone(saved.id, !!config.oauth);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mt-2 pt-2 border-t border-stone-100 dark:border-[#2C2C29] space-y-2" data-testid="catalog-install">
      {entry.description && <p className="text-[11px] text-stone-600 dark:text-stone-400">{entry.description}</p>}
      {entry.guide && (
        <div className="flex items-center justify-between gap-2 px-3 py-2 rounded-lg bg-[#A50034]/5 dark:bg-[#FF5C8D]/10 text-[11px] text-stone-700 dark:text-stone-300">
          <span>처음이라면 설정 안내를 먼저 따라 하세요. 아래에 넣을 값을 만드는 방법이 단계별로 있습니다.</span>
          <button onClick={() => openUrl(entry.guide!)} className={`${primary} shrink-0`}>
            설정 안내 열기 <ExternalLink className="w-3 h-3" />
          </button>
        </div>
      )}
      {sibling && (
        <p className="text-[11px] text-emerald-800 dark:text-emerald-300">
          '{sibling.config.name}'에 넣은 클라이언트 정보를 함께 씁니다. 로그인도 한 번만 하면 됩니다.
        </p>
      )}
      {entry.inputs.filter((i) => !(sibling && keys.clientSecret.includes(i.key))).map((i) => (
        <div key={i.key}>
          <label className="font-semibold text-stone-700 dark:text-stone-300 block mb-1">
            {i.label} {i.required && <span className="text-[#A50034]">*</span>}
          </label>
          <div className="flex items-center gap-1.5">
            <input
              type={i.type === "secret" ? "password" : "text"}
              autoComplete="off"
              value={values[i.key] ?? ""}
              onChange={(e) => setValues({ ...values, [i.key]: e.target.value })}
              placeholder={i.placeholder}
              className={`${inputCls} ${i.type !== "secret" ? "font-mono" : ""}`}
              spellCheck={false}
            />
            {i.type === "folder" && (
              <button
                onClick={async () => {
                  const p = await BackendAPI.pickFolderDialog(values[i.key] || undefined).catch(() => null);
                  if (p) setValues((v) => ({ ...v, [i.key]: p }));
                }}
                className="px-2.5 py-1.5 rounded-lg border border-stone-300 dark:border-[#3A3A36] bg-white dark:bg-[#1E1E1C] flex items-center gap-1 cursor-pointer shrink-0"
              >
                <FolderOpen className="w-3.5 h-3.5" /> 선택
              </button>
            )}
          </div>
          {(i.help || i.helpUrl) && (
            <p className="mt-0.5 text-[11px] text-stone-500 dark:text-stone-400">
              {i.help}{" "}
              {i.helpUrl && (
                <button onClick={() => openUrl(i.helpUrl!)} className={linkCls}>
                  안내 열기
                </button>
              )}
            </p>
          )}
        </div>
      ))}
      {preview && (
        <div className="text-[11px] text-stone-500 dark:text-stone-400">
          {entry.install.transport === "http" ? "연결할 주소" : "실행될 명령"}:
          <code className="block mt-0.5 px-2 py-1 rounded bg-stone-100 dark:bg-[#171715] font-mono break-all text-stone-700 dark:text-stone-300">{preview}</code>
        </div>
      )}
      {entry.safeTools.length > 0 && (
        <p className="text-[11px] text-stone-500 dark:text-stone-400">
          읽기 전용 도구({entry.safeTools.length}개)는 승인 없이 실행하고, 나머지는 실행할 때마다 묻습니다. 설치한 뒤 MCP 서버 창에서 바꿀 수 있습니다.
        </p>
      )}
      {entry.inputs.some((i) => i.type === "secret") && (
        <p className="text-[11px] text-stone-500 dark:text-stone-400">입력한 키·토큰은 Windows 자격 증명 관리자에 저장됩니다.</p>
      )}
      {error && <div className="text-[11px] text-red-700 dark:text-red-400">{error}</div>}
      <div className="flex justify-end gap-1.5">
        <button onClick={onCancel} className="px-2.5 py-1.5 rounded-lg border border-stone-300 dark:border-[#3A3A36] cursor-pointer">
          취소
        </button>
        <button onClick={install} disabled={busy || missing.length > 0} className={primary}>
          <Plug className="w-3.5 h-3.5" /> {busy ? "연결 중..." : "연결"}
        </button>
      </div>
    </div>
  );
}

/** MCP 서버 찾아보기 */
export function McpCatalog({
  installed,
  onInstalled,
}: {
  installed: McpServerView[];
  onInstalled: (serverId: string, name: string, needsLogin: boolean) => void;
}) {
  const { data, runtimes, loading, error, reload, setUrl, refreshRuntimes } = useCatalog();
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState("");
  const [openId, setOpenId] = useState<string | null>(null);
  const items = data?.catalog.mcp ?? [];
  const shown = useMemo(() => filterEntries(items, query, category), [items, query, category]);

  return (
    <div data-testid="mcp-catalog">
      <StatusLine loading={loading} error={error} data={data} />
      {data && (
        <>
          <Toolbar query={query} setQuery={setQuery} categories={categoriesOf(items)} category={category} setCategory={setCategory} onReload={reload} loading={loading} />
          <ul className="space-y-1.5">
            {shown.map((m) => {
              const server = installed.find((s) => s.config.catalogId === m.id);
              const missing = missingRuntimes(m.requires, runtimes);
              const open = openId === m.id;
              return (
                <li key={m.id} className="rounded-xl border border-stone-200 dark:border-[#383834] bg-white dark:bg-[#1E1E1C] px-3 py-2.5" data-testid="catalog-item">
                  <div className="flex items-start gap-2.5">
                    <div className="mt-0.5 shrink-0 w-7 h-7 rounded-lg bg-stone-100 dark:bg-[#2C2C29] flex items-center justify-center text-stone-500">
                      {m.install.transport === "http" ? <Globe className="w-3.5 h-3.5" /> : <Laptop className="w-3.5 h-3.5" />}
                    </div>
                    <div className="min-w-0 flex-1 space-y-0.5">
                      <div className="font-semibold text-stone-900 dark:text-stone-100">{m.name}</div>
                      <p className="text-[11px] text-stone-600 dark:text-stone-400">{m.summary}</p>
                      <Meta e={m} />
                      <RuntimeNotice missing={missing} onInstalled={refreshRuntimes} />
                    </div>
                    <div className="shrink-0">
                      {server ? (
                        <span className="flex items-center gap-1 text-[11px] font-semibold text-emerald-700 dark:text-emerald-400 py-1.5">
                          <Check className="w-3.5 h-3.5" /> 추가됨
                        </span>
                      ) : (
                        <button onClick={() => setOpenId(open ? null : m.id)} disabled={missing.length > 0} className={primary}>
                          {open ? "닫기" : "연결"}
                        </button>
                      )}
                    </div>
                  </div>
                  {open && !server && (
                    <McpInstallPanel
                      entry={m}
                      missing={missing}
                      installed={installed}
                      onCancel={() => setOpenId(null)}
                      onDone={(id, needsLogin) => {
                        setOpenId(null);
                        onInstalled(id, m.name, needsLogin);
                      }}
                    />
                  )}
                </li>
              );
            })}
            {shown.length === 0 && <li className="py-6 text-center text-stone-500">찾는 서버가 없습니다.</li>}
          </ul>
        </>
      )}
      <SourceFooter data={data} setUrl={setUrl} />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Skill
// ---------------------------------------------------------------------------

/** Skill 찾아보기 */
export function SkillCatalog({ installed, onInstalled }: { installed: SkillInfo[]; onInstalled: (name: string) => void }) {
  const { data, runtimes, loading, error, reload, setUrl, refreshRuntimes } = useCatalog();
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState("");
  const [busyId, setBusyId] = useState<string | null>(null);
  const [itemError, setItemError] = useState<Record<string, string>>({});
  const items = data?.catalog.skills ?? [];
  const shown = useMemo(() => filterEntries(items, query, category), [items, query, category]);

  const install = async (s: CatalogSkill, overwrite: boolean) => {
    setBusyId(s.id);
    setItemError((e) => ({ ...e, [s.id]: "" }));
    try {
      let info: SkillInfo;
      try {
        info = await BackendAPI.catalogInstallSkill(s.download, s.sha256, overwrite);
      } catch (e) {
        const msg = errorText(e);
        if (!msg.startsWith("ALREADY_EXISTS:")) throw e;
        if (!window.confirm(`'${s.name}' Skill이 이미 있습니다. 목록의 최신 내용으로 바꿀까요?\n(기존 Skill은 휴지통으로 옮겨집니다.)`)) return;
        info = await BackendAPI.catalogInstallSkill(s.download, s.sha256, true);
      }
      onInstalled(info.name);
    } catch (e) {
      setItemError((x) => ({ ...x, [s.id]: errorText(e) }));
    } finally {
      setBusyId(null);
    }
  };

  return (
    <div data-testid="skill-catalog">
      <StatusLine loading={loading} error={error} data={data} />
      {data && (
        <>
          <Toolbar query={query} setQuery={setQuery} categories={categoriesOf(items)} category={category} setCategory={setCategory} onReload={reload} loading={loading} />
          <ul className="space-y-1.5">
            {shown.map((s) => {
              const have = installed.some((i) => i.scope === "user" && i.name.toLowerCase() === s.skillName.toLowerCase());
              const missing = missingRuntimes(s.requires, runtimes);
              return (
                <li key={s.id} className="rounded-xl border border-stone-200 dark:border-[#383834] bg-white dark:bg-[#1E1E1C] px-3 py-2.5" data-testid="catalog-item">
                  <div className="flex items-start gap-2.5">
                    <div className="min-w-0 flex-1 space-y-0.5">
                      <div className="font-semibold text-stone-900 dark:text-stone-100">{s.name}</div>
                      <p className="text-[11px] text-stone-600 dark:text-stone-400">{s.summary}</p>
                      <Meta e={s} />
                      {s.files.some((f) => f !== "SKILL.md") && (
                        <div className="text-[10px] text-stone-400 dark:text-stone-500 line-clamp-2" title={s.files.join("\n")}>
                          {s.fileCount
                            ? `파일 ${s.fileCount}개${s.size ? ` · ${(s.size / 1024 / 1024).toFixed(1)}MB` : ""}`
                            : `함께 든 파일: ${s.files.filter((f) => f !== "SKILL.md").join(", ")}`}
                        </div>
                      )}
                      <RuntimeNotice missing={missing} onInstalled={refreshRuntimes} />
                      {itemError[s.id] && <div className="text-[11px] text-red-700 dark:text-red-400">{itemError[s.id]}</div>}
                    </div>
                    <div className="shrink-0 flex flex-col items-end gap-1">
                      {have ? (
                        <>
                          <span className="flex items-center gap-1 text-[11px] font-semibold text-emerald-700 dark:text-emerald-400">
                            <Check className="w-3.5 h-3.5" /> 설치됨
                          </span>
                          <button onClick={() => install(s, false)} disabled={busyId !== null} className="text-[11px] text-stone-500 hover:underline cursor-pointer disabled:opacity-50">
                            {busyId === s.id ? "받는 중..." : "다시 받기"}
                          </button>
                        </>
                      ) : (
                        <button onClick={() => install(s, false)} disabled={busyId !== null} className={primary}>
                          <Download className="w-3.5 h-3.5" /> {busyId === s.id ? "받는 중..." : "설치"}
                        </button>
                      )}
                    </div>
                  </div>
                </li>
              );
            })}
            {shown.length === 0 && <li className="py-6 text-center text-stone-500">찾는 Skill이 없습니다.</li>}
          </ul>
        </>
      )}
      <SourceFooter data={data} setUrl={setUrl} />
    </div>
  );
}
