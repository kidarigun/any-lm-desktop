/**
 * 터미널 화면(xterm) 쓰기 흐름 조절 (검토 C-I2).
 * xterm `write`의 완료 콜백으로 아직 그리지 못한 양을 세고, 그 양이 `highWater`를 넘으면 새 출력을
 * 대기열에 둔다. 대기열이 `maxQueued`를 넘으면 오래된 것부터 버리고, 다시 쓸 때 백엔드와 같은
 * 생략 표시를 한 줄 넣는다.
 */

/** 그리기를 기다리는 글자 수가 이보다 많으면 새 출력은 대기열에 둔다. */
export const XTERM_HIGH_WATER = 256 * 1024;
/** 화면 대기열의 상한(글자 수). 넘치면 오래된 것부터 버린다. */
export const XTERM_MAX_QUEUED = 1024 * 1024;

/** 백엔드 `terminal::dropped_marker`와 같은 문구 */
export function droppedMarker(bytes: number): string {
  return `\r\n[출력 ${bytes}바이트 생략]\r\n`;
}

const encoder = new TextEncoder();
const utf8Length = (s: string) => encoder.encode(s).length;

export type WriteFn = (data: string, done: () => void) => void;

export class OutputFlow {
  private inFlight = 0;
  private queue: string[] = [];
  private queued = 0;
  private droppedBytes = 0;

  constructor(
    private readonly write: WriteFn,
    private readonly highWater = XTERM_HIGH_WATER,
    private readonly maxQueued = XTERM_MAX_QUEUED
  ) {}

  /** 아직 그리지 못한 글자 수(쓴 것 + 대기열) */
  get pending(): number {
    return this.inFlight + this.queued;
  }

  push(data: string): void {
    if (!data) return;
    if (this.queue.length === 0 && this.droppedBytes === 0 && this.inFlight < this.highWater) {
      this.send(data);
      return;
    }
    this.queue.push(data);
    this.queued += data.length;
    while (this.queued > this.maxQueued && this.queue.length > 1) {
      const old = this.queue.shift()!;
      this.queued -= old.length;
      this.droppedBytes += utf8Length(old);
    }
  }

  private send(data: string) {
    this.inFlight += data.length;
    this.write(data, () => {
      this.inFlight -= data.length;
      this.drain();
    });
  }

  private drain() {
    while (this.inFlight < this.highWater && this.queue.length > 0) {
      if (this.droppedBytes > 0) {
        const marker = droppedMarker(this.droppedBytes);
        this.droppedBytes = 0;
        this.send(marker);
      }
      const next = this.queue.shift()!;
      this.queued -= next.length;
      this.send(next);
    }
  }
}
