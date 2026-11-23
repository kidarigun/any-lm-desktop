import React, { useCallback, useEffect, useState } from "react";
import { X } from "lucide-react";
import { BackendAPI } from "../api";
import { RuntimePanel } from "../runtime/RuntimePanel";
import type { AppSettings, LmModel } from "../types";
import { capabilityBadges, describeModel, loadedLabel, normalizeContext } from "./modelText";
import { isServerOffMessage, smallerContext, smallerContextLabel } from "./sendFlow";

const DEFAULT_CONTEXT = 32768;
const BADGE_TEXT = { vision: "비전", tools: "도구", reasoning: "추론" } as const;

const inputCls =
  "w-full bg-white dark:bg-[#1E1E1C] border border-stone-300 dark:border-[#3A3A36] rounded-xl px-3 py-2 text-stone-900 dark:text-stone-100 focus:outline-none focus:border-[#A50034]";

interface Props {
  settings: AppSettings;
  /** 저장. 실패하면 던진다. */
  onSave: (settings: AppSettings) => Promise<void>;
  onClose: () => void;
  /** 토큰 삭제처럼 대화상자 안에서 설정이 바뀐 경우 */
  onSettingsChanged?: (settings: AppSettings) => void;
}

export function ConnectionDialog({ settings, onSave, onClose, onSettingsChanged }: Props) {
  const [draft, setDraft] = useState<AppSettings>(settings);
  const [running, setRunning] = useState<boolean | null>(null);
  const [models, setModels] = useState<LmModel[]>([]);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** 로드에 실패한 모델과 시도한 컨텍스트 (오류 옆에 [N K로 다시 시도]) */
  const [loadFailed, setLoadFailed] = useState<{ key: string; context: number } | null>(null);

  const refresh = useCallback(async () => {
    let up = false;
    try {
      up = await BackendAPI.serverRunning();
    } catch {
      up = false;
    }
    setRunning(up);
    if (!up) {
      setModels([]);
      return;
    }
    try {
      setModels(await BackendAPI.listModels());
    } catch (err) {
      setLoadFailed(null);
      setError(String(err));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  /** 입력 중인 글자(모델 key별). 없으면 저장된 값을 보여 준다. */
  const [ctxText, setCtxText] = useState<Record<string, string>>({});

  const contextOf = (m: LmModel) => {
    const saved = draft.modelPrefs?.[m.key]?.contextLength;
    const v = saved !== undefined ? normalizeContext(String(saved), m.maxContextLength) : null;
    return v ?? Math.min(DEFAULT_CONTEXT, m.maxContextLength || DEFAULT_CONTEXT);
  };

  /** 입력을 확정한다: 올바른 값이면 저장, 아니면 설정을 지워 기본값으로 돌린다. */
  function commitContext(d: AppSettings, m: LmModel, raw: string): AppSettings {
    const v = normalizeContext(raw, m.maxContextLength);
    const prefs = { ...d.modelPrefs };
    const cur = { ...prefs[m.key] };
    if (v === null) delete cur.contextLength;
    else cur.contextLength = v;
    if (Object.keys(cur).length === 0) delete prefs[m.key];
    else prefs[m.key] = cur;
    return { ...d, modelPrefs: prefs };
  }

  function blurContext(m: LmModel) {
    const raw = ctxText[m.key];
    if (raw === undefined) return;
    setDraft((d) => commitContext(d, m, raw));
    setCtxText(({ [m.key]: _drop, ...rest }) => rest);
  }

  async function startServer() {
    setStarting(true);
    setError(null);
    setLoadFailed(null);
    try {
      await BackendAPI.startServer();
    } catch (err) {
      setError(String(err));
    } finally {
      setStarting(false);
      await refresh();
    }
  }

  async function load(m: LmModel, reload: boolean, context?: number) {
    const raw = ctxText[m.key];
    const ctx =
      context ?? (raw !== undefined ? (normalizeContext(raw, m.maxContextLength) ?? contextOf(m)) : contextOf(m));
    setBusyKey(m.key);
    setError(null);
    setLoadFailed(null);
    try {
      if (reload) {
        for (const inst of m.loaded) await BackendAPI.unloadModel(inst.id);
      }
      await BackendAPI.loadModel(m.key, ctx);
    } catch (err) {
      setError(String(err));
      setLoadFailed({ key: m.key, context: ctx });
    } finally {
      setBusyKey(null);
      await refresh();
    }
  }

  /** [N K로 다시 시도]: 절반 컨텍스트(바닥 4K)를 이 모델의 컨텍스트 칸에 넣고 로드한다([저장]하면 유지된다). */
  async function retrySmaller() {
    if (!loadFailed) return;
    const n = smallerContext(loadFailed.context);
    const m = models.find((x) => x.key === loadFailed.key);
    if (n === null || !m) return;
    setDraft((d) => commitContext(d, m, String(n)));
    setCtxText(({ [m.key]: _drop, ...rest }) => rest);
    await load(m, m.loaded.length > 0, n);
  }

  async function unload(m: LmModel) {
    setBusyKey(m.key);
    setError(null);
    setLoadFailed(null);
    try {
      for (const inst of m.loaded) await BackendAPI.unloadModel(inst.id);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusyKey(null);
      await refresh();
    }
  }

  async function save() {
    let next = draft;
    for (const m of models) {
      const raw = ctxText[m.key];
      if (raw !== undefined) next = commitContext(next, m, raw);
    }
    try {
      await onSave(next);
    } catch (err) {
      setLoadFailed(null);
      setError(String(err));
    }
  }

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-white dark:bg-[#262624] border border-stone-200 dark:border-[#3A3A36] rounded-2xl w-full max-w-xl max-h-[90vh] overflow-y-auto p-6 shadow-xl space-y-4">
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-bold text-stone-900 dark:text-stone-100">LM Studio 연결</h3>
          <button onClick={onClose} aria-label="닫기">
            <X className="w-4 h-4 text-stone-400" />
          </button>
        </div>

        <div className="space-y-3 text-xs">
          <div>
            <label className="font-semibold text-stone-700 dark:text-stone-300 block mb-1">
              LM Studio 서버 주소
            </label>
            <div className="flex items-center gap-2">
              <input
                type="text"
                value={draft.serverUrl}
                onChange={(e) => setDraft({ ...draft, serverUrl: e.target.value })}
                className={`${inputCls} font-mono`}
              />
              <span
                className={`shrink-0 flex items-center gap-1 font-semibold ${
                  running === null
                    ? "text-stone-400"
                    : running
                      ? "text-emerald-700 dark:text-emerald-400"
                      : "text-red-600 dark:text-red-400"
                }`}
              >
                <span
                  className={`w-2 h-2 rounded-full ${
                    running === null ? "bg-stone-400" : running ? "bg-emerald-500" : "bg-red-500"
                  }`}
                />
                {running === null ? "확인 중" : running ? "연결됨" : "꺼져 있음"}
              </span>
              {running === false && (
                <button
                  onClick={startServer}
                  disabled={starting}
                  className="shrink-0 px-3 py-2 rounded-xl bg-[#A50034] text-white font-bold cursor-pointer disabled:opacity-50"
                >
                  {starting ? "시작 중…" : "서버 시작"}
                </button>
              )}
            </div>
            <p className="mt-1 text-[11px] text-stone-500 dark:text-stone-400">
              주소를 바꿨다면 저장한 뒤에 연결 상태가 갱신됩니다.
            </p>
          </div>

          <div>
            <label className="font-semibold text-stone-700 dark:text-stone-300 block mb-1">
              API 토큰 (선택)
            </label>
            <input
              type="password"
              autoComplete="off"
              value={draft.apiKey ?? ""}
              onChange={(e) => setDraft({ ...draft, apiKey: e.target.value })}
              placeholder={
                draft.apiKeySet
                  ? "저장된 토큰 사용 중 — 바꾸려면 새 토큰 입력"
                  : "LM Studio에서 인증을 켠 경우에만 입력"
              }
              className={inputCls}
            />
            <div className="mt-1 flex items-center justify-between text-[11px] text-stone-500 dark:text-stone-400">
              <span>토큰은 Windows 자격 증명 관리자에 암호화되어 저장됩니다.</span>
              {draft.apiKeySet && (
                <button
                  type="button"
                  onClick={async () => {
                    if (!window.confirm("저장된 토큰을 삭제할까요?")) return;
                    try {
                      const saved = await BackendAPI.clearApiKey();
                      setDraft((d) => ({ ...d, apiKey: "", apiKeySet: saved.apiKeySet }));
                      onSettingsChanged?.({ ...saved, apiKey: "" });
                      setError(null);
                    } catch (err) {
                      setError(String(err));
                    }
                  }}
                  className="text-[#A50034] dark:text-[#FF5C8D] font-semibold hover:underline cursor-pointer"
                >
                  저장된 토큰 삭제
                </button>
              )}
            </div>
          </div>

          <div>
            <div className="font-semibold text-stone-700 dark:text-stone-300 mb-1">모델</div>
            {running === false && (
              <p className="text-stone-500 dark:text-stone-400">서버가 꺼져 있어 모델 목록을 볼 수 없습니다.</p>
            )}
            {running && models.length === 0 && (
              <p className="text-stone-500 dark:text-stone-400">LM Studio에 설치된 모델이 없습니다.</p>
            )}
            <div className="space-y-2">
              {models.map((m) => {
                const isLoaded = m.loaded.length > 0;
                const busy = busyKey === m.key;
                return (
                  <div
                    key={m.key}
                    className="rounded-xl border border-stone-200 dark:border-[#3A3A36] p-3 space-y-2"
                  >
                    <div className="flex items-start justify-between gap-2">
                      <label className="flex items-start gap-2 cursor-pointer min-w-0">
                        <input
                          type="radio"
                          name="default-model"
                          checked={draft.model === m.key}
                          onChange={() => setDraft({ ...draft, model: m.key })}
                          className="mt-0.5 accent-[#A50034]"
                        />
                        <span className="min-w-0">
                          <span className="block font-semibold text-stone-900 dark:text-stone-100 truncate">
                            {m.displayName}
                            {draft.model === m.key && (
                              <span className="ml-1.5 text-[10px] font-bold text-[#A50034] dark:text-[#FF5C8D]">
                                기본 모델
                              </span>
                            )}
                          </span>
                          <span className="block text-stone-500 dark:text-stone-400">{describeModel(m)}</span>
                        </span>
                      </label>
                      <div className="shrink-0 flex flex-col items-end gap-1">
                        <span
                          className={`font-semibold ${
                            isLoaded ? "text-emerald-700 dark:text-emerald-400" : "text-stone-500 dark:text-stone-400"
                          }`}
                        >
                          {loadedLabel(m)}
                        </span>
                        <span className="flex gap-1">
                          {capabilityBadges(m).map((b) => (
                            <span
                              key={b}
                              className="px-1.5 py-0.5 rounded-md bg-stone-100 dark:bg-[#32322F] text-[10px] text-stone-600 dark:text-stone-300"
                            >
                              {BADGE_TEXT[b]}
                            </span>
                          ))}
                        </span>
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      <label className="text-stone-600 dark:text-stone-300">컨텍스트</label>
                      <input
                        type="number"
                        min={1024}
                        max={m.maxContextLength}
                        step={1024}
                        value={ctxText[m.key] ?? String(contextOf(m))}
                        onChange={(e) => setCtxText({ ...ctxText, [m.key]: e.target.value })}
                        onBlur={() => blurContext(m)}
                        className="w-28 bg-white dark:bg-[#1E1E1C] border border-stone-300 dark:border-[#3A3A36] rounded-lg px-2 py-1 font-mono text-stone-900 dark:text-stone-100 focus:outline-none focus:border-[#A50034]"
                      />
                      <span className="text-[11px] text-stone-500 dark:text-stone-400">
                        최대 {m.maxContextLength.toLocaleString()}
                      </span>
                      <span className="flex-1" />
                      {busy ? (
                        <span className="text-stone-500 dark:text-stone-400">처리 중…</span>
                      ) : (
                        <>
                          <button
                            onClick={() => load(m, isLoaded)}
                            disabled={busyKey !== null}
                            className="px-3 py-1.5 rounded-lg bg-stone-100 dark:bg-[#32322F] text-stone-700 dark:text-stone-200 font-semibold cursor-pointer disabled:opacity-50"
                          >
                            {isLoaded ? "다시 로드" : "로드"}
                          </button>
                          {isLoaded && (
                            <button
                              onClick={() => unload(m)}
                              disabled={busyKey !== null}
                              className="px-3 py-1.5 rounded-lg bg-stone-100 dark:bg-[#32322F] text-stone-700 dark:text-stone-200 font-semibold cursor-pointer disabled:opacity-50"
                            >
                              언로드
                            </button>
                          )}
                        </>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={draft.autoCompact}
              onChange={(e) => setDraft({ ...draft, autoCompact: e.target.checked })}
              className="accent-[#A50034]"
            />
            <span className="font-semibold text-stone-700 dark:text-stone-300">자동 요약 압축</span>
            <span className="text-[11px] text-stone-500 dark:text-stone-400">
              컨텍스트가 차면 앞부분을 요약해 이어 갑니다.
            </span>
          </label>
        </div>

        <div className="pt-3 border-t border-stone-200 dark:border-[#34342F] text-xs space-y-2">
          <h4 className="font-bold text-stone-900 dark:text-stone-100">실행 환경</h4>
          <RuntimePanel />
        </div>

        {error && (
          <div className="rounded-xl border border-red-200 dark:border-red-900/60 bg-red-50 dark:bg-red-950/40 px-3 py-2 text-xs text-red-700 dark:text-red-300 flex items-center justify-between gap-3">
            <span className="min-w-0 break-words">{error}</span>
            {loadFailed && !isServerOffMessage(error) && smallerContext(loadFailed.context) !== null && (
              <button
                onClick={() => void retrySmaller()}
                disabled={busyKey !== null}
                className="shrink-0 px-2 py-1 rounded-md border border-red-300 dark:border-red-800 font-semibold cursor-pointer disabled:opacity-50"
              >
                {smallerContextLabel(smallerContext(loadFailed.context)!)}
              </button>
            )}
          </div>
        )}
        <div className="flex justify-end gap-2 pt-2">
          <button
            onClick={onClose}
            className="px-4 py-2 rounded-xl bg-stone-100 dark:bg-[#32322F] text-stone-700 dark:text-stone-200 text-xs font-semibold cursor-pointer"
          >
            취소
          </button>
          <button
            onClick={save}
            className="px-4 py-2 rounded-xl bg-[#A50034] text-white text-xs font-bold cursor-pointer"
          >
            저장
          </button>
        </div>
      </div>
    </div>
  );
}
