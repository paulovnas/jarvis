/** Unlisted items keep their default order on the chosen side of arranged items. */
export function orderedItems<T>(items: readonly T[], order: readonly string[] | undefined, key: (item: T) => string, unlisted: "first" | "last" = "last"): T[] {
  const rank = new Map((order ?? []).map((id, index) => [id, index]));
  const fallback = unlisted === "first" ? -1 : Infinity;
  return [...items].sort((a, b) => (rank.get(key(a)) ?? fallback) - (rank.get(key(b)) ?? fallback));
}

export function moveItem(ids: readonly string[], from: string, to: string): string[] {
  const source = ids.indexOf(from), target = ids.indexOf(to);
  if (source < 0 || target < 0 || source === target) return [...ids];
  const next = [...ids];
  next.splice(source, 1);
  next.splice(target, 0, from);
  return next;
}
