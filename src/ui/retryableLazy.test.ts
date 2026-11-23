import { describe, expect, it, vi } from "vitest";
import { retryableLazy } from "./retryableLazy";

describe("retryableLazy", () => {
  it("keeps the same lazy component until reset", () => {
    const makeLazy = vi.fn((_load: unknown) => ({ id: makeLazy.mock.calls.length }) as never);
    const load = vi.fn();
    const lazy = retryableLazy(load as never, makeLazy as never);
    const first = lazy.get();
    expect(lazy.get()).toBe(first);
    expect(makeLazy).toHaveBeenCalledTimes(1);
    lazy.reset();
    const second = lazy.get();
    expect(second).not.toBe(first);
    expect(makeLazy).toHaveBeenCalledTimes(2);
    // 같은 import 함수로 다시 만든다(다음 렌더에서 청크를 새로 받는다)
    expect(makeLazy.mock.calls[1][0]).toBe(load);
    expect(load).not.toHaveBeenCalled();
  });

  it("re-runs the import after a failed load once reset", async () => {
    let attempts = 0;
    const load = () => {
      attempts += 1;
      return attempts === 1 ? Promise.reject(new Error("chunk")) : Promise.resolve({ default: () => null });
    };
    // React.lazy 대신 첫 렌더 때 import를 부르는 흉내
    const makeLazy = ((f: () => Promise<unknown>) => ({ run: f })) as never;
    const lazy = retryableLazy(load as never, makeLazy);
    await expect((lazy.get() as unknown as { run: () => Promise<unknown> }).run()).rejects.toThrow("chunk");
    lazy.reset();
    await expect((lazy.get() as unknown as { run: () => Promise<unknown> }).run()).resolves.toBeTruthy();
    expect(attempts).toBe(2);
  });
});

describe("retryableLazy.Component", () => {
  it("renders whichever lazy component is current", () => {
    let n = 0;
    const makeLazy = (() => {
      n += 1;
      return `lazy-${n}`;
    }) as never;
    const lazy = retryableLazy(() => Promise.resolve({ default: () => null }), makeLazy);
    const C = lazy.Component as unknown as (p: object) => { type: unknown };
    expect(C({}).type).toBe("lazy-1");
    lazy.reset();
    expect(C({}).type).toBe("lazy-2");
  });
});
