import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import "@xterm/xterm/css/xterm.css";
import { TerminalAPI, type TerminalOutput } from "./terminalApi";
import { canTakeFocus, terminalKeyAction } from "./panelState";
import { terminalTheme } from "./xtermTheme";
import { OutputFlow } from "./outputFlow";

export const TERMINAL_SCROLLBACK = 5000;
/** 크기 변경을 모았다가 백엔드에 알리는 간격 */
const RESIZE_DEBOUNCE_MS = 80;

export interface TerminalViewProps {
  sessionId: string;
  /** 선택된 탭인지 (선택될 때 크기를 맞추고 포커스) */
  active: boolean;
  isDark: boolean;
  /** 출력 속 링크를 눌렀을 때 (T1: 기본 브라우저, T2: localhost는 미리보기) */
  onLink: (url: string) => void;
}

/**
 * 터미널 세션 하나를 그리는 xterm 화면.
 * 처음 그릴 때 링 버퍼 스냅샷으로 화면을 복원하고, 스냅샷 번호(`seq`)보다 큰 출력 이벤트만 이어 쓴다.
 */
export function TerminalView({ sessionId, active, isDark, onLink }: TerminalViewProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<(() => void) | null>(null);
  const onLinkRef = useRef(onLink);
  onLinkRef.current = onLink;
  const activeRef = useRef(active);
  activeRef.current = active;

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    let disposed = false;
    const term = new Terminal({
      scrollback: TERMINAL_SCROLLBACK,
      fontFamily: '"Cascadia Mono", "Cascadia Code", Consolas, "D2Coding", "Malgun Gothic", monospace',
      fontSize: 13,
      lineHeight: 1.15,
      cursorBlink: true,
      theme: terminalTheme(isDark),
      allowProposedApi: false,
    });
    termRef.current = term;
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.loadAddon(new WebLinksAddon((_e, uri) => onLinkRef.current(uri)));
    term.open(host);

    // 복사(선택이 있을 때 Ctrl+C), 붙여넣기(Ctrl+V는 브라우저 붙여넣기 → xterm), Ctrl+`는 앱 단축키로
    term.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown") return true;
      const action = terminalKeyAction(e, term.hasSelection());
      if (action === "copy") {
        const text = term.getSelection();
        void navigator.clipboard?.writeText(text).catch(() => {});
        term.clearSelection();
        e.preventDefault();
        return false;
      }
      return action === "terminal";
    });

    let lastCols = 0;
    let lastRows = 0;
    const fitNow = () => {
      if (disposed || host.offsetWidth === 0 || host.offsetHeight === 0) return;
      try {
        fit.fit();
      } catch {
        return;
      }
      if (term.cols !== lastCols || term.rows !== lastRows) {
        lastCols = term.cols;
        lastRows = term.rows;
        void TerminalAPI.resize(sessionId, term.cols, term.rows).catch(() => {});
      }
    };
    fitRef.current = fitNow;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const observer = new ResizeObserver(() => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(fitNow, RESIZE_DEBOUNCE_MS);
    });
    observer.observe(host);

    // 스냅샷을 다시 그리는 동안 xterm이 옛 출력 속 질의(커서 위치 등)에 답한 것은 셸로 보내지 않는다.
    let replaying = false;
    const input = term.onData((data) => {
      if (replaying) return;
      void TerminalAPI.write(sessionId, data).catch(() => {});
    });

    // 출력이 쏟아져도 xterm 쓰기 버퍼가 끝없이 커지지 않게 쓰기 완료 콜백으로 흐름을 조절한다.
    const flow = new OutputFlow((data, done) => term.write(data, done));
    // 스냅샷을 받기 전에 온 이벤트는 모아 두었다가 스냅샷 뒤의 것만 쓴다.
    let lastSeq: number | null = null;
    const queued: TerminalOutput[] = [];
    let unlisten: (() => void) | null = null;
    void (async () => {
      const off = await TerminalAPI.onOutput((p) => {
        if (p.id !== sessionId) return;
        if (lastSeq === null) {
          queued.push(p);
        } else if (p.seq > lastSeq) {
          lastSeq = p.seq;
          flow.push(p.data);
        }
      });
      if (disposed) {
        off();
        return;
      }
      unlisten = off;
      try {
        const snap = await TerminalAPI.snapshot(sessionId);
        if (disposed) return;
        if (snap.data) {
          replaying = true;
          term.write(snap.data, () => {
            replaying = false;
          });
        }
        lastSeq = snap.seq;
      } catch {
        lastSeq = 0;
      }
      for (const p of queued) {
        if (p.seq > (lastSeq ?? 0)) {
          lastSeq = p.seq;
          flow.push(p.data);
        }
      }
      queued.length = 0;
      fitNow();
      // 대화상자에서 입력 중이면 포커스를 빼앗지 않는다.
      if (activeRef.current && canTakeFocus(document.activeElement)) term.focus();
    })();

    return () => {
      disposed = true;
      if (timer) clearTimeout(timer);
      observer.disconnect();
      input.dispose();
      flow.dispose();
      unlisten?.();
      fitRef.current = null;
      termRef.current = null;
      term.dispose();
    };
    // 세션마다 한 번만 만든다(테마는 아래에서 바꾼다).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId]);

  useEffect(() => {
    if (termRef.current) termRef.current.options.theme = terminalTheme(isDark);
  }, [isDark]);

  useEffect(() => {
    if (!active) return;
    // 숨겨져 있던 탭은 보이고 나서 크기를 맞춘다.
    const id = requestAnimationFrame(() => {
      fitRef.current?.();
      if (canTakeFocus(document.activeElement)) termRef.current?.focus();
    });
    return () => cancelAnimationFrame(id);
  }, [active]);

  return (
    <div className="h-full w-full pl-2 pt-1.5 pb-1">
      <div ref={hostRef} className="h-full w-full overflow-hidden" />
    </div>
  );
}
