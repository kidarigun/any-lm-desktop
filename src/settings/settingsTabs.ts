export type SettingsTab = "connection" | "skills" | "mcp" | "system";

export interface SettingsTabDef {
  id: SettingsTab;
  label: string;
}

export const SETTINGS_TABS: readonly SettingsTabDef[] = [
  { id: "connection", label: "연결 · 모델" },
  { id: "skills", label: "Skill" },
  { id: "mcp", label: "MCP 서버" },
  { id: "system", label: "환경 · 업데이트" },
];

export function settingsButtonStatusTitle(providerLabel: string, serverUp: boolean | null): string {
  const state = serverUp === null ? "확인 중" : serverUp ? "연결됨" : "꺼져 있음";
  return `${providerLabel}: ${state}`;
}
