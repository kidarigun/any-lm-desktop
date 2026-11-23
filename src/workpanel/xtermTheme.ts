import type { ITheme } from "@xterm/xterm";

/** 앱의 밝은 화면 색에 맞춘 터미널 색 (밝은 바탕에서도 노랑·흰색 글자가 보이게 어둡게 잡았다) */
export const LIGHT_TERMINAL_THEME: ITheme = {
  background: "#FFFFFF",
  foreground: "#2B2B28",
  cursor: "#A50034",
  cursorAccent: "#FFFFFF",
  selectionBackground: "#A5003433",
  black: "#2B2B28",
  red: "#C4262E",
  green: "#2E7D32",
  yellow: "#8A6100",
  blue: "#1D5FBF",
  magenta: "#A13FA1",
  cyan: "#087F8C",
  white: "#6B6B66",
  brightBlack: "#5C5C57",
  brightRed: "#E0353D",
  brightGreen: "#3B9C40",
  brightYellow: "#A87800",
  brightBlue: "#2F74D9",
  brightMagenta: "#BC52BC",
  brightCyan: "#0A99A8",
  brightWhite: "#8A8A84",
};

/** 앱의 어두운 화면 색에 맞춘 터미널 색 */
export const DARK_TERMINAL_THEME: ITheme = {
  background: "#1A1A18",
  foreground: "#E7E5DF",
  cursor: "#FF5C8D",
  cursorAccent: "#1A1A18",
  selectionBackground: "#FF5C8D40",
  black: "#1A1A18",
  red: "#F2707A",
  green: "#7CCB7F",
  yellow: "#E6C35C",
  blue: "#6FA8F2",
  magenta: "#D58FE0",
  cyan: "#5CC9D4",
  white: "#D6D4CE",
  brightBlack: "#77756F",
  brightRed: "#FF8F98",
  brightGreen: "#9BE09E",
  brightYellow: "#F5D77E",
  brightBlue: "#94C0FA",
  brightMagenta: "#E6AEEE",
  brightCyan: "#82DDE6",
  brightWhite: "#FFFFFF",
};

export function terminalTheme(isDark: boolean): ITheme {
  return isDark ? DARK_TERMINAL_THEME : LIGHT_TERMINAL_THEME;
}
