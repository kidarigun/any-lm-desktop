/**
 * 스크롤이 일어난 요소에 잠시 `data-scrolling` 속성을 붙여 스크롤바를 보이게 한다.
 * 실제 표시·사라짐 애니메이션은 styles.css의 --sb-thumb 전환이 맡는다.
 *
 * class 대신 data 속성을 쓰는 이유: React가 className을 다시 그릴 때 덮어쓰지 않도록.
 */
const HIDE_DELAY_MS = 800;

let installed = false;

export function installAutoHideScrollbars(doc: Document = document): () => void {
  if (installed) return () => {};
  installed = true;
  const timers = new WeakMap<Element, number>();

  const onScroll = (e: Event) => {
    const target = e.target;
    const el =
      target === doc ? doc.scrollingElement : target instanceof Element ? target : null;
    if (!el) return;
    if (!el.hasAttribute("data-scrolling")) el.setAttribute("data-scrolling", "");
    const prev = timers.get(el);
    if (prev !== undefined) window.clearTimeout(prev);
    timers.set(
      el,
      window.setTimeout(() => {
        el.removeAttribute("data-scrolling");
        timers.delete(el);
      }, HIDE_DELAY_MS)
    );
  };

  // scroll 이벤트는 버블링되지 않으므로 캡처 단계에서 모든 스크롤 영역을 한 번에 받는다.
  doc.addEventListener("scroll", onScroll, { capture: true, passive: true });
  return () => {
    doc.removeEventListener("scroll", onScroll, { capture: true });
    installed = false;
  };
}
