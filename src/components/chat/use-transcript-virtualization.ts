import { useCallback, useLayoutEffect, useMemo, useRef, useState, type RefObject } from "react";

type TurnRow = { id: string; index: number; top: number; bottom: number; height: number };
type ReadingAnchor = { id: string; offset: number };

function transcriptViewport(root: RefObject<HTMLDivElement | null>) {
  return root.current?.querySelector<HTMLElement>('[data-slot="scroll-area-viewport"]');
}

export function useTranscriptVirtualization(root: RefObject<HTMLDivElement | null>, ids: string[], retained: string[], follow: RefObject<boolean>, canFollow: boolean, onAdjusted: (top: number) => void) {
  const list = useRef<HTMLDivElement>(null);
  const [heights, setHeights] = useState<ReadonlyMap<string, number>>(() => new Map());
  const [range, setRange] = useState<{ start: number; end: number; first?: string }>(() => ({ start: Math.max(0, ids.length - 6), end: ids.length, first: ids[Math.max(0, ids.length - 6)] }));
  const [pinned, setPinned] = useState<string[]>([]);
  const geometry = useRef<{ rows: TurnRow[]; anchor: ReadingAnchor | null; bottomDistance: number }>({ rows: [], anchor: null, bottomDistance: 0 });
  const observer = useRef<ResizeObserver | null>(null);
  const elements = useRef(new Map<string, HTMLElement>());
  const measurements = useRef(new Map<HTMLElement, number>());
  const frame = useRef<number | null>(null);
  const rows = useMemo(() => {
    const layout: TurnRow[] = [];
    for (const [index, id] of ids.entries()) {
      const top = layout[layout.length - 1]?.bottom ?? 0;
      const height = heights.get(id) ?? 280;
      layout.push({ id, index, top, height, bottom: top + height });
    }
    return layout;
  }, [ids, heights]);
  const listOffset = useCallback((view: HTMLElement) => view.scrollTop + (list.current?.getBoundingClientRect().top ?? 0) - view.getBoundingClientRect().top, []);
  const update = useCallback(() => {
    const view = transcriptViewport(root); if (!view) return;
    const layout = geometry.current.rows;
    const top = view.scrollTop - listOffset(view);
    const first = layout.findIndex(row => row.bottom > top);
    const end = layout.findIndex(row => row.top >= top + view.clientHeight);
    const start = Math.max(0, first);
    // A hidden viewport has no usable layout; reproject it when it becomes measurable.
    const next = view.clientHeight > 0 ? { start: Math.max(0, start - 2), end: Math.min(layout.length, (end < 0 ? layout.length : end) + 2) } : { start: 0, end: layout.length };
    const firstId = layout[next.start]?.id;
    setRange(current => current.start === next.start && current.end === next.end && current.first === firstId ? current : { ...next, first: firstId });
    const row = layout[start];
    geometry.current.anchor = !(follow.current && canFollow) && row ? { id: row.id, offset: row.top - top } : null;
    geometry.current.bottomDistance = Math.max(0, view.scrollHeight - view.scrollTop - view.clientHeight);
    return row?.index;
  }, [root, listOffset, follow, canFollow]);
  const jump = useCallback((id: string, offset: number) => {
    const view = transcriptViewport(root);
    const row = geometry.current.rows.find(row => row.id === id);
    if (!view || !row) return false;
    view.scrollTop = listOffset(view) + row.top - offset;
    onAdjusted(view.scrollTop); update();
    return true;
  }, [root, listOffset, onAdjusted, update]);

  useLayoutEffect(() => {
    const view = transcriptViewport(root);
    const saved = geometry.current.anchor;
    const bottomDistance = geometry.current.bottomDistance;
    const hadRows = geometry.current.rows.length > 0;
    geometry.current.rows = rows;
    if (!view) return;
    if (follow.current && canFollow) view.scrollTop = view.scrollHeight;
    else if (saved) {
      const row = rows.find(row => row.id === saved.id);
      if (row) view.scrollTop = listOffset(view) + row.top - saved.offset;
      else if (hadRows) view.scrollTop = Math.max(0, view.scrollHeight - view.clientHeight - bottomDistance);
    }
    onAdjusted(view.scrollTop); update();
  }, [rows, root, listOffset, follow, canFollow, onAdjusted, update]);

  const measure = useCallback(function measure() {
    frame.current = null;
    const changes = new Map<string, number>();
    const deadline = performance.now() + 8;
    let count = 0;
    for (const [element, size] of measurements.current) {
      measurements.current.delete(element);
      const id = element.dataset.turnId;
      if (id && elements.current.get(id) === element && element.isConnected && size > 0) {
        const scale = window.devicePixelRatio || 1;
        changes.set(id, Math.round(size * scale) / scale);
      }
      if (++count >= 8 || performance.now() >= deadline) break;
    }
    if (changes.size) setHeights(current => {
      const valid = new Set(geometry.current.rows.map(row => row.id));
      let next = [...current.keys()].some(id => !valid.has(id)) ? new Map([...current].filter(([id]) => valid.has(id))) : undefined;
      for (const [id, height] of changes) if ((current.get(id) ?? 280) !== height) { next ??= new Map(current); next.set(id, height); }
      return next ?? current;
    });
    if (measurements.current.size) frame.current = requestAnimationFrame(measure);
  }, []);
  useLayoutEffect(() => {
    const view = transcriptViewport(root); if (!view) return;
    const pendingMeasurements = measurements.current;
    const resize = new ResizeObserver(entries => {
      for (const entry of entries) {
        if (entry.target === view) { if (follow.current && canFollow) view.scrollTop = view.scrollHeight; update(); onAdjusted(view.scrollTop); continue; }
        if (!(entry.target instanceof HTMLElement)) continue;
        const height = entry.borderBoxSize?.[0]?.blockSize ?? entry.target.getBoundingClientRect().height;
        pendingMeasurements.set(entry.target, height);
      }
      if (pendingMeasurements.size && frame.current === null) frame.current = requestAnimationFrame(measure);
    });
    observer.current = resize; resize.observe(view);
    if (list.current) resize.observe(list.current);
    return () => {
      resize.disconnect(); observer.current = null; elements.current = new Map(); pendingMeasurements.clear();
      if (frame.current !== null) cancelAnimationFrame(frame.current);
      frame.current = null;
    };
  }, [root, measure, update, onAdjusted, follow, canFollow]);
  useLayoutEffect(() => {
    const current = new Map<string, HTMLElement>();
    for (const element of root.current?.querySelectorAll<HTMLElement>("[data-turn-id]") ?? []) {
      const id = element.dataset.turnId!;
      current.set(id, element);
      if (elements.current.get(id) === element) continue;
      observer.current?.observe(element);
      measurements.current.set(element, element.getBoundingClientRect().height);
    }
    for (const [id, element] of elements.current) if (current.get(id) !== element) observer.current?.unobserve(element);
    elements.current = current;
    if (measurements.current.size && frame.current === null) measure();
  });
  useLayoutEffect(() => {
    const content = list.current; if (!content) return;
    const retainInteractions = () => {
      const selection = document.getSelection();
      const selected = selection && !selection.isCollapsed && selection.rangeCount ? selection.getRangeAt(0) : null;
      const keep = [...content.querySelectorAll<HTMLElement>("[data-turn-id]")].filter(row => row.contains(document.activeElement)
        || row.querySelector('[aria-expanded="true"], details[open]')
        || (selected && selected.intersectsNode(row))).map(row => row.dataset.turnId!);
      setPinned(current => current.length === keep.length && current.every((id, index) => id === keep[index]) ? current : keep);
    };
    document.addEventListener("selectionchange", retainInteractions);
    content.addEventListener("focusin", retainInteractions);
    content.addEventListener("focusout", retainInteractions);
    const mutations = new MutationObserver(retainInteractions);
    mutations.observe(content, { subtree: true, attributes: true, attributeFilter: ["aria-expanded", "open"] });
    return () => {
      document.removeEventListener("selectionchange", retainInteractions);
      content.removeEventListener("focusin", retainInteractions); content.removeEventListener("focusout", retainInteractions);
      mutations.disconnect();
    };
  }, []);
  const kept = new Set([...retained, ...pinned]);
  const first = rows.findIndex(row => row.id === range.first);
  const start = first < 0 ? range.start : first;
  const end = start + range.end - range.start;
  return { list, height: rows[rows.length - 1]?.bottom ?? 0, rows: rows.filter(row => (row.index >= start && row.index < end) || kept.has(row.id)), update, jump };
}
