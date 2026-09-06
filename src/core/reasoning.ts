const LABELS: Record<string, string> = {
  none: "Desativado", off: "Desativado", minimal: "Mínimo", low: "Baixo",
  medium: "Médio", high: "Alto", xhigh: "Extra alto", max: "Máximo", ultra: "Ultra",
};
export function reasoningLabel(level: string): string {
  return Object.prototype.hasOwnProperty.call(LABELS, level) ? LABELS[level] : level;
}
