/** 글자→토큰 추정(전역 규칙: 1토큰 = 1.5자) */
export const CHARS_PER_TOKEN = 1.5;
/** 지침이 로드된 컨텍스트의 이 비율을 넘으면 경고 */
export const INSTRUCTIONS_WARN_RATIO = 0.1;

export interface InstructionsStats {
  chars: number;
  tokens: number;
  /** 로드된 컨텍스트 대비 비율(0~100+, 소수 첫째 자리). 컨텍스트를 모르면 null */
  percent: number | null;
  warn: boolean;
}

export function instructionsStats(text: string, contextLength: number | null): InstructionsStats {
  const chars = [...text].length;
  const tokens = Math.ceil(chars / CHARS_PER_TOKEN);
  if (!contextLength || contextLength <= 0) return { chars, tokens, percent: null, warn: false };
  const ratio = tokens / contextLength;
  return { chars, tokens, percent: Math.round(ratio * 1000) / 10, warn: ratio > INSTRUCTIONS_WARN_RATIO };
}

export function formatInstructionsStats(s: InstructionsStats): string {
  const base = `${s.chars.toLocaleString("ko-KR")}자 · 약 ${s.tokens.toLocaleString("ko-KR")}토큰`;
  return s.percent === null ? base : `${base} · 컨텍스트의 ${s.percent}%`;
}

/** 비어 있으면(공백뿐 포함) 바꾸고, 아니면 끝에 빈 줄을 두고 덧붙인다. */
export function insertTemplate(current: string, template: string): string {
  if (!current.trim()) return template;
  return `${current.replace(/\s+$/, "")}\n\n${template}`;
}

export interface InstructionTemplate {
  id: string;
  label: string;
  text: string;
}

export const INSTRUCTION_TEMPLATES: InstructionTemplate[] = [
  {
    id: "report",
    label: "보고서·문서 작성",
    text: `# 보고서·문서 작성
- 결론을 먼저 쓰고(두괄식), 근거와 세부 내용은 개조식으로 이어서 정리한다.
- 문체는 존댓말 공문서체로 통일하고, 숫자·날짜·고유명사는 원문 자료 그대로 쓴다.
- 자료에서 가져온 내용에는 출처(파일 경로나 문서 이름)를 함께 적는다.
- 자료에 없는 내용은 지어내지 말고 "확인 필요"로 표시한다.
- 완성본은 프로젝트 폴더에 .docx 또는 .hwpx 파일로 저장한다.
- 파일을 만들기 전에 목차(제목 구조)를 먼저 보여 주고 확인받는다.`,
  },
  {
    id: "coding",
    label: "코딩",
    text: `# 코딩
- 작업하기 전에 관련 파일을 먼저 읽고 기존 코드의 스타일과 이름 규칙을 따른다.
- 한 번에 작은 단위로 고친다. 요청하지 않은 파일이나 구조는 바꾸지 않는다.
- 고친 뒤에는 테스트나 빌드 명령을 실행해 결과를 확인하고, 실패하면 원인을 설명한 뒤 고친다.
- 새 기능에는 테스트를 함께 추가한다.
- 삭제나 이름 변경처럼 되돌리기 어려운 작업은 먼저 알리고 진행한다.
- 마지막에 바꾼 파일과 이유를 짧게 요약한다.`,
  },
  {
    id: "research",
    label: "자료 조사·요약",
    text: `# 자료 조사·요약
- 프로젝트 폴더의 자료를 먼저 찾아 읽고, 읽은 파일의 경로를 답변에 밝힌다.
- 요약은 핵심 3~5줄을 먼저, 세부 내용은 항목별로 정리한다.
- 사실과 추측을 구분해서 쓰고, 자료에 없으면 모른다고 답한다.
- 자료끼리 내용이 다르면 둘 다 적고 어느 쪽 출처인지 표시한다.
- 긴 문서는 부분별로 나누어 읽고, 놓친 부분이 있으면 그 사실을 알린다.`,
  },
  {
    id: "translate",
    label: "번역·교정",
    text: `# 번역·교정
- 원문은 그대로 보존하고, 번역·교정본은 별도 파일이나 별도 절로 만든다.
- 의미를 덧붙이거나 빼지 않는다. 불확실한 부분은 원문을 괄호로 남기고 표시한다.
- 같은 용어는 문서 전체에서 같은 말로 옮긴다. 처음 정한 용어는 목록으로 정리해 둔다.
- 교정할 때는 고친 곳과 이유를 짧게 알려 준다(맞춤법, 띄어쓰기, 문장 호응).
- 존댓말·평어 등 문체와 숫자·날짜 표기 방식은 원문을 따른다.`,
  },
];
