import { describe, expect, it } from "vitest";
import type { PythonRunOutcome } from "../types";
import { fixRequestPrompt } from "./PythonRunDialog";

const base: PythonRunOutcome = {
  script: "collect_samsung_2025_f.py",
  python: { path: "C:\\py\\python.exe", version: "3.15.2", inVenv: false, projectVenv: false },
  exitCode: 1,
  timedOut: false,
  durationMs: 100,
  stdout: "",
  stderr: '  File "collect_samsung_2025_f.py", line 63\n    else:\n    ^^^^\nSyntaxError: invalid syntax\n',
  created: [],
  modified: [],
  missingModule: null,
  suggestedPackage: null,
};

describe("모델에게 고쳐 달라고 하기", () => {
  it("같은 파일을 고쳐 다시 실행하라는 요청과 오류 끝부분을 담는다", () => {
    const p = fixRequestPrompt(base);
    expect(p).toContain("`collect_samsung_2025_f.py`을 실행했더니 오류가 났어 (종료 코드 1)");
    expect(p).toContain("새 파일을 만들지 말고 같은 파일을 고친 뒤 run_python");
    expect(p).toContain("```\n  File \"collect_samsung_2025_f.py\", line 63\n    else:\n    ^^^^\nSyntaxError: invalid syntax\n```");
  });

  it("긴 오류는 마지막 25줄만, 시간 초과도 알린다", () => {
    const long = Array.from({ length: 40 }, (_, i) => `줄${i + 1}`).join("\n");
    const p = fixRequestPrompt({ ...base, stderr: long, timedOut: true, exitCode: null });
    expect(p).toContain("3분 안에 끝나지 않아 중단됐어");
    expect(p).not.toContain("줄15\n");
    expect(p).toContain("줄16\n");
    expect(p).toContain("줄40\n```");
  });
});
