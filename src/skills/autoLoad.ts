import type { AppSettings, ToolExecutionRecord } from "../types";

/** 도구 카드 머리의 이름표. 앱이 Skill을 자동으로 불러온 기록이면 "Skill 자동 불러옴: <이름>" */
export function toolBadgeLabel(record: ToolExecutionRecord): string {
  if (!record.automatic || record.toolName !== "load_skill") return record.toolName;
  try {
    const name = (JSON.parse(record.argumentsJson) as { name?: unknown }).name;
    if (typeof name === "string" && name.trim()) return `Skill 자동 불러옴: ${name.trim()}`;
  } catch {
    // 인자가 깨졌으면 이름 없이 보여 준다.
  }
  return "Skill 자동 불러옴";
}

/** "Skill 자동 불러오기" 설정. 값이 없으면(예전 설정 파일) 켜짐. */
export function skillAutoLoadOn(settings: AppSettings): boolean {
  return settings.skillAutoLoad !== false;
}

/** 스위치만 바꾼 저장용 설정. 저장된 토큰은 화면에서 다시 보내지 않는다(비우면 기존 토큰 유지). */
export function withSkillAutoLoad(settings: AppSettings, on: boolean): AppSettings {
  return { ...settings, apiKey: "", skillAutoLoad: on };
}
