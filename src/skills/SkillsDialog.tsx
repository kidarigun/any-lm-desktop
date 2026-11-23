import { useCallback, useEffect, useState } from "react";
import {
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  FileArchive,
  FolderOpen,
  FolderPlus,
  NotebookPen,
  Plus,
  RefreshCw,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { BackendAPI } from "../api";
import type { SkillInfo } from "../types";
import { SkillCatalog } from "../catalog/CatalogViews";

const ALREADY_EXISTS = "ALREADY_EXISTS:";

function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** 설치된 Skill을 보고, 추가·끄기·삭제하는 창 */
export function SkillsDialog({
  projectId,
  projectName,
  autoLoad,
  onAutoLoadChange,
  onClose,
}: {
  /** 프로젝트·코드 화면이면 그 프로젝트 (프로젝트 Skill도 보여 준다) */
  projectId?: string;
  projectName?: string;
  /** 설정 "Skill 자동 불러오기" (요청에 맞는 Skill을 앱이 골라 미리 넣는다) */
  autoLoad: boolean;
  onAutoLoadChange: (on: boolean) => Promise<void>;
  onClose: () => void;
}) {
  const [skills, setSkills] = useState<SkillInfo[] | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const [newName, setNewName] = useState<string | null>(null);
  const [openId, setOpenId] = useState<string | null>(null);
  const [preview, setPreview] = useState<Record<string, string>>({});
  const [tab, setTab] = useState<"installed" | "browse" | null>(null);

  const refresh = useCallback(async () => {
    try {
      const list = await BackendAPI.listSkills(projectId);
      setSkills(list);
      // 처음 열 때 설치한 Skill이 없으면 찾아보기부터 보여 준다.
      setTab((t) => t ?? (list.length === 0 ? "browse" : "installed"));
    } catch (e) {
      setSkills([]);
      setError(errorText(e));
    }
  }, [projectId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

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

  const install = (kind: "folder" | "zip") =>
    run(async () => {
      const src = await BackendAPI.pickSkillSource(kind);
      if (!src) return;
      let info: SkillInfo;
      try {
        info = await BackendAPI.installSkill(src, false);
      } catch (e) {
        const msg = errorText(e);
        if (!msg.startsWith(ALREADY_EXISTS)) throw e;
        const name = msg.slice(ALREADY_EXISTS.length);
        if (!window.confirm(`'${name}' Skill이 이미 있습니다. 새 것으로 바꿀까요?\n(기존 Skill은 휴지통으로 옮겨집니다.)`)) return;
        info = await BackendAPI.installSkill(src, true);
      }
      setNotice(`'${info.name}' Skill을 설치했습니다.`);
    });

  const create = () =>
    run(async () => {
      const name = (newName ?? "").trim();
      if (!name) return;
      const info = await BackendAPI.createSkill(name);
      setNewName(null);
      setNotice(`'${info.name}' Skill 틀을 만들고 메모장으로 열었습니다. 설명(description)과 지침을 채운 뒤 저장하세요.`);
      await BackendAPI.openSkillLocation("edit", info.id);
    });

  const toggle = (s: SkillInfo) =>
    run(async () => {
      await BackendAPI.setSkillEnabled(s.id, !s.enabled);
    });

  const remove = (s: SkillInfo) =>
    run(async () => {
      if (!window.confirm(`'${s.name}' Skill을 삭제할까요?\n(폴더는 휴지통으로 옮겨집니다.)`)) return;
      await BackendAPI.deleteSkill(s.id);
      setNotice(`'${s.name}' Skill을 휴지통으로 옮겼습니다.`);
    });

  async function togglePreview(s: SkillInfo) {
    if (openId === s.id) {
      setOpenId(null);
      return;
    }
    setOpenId(s.id);
    if (preview[s.id] === undefined) {
      try {
        const text = await BackendAPI.skillText(s.id, projectId);
        setPreview((p) => ({ ...p, [s.id]: text }));
      } catch (e) {
        setPreview((p) => ({ ...p, [s.id]: `(읽지 못했습니다: ${errorText(e)})` }));
      }
    }
  }

  const open = (target: "folder" | "edit", s?: SkillInfo) =>
    BackendAPI.openSkillLocation(target, s?.id, projectId).catch((e) => setError(errorText(e)));

  const projectSkills = (skills ?? []).filter((s) => s.scope === "project");
  const userSkills = (skills ?? []).filter((s) => s.scope === "user");
  const activeCount = (skills ?? []).filter((s) => s.enabled && !s.error && !s.shadowed).length;

  const row = (s: SkillInfo) => {
    const usable = s.enabled && !s.error && !s.shadowed;
    const expanded = openId === s.id;
    return (
      <li key={s.id} className="rounded-xl border border-stone-200 dark:border-[#383834] bg-white dark:bg-[#1E1E1C]" data-testid="skill-row">
        <div className="flex items-start gap-2.5 px-3 py-2.5">
          <button
            role="switch"
            aria-checked={s.enabled}
            aria-label={`${s.name} ${s.enabled ? "끄기" : "켜기"}`}
            onClick={() => toggle(s)}
            disabled={busy}
            className={`mt-0.5 shrink-0 w-8 h-[18px] rounded-full relative transition cursor-pointer disabled:opacity-50 ${
              s.enabled ? "bg-[#A50034]" : "bg-stone-300 dark:bg-[#44443F]"
            }`}
          >
            <span
              className={`absolute top-[2px] w-[14px] h-[14px] rounded-full bg-white shadow transition-all ${
                s.enabled ? "left-[16px]" : "left-[2px]"
              }`}
            />
          </button>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5 flex-wrap">
              <button
                onClick={() => togglePreview(s)}
                className="flex items-center gap-0.5 font-semibold text-stone-900 dark:text-stone-100 hover:underline cursor-pointer text-left"
                title="SKILL.md 내용 보기"
              >
                {expanded ? <ChevronDown className="w-3.5 h-3.5 shrink-0" /> : <ChevronRight className="w-3.5 h-3.5 shrink-0" />}
                <span className="break-all">{s.name}</span>
              </button>
              {s.files.length > 0 && (
                <span className="px-1.5 py-px rounded bg-stone-100 dark:bg-[#2C2C29] text-[10px] text-stone-500 dark:text-stone-400">
                  파일 {s.files.length}개{s.files.some((f) => f.toLowerCase().endsWith(".py")) ? " · 스크립트 포함" : ""}
                </span>
              )}
              {s.shadowed && (
                <span className="px-1.5 py-px rounded bg-amber-100 dark:bg-amber-900/30 text-[10px] text-amber-800 dark:text-amber-300">
                  같은 이름의 프로젝트 Skill이 대신 쓰임
                </span>
              )}
              {usable && <span className="text-[10px] text-emerald-700 dark:text-emerald-400">사용 중</span>}
            </div>
            {s.error ? (
              <div className="mt-0.5 flex items-start gap-1 text-[11px] text-red-700 dark:text-red-400">
                <AlertTriangle className="w-3 h-3 mt-0.5 shrink-0" />
                <span>{s.error}</span>
              </div>
            ) : (
              <p className="mt-0.5 text-[11px] text-stone-600 dark:text-stone-400 line-clamp-2" title={s.description}>
                {s.description}
              </p>
            )}
            <div className="mt-0.5 text-[10px] text-stone-400 dark:text-stone-500 font-mono truncate" title={s.location}>
              {s.location}
            </div>
          </div>
          <div className="flex items-center gap-0.5 shrink-0">
            <button onClick={() => open("edit", s)} title="SKILL.md를 메모장으로 편집" className="p-1.5 rounded-lg hover:bg-stone-100 dark:hover:bg-[#2C2C29] text-stone-500 cursor-pointer">
              <NotebookPen className="w-3.5 h-3.5" />
            </button>
            <button onClick={() => open("folder", s)} title="폴더 열기" className="p-1.5 rounded-lg hover:bg-stone-100 dark:hover:bg-[#2C2C29] text-stone-500 cursor-pointer">
              <FolderOpen className="w-3.5 h-3.5" />
            </button>
            {s.scope === "user" && (
              <button onClick={() => remove(s)} disabled={busy} title="삭제 (휴지통으로)" className="p-1.5 rounded-lg hover:bg-red-50 dark:hover:bg-red-900/20 text-stone-500 hover:text-red-600 cursor-pointer disabled:opacity-50">
                <Trash2 className="w-3.5 h-3.5" />
              </button>
            )}
          </div>
        </div>
        {expanded && (
          <pre className="mx-3 mb-3 max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-[#F6F5F0] dark:bg-[#171715] border border-stone-200 dark:border-[#34342F] px-3 py-2 text-[11px] text-stone-700 dark:text-stone-300 font-mono">
            {preview[s.id] ?? "읽는 중..."}
          </pre>
        )}
      </li>
    );
  };

  const btn =
    "px-2.5 py-1.5 rounded-lg border border-stone-300 dark:border-[#3A3A36] bg-white dark:bg-[#1E1E1C] hover:bg-stone-50 dark:hover:bg-[#2A2A27] text-stone-700 dark:text-stone-200 font-medium flex items-center gap-1.5 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed";

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div role="dialog" aria-label="Skill" className="bg-[#FAF9F5] dark:bg-[#262624] border border-stone-200 dark:border-[#3A3A36] rounded-2xl w-full max-w-2xl max-h-[85vh] flex flex-col shadow-xl text-xs">
        <div className="flex items-start justify-between gap-3 px-5 pt-5 pb-3">
          <div>
            <h3 className="text-sm font-bold text-stone-900 dark:text-stone-100 flex items-center gap-1.5">
              <Sparkles className="w-4 h-4 text-[#A50034] dark:text-[#FF5C8D]" /> Skill
            </h3>
            <p className="mt-1 text-stone-500 dark:text-stone-400 leading-relaxed">
              모델이 요청에 맞는 Skill을 골라 그 지침대로 작업합니다. 켜 둔 Skill은 이름과 설명만 먼저 전달되고, 필요할 때 본문을 읽습니다.
              {activeCount > 0 && <> 지금 <b className="text-stone-700 dark:text-stone-200">{activeCount}개</b> 사용 중.</>}
            </p>
          </div>
          <button onClick={onClose} aria-label="닫기" className="p-1 cursor-pointer">
            <X className="w-4 h-4 text-stone-400" />
          </button>
        </div>

        <div className="px-5 pb-3 flex gap-1" role="tablist">
          {(
            [
              ["installed", "설치됨"],
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

        {tab === "installed" && (
        <div className="px-5 pb-3 flex flex-wrap items-center gap-1.5">
          <button onClick={() => install("folder")} disabled={busy} className={btn}>
            <FolderPlus className="w-3.5 h-3.5" /> 폴더에서 추가
          </button>
          <button onClick={() => install("zip")} disabled={busy} className={btn}>
            <FileArchive className="w-3.5 h-3.5" /> .zip / .skill 파일에서 추가
          </button>
          <button onClick={() => setNewName(newName === null ? "" : null)} disabled={busy} className={btn}>
            <Plus className="w-3.5 h-3.5" /> 새로 만들기
          </button>
          <div className="flex-1" />
          <button onClick={() => open("folder")} title="모든 대화용 Skill 폴더 열기" className="p-1.5 rounded-lg hover:bg-stone-200/60 dark:hover:bg-[#2C2C29] text-stone-500 cursor-pointer">
            <FolderOpen className="w-3.5 h-3.5" />
          </button>
          <button onClick={() => run(async () => {})} disabled={busy} title="새로 고침" className="p-1.5 rounded-lg hover:bg-stone-200/60 dark:hover:bg-[#2C2C29] text-stone-500 cursor-pointer disabled:opacity-50">
            <RefreshCw className={`w-3.5 h-3.5 ${busy ? "animate-spin" : ""}`} />
          </button>
        </div>
        )}

        {tab === "installed" && (
          <div className="mx-5 mb-3 flex items-start gap-2.5 rounded-xl border border-stone-200 dark:border-[#383834] bg-white dark:bg-[#1E1E1C] px-3 py-2.5">
            <button
              role="switch"
              aria-checked={autoLoad}
              aria-label={`Skill 자동 불러오기 ${autoLoad ? "끄기" : "켜기"}`}
              onClick={() => run(() => onAutoLoadChange(!autoLoad))}
              disabled={busy}
              className={`mt-0.5 shrink-0 w-8 h-[18px] rounded-full relative transition cursor-pointer disabled:opacity-50 ${
                autoLoad ? "bg-[#A50034]" : "bg-stone-300 dark:bg-[#44443F]"
              }`}
            >
              <span
                className={`absolute top-[2px] w-[14px] h-[14px] rounded-full bg-white shadow transition-all ${
                  autoLoad ? "left-[16px]" : "left-[2px]"
                }`}
              />
            </button>
            <div className="min-w-0">
              <div className="font-semibold text-stone-900 dark:text-stone-100">Skill 자동 불러오기</div>
              <p className="mt-0.5 text-[11px] text-stone-600 dark:text-stone-400 leading-relaxed">
                요청에 Skill 이름(/이름)이 있거나 내용이 Skill 설명과 잘 맞으면, 앱이 그 Skill의 지침을 미리 넣어 보냅니다. 작은 모델이 Skill을 직접 불러오지 않을 때 도움이 됩니다. 답변에 "Skill 자동 불러옴" 카드가 보입니다.
              </p>
            </div>
          </div>
        )}

        {tab === "installed" && newName !== null && (
          <form
            className="mx-5 mb-3 flex items-center gap-1.5"
            onSubmit={(e) => {
              e.preventDefault();
              void create();
            }}
          >
            <input
              autoFocus
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder="Skill 이름 (예: 주간 보고서)"
              maxLength={64}
              className="flex-1 bg-white dark:bg-[#1E1E1C] border border-stone-300 dark:border-[#3A3A36] rounded-lg px-2.5 py-1.5 text-stone-900 dark:text-stone-100 focus:outline-none focus:border-[#A50034]"
            />
            <button type="submit" disabled={busy || !newName.trim()} className="px-3 py-1.5 rounded-lg bg-[#A50034] text-white font-semibold cursor-pointer disabled:opacity-40">
              만들기
            </button>
          </form>
        )}

        {(error || notice) && (
          <div
            className={`mx-5 mb-3 px-3 py-2 rounded-lg ${
              error
                ? "bg-red-50 dark:bg-red-900/20 text-red-800 dark:text-red-300"
                : "bg-emerald-50 dark:bg-emerald-900/20 text-emerald-800 dark:text-emerald-300"
            }`}
            role={error ? "alert" : "status"}
          >
            {error || notice}
          </div>
        )}

        <div className="flex-1 min-h-0 overflow-y-auto px-5 pb-4 space-y-4">
          {tab === "browse" ? (
            <SkillCatalog
              installed={skills ?? []}
              onInstalled={(name) => {
                setError("");
                setNotice(`'${name}' Skill을 설치했습니다. 이제 모델이 필요할 때 이 Skill을 씁니다.`);
                void refresh();
              }}
            />
          ) : skills === null ? (
            <div className="py-8 text-center text-stone-500">불러오는 중...</div>
          ) : (
            <>
              {projectId && (
                <section>
                  <h4 className="mb-1.5 font-semibold text-stone-700 dark:text-stone-300">
                    이 프로젝트{projectName ? ` (${projectName})` : ""}
                  </h4>
                  {projectSkills.length > 0 ? (
                    <ul className="space-y-1.5">{projectSkills.map(row)}</ul>
                  ) : (
                    <p className="text-stone-500 dark:text-stone-400 leading-relaxed">
                      프로젝트의 <code className="font-mono">.skills\&lt;이름&gt;\SKILL.md</code>에 둔 Skill은 이 프로젝트 대화에서만 쓰입니다. 모델에게 "○○ Skill을 만들어 줘"라고 요청해도 됩니다.
                    </p>
                  )}
                </section>
              )}
              <section>
                <h4 className="mb-1.5 font-semibold text-stone-700 dark:text-stone-300">모든 대화</h4>
                {userSkills.length > 0 ? (
                  <ul className="space-y-1.5">{userSkills.map(row)}</ul>
                ) : (
                  <p className="text-stone-500 dark:text-stone-400 leading-relaxed">
                    아직 설치한 Skill이 없습니다. 내려받은 Skill 폴더나 .zip/.skill 파일을 추가하거나, [새로 만들기]로 직접 지침을 적어 보세요.
                  </p>
                )}
              </section>
            </>
          )}
        </div>

        <div className="px-5 py-3 border-t border-stone-200 dark:border-[#34342F] text-[11px] text-stone-500 dark:text-stone-400 flex items-start gap-1.5">
          <AlertTriangle className="w-3.5 h-3.5 mt-px shrink-0 text-amber-600" />
          <span>
            Skill 안의 스크립트는 실행하기 전에 항상 승인을 묻습니다. 믿을 수 있는 곳에서 받은 Skill만 설치하세요. 스크립트 실행은 프로젝트 화면에서만 됩니다.
          </span>
        </div>
      </div>
    </div>
  );
}
