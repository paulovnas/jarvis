import { useEffect, useLayoutEffect, useState, useSyncExternalStore } from "react";

const MAX_DISPLAY_LAG_MS = 160;
type DisplaySnapshot = { source: string; text: string; streamId: string; streaming: boolean };

class StreamTextBuffer {
  private snapshot: DisplaySnapshot;
  private source: string;
  private streamId: string;
  private streaming: boolean;
  private shown: number;
  private rate = 0;
  private lastArrival = performance.now();
  private lastAdvance = this.lastArrival;
  private backlogSince = this.lastArrival;
  private frame: number | undefined;
  private deadline: ReturnType<typeof setTimeout> | undefined;
  private listeners = new Set<() => void>();
  readonly motion = window.matchMedia("(prefers-reduced-motion: reduce)");

  constructor(source: string, streaming: boolean, streamId: string) {
    this.source = source;
    this.streamId = streamId;
    this.streaming = streaming;
    this.shown = source.length;
    this.snapshot = { source, text: source, streamId, streaming };
  }

  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  };

  getSnapshot = () => this.snapshot;

  update(source: string, streaming: boolean, streamId: string) {
    const reset = streamId !== this.streamId || !source.startsWith(this.source) || streaming && !this.streaming;
    const arrived = reset ? 0 : source.length - this.source.length;
    const now = performance.now();
    if (this.shown === this.source.length) {
      this.backlogSince = now;
      this.lastAdvance = now;
    }
    this.source = source;
    this.streamId = streamId;
    this.streaming = streaming;
    if (reset || !streaming || this.motion.matches || document.visibilityState === "hidden") {
      this.rate = 0;
      this.lastArrival = now;
      this.flush();
      return;
    }
    if (arrived > 0) {
      this.rate = this.rate * 0.75 + arrived / Math.max(16, now - this.lastArrival) * 0.25;
      this.lastArrival = now;
    }
    if (this.shown < source.length) this.schedule();
  }

  environmentChanged = () => {
    if (this.motion.matches || document.visibilityState === "hidden") this.flush();
  };

  cancel = () => {
    if (this.frame !== undefined) window.cancelAnimationFrame(this.frame);
    if (this.deadline !== undefined) clearTimeout(this.deadline);
    this.frame = undefined;
    this.deadline = undefined;
  };

  private flush() {
    this.cancel();
    this.shown = this.source.length;
    this.publish();
  }

  private schedule() {
    this.frame ??= window.requestAnimationFrame(this.advance);
    // Frames may be suspended by the host; source must never wait indefinitely for presentation.
    this.deadline ??= setTimeout(() => this.flush(), Math.max(0, MAX_DISPLAY_LAG_MS - (performance.now() - this.backlogSince)));
  }

  private advance = () => {
    this.frame = undefined;
    const now = performance.now();
    const backlog = this.source.length - this.shown;
    const remaining = Math.max(1, MAX_DISPLAY_LAG_MS - (now - this.backlogSince));
    const pace = Math.max(0.04, this.rate * 1.2, backlog / remaining);
    this.shown = Math.min(this.source.length, this.shown + Math.max(1, Math.ceil(pace * Math.max(1, now - this.lastAdvance))));
    if (this.shown < this.source.length && /[\uD800-\uDBFF]/u.test(this.source[this.shown - 1]) && /[\uDC00-\uDFFF]/u.test(this.source[this.shown])) this.shown++;
    this.lastAdvance = now;
    this.publish();
    if (this.shown < this.source.length) this.schedule();
    else this.cancel();
  };

  private publish() {
    const text = this.source.slice(0, this.shown);
    if (text === this.snapshot.text && this.source === this.snapshot.source && this.streamId === this.snapshot.streamId && this.streaming === this.snapshot.streaming) return;
    this.snapshot = { source: this.source, text, streamId: this.streamId, streaming: this.streaming };
    this.listeners.forEach(listener => listener());
  }
}

export function useStreamedText(source: string, isStreaming: boolean, streamId: string): string {
  const [buffer] = useState(() => new StreamTextBuffer(source, isStreaming, streamId));
  const snapshot = useSyncExternalStore(buffer.subscribe, buffer.getSnapshot, buffer.getSnapshot);
  useLayoutEffect(() => { buffer.update(source, isStreaming, streamId); }, [buffer, source, isStreaming, streamId]);
  useEffect(() => {
    if (!isStreaming) return;
    document.addEventListener("visibilitychange", buffer.environmentChanged);
    buffer.motion.addEventListener("change", buffer.environmentChanged);
    return () => {
      buffer.cancel();
      document.removeEventListener("visibilitychange", buffer.environmentChanged);
      buffer.motion.removeEventListener("change", buffer.environmentChanged);
    };
  }, [buffer, isStreaming]);
  return isStreaming && snapshot.streaming && snapshot.streamId === streamId && source.startsWith(snapshot.source) ? snapshot.text : source;
}
