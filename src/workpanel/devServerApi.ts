import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "../api";
import type { TerminalInfo } from "./terminalApi";

export interface DevServerConfig {
  id: string;
  name: string;
  command: string;
  cwd: string;
  url?: string | null;
}

export interface ServerInfo {
  projectId: string;
  terminal: TerminalInfo;
  config: DevServerConfig;
  status: "starting" | "ready" | "exited" | "unconfirmed";
  url: string | null;
}

/** Tauri가 앱 자신으로 취급하는 예약 호스트: 미리보기에서도 브라우저에서도 열지 않는다. */
const RESERVED_HOSTS = ["ipc.localhost", "tauri.localhost", "asset.localhost"];
const FRAME_HOSTS = ["localhost", "127.0.0.1", "[::1]", "anylm-preview.localhost"];

export function previewDestination(input: string, appOrigin = globalThis.location?.origin): { url: string; local: boolean } | null {
  try {
    const url = new URL(input.trim());
    if ((url.protocol !== "http:" && url.protocol !== "https:") || url.username || url.password || url.port === "0") return null;
    if (url.hostname === "0.0.0.0") url.hostname = "localhost";
    if (url.origin === appOrigin || RESERVED_HOSTS.includes(url.hostname)) return null;
    // 앱 CSP(frame-src)가 프레임에 허용하는 호스트만 미리보기 창에 넣는다. 다른 *.localhost는 브라우저로 연다 (결정 0019).
    const local = FRAME_HOSTS.includes(url.hostname);
    return { url: url.href, local };
  } catch {
    return null;
  }
}

/** 프레임 CSP에서 지원하지 않는 IPv6 리터럴은 입력한 주소로 브라우저에서 연다. */
export function previewFrameUrl(raw: string): string | null {
  const target = previewDestination(raw);
  return target?.local && new URL(target.url).hostname !== "[::1]" ? target.url : null;
}

export const DevServerAPI = {
  options(projectId: string) {
    return invoke<DevServerConfig[]>("dev_server_options", { projectId });
  },
  start(projectId: string, config: DevServerConfig, save: boolean) {
    return invoke<ServerInfo>("dev_server_start", { projectId, config, save });
  },
  list(projectId: string) {
    return isTauri() ? invoke<ServerInfo[]>("dev_server_list", { projectId }) : Promise.resolve([]);
  },
  checkPreview(raw: string) {
    return isTauri() ? invoke<void>("dev_preview_check", { raw }) : Promise.resolve();
  },
  onServer(callback: (server: ServerInfo) => void) {
    return listen<ServerInfo>("anylm-dev-server", ({ payload }) => callback(payload));
  },
  onPreview(callback: (event: { projectId: string; url: string }) => void) {
    return listen<{ projectId: string; url: string }>("anylm-open-preview", ({ payload }) => callback(payload));
  },
};
