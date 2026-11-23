"""배포되는 외부 라이브러리 목록(THIRD_PARTY_NOTICES.md)을 만든다.

사용법 (저장소 최상위에서):  python scripts/third_party_notices.py
- Rust: cargo metadata로 앱에 실제로 들어가는 크레이트만 (개발용 의존성 제외, Windows 기준)
- 웹 화면: npm ls --omit=dev로 배포용 패키지만
의존성을 바꾼 뒤 다시 실행해 갱신하세요.
"""
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "THIRD_PARTY_NOTICES.md"
TARGET = "x86_64-pc-windows-gnu"


def run(cmd, cwd):
    return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, encoding="utf-8", shell=False, check=True).stdout


def rust_crates():
    meta = json.loads(
        run(["cargo", "metadata", "--format-version", "1", "--offline", "--filter-platform", TARGET], ROOT / "src-tauri")
    )
    pkgs = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = meta["resolve"]["root"]
    seen, stack = set(), [root]
    while stack:
        nid = stack.pop()
        if nid in seen:
            continue
        seen.add(nid)
        for dep in nodes[nid]["deps"]:
            kinds = {k["kind"] for k in dep["dep_kinds"]}
            if kinds & {None, "build"}:  # 개발용(dev) 의존성은 배포에 들어가지 않는다
                stack.append(dep["pkg"])
    seen.discard(root)
    out = []
    for pid in seen:
        p = pkgs[pid]
        out.append((p["name"], p["version"], p.get("license") or "(라이선스 파일 참고)", p.get("repository") or ""))
    return sorted(out, key=lambda x: (x[0].lower(), x[1]))


def npm_packages():
    tree = json.loads(run(["npm.cmd", "ls", "--omit=dev", "--all", "--json"], ROOT))
    found = {}

    def walk(deps):
        for name, info in (deps or {}).items():
            ver = info.get("version")
            if ver and (name, ver) not in found:
                pj = ROOT / "node_modules" / name / "package.json"
                lic, repo = "(라이선스 파일 참고)", ""
                if pj.exists():
                    d = json.loads(pj.read_text(encoding="utf-8"))
                    l = d.get("license")
                    if isinstance(l, dict):
                        l = l.get("type")
                    lic = l or lic
                    r = d.get("repository")
                    repo = r.get("url", "") if isinstance(r, dict) else (r or "")
                found[(name, ver)] = (name, ver, lic, repo.replace("git+", "").replace(".git", ""))
            walk(info.get("dependencies"))

    walk(tree.get("dependencies"))
    return sorted(found.values(), key=lambda x: (x[0].lower(), x[1]))


def table(rows):
    lines = ["| 이름 | 버전 | 라이선스 | 저장소 |", "|---|---|---|---|"]
    for name, ver, lic, repo in rows:
        lines.append(f"| {name} | {ver} | {lic} | {repo} |")
    return "\n".join(lines)


def summary(rows):
    counts = {}
    for _, _, lic, _ in rows:
        counts[lic] = counts.get(lic, 0) + 1
    return ", ".join(f"{k} {v}개" for k, v in sorted(counts.items(), key=lambda kv: -kv[1]))


def main():
    rust = rust_crates()
    npm = npm_packages()
    text = f"""# 외부 라이브러리 안내 (Third-Party Notices)

Any LM Desktop은 아래 오픈소스 라이브러리를 사용합니다. 각 라이브러리의 저작권은 해당 저작자에게 있으며,
각 라이선스 전문은 해당 저장소(또는 설치된 패키지 안의 LICENSE 파일)에서 볼 수 있습니다.

이 파일은 `python scripts/third_party_notices.py`로 만듭니다. 의존성을 바꾸면 다시 실행하세요.

또한 앱이 사용자의 요청에 따라 내려받는 실행 환경(Python, Node.js, uv)과 그 패키지는
앱에 포함되어 배포되지 않으며, 각 공식 배포처의 라이선스를 따릅니다.

- 라이선스를 고를 수 있는 라이브러리는 허용형 쪽을 따릅니다: jszip은 MIT, DOMPurify는 Apache-2.0,
  "MIT OR Apache-2.0" 형식의 Rust 크레이트는 그중 하나를 따릅니다.
- MPL-2.0 라이브러리(cssparser, selectors 등 Tauri가 쓰는 크레이트)는 수정하지 않고 그대로 씁니다.
  각 라이브러리의 소스는 아래 표의 저장소에서 받을 수 있습니다.
- 한글 문서 엔진 @rhwp/core(rhwp, MIT)의 WASM 안에는 rhwp가 함께 묶은 라이브러리(JPEG·JPEG 2000·JBIG2 디코더,
  qcms, svg2pdf, pdfium 등)가 들어 있습니다. 그 저작권 표시와 라이선스 전문은
  `node_modules/@rhwp/core/notices/`와 https://github.com/edwardkim/rhwp 에 있습니다.

## 웹 화면 (npm) — {len(npm)}개

라이선스 분포: {summary(npm)}

{table(npm)}

## 앱 본체 (Rust) — {len(rust)}개

라이선스 분포: {summary(rust)}

{table(rust)}
"""
    OUT.write_text(text, encoding="utf-8")
    print(f"THIRD_PARTY_NOTICES.md: npm {len(npm)}개, Rust {len(rust)}개")
    print("npm:", summary(npm))
    print("Rust:", summary(rust))


if __name__ == "__main__":
    main()
