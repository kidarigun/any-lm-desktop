// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { LoadBoundary } from "./LoadBoundary";
import { retryableLazy } from "./retryableLazy";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement | null = null;
afterEach(() => {
  container?.remove();
  container = null;
  vi.restoreAllMocks();
});

async function flush() {
  for (let i = 0; i < 5; i++) await act(async () => new Promise((r) => setTimeout(r, 0)));
}

describe("LoadBoundary", () => {
  it("지연 청크를 못 불러오면 안내와 [다시 시도]를 보이고, 다시 시도하면 새로 불러온다", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    let attempts = 0;
    const lazy = retryableLazy<{ label: string }>(() => {
      attempts += 1;
      if (attempts === 1) return Promise.reject(new Error("Failed to fetch dynamically imported module"));
      return Promise.resolve({ default: ({ label }: { label: string }) => React.createElement("p", null, label) });
    });
    container = document.createElement("div");
    document.body.appendChild(container);
    const root = createRoot(container);
    await act(async () => {
      root.render(
        React.createElement(
          "main",
          null,
          React.createElement("h1", null, "채팅"),
          React.createElement(
            LoadBoundary,
            { message: "터미널을 불러오지 못했습니다", onRetry: lazy.reset },
            React.createElement(React.Suspense, { fallback: "불러오는 중" }, React.createElement(lazy.Component, { label: "터미널 준비됨" }))
          )
        )
      );
    });
    await flush();
    // 앱의 나머지는 그대로 남는다(하얀 화면이 아니다)
    expect(container.textContent).toContain("채팅");
    expect(container.textContent).toContain("터미널을 불러오지 못했습니다");
    const button = container.querySelector("button")!;
    expect(button.textContent).toBe("다시 시도");
    await act(async () => button.click());
    await flush();
    expect(attempts).toBe(2);
    expect(container.textContent).toContain("터미널 준비됨");
    expect(container.textContent).not.toContain("불러오지 못했습니다");
    await act(async () => root.unmount());
  });
});
