import { base64ToBytes } from "../docs/base64";
import type { DocKind } from "../docs/formats";
import type { FileSnapshot } from "../types";

/** 캔버스에 열린 문서 */
export interface CanvasDoc {
  /** 열 때마다 새로 만드는 식별자 (같은 파일을 다시 열면 편집기를 새로 띄운다) */
  id: string;
  title: string;
  kind: DocKind;
  /** 프로젝트 기준 상대 경로 */
  filePath?: string;
  /** 프로젝트 밖에서 연 파일의 절대 경로 */
  absolutePath?: string;
  text?: string;
  bytes?: Uint8Array;
}

export interface CanvasSavedInfo {
  kind: DocKind;
  title: string;
  snapshot?: FileSnapshot | null;
  relativePath?: string | null;
  absolutePath?: string;
  /** 같은 형식으로 저장해 캔버스의 문서가 저장된 파일을 가리키게 됐는지 */
  replacesCurrent: boolean;
}

/** base64로 읽은 파일에서 캔버스 문서를 만든다. */
export function canvasDocFromBase64(
  base: Omit<CanvasDoc, "id" | "text" | "bytes">,
  dataBase64: string,
  binary: boolean
): CanvasDoc {
  const bytes = base64ToBytes(dataBase64);
  return {
    ...base,
    id: `${base.filePath || base.absolutePath || base.title}#${Date.now()}`,
    ...(binary ? { bytes } : { text: new TextDecoder().decode(bytes) }),
  };
}
