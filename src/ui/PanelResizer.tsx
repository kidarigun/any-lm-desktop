import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent, type RefObject } from "react";

export interface PanelResizerProps {
  /** 폭을 바꿀 패널 (화면 오른쪽에 붙은 패널: 손잡이를 왼쪽으로 끌면 넓어진다) */
  target: RefObject<HTMLElement | null>;
  width: number;
  min: number;
  max: number;
  /** 끌기를 마쳤거나 키보드로 바꿨을 때 최종 폭 */
  onChange: (width: number) => void;
  /** 두 번 클릭(또는 Enter): 기본 폭으로 */
  onReset: () => void;
  label?: string;
}

const KEY_STEP = 16;

/**
 * 채팅창과 캔버스 사이의 경계선 손잡이.
 * 끄는 동안에는 React 상태를 바꾸지 않고 패널 폭만 직접 바꿔 앱 전체가 다시 그려지지 않게 한다.
 */
export function PanelResizer({ target, width, min, max, onChange, onReset, label = "캔버스 폭 조절" }: PanelResizerProps) {
  const [active, setActive] = useState(false);
  const stopRef = useRef<(() => void) | null>(null);
  // 최신 값을 끌기 처리기에서 쓰기 위해 보관한다.
  const latest = useRef({ min, max, onChange });
  latest.current = { min, max, onChange };

  // 끄는 도중에 패널이 닫히면 정리한다.
  useEffect(() => () => stopRef.current?.(), []);

  function onPointerDown(e: PointerEvent<HTMLDivElement>) {
    const panel = target.current;
    if (e.button !== 0 || !panel || stopRef.current) return;
    e.preventDefault();
    const handle = e.currentTarget;
    const pointerId = e.pointerId;
    try {
      handle.setPointerCapture(pointerId);
    } catch {
      // 캡처하지 못해도 아래 window 리스너로 동작한다.
    }
    // 캔버스 안의 iframe(HTML 미리보기 등)이 포인터를 가져가지 않도록 화면 전체를 덮는다.
    const overlay = document.createElement("div");
    overlay.style.cssText = "position:fixed;inset:0;z-index:9999;cursor:col-resize;";
    document.body.appendChild(overlay);
    document.body.style.userSelect = "none";
    panel.style.transition = "none";

    const startX = e.clientX;
    const startWidth = panel.getBoundingClientRect().width;
    let last = startWidth;

    const move = (ev: globalThis.PointerEvent) => {
      if (ev.pointerId !== pointerId) return;
      const { min: lo, max: hi } = latest.current;
      const w = Math.round(Math.min(Math.max(startWidth + (startX - ev.clientX), lo), Math.max(lo, hi)));
      if (w === last) return;
      last = w;
      panel.style.width = `${w}px`;
    };
    // 놓기 이벤트를 어디서 받든(손잡이·창 밖·캡처 해제·창 전환) 반드시 끝낸다.
    const stop = () => {
      if (stopRef.current !== stop) return;
      stopRef.current = null;
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", stop, true);
      window.removeEventListener("pointercancel", stop, true);
      window.removeEventListener("blur", stop);
      handle.removeEventListener("lostpointercapture", stop);
      try {
        if (handle.hasPointerCapture(pointerId)) handle.releasePointerCapture(pointerId);
      } catch {
        // 이미 해제됨
      }
      overlay.remove();
      document.body.style.userSelect = "";
      panel.style.transition = "";
      setActive(false);
      if (Math.round(last) !== Math.round(startWidth)) latest.current.onChange(last);
    };
    stopRef.current = stop;
    window.addEventListener("pointermove", move, true);
    window.addEventListener("pointerup", stop, true);
    window.addEventListener("pointercancel", stop, true);
    window.addEventListener("blur", stop);
    handle.addEventListener("lostpointercapture", stop);
    setActive(true);
  }

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    const step = e.shiftKey ? KEY_STEP * 4 : KEY_STEP;
    let next: number | null = null;
    if (e.key === "ArrowLeft") next = width + step;
    else if (e.key === "ArrowRight") next = width - step;
    else if (e.key === "Home") next = max;
    else if (e.key === "End") next = min;
    else if (e.key === "Enter") {
      e.preventDefault();
      onReset();
      return;
    }
    if (next === null) return;
    e.preventDefault();
    onChange(Math.round(Math.min(Math.max(next, min), Math.max(min, max))));
  }

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={Math.round(width)}
      aria-valuemin={min}
      aria-valuemax={Math.max(min, max)}
      tabIndex={0}
      title="끌어서 폭 조절 · 두 번 클릭하면 기본 폭"
      onPointerDown={onPointerDown}
      onDoubleClick={onReset}
      onKeyDown={onKeyDown}
      className="group absolute inset-y-0 -left-[4px] w-[8px] z-20 cursor-col-resize touch-none outline-none"
    >
      <div
        className={`mx-auto h-full w-[2px] transition-colors duration-150 ${
          active
            ? "bg-[#A50034] dark:bg-[#FF5C8D]"
            : "bg-transparent group-hover:bg-[#A50034]/50 dark:group-hover:bg-[#FF5C8D]/50 group-focus-visible:bg-[#A50034] dark:group-focus-visible:bg-[#FF5C8D]"
        }`}
      />
    </div>
  );
}
