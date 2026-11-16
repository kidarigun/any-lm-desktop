/**
 * rhwp(@rhwp/core, MIT) WASM 엔진을 한 번만 불러온다.
 * 26MB쯤 되는 WASM이라 한글 문서를 처음 열 때만 읽는다. 네트워크에 접속하지 않는다
 * (WASM 파일은 앱에 함께 들어 있다).
 */
import type { HwpDocument } from "@rhwp/core";

type RhwpModule = typeof import("@rhwp/core");

let loading: Promise<RhwpModule> | null = null;

export function loadRhwp(): Promise<RhwpModule> {
  if (!loading) {
    loading = (async () => {
      const [mod, wasm] = await Promise.all([import("@rhwp/core"), import("@rhwp/core/rhwp_bg.wasm?url")]);
      await mod.default({ module_or_path: wasm.default });
      return mod;
    })().catch((e) => {
      // 다음에 다시 시도할 수 있게 한다.
      loading = null;
      throw e;
    });
  }
  return loading;
}

/** HWPX(또는 HWP) 바이트를 rhwp 문서로 연다. */
export async function openRhwpDocument(bytes: Uint8Array): Promise<HwpDocument> {
  const mod = await loadRhwp();
  return new mod.HwpDocument(bytes);
}

export interface PageSize {
  width: number;
  height: number;
}

/** 쪽 크기(px, 96dpi). 읽을 수 없으면 A4. */
export function pageSize(doc: HwpDocument, page: number): PageSize {
  try {
    const info = JSON.parse(doc.getPageInfo(page)) as { width?: number; height?: number };
    if (info.width && info.height) return { width: info.width, height: info.height };
  } catch {
    // 아래 기본값
  }
  return { width: 793.7, height: 1122.5 };
}

/** 저장할 HWPX 바이트와 rhwp가 알려 주는 내용 손실 수 */
export function exportHwpx(doc: HwpDocument): { bytes: Uint8Array; lossCount: number } {
  const report = doc.exportHwpxWithReport();
  try {
    let lossCount = 0;
    try {
      lossCount = Number((JSON.parse(report.contentLoss()) as { count?: number }).count) || 0;
    } catch {
      lossCount = 0;
    }
    return { bytes: report.takeBytes(), lossCount };
  } finally {
    report.free();
  }
}
