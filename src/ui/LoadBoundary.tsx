/** 지연 로딩한 화면 조각이 실패해도 앱 전체가 하얗게 되지 않도록 그 자리에 안내와 [다시 시도]를 보여 준다. */
import React from "react";

interface LoadBoundaryProps {
  /** 실패했을 때 보여 줄 문장 (예: "터미널을 불러오지 못했습니다") */
  message: string;
  /** [다시 시도]를 누르면 먼저 부른다 (예: 지연 import 초기화) */
  onRetry?: () => void;
  /** 실패 안내를 감쌀 클래스 (자리 폭·배경을 맞출 때) */
  className?: string;
  style?: React.CSSProperties;
  children?: React.ReactNode;
}

interface LoadBoundaryState {
  error: Error | null;
}

export class LoadBoundary extends React.Component<LoadBoundaryProps, LoadBoundaryState> {
  state: LoadBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): LoadBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error) {
    console.error(this.props.message, error);
  }

  private retry = () => {
    this.props.onRetry?.();
    this.setState({ error: null });
  };

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div role="alert" style={this.props.style} className={this.props.className ?? "p-4 text-xs"}>
        <p className="text-stone-600 dark:text-stone-300">{this.props.message}</p>
        <button
          type="button"
          onClick={this.retry}
          className="mt-2 rounded-md border border-[#E2E0D8] dark:border-[#3A3A36] px-2.5 py-1 text-xs text-stone-700 dark:text-stone-200 hover:bg-stone-100 dark:hover:bg-stone-800"
        >
          다시 시도
        </button>
      </div>
    );
  }
}
