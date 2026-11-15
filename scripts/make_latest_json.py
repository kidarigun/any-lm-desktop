"""자동 업데이트용 latest.json을 만든다.

앱은 깃허브 릴리스의 latest.json을 읽어 새 버전이 있는지 확인한다
(src-tauri/tauri.conf.json의 plugins.updater.endpoints).

릴리스 순서:
  1. 서명 키를 환경 변수로 주고 빌드한다 (설치 파일과 함께 .sig 파일이 생긴다).
       TAURI_SIGNING_PRIVATE_KEY=<개인 키 파일 경로>  TAURI_SIGNING_PRIVATE_KEY_PASSWORD=  npm run tauri build
  2. 이 스크립트로 릴리스에 올릴 파일을 모은다.
       python scripts/make_latest_json.py --notes "바뀐 점 요약" --out release
  3. release 폴더의 설치 파일과 latest.json을 깃허브 릴리스 v<버전>에 올린다.

서명(.sig)은 파일 내용에 대한 것이라, 올릴 때 파일 이름을 바꿔도 유효하다.
"""
import argparse
import datetime
import json
import pathlib
import shutil
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
REPO = "kidarigun/k-exaone-desktop"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--notes", default="", help="앱의 업데이트 창에 보여 줄 바뀐 점")
    ap.add_argument("--out", default="release", help="올릴 파일을 모을 폴더")
    args = ap.parse_args()

    conf = json.loads((ROOT / "src-tauri" / "tauri.conf.json").read_text(encoding="utf-8"))
    version = conf["version"]
    product = conf["productName"]
    built = ROOT / "src-tauri" / "target" / "release" / "bundle" / "nsis" / f"{product}_{version}_x64-setup.exe"
    sig = built.with_name(built.name + ".sig")
    if not built.is_file():
        print(f"설치 파일이 없습니다: {built}\n먼저 npm run tauri build를 실행하세요.", file=sys.stderr)
        return 1
    if not sig.is_file():
        print(
            f"서명 파일이 없습니다: {sig}\nTAURI_SIGNING_PRIVATE_KEY를 주고 다시 빌드하세요.",
            file=sys.stderr,
        )
        return 1
    if sig.stat().st_mtime < built.stat().st_mtime - 60:
        print("서명 파일이 설치 파일보다 오래됐습니다. 다시 빌드하세요.", file=sys.stderr)
        return 1

    out = (ROOT / args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    # 내려받기 주소에 공백이 들어가지 않게 이름을 바꾼다.
    asset = f"K-EXAONE-Desktop_{version}_x64-setup.exe"
    shutil.copyfile(built, out / asset)

    latest = {
        "version": version,
        "notes": args.notes,
        "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": {
            "windows-x86_64": {
                "signature": sig.read_text(encoding="utf-8").strip(),
                "url": f"https://github.com/{REPO}/releases/download/v{version}/{asset}",
            }
        },
    }
    (out / "latest.json").write_text(json.dumps(latest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"{out / asset}\n{out / 'latest.json'}\n이 두 파일을 릴리스 v{version}에 올리세요.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
