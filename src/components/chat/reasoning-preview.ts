/** Use only the visible summary supplied by the provider. Keep full text in history. */
export function reasoningSections(summary: string): string[] {
  const starts = [...summary.matchAll(/^[ \t]*(?:\*\*[^\n]+|#{1,6}[ \t]+[^\n]+)/gm)].map(match => match.index);
  if (starts.length) {
    if (starts[0] > 0) starts.unshift(0);
    return starts.map((start, index) => summary.slice(start, starts[index + 1] ?? summary.length).trim()).filter(Boolean);
  }
  return summary.split(/\n\s*\n/).map(text => text.trim()).filter(Boolean);
}

export function reasoningPreview(summary: string): string {
  const headings = [...summary.matchAll(/(?:^|\n)\s*(?:\*\*([^\n]+?)(?:\*\*|$)|#{1,6}\s+([^\n]+))/g)];
  const lastHeading = headings[headings.length - 1];
  const text = lastHeading ? lastHeading[1] ?? lastHeading[2] : summary.trim().split(/\n\s*\n/).pop() ?? "";
  return text.replace(/[*_`#]/g, "").replace(/\s+/g, " ").trim().slice(0, 180);
}
