import { useState } from "react";
import { BackendAPI } from "../api";

interface Props {
  /** 시작 명령이 끝난 뒤(성공·실패 모두) 부른다. 연결 상태를 다시 확인하는 데 쓴다. */
  onDone: () => void | Promise<void>;
}

/** "서버가 꺼져 있습니다" 안내 옆의 [서버 시작]. 연결 창의 버튼과 같은 `lms server start`를 부른다. */
export function StartServerButton({ onDone }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function start() {
    setBusy(true);
    setError(null);
    try {
      await BackendAPI.startServer();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
    await onDone();
  }

  return (
    <span className="flex items-center gap-2 min-w-0">
      {error && <span className="min-w-0 break-words">{error}</span>}
      <button
        onClick={() => void start()}
        disabled={busy}
        className="shrink-0 px-2 py-1 rounded-md border border-red-300 dark:border-red-800 font-semibold cursor-pointer disabled:opacity-50"
      >
        {busy ? "시작 중…" : "서버 시작"}
      </button>
    </span>
  );
}
