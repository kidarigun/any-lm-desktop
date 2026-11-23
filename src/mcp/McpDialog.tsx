import { useCallback, useEffect, useState } from "react";
import {
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  ClipboardPaste,
  Edit2,
  Globe,
  Laptop,
  LogIn,
  LogOut,
  Plug,
  Plus,
  RefreshCw,
  Trash2,
  X,
} from "lucide-react";
import { BackendAPI } from "../api";
import type { McpServerConfig, McpServerView } from "../types";
import { emptyConfig, joinArgs, parseMcpJson, splitArgs } from "./importConfig";
import { McpCatalog } from "../catalog/CatalogViews";

const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));

interface KeyValue {
  key: string;
  value: string;
  /** 이미 저장된 값이 있는 항목 (값을 비우면 그대로 유지) */
  saved: boolean;
}

interface Draft {
  config: McpServerConfig;
  argsLine: string;
  env: KeyValue[];
  headers: KeyValue[];
  /** 로그인(OAuth) 사용 */
  oauthOn: boolean;
  scopesLine: string;
  /** 클라이언트 보안 비밀번호 (비우면 저장된 값 유지) */
  oauthSecret: string;
}

const emptyOAuth = () => ({ clientId: "", scopes: [], authorizationEndpoint: "", tokenEndpoint: "", extraParams: {}, group: "" });

function draftOf(config: McpServerConfig): Draft {
  return {
    config: { ...config, oauth: config.oauth ? { ...config.oauth } : null },
    argsLine: joinArgs(config.args),
    env: config.envKeys.map((key) => ({ key, value: "", saved: !!config.id })),
    headers: config.headerKeys.map((key) => ({ key, value: "", saved: !!config.id })),
    oauthOn: !!config.oauth,
    scopesLine: (config.oauth?.scopes ?? []).join(" "),
    oauthSecret: "",
  };
}

const toRecord = (rows: KeyValue[]) => Object.fromEntries(rows.filter((r) => r.key.trim()).map((r) => [r.key.trim(), r.value]));

const STATUS: Record<string, { label: string; dot: string }> = {
  connected: { label: "연결됨", dot: "bg-emerald-500" },
  connecting: { label: "연결 중...", dot: "bg-amber-400 animate-pulse" },
  needs_login: { label: "로그인 필요", dot: "bg-amber-500" },
  error: { label: "오류", dot: "bg-red-500" },
  off: { label: "꺼짐", dot: "bg-stone-300 dark:bg-stone-600" },
};

const input =
  "w-full bg-white dark:bg-[#1E1E1C] border border-stone-300 dark:border-[#3A3A36] rounded-lg px-2.5 py-1.5 text-stone-900 dark:text-stone-100 focus:outline-none focus:border-[#A50034]";
const btn =
  "px-2.5 py-1.5 rounded-lg border border-stone-300 dark:border-[#3A3A36] bg-white dark:bg-[#1E1E1C] hover:bg-stone-50 dark:hover:bg-[#2A2A27] text-stone-700 dark:text-stone-200 font-medium flex items-center gap-1.5 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed";
const iconBtn = "p-1.5 rounded-lg hover:bg-stone-100 dark:hover:bg-[#2C2C29] text-stone-500 cursor-pointer disabled:opacity-50";

function Switch({ on, label, onChange, disabled }: { on: boolean; label: string; onChange: () => void; disabled?: boolean }) {
  return (
    <button
      role="switch"
      aria-checked={on}
      aria-label={label}
      onClick={onChange}
      disabled={disabled}
      className={`shrink-0 w-8 h-[18px] rounded-full relative transition cursor-pointer disabled:opacity-50 ${
        on ? "bg-[#A50034]" : "bg-stone-300 dark:bg-[#44443F]"
      }`}
    >
      <span className={`absolute top-[2px] w-[14px] h-[14px] rounded-full bg-white shadow transition-all ${on ? "left-[16px]" : "left-[2px]"}`} />
    </button>
  );
}

function KeyValueRows({
  rows,
  setRows,
  keyPlaceholder,
  valuePlaceholder,
}: {
  rows: KeyValue[];
  setRows: (rows: KeyValue[]) => void;
  keyPlaceholder: string;
  valuePlaceholder: string;
}) {
  return (
    <div className="space-y-1">
      {rows.map((r, i) => (
        <div key={i} className="flex items-center gap-1.5">
          <input
            value={r.key}
            onChange={(e) => setRows(rows.map((x, j) => (j === i ? { ...x, key: e.target.value, saved: false } : x)))}
            placeholder={keyPlaceholder}
            className={`${input} font-mono w-2/5`}
            spellCheck={false}
          />
          <input
            type="password"
            autoComplete="off"
            value={r.value}
            onChange={(e) => setRows(rows.map((x, j) => (j === i ? { ...x, value: e.target.value } : x)))}
            placeholder={r.saved ? "저장된 값 유지 — 바꾸려면 입력" : valuePlaceholder}
            className={`${input} flex-1`}
          />
          <button onClick={() => setRows(rows.filter((_, j) => j !== i))} aria-label="삭제" className={iconBtn}>
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      ))}
      <button
        onClick={() => setRows([...rows, { key: "", value: "", saved: false }])}
        className="text-[11px] text-[#A50034] dark:text-[#FF5C8D] font-medium hover:underline cursor-pointer"
      >
        + 추가
      </button>
    </div>
  );
}

/** MCP 서버를 추가·연결·관리하는 창 */
export function McpDialog({ onClose }: { onClose: () => void }) {
  const [servers, setServers] = useState<McpServerView[] | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [importText, setImportText] = useState<string | null>(null);
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const [tab, setTab] = useState<"installed" | "browse" | null>(null);
  /** 브라우저 로그인을 기다리는 서버 */
  const [loginId, setLoginId] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const list = await BackendAPI.mcpListServers();
      setServers(list);
      // 처음 열 때 추가한 서버가 없으면 찾아보기부터 보여 준다.
      setTab((t) => t ?? (list.length === 0 ? "browse" : "installed"));
    } catch (e) {
      setServers([]);
      setError(errorText(e));
    }
  }, []);

  // 연결 상태가 바뀌므로 창이 열려 있는 동안 주기적으로 다시 읽는다.
  useEffect(() => {
    void refresh();
    const t = window.setInterval(refresh, 1500);
    return () => window.clearInterval(t);
  }, [refresh]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (draft) setDraft(null);
      else if (importText !== null) setImportText(null);
      else onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, draft, importText]);

  async function run(task: () => Promise<void>) {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await task();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
      void refresh();
    }
  }

  // 로그인은 최대 5분 기다리므로 다른 작업을 막지 않게 따로 다룬다.
  const login = async (id: string, name: string) => {
    setLoginId(id);
    setError("");
    setNotice(`'${name}': 브라우저에서 로그인하고 권한을 허락하세요. 끝나면 여기로 돌아오면 됩니다.`);
    try {
      await BackendAPI.mcpOauthLogin(id);
      setNotice(`'${name}'에 로그인했습니다. 연결하고 있습니다...`);
    } catch (e) {
      setNotice("");
      setError(errorText(e));
    } finally {
      setLoginId(null);
      void refresh();
    }
  };

  const saveDraft = () =>
    run(async () => {
      if (!draft) return;
      const isHttp = draft.config.transport === "http";
      const config: McpServerConfig = {
        ...draft.config,
        oauth:
          isHttp && draft.oauthOn
            ? { ...(draft.config.oauth ?? emptyOAuth()), scopes: draft.scopesLine.split(/[\s,]+/).filter(Boolean) }
            : null,
        args: draft.config.transport === "stdio" ? splitArgs(draft.argsLine) : [],
        envKeys: draft.config.transport === "stdio" ? draft.env.map((r) => r.key.trim()).filter(Boolean) : [],
        headerKeys: draft.config.transport === "http" ? draft.headers.map((r) => r.key.trim()).filter(Boolean) : [],
      };
      const saved = await BackendAPI.mcpSaveServer(config, toRecord(draft.env), toRecord(draft.headers), draft.oauthSecret);
      setDraft(null);
      setOpen((o) => ({ ...o, [saved.id]: true }));
      setNotice(`'${saved.name}' 서버를 저장했습니다. ${saved.enabled ? "연결하고 있습니다..." : ""}`);
    });

  const importServers = () =>
    run(async () => {
      const parsed = parseMcpJson(importText ?? "");
      const existing = new Set((servers ?? []).map((s) => s.config.name));
      const done: string[] = [];
      const failed: string[] = [];
      for (const s of parsed) {
        let name = s.config.name;
        for (let n = 2; existing.has(name); n++) name = `${s.config.name} (${n})`;
        try {
          await BackendAPI.mcpSaveServer({ ...s.config, name }, s.env, s.headers);
          existing.add(name);
          done.push(name);
        } catch (e) {
          failed.push(`${name}: ${errorText(e)}`);
        }
      }
      if (done.length) setImportText(null);
      if (failed.length) setError(`${done.length ? `${done.length}개는 추가했고, ` : ""}다음은 추가하지 못했습니다.\n${failed.join("\n")}`);
      else setNotice(`${done.length}개 서버를 추가했습니다: ${done.join(", ")}`);
    });

  const remove = (v: McpServerView) =>
    run(async () => {
      if (!window.confirm(`'${v.config.name}' 서버를 삭제할까요?\n저장된 API 키 등 비밀 값도 함께 지워집니다.`)) return;
      await BackendAPI.mcpDeleteServer(v.config.id);
    });

  const setTool = (v: McpServerView, tool: string, opts: { enabled?: boolean; autoApprove?: boolean }) =>
    run(() => BackendAPI.mcpSetTool(v.config.id, tool, opts));

  const card = (v: McpServerView) => {
    const { config: c, state: st } = v;
    const status = STATUS[st.status] ?? STATUS.off;
    const expanded = open[c.id] ?? false;
    const usable = st.tools.filter((t) => !c.disabledTools.includes(t.name)).length;
    return (
      <li key={c.id} className="rounded-xl border border-stone-200 dark:border-[#383834] bg-white dark:bg-[#1E1E1C]" data-testid="mcp-server">
        <div className="flex items-start gap-2.5 px-3 py-2.5">
          <Switch
            on={c.enabled}
            label={`${c.name} ${c.enabled ? "끄기" : "켜기"}`}
            disabled={busy}
            onChange={() => run(() => BackendAPI.mcpSetServerEnabled(c.id, !c.enabled))}
          />
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5 flex-wrap">
              <button
                onClick={() => setOpen((o) => ({ ...o, [c.id]: !expanded }))}
                className="flex items-center gap-0.5 font-semibold text-stone-900 dark:text-stone-100 hover:underline cursor-pointer text-left"
              >
                {expanded ? <ChevronDown className="w-3.5 h-3.5 shrink-0" /> : <ChevronRight className="w-3.5 h-3.5 shrink-0" />}
                <span className="break-all">{c.name}</span>
              </button>
              <span className="flex items-center gap-1 text-[11px] text-stone-500 dark:text-stone-400">
                <span className={`w-1.5 h-1.5 rounded-full ${status.dot}`} />
                {status.label}
                {st.status === "connected" && ` · 도구 ${usable}/${st.tools.length}개`}
                {st.serverInfo && st.status === "connected" && <span className="text-stone-400"> · {st.serverInfo}</span>}
              </span>
            </div>
            <div className="mt-0.5 flex items-center gap-1 text-[10px] text-stone-400 dark:text-stone-500 font-mono min-w-0">
              {c.transport === "http" ? <Globe className="w-3 h-3 shrink-0" /> : <Laptop className="w-3 h-3 shrink-0" />}
              <span className="truncate" title={c.transport === "http" ? c.url : `${c.command} ${joinArgs(c.args)}`}>
                {c.transport === "http" ? c.url : `${c.command} ${joinArgs(c.args)}`}
              </span>
            </div>
            {st.status === "needs_login" && (
              <div className="mt-1.5 flex items-center gap-2 flex-wrap">
                {loginId === c.id || st.signingIn ? (
                  <>
                    <span className="text-[11px] text-amber-800 dark:text-amber-300">브라우저에서 로그인하는 중...</span>
                    <button onClick={() => BackendAPI.mcpOauthCancel()} className="text-[11px] text-stone-500 hover:underline cursor-pointer">
                      취소
                    </button>
                  </>
                ) : (
                  <>
                    <button
                      onClick={() => login(c.id, c.name)}
                      disabled={loginId !== null}
                      className="px-2.5 py-1 rounded-lg bg-[#A50034] hover:bg-[#8A002B] text-white font-semibold flex items-center gap-1 cursor-pointer disabled:opacity-40"
                    >
                      <LogIn className="w-3.5 h-3.5" /> 로그인
                    </button>
                    {st.error && <span className="text-[11px] text-stone-500 dark:text-stone-400">{st.error}</span>}
                  </>
                )}
              </div>
            )}
            {st.status === "error" && st.error && (
              <div className="mt-1.5 flex items-start gap-1 text-[11px] text-red-700 dark:text-red-400 whitespace-pre-wrap break-words">
                <AlertTriangle className="w-3 h-3 mt-0.5 shrink-0" />
                <span>{st.error}</span>
              </div>
            )}
          </div>
          <div className="flex items-center gap-0.5 shrink-0">
            <button onClick={() => run(() => BackendAPI.mcpRestartServer(c.id))} disabled={busy || !c.enabled} title="다시 연결" className={iconBtn}>
              <RefreshCw className="w-3.5 h-3.5" />
            </button>
            {c.oauth && st.signedIn && (
              <button
                onClick={() => {
                  if (window.confirm(`'${c.name}'에서 로그아웃할까요? 다시 쓰려면 로그인해야 합니다.`)) void run(() => BackendAPI.mcpOauthLogout(c.id));
                }}
                disabled={busy}
                title="로그아웃"
                className={iconBtn}
              >
                <LogOut className="w-3.5 h-3.5" />
              </button>
            )}
            <button onClick={() => setDraft(draftOf(c))} title="편집" className={iconBtn}>
              <Edit2 className="w-3.5 h-3.5" />
            </button>
            <button onClick={() => remove(v)} disabled={busy} title="삭제" className={`${iconBtn} hover:text-red-600`}>
              <Trash2 className="w-3.5 h-3.5" />
            </button>
          </div>
        </div>

        {expanded && (
          <div className="px-3 pb-3 space-y-2">
            {st.tools.length > 0 ? (
              <table className="w-full text-[11px]">
                <thead>
                  <tr className="text-stone-500 dark:text-stone-400 text-left">
                    <th className="font-medium py-1">도구</th>
                    <th className="font-medium py-1 w-12 text-center">사용</th>
                    <th className="font-medium py-1 w-20 text-center" title="켜면 이 도구는 실행 전에 묻지 않습니다">승인 없이</th>
                  </tr>
                </thead>
                <tbody>
                  {st.tools.map((t) => {
                    const enabled = !c.disabledTools.includes(t.name);
                    const auto = c.autoApprove.includes(t.name);
                    return (
                      <tr key={t.name} className="border-t border-stone-100 dark:border-[#2C2C29] align-top">
                        <td className="py-1.5 pr-2">
                          <div className="font-mono text-stone-800 dark:text-stone-200">{t.name}</div>
                          {t.description && (
                            <div className="text-stone-500 dark:text-stone-400 line-clamp-2" title={t.description}>
                              {t.description}
                            </div>
                          )}
                        </td>
                        <td className="py-1.5 text-center">
                          <input
                            type="checkbox"
                            checked={enabled}
                            disabled={busy}
                            onChange={() => setTool(v, t.name, { enabled: !enabled })}
                            aria-label={`${t.name} 사용`}
                            className="accent-[#A50034] cursor-pointer"
                          />
                        </td>
                        <td className="py-1.5 text-center">
                          <input
                            type="checkbox"
                            checked={auto}
                            disabled={busy || !enabled}
                            onChange={() => setTool(v, t.name, { autoApprove: !auto })}
                            aria-label={`${t.name} 승인 없이 실행`}
                            className="accent-[#A50034] cursor-pointer"
                          />
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            ) : (
              <p className="text-[11px] text-stone-500 dark:text-stone-400">
                {st.status === "connected" ? "이 서버는 도구를 제공하지 않습니다." : "연결되면 도구 목록이 여기에 나옵니다."}
              </p>
            )}
            {st.log.trim() && (
              <details className="text-[11px]">
                <summary className="cursor-pointer text-stone-500 dark:text-stone-400">서버 출력 보기</summary>
                <pre className="mt-1 max-h-40 overflow-auto whitespace-pre-wrap break-all rounded-lg bg-stone-900 text-stone-100 px-2.5 py-2 font-mono text-[10px]">
                  {st.log}
                </pre>
              </details>
            )}
          </div>
        )}
      </li>
    );
  };

  const form = draft && (
    <div className="space-y-3" data-testid="mcp-form">
      <div>
        <label className="font-semibold text-stone-700 dark:text-stone-300 block mb-1">이름</label>
        <input
          autoFocus
          value={draft.config.name}
          onChange={(e) => setDraft({ ...draft, config: { ...draft.config, name: e.target.value } })}
          placeholder="예: 파일 시스템"
          className={input}
        />
      </div>
      <div>
        <label className="font-semibold text-stone-700 dark:text-stone-300 block mb-1">연결 방식</label>
        <div className="grid grid-cols-2 gap-1.5">
          {(
            [
              ["stdio", "내 PC에서 실행", "npx, uvx 같은 명령으로 서버 프로그램을 실행", Laptop],
              ["http", "원격 서버 (URL)", "https:// 주소로 연결 (Streamable HTTP)", Globe],
            ] as const
          ).map(([value, label, desc, Icon]) => (
            <button
              key={value}
              onClick={() => setDraft({ ...draft, config: { ...draft.config, transport: value } })}
              className={`text-left rounded-lg border px-2.5 py-2 cursor-pointer ${
                draft.config.transport === value
                  ? "border-[#A50034] bg-[#A50034]/5 dark:bg-[#FF5C8D]/10"
                  : "border-stone-300 dark:border-[#3A3A36] bg-white dark:bg-[#1E1E1C]"
              }`}
            >
              <div className="font-semibold text-stone-800 dark:text-stone-100 flex items-center gap-1">
                <Icon className="w-3.5 h-3.5" /> {label}
              </div>
              <div className="text-[11px] text-stone-500 dark:text-stone-400">{desc}</div>
            </button>
          ))}
        </div>
      </div>
      {draft.config.transport === "stdio" ? (
        <>
          <div className="grid grid-cols-[1fr_2fr] gap-1.5">
            <div>
              <label className="font-semibold text-stone-700 dark:text-stone-300 block mb-1">명령</label>
              <input
                value={draft.config.command}
                onChange={(e) => setDraft({ ...draft, config: { ...draft.config, command: e.target.value } })}
                placeholder="npx"
                className={`${input} font-mono`}
                spellCheck={false}
              />
            </div>
            <div>
              <label className="font-semibold text-stone-700 dark:text-stone-300 block mb-1">인자</label>
              <input
                value={draft.argsLine}
                onChange={(e) => setDraft({ ...draft, argsLine: e.target.value })}
                placeholder='-y @modelcontextprotocol/server-filesystem "C:\내 문서"'
                className={`${input} font-mono`}
                spellCheck={false}
              />
            </div>
          </div>
          <div>
            <label className="font-semibold text-stone-700 dark:text-stone-300 block mb-1">
              실행 위치 <span className="font-normal text-stone-400">(선택, 비우면 사용자 폴더)</span>
            </label>
            <input
              value={draft.config.cwd}
              onChange={(e) => setDraft({ ...draft, config: { ...draft.config, cwd: e.target.value } })}
              className={`${input} font-mono`}
              spellCheck={false}
            />
          </div>
          <div>
            <label className="font-semibold text-stone-700 dark:text-stone-300 block mb-1">환경 변수</label>
            <KeyValueRows
              rows={draft.env}
              setRows={(env) => setDraft({ ...draft, env })}
              keyPlaceholder="API_KEY"
              valuePlaceholder="값"
            />
          </div>
        </>
      ) : (
        <>
          <div>
            <label className="font-semibold text-stone-700 dark:text-stone-300 block mb-1">URL</label>
            <input
              value={draft.config.url}
              onChange={(e) => setDraft({ ...draft, config: { ...draft.config, url: e.target.value } })}
              placeholder="https://example.com/mcp"
              className={`${input} font-mono`}
              spellCheck={false}
            />
          </div>
          <div>
            <label className="font-semibold text-stone-700 dark:text-stone-300 block mb-1">HTTP 헤더</label>
            <KeyValueRows
              rows={draft.headers}
              setRows={(headers) => setDraft({ ...draft, headers })}
              keyPlaceholder="Authorization"
              valuePlaceholder="Bearer 토큰"
            />
          </div>
          <div className="rounded-lg border border-stone-200 dark:border-[#383834] px-3 py-2 space-y-2">
            <label className="flex items-center gap-2 font-semibold text-stone-700 dark:text-stone-300 cursor-pointer">
              <input
                type="checkbox"
                checked={draft.oauthOn}
                onChange={(e) => setDraft({ ...draft, oauthOn: e.target.checked })}
                className="accent-[#A50034]"
              />
              로그인(OAuth) 사용
            </label>
            {draft.oauthOn && (
              <>
                <p className="text-[11px] text-stone-500 dark:text-stone-400">
                  저장한 뒤 [로그인]을 누르면 브라우저에서 계정에 로그인합니다. 클라이언트 ID를 비우면 서버에 자동 등록을 시도합니다. 직접 만든 클라이언트라면 리디렉션 URI에 <code className="font-mono">http://localhost:45789/oauth2callback</code>을 등록하세요.
                </p>
                <div className="grid grid-cols-2 gap-1.5">
                  <input
                    value={draft.config.oauth?.clientId ?? ""}
                    onChange={(e) => setDraft({ ...draft, config: { ...draft.config, oauth: { ...(draft.config.oauth ?? emptyOAuth()), clientId: e.target.value } } })}
                    placeholder="클라이언트 ID (선택)"
                    className={`${input} font-mono`}
                    spellCheck={false}
                  />
                  <input
                    type="password"
                    autoComplete="off"
                    value={draft.oauthSecret}
                    onChange={(e) => setDraft({ ...draft, oauthSecret: e.target.value })}
                    placeholder={draft.config.id && draft.config.oauth?.clientId ? "저장된 값 유지 — 바꾸려면 입력" : "클라이언트 보안 비밀번호 (선택)"}
                    className={input}
                  />
                </div>
                <input
                  value={draft.scopesLine}
                  onChange={(e) => setDraft({ ...draft, scopesLine: e.target.value })}
                  placeholder="권한(scope) — 공백으로 구분, 비우면 서버가 알려 주는 값"
                  className={`${input} font-mono`}
                  spellCheck={false}
                />
                <details className="text-[11px]">
                  <summary className="cursor-pointer text-stone-500 dark:text-stone-400">고급: 로그인·토큰 주소 직접 입력</summary>
                  <div className="mt-1.5 grid grid-cols-2 gap-1.5">
                    <input
                      value={draft.config.oauth?.authorizationEndpoint ?? ""}
                      onChange={(e) =>
                        setDraft({ ...draft, config: { ...draft.config, oauth: { ...(draft.config.oauth ?? emptyOAuth()), authorizationEndpoint: e.target.value } } })
                      }
                      placeholder="로그인 주소 (authorization)"
                      className={`${input} font-mono`}
                      spellCheck={false}
                    />
                    <input
                      value={draft.config.oauth?.tokenEndpoint ?? ""}
                      onChange={(e) =>
                        setDraft({ ...draft, config: { ...draft.config, oauth: { ...(draft.config.oauth ?? emptyOAuth()), tokenEndpoint: e.target.value } } })
                      }
                      placeholder="토큰 주소 (token)"
                      className={`${input} font-mono`}
                      spellCheck={false}
                    />
                  </div>
                </details>
              </>
            )}
          </div>
        </>
      )}
      <p className="text-[11px] text-stone-500 dark:text-stone-400">
        환경 변수·헤더 값은 설정 파일이 아니라 Windows 자격 증명 관리자에 저장됩니다.
      </p>
      <div className="flex justify-end gap-1.5">
        <button onClick={() => setDraft(null)} className={btn}>
          취소
        </button>
        <button onClick={saveDraft} disabled={busy} className="px-3 py-1.5 rounded-lg bg-[#A50034] text-white font-semibold cursor-pointer disabled:opacity-40">
          저장하고 연결
        </button>
      </div>
    </div>
  );

  const importPanel = importText !== null && (
    <div className="space-y-2" data-testid="mcp-import">
      <p className="text-stone-600 dark:text-stone-400 leading-relaxed">
        MCP 서버 안내 문서나 Claude Desktop 설정 파일(<code className="font-mono">claude_desktop_config.json</code>)의 <code className="font-mono">"mcpServers"</code> 내용을 그대로 붙여 넣으세요.
      </p>
      <textarea
        autoFocus
        value={importText}
        onChange={(e) => setImportText(e.target.value)}
        rows={10}
        spellCheck={false}
        placeholder={'{\n  "mcpServers": {\n    "filesystem": {\n      "command": "npx",\n      "args": ["-y", "@modelcontextprotocol/server-filesystem", "C:\\\\Users\\\\me\\\\Documents"]\n    }\n  }\n}'}
        className={`${input} font-mono text-[11px] resize-y`}
      />
      <div className="flex justify-end gap-1.5">
        <button onClick={() => setImportText(null)} className={btn}>
          취소
        </button>
        <button onClick={importServers} disabled={busy || !importText.trim()} className="px-3 py-1.5 rounded-lg bg-[#A50034] text-white font-semibold cursor-pointer disabled:opacity-40">
          가져오기
        </button>
      </div>
    </div>
  );

  const connected = (servers ?? []).filter((s) => s.state.status === "connected").length;

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div role="dialog" aria-label="MCP 서버" className="bg-[#FAF9F5] dark:bg-[#262624] border border-stone-200 dark:border-[#3A3A36] rounded-2xl w-full max-w-2xl max-h-[85vh] flex flex-col shadow-xl text-xs">
        <div className="flex items-start justify-between gap-3 px-5 pt-5 pb-3">
          <div>
            <h3 className="text-sm font-bold text-stone-900 dark:text-stone-100 flex items-center gap-1.5">
              <Plug className="w-4 h-4 text-[#A50034] dark:text-[#FF5C8D]" /> MCP 서버
              {draft && <span className="font-normal text-stone-500">· {draft.config.id ? "편집" : "추가"}</span>}
            </h3>
            <p className="mt-1 text-stone-500 dark:text-stone-400 leading-relaxed">
              외부 도구 서버를 연결하면 모델이 그 도구를 쓸 수 있습니다. 모든 화면(채팅·프로젝트·코드)에서 쓰입니다.
              {connected > 0 && <> 지금 <b className="text-stone-700 dark:text-stone-200">{connected}개</b> 연결됨.</>}
            </p>
          </div>
          <button onClick={onClose} aria-label="닫기" className="p-1 cursor-pointer">
            <X className="w-4 h-4 text-stone-400" />
          </button>
        </div>

        {!draft && importText === null && (
        <div className="px-5 pb-3 flex gap-1" role="tablist">
          {(
            [
              ["installed", "내 서버"],
              ["browse", "찾아보기"],
            ] as const
          ).map(([value, label]) => (
            <button
              key={value}
              role="tab"
              aria-selected={tab === value}
              onClick={() => setTab(value)}
              className={`px-3 py-1.5 rounded-lg font-semibold cursor-pointer ${
                tab === value
                  ? "bg-stone-800 text-white dark:bg-stone-200 dark:text-stone-900"
                  : "text-stone-600 dark:text-stone-300 hover:bg-stone-200/60 dark:hover:bg-[#2C2C29]"
              }`}
            >
              {label}
            </button>
          ))}
        </div>
        )}

        {!draft && importText === null && tab === "installed" && (
          <div className="px-5 pb-3 flex flex-wrap items-center gap-1.5">
            <button onClick={() => setDraft(draftOf(emptyConfig()))} className={btn}>
              <Plus className="w-3.5 h-3.5" /> 서버 추가
            </button>
            <button onClick={() => setImportText("")} className={btn}>
              <ClipboardPaste className="w-3.5 h-3.5" /> 설정 JSON 붙여넣기
            </button>
          </div>
        )}

        {(error || notice) && (
          <div
            className={`mx-5 mb-3 px-3 py-2 rounded-lg whitespace-pre-wrap ${
              error ? "bg-red-50 dark:bg-red-900/20 text-red-800 dark:text-red-300" : "bg-emerald-50 dark:bg-emerald-900/20 text-emerald-800 dark:text-emerald-300"
            }`}
            role={error ? "alert" : "status"}
          >
            {error || notice}
          </div>
        )}

        <div className="flex-1 min-h-0 overflow-y-auto px-5 pb-4">
          {draft ? (
            form
          ) : importText !== null ? (
            importPanel
          ) : tab === "browse" ? (
            <McpCatalog
              installed={servers ?? []}
              onInstalled={(id, name, needsLogin) => {
                setError("");
                setNotice(`'${name}' 서버를 추가했습니다. 연결 상태와 도구 목록은 여기서 확인하세요.`);
                setOpen((o) => ({ ...o, [id]: true }));
                setTab("installed");
                void refresh();
                // 로그인이 필요한 서버는 바로 브라우저 로그인을 시작한다.
                if (needsLogin) void login(id, name);
              }}
            />
          ) : servers === null ? (
            <div className="py-8 text-center text-stone-500">불러오는 중...</div>
          ) : servers.length === 0 ? (
            <div className="py-6 text-stone-500 dark:text-stone-400 leading-relaxed space-y-1">
              <p>아직 연결한 MCP 서버가 없습니다.</p>
              <p>
                쓰려는 MCP 서버의 안내 문서에 있는 설정 JSON을 [설정 JSON 붙여넣기]로 넣는 것이 가장 쉽습니다. 내 PC에서 실행하는 서버는 대부분 Node.js(npx)나 uv(uvx)가 설치되어 있어야 합니다.
              </p>
            </div>
          ) : (
            <ul className="space-y-1.5">{servers.map(card)}</ul>
          )}
        </div>

        <div className="px-5 py-3 border-t border-stone-200 dark:border-[#34342F] text-[11px] text-stone-500 dark:text-stone-400 flex items-start gap-1.5">
          <AlertTriangle className="w-3.5 h-3.5 mt-px shrink-0 text-amber-600" />
          <span>
            내 PC에서 실행하는 MCP 서버는 이 컴퓨터의 파일과 네트워크에 접근할 수 있는 프로그램입니다. 믿을 수 있는 서버만 추가하세요. 도구는 기본적으로 실행할 때마다 승인을 묻습니다.
          </span>
        </div>
      </div>
    </div>
  );
}
