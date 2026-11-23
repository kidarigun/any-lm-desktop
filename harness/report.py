"""하니스 실행 결과를 표로 요약한다.

    python harness/report.py harness/runs/batch1            # 시나리오별 요약
    python harness/report.py harness/runs/batch1 proj-report # 한 시나리오의 대화 내용
"""
import io
import json
import pathlib
import sys

sys.stdout.reconfigure(encoding="utf-8")


def load(run_dir):
    out = []
    for p in sorted((run_dir / "transcripts").glob("*.json"), key=lambda p: p.stat().st_mtime):
        out.append(json.load(io.open(p, encoding="utf-8")))
    return out


def speed(t):
    """글자가 나오기 시작한 뒤의 초당 출력 토큰 수"""
    usage = t.get("usage") or {}
    out = usage.get("completion_tokens") or 0
    ms = t["totalMs"] - (t.get("headersMs") or 0)
    return out / (ms / 1000) if ms > 0 and out else 0


def summary(run_dir):
    for d in load(run_dir):
        s = d["summary"]
        print(f"\n## {s['id']}  [{s['mode']} / 추론 {s['effort']}{' / 추론 유지' if s['tuning'].get('preserveThinking') else ''}]"
              f"{'  (중단됨)' if s['incomplete'] else ''}")
        print(f"   {d['scenario']['note']}")
        print(f"   호출 {s['apiCalls']}회, {s['wallMs'] / 1000:.0f}초, 검사 {s['checksPassed']}/{s['checksTotal']}, "
              f"도구 오류 {s['toolErrors']}, 잘못된 도구 JSON {s['badToolJson']}")
        for c in s["checks"]:
            if not c["ok"]:
                print(f"   ✗ {c['check']}")
        for e in s["errors"]:
            print(f"   ! {e[:200]}")
        for ti, turn in enumerate(d["turns"]):
            for ri, t in enumerate(turn["traces"]):
                u = t.get("usage") or {}
                tools = ",".join(c["name"] for c in t["toolCalls"]) or "-"
                first = t.get("firstContentMs") or t.get("firstToolMs")
                print(f"   턴{ti + 1} 라운드{ri + 1}: {t['totalMs'] / 1000:6.1f}초"
                      f" | 첫 글·도구 {first / 1000 if first else 0:6.1f}초"
                      f" | 최장 멈춤 {t['maxGapMs'] / 1000:5.1f}초"
                      f" | 입력 {u.get('prompt_tokens', 0):6d} 출력 {u.get('completion_tokens', 0):6d} (추론 {u.get('reasoning_tokens', 0):6d})"
                      f" | {speed(t):5.1f} 토큰/초"
                      f" | 종료 {t.get('finishReason')} | 도구 {tools}"
                      f"{' | 오류 ' + t['error'][:80] if t.get('error') else ''}")


def detail(run_dir, sid):
    d = json.load(io.open(run_dir / "transcripts" / f"{sid}.json", encoding="utf-8"))
    for ti, turn in enumerate(d["turns"]):
        print(f"\n===== 턴 {ti + 1} 사용자 =====\n{turn['prompt']}")
        if turn.get("error"):
            print(f"\n[오류] {turn['error']}")
        m = turn.get("message")
        if not m:
            continue
        print(f"\n----- 추론 ({len(m.get('reasoningContent') or '')}자) -----\n{(m.get('reasoningContent') or '')[:6000]}")
        for r in m["toolRecords"]:
            args = r["argumentsJson"]
            print(f"\n----- 도구 [{r['round']}] {r['toolName']} → {r['status']} -----\n인자: {args[:1500]}\n결과: {r['output'][:800]}")
        print(f"\n----- 답변 ({len(m['content'])}자) -----\n{m['content'][:8000]}")
    print("\n----- 파일 -----")
    for name, size in d["files"]:
        print(f"{size:8d}  {name}")


if __name__ == "__main__":
    run = pathlib.Path(sys.argv[1])
    if len(sys.argv) > 2:
        detail(run, sys.argv[2])
    else:
        summary(run)
