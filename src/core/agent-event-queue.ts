import type { AgentEventBatch } from "./agent-events";

const MAX_PENDING_BATCHES = 512;
const MAX_PENDING_BYTES = 4 * 1024 * 1024;
const MAX_SLICE_EVENTS = 200;
const MAX_SLICE_MS = 8;

/** Revision batches stay atomic; a burst yields between batches, never between revisions. */
export function createAgentEventQueue(
  consume: (batch: AgentEventBatch) => boolean,
  resync: (freshSnapshot: boolean) => void,
  initiallyPaused = false,
) {
  let pending: Array<{ batch: AgentEventBatch; bytes: number } | undefined> = [];
  let head = 0;
  let bytes = 0;
  let paused = initiallyPaused;
  let disposed = false;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const cancel = () => { if (timer !== undefined) clearTimeout(timer); timer = undefined; };
  const clear = () => { pending = []; head = 0; bytes = 0; };
  const schedule = () => {
    if (!disposed && !paused && head < pending.length && timer === undefined) timer = setTimeout(drain, 0);
  };
  const drain = () => {
    timer = undefined;
    const start = performance.now();
    let events = 0;
    while (!disposed && !paused && head < pending.length) {
      if (events >= MAX_SLICE_EVENTS || (events > 0 && performance.now() - start >= MAX_SLICE_MS)) break;
      const item = pending[head];
      pending[head++] = undefined;
      if (!item) continue;
      bytes -= item.bytes;
      events += Math.max(1, item.batch.events.length);
      if (!consume(item.batch)) { paused = true; resync(false); break; }
    }
    if (head === pending.length) clear();
    else if (head >= MAX_SLICE_EVENTS) { pending = pending.slice(head); head = 0; }
    schedule();
  };
  const append = (batch: AgentEventBatch) => {
    // Bound retained JS strings conservatively by UTF-16 size, including metadata.
    const size = JSON.stringify(batch).length * 2;
    if (pending.length - head >= MAX_PENDING_BATCHES || bytes + size > MAX_PENDING_BYTES) {
      cancel(); clear(); paused = true; resync(true); return;
    }
    pending.push({ batch, bytes: size }); bytes += size;
  };
  return {
    enqueue(batch: AgentEventBatch) { if (disposed) return; append(batch); schedule(); },
    resume(replay: AgentEventBatch[] = []) {
      if (disposed) return;
      cancel();
      const waiting = pending.slice(head).flatMap(item => item ? [item.batch] : []);
      clear(); paused = false;
      for (const batch of [...replay, ...waiting].sort((left, right) => left.revision - right.revision)) append(batch);
      schedule();
    },
    dispose() { disposed = true; cancel(); clear(); },
  };
}
