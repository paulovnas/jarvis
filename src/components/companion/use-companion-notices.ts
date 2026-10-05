import { useCallback, useEffect, useRef, useState } from "react";
import { companionIsStageCompletion, companionItemKey, type CompanionItem } from "@/core/companion";

export interface CompanionNotice { id: string; item: CompanionItem }

const noticeId = (item: CompanionItem) => item.status === "waiting"
  ? `${companionItemKey(item)}/${item.pendingQuestion?.turnId ?? item.attentionId}/${item.pendingQuestion?.toolId ?? "waiting"}`
  : item.attentionId;

/** Each notice has its own lifetime; dismissing a notice never acknowledges work. */
export function useCompanionNotices() {
  const [queue, setQueue] = useState<CompanionNotice[]>([]);
  const seen = useRef(new Set<string>());
  const sync = useCallback((items: CompanionItem[]) => {
    const eligible = items.filter(item => item.status === "waiting" || (!item.acknowledged && !companionIsStageCompletion(item) && (item.status === "completed" || item.status === "failed")));
    const fresh = eligible.filter(item => !seen.current.has(noticeId(item))).sort((a, b) => a.updatedAt - b.updatedAt);
    for (const item of fresh) seen.current.add(noticeId(item));
    while (seen.current.size > 128) { const oldest = seen.current.values().next().value; if (oldest) seen.current.delete(oldest); }
    const remaining = new Set(eligible.map(noticeId));
    setQueue(current => {
      const next = current.filter(notice => remaining.has(notice.id));
      for (const item of fresh) next.push({ id: noticeId(item), item });
      return next.length === current.length && next.every((notice, index) => notice === current[index]) ? current : next.slice(-32);
    });
  }, []);
  const clear = useCallback(() => setQueue([]), []);
  const dismiss = useCallback((id: string) => setQueue(current => current[0]?.id === id ? current.slice(1) : current), []);
  return { notice: queue[0] ?? null, sync, clear, dismiss };
}

/** Count display time only after preparation; keep the current notice while it speaks. */
export function useCompanionNoticeLifetime(id: string | null, paused: boolean, dismiss: (id: string) => void) {
  useEffect(() => {
    if (!id || paused) return;
    const timer = window.setTimeout(() => dismiss(id), 30_000);
    return () => window.clearTimeout(timer);
  }, [id, paused, dismiss]);
}
