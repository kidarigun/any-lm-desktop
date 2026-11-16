/** PNG/JPEG/GIF/BMP 헤더에서 이미지 크기를 읽는다. */
export function imageSize(bytes: Uint8Array): { width: number; height: number } | null {
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (bytes.length > 24 && bytes[0] === 0x89 && bytes[1] === 0x50) {
    return { width: dv.getUint32(16), height: dv.getUint32(20) };
  }
  if (bytes.length > 10 && bytes[0] === 0x47 && bytes[1] === 0x49) {
    return { width: dv.getUint16(6, true), height: dv.getUint16(8, true) };
  }
  if (bytes.length > 26 && bytes[0] === 0x42 && bytes[1] === 0x4d) {
    return { width: dv.getInt32(18, true), height: Math.abs(dv.getInt32(22, true)) };
  }
  if (bytes.length > 4 && bytes[0] === 0xff && bytes[1] === 0xd8) {
    let off = 2;
    while (off + 9 < bytes.length) {
      if (bytes[off] !== 0xff) {
        off++;
        continue;
      }
      const marker = bytes[off + 1];
      const len = dv.getUint16(off + 2);
      if (marker >= 0xc0 && marker <= 0xcf && marker !== 0xc4 && marker !== 0xc8 && marker !== 0xcc) {
        return { width: dv.getUint16(off + 7), height: dv.getUint16(off + 5) };
      }
      off += 2 + len;
    }
  }
  return null;
}
