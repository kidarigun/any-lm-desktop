import { useEffect, useRef, useState, type KeyboardEvent, type RefObject } from "react";
import { FileIcon } from "./fileIcons";

/** 커서 바로 앞의 "@검색어"를 찾는다. 이메일 주소처럼 글자 뒤에 붙은 @는 무시한다. */
export function mentionAt(text: string, caret: number): { start: number; query: string } | null {
  const m = /(^|\s)@([^\s@]*)$/.exec(text.slice(0, caret));
  if (!m) return null;
  return { start: caret - m[2].length - 1, query: m[2] };
}

export interface FileMentionOptions {
  /** null이면 멘션을 쓰지 않는다 (일반 채팅) */
  search: ((query: string) => Promise<string[]>) | null;
  value: string;
  setValue: (v: string) => void;
  textareaRef: RefObject<HTMLTextAreaElement | null>;
  onPick: (rel: string) => void;
}

/** 채팅 입력창에서 "@"를 입력하면 프로젝트 파일을 골라 첨부하게 한다. */
export function useFileMention({ search, value, setValue, textareaRef, onPick }: FileMentionOptions) {
  const [mention, setMention] = useState<{ start: number; query: string } | null>(null);
  const [items, setItems] = useState<string[]>([]);
  const [active, setActive] = useState(0);
  const seq = useRef(0);

  function sync() {
    const el = textareaRef.current;
    if (!el || !search) return setMention(null);
    setMention(mentionAt(el.value, el.selectionStart ?? el.value.length));
  }

  useEffect(() => {
    if (!mention || !search) {
      setItems([]);
      return;
    }
    const id = ++seq.current;
    const t = setTimeout(() => {
      search(mention.query)
        .then((r) => {
          if (id !== seq.current) return;
          setItems(r.slice(0, 12));
          setActive(0);
        })
        .catch(() => setItems([]));
    }, 100);
    return () => clearTimeout(t);
  }, [mention?.query, mention?.start, search]);

  function pick(rel: string) {
    const el = textareaRef.current;
    if (!mention || !el) return;
    const caret = el.selectionStart ?? value.length;
    // "@검색어"는 지우고 파일은 첨부 목록에 넣는다.
    const next = value.slice(0, mention.start) + value.slice(caret);
    setValue(next);
    onPick(rel);
    setMention(null);
    requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(mention.start, mention.start);
    });
  }

  /** 입력창 onKeyDown에서 먼저 호출한다. 멘션 목록이 처리했으면 true. */
  function handleKeyDown(e: KeyboardEvent<HTMLTextAreaElement>): boolean {
    // 한글 입력 조합 중의 Enter는 글자를 확정하는 키이므로 건드리지 않는다.
    if (e.nativeEvent.isComposing) return false;
    if (!mention || items.length === 0) {
      if (mention && e.key === "Escape") {
        setMention(null);
        return true;
      }
      return false;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => (a + (e.key === "ArrowDown" ? 1 : items.length - 1)) % items.length);
      return true;
    }
    if (e.key === "Enter" || e.key === "Tab") {
      e.preventDefault();
      pick(items[active]);
      return true;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      setMention(null);
      return true;
    }
    return false;
  }

  const popup =
    mention && search ? (
      <div className="absolute left-3 right-3 bottom-full mb-2 z-30 rounded-xl border border-stone-200 dark:border-[#3A3A36] bg-white dark:bg-[#262624] shadow-lg py-1 max-h-64 overflow-y-auto text-xs">
        <div className="px-3 py-1 text-stone-400">
          파일 첨부 {mention.query ? `· "${mention.query}"` : "· 이름을 입력해 찾기"}
        </div>
        {items.length === 0 ? (
          <div className="px-3 py-1.5 text-stone-400">일치하는 파일이 없습니다.</div>
        ) : (
          items.map((rel, i) => {
            const name = rel.slice(rel.lastIndexOf("/") + 1);
            const dir = rel.slice(0, Math.max(0, rel.lastIndexOf("/")));
            return (
              <button
                key={rel}
                onMouseDown={(e) => {
                  e.preventDefault();
                  pick(rel);
                }}
                onMouseEnter={() => setActive(i)}
                className={`w-full flex items-center gap-2 px-3 py-1.5 text-left cursor-pointer ${
                  i === active ? "bg-[#A50034]/10 dark:bg-[#FF5C8D]/15" : ""
                }`}
              >
                <FileIcon name={name} isDir={false} />
                <span className="font-medium text-stone-800 dark:text-stone-100 truncate">{name}</span>
                <span className="text-stone-400 truncate">{dir}</span>
              </button>
            );
          })
        )}
      </div>
    ) : null;

  return { sync, handleKeyDown, popup, close: () => setMention(null) };
}
