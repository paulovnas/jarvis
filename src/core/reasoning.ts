const LABELS: Record<string, string> = {
  none: "Desativado", off: "Desativado", minimal: "Mínimo", low: "Baixo",
  medium: "Médio", high: "Alto", xhigh: "Extra alto", max: "Máximo", ultra: "Ultra",
};
export function reasoningLabel(level: string): string {
  return Object.prototype.hasOwnProperty.call(LABELS, level) ? LABELS[level] : level;
}

export function selectableReasoningLevels(levels: readonly string[]): string[] {
  return levels.filter(level => level !== "ultra");
}

export function defaultReasoning(model: { reasoningLevels: readonly string[]; defaultReasoningLevel: string | null }): string | null {
  const levels = selectableReasoningLevels(model.reasoningLevels);
  if (model.defaultReasoningLevel === "ultra") return levels.includes("max") ? "max" : levels[levels.length - 1] ?? null;
  return model.defaultReasoningLevel && levels.includes(model.defaultReasoningLevel) ? model.defaultReasoningLevel : levels[0] ?? null;
}
