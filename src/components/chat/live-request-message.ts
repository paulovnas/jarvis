const actions = {
  generate: "Gerar alternativas",
  steer: "Ajustar página",
  accept: "Aplicar alternativa",
  discard: "Descartar alternativas",
  manual_edit_apply: "Aplicar edição",
  carbonize_cleanup: "Concluir alteração",
  variant_mount_failed: "Recuperar alternativa",
};

export interface LiveRequestMessage {
  actionId: string;
  action: string;
  instruction: string;
  selection?: string;
  page?: string;
  file?: string;
  alternatives?: number;
  raw: string;
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function text(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

/** Project the native Live envelope for display; never replace the agent's input. */
export function liveRequestMessage(content: string): LiveRequestMessage | undefined {
  const header = /^Impeccable Live · ação ([A-Za-z0-9_-]{1,128})(?:: [a-z_]+)?\r?\n/.exec(content);
  if (!header) return;
  const instructionStart = "Execute SOMENTE o pedido atual abaixo como Designer.";
  if (!content.startsWith(instructionStart, header[0].length)) return;
  const marker = "O pedido e a seleção atuais são:\n";
  const markerOffset = content.indexOf(marker, header[0].length);
  if (markerOffset < 0) return;
  const start = markerOffset + marker.length;
  const end = content.indexOf("\n", start);
  try {
    const payload: unknown = JSON.parse(content.slice(start, end < 0 ? undefined : end));
    if (!record(payload) || payload.id !== header[1] || typeof payload.type !== "string" || !Object.prototype.hasOwnProperty.call(actions, payload.type)) return;
    const action = actions[payload.type as keyof typeof actions];
    const element = record(payload.element) ? payload.element : {};
    const scaffold = record(payload.scaffold) ? payload.scaffold : {};
    return {
      actionId: header[1],
      action,
      instruction: text(payload.freeformPrompt) ?? text(payload.message) ?? text(payload.prompt) ?? text(payload.text) ?? action,
      selection: text(element.anchor) ?? text(element.selector) ?? text(payload.selector) ?? text(element.tagName),
      page: text(payload.pageUrl),
      file: text(scaffold.file) ?? text(scaffold.sourceFile) ?? text(payload.sourceFile),
      alternatives: payload.type === "generate" && typeof payload.count === "number" && Number.isSafeInteger(payload.count) && payload.count > 0 ? payload.count : undefined,
      raw: content,
    };
  } catch {
    // Legacy or malformed messages retain their ordinary text presentation.
    return;
  }
}
