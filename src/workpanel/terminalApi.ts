/** 작업 패널 터미널 명령·이벤트 (Rust `terminal.rs`) */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isTauri } from "../api";

export type TerminalKind = "shell" | "server";

export interface TerminalInfo {
  id: string;
  kind: TerminalKind;
  title: string;
  cwd: string;
  command: string | null;
  /** "powershell" | "pwsh" | "cmd" */
  shell: string;
  pid: number | null;
  running: boolean;
  exitCode: number | null;
}

export interface TerminalSnapshot {
  /** 최근 출력(최대 256KB, ANSI 포함) */
  data: string;
  /** 스냅샷에 들어 있는 마지막 출력 이벤트 번호 */
  seq: number;
  running: boolean;
  exitCode: number | null;
}

export interface TerminalOutput {
  id: string;
  data: string;
  seq: number;
}

export interface TerminalExit {
  id: string;
  code: number | null;
}

export interface ShellOption {
  id: string;
  label: string;
}

export interface OpenTerminalRequest {
  /** 작업 폴더. 비우면 사용자 홈 */
  cwd: string;
  cols: number;
  rows: number;
  kind: TerminalKind;
  command?: string | null;
  title?: string | null;
}

export const TERMINAL_OUTPUT_EVENT = "anylm-terminal-output";
export const TERMINAL_EXIT_EVENT = "anylm-terminal-exit";

const DESKTOP_ONLY = "터미널은 데스크톱 앱에서만 쓸 수 있습니다.";

export const TerminalAPI = {
  available(): boolean {
    return isTauri();
  },

  async open(request: OpenTerminalRequest): Promise<TerminalInfo> {
    if (!isTauri()) throw new Error(DESKTOP_ONLY);
    const r = await invoke<{ id: string; info: TerminalInfo }>("terminal_open", { request });
    return r.info;
  },

  async write(id: string, data: string): Promise<void> {
    if (isTauri()) await invoke("terminal_write", { id, data });
  },

  async resize(id: string, cols: number, rows: number): Promise<void> {
    if (isTauri()) await invoke("terminal_resize", { id, cols, rows });
  },

  async close(id: string): Promise<void> {
    if (isTauri()) await invoke("terminal_close", { id });
  },

  async list(): Promise<TerminalInfo[]> {
    return isTauri() ? await invoke<TerminalInfo[]>("terminal_list") : [];
  },

  async snapshot(id: string): Promise<TerminalSnapshot> {
    if (!isTauri()) return { data: "", seq: 0, running: false, exitCode: null };
    return await invoke<TerminalSnapshot>("terminal_snapshot", { id });
  },

  /** 셸 말고 다른 프로그램이 돌고 있는지 */
  async busy(id: string): Promise<boolean> {
    return isTauri() ? await invoke<boolean>("terminal_busy", { id }) : false;
  },

  async shells(): Promise<ShellOption[]> {
    return isTauri() ? await invoke<ShellOption[]>("terminal_shells") : [];
  },

  async onOutput(cb: (p: TerminalOutput) => void): Promise<UnlistenFn> {
    if (!isTauri()) return () => {};
    return await listen<TerminalOutput>(TERMINAL_OUTPUT_EVENT, (e) => cb(e.payload));
  },

  async onExit(cb: (p: TerminalExit) => void): Promise<UnlistenFn> {
    if (!isTauri()) return () => {};
    return await listen<TerminalExit>(TERMINAL_EXIT_EVENT, (e) => cb(e.payload));
  },
};
