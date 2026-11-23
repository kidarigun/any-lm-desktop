/** 다시 시도할 수 있는 지연 로딩 컴포넌트 (청크를 못 불러오면 reset 후 새로 import한다) */
import React from "react";

export interface RetryableLazy<P> {
  /** 지금 쓸 지연 컴포넌트 (reset 전까지는 같은 것) */
  get(): React.LazyExoticComponent<React.ComponentType<P>>;
  /** 실패한 import를 버리고 다음 get()에서 다시 불러오게 한다 */
  reset(): void;
  /** 그릴 때마다 get()을 쓰는 고정 컴포넌트 (다시 시도로 새로 마운트되면 새 import를 쓴다) */
  Component: React.ComponentType<P>;
}

/**
 * React.lazy는 한 번 실패한 import 결과를 기억해 다시 그려도 계속 실패한다.
 * reset()으로 새 lazy를 만들어 [다시 시도]가 실제로 청크를 다시 받게 한다.
 */
export function retryableLazy<P>(
  load: () => Promise<{ default: React.ComponentType<P> }>,
  makeLazy: typeof React.lazy = React.lazy
): RetryableLazy<P> {
  let current = makeLazy(load);
  const Component = (props: P) =>
    React.createElement(current as unknown as React.ComponentType<object>, props as object);
  return {
    get: () => current,
    reset: () => {
      current = makeLazy(load);
    },
    Component: Component as React.ComponentType<P>,
  };
}
