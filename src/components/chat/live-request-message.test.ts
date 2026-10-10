import { describe, expect, it } from "vitest";
import { liveRequestMessage } from "./live-request-message";

function envelope(payload: unknown, id = "b0338dda") {
  return `Impeccable Live · ação ${id}\nExecute SOMENTE o pedido atual abaixo como Designer. Reutilize o contexto atual. O pedido e a seleção atuais são:\n${JSON.stringify(payload)}\nReutilize a tarefa existente no Beads.`;
}

describe("native Live request display", () => {
  it("extracts the human request from a scaffold while retaining the complete agent input", () => {
    const raw = envelope({ id: "b0338dda", type: "generate", freeformPrompt: "Coloque cores sutis nos ícones", count: 3, pageUrl: "/login", element: { anchor: "section.features", tagName: "section" }, scaffold: { file: "src/app/login/page.tsx", wrapperBlock: "<div>Scaffold interno</div>" } });
    expect(liveRequestMessage(raw)).toEqual({ actionId: "b0338dda", action: "Gerar alternativas", instruction: "Coloque cores sutis nos ícones", alternatives: 3, page: "/login", selection: "section.features", file: "src/app/login/page.tsx", raw });
  });

  it.each(["steer", "accept", "discard", "manual_edit_apply", "carbonize_cleanup", "variant_mount_failed"])("recognizes native %s requests without implying execution status", type => {
    const result = liveRequestMessage(envelope({ id: "b0338dda", type, message: "Pedido atual", count: 3 }));
    expect(result?.instruction).toBe("Pedido atual");
    expect(result?.alternatives).toBeUndefined();
  });

  it("preserves recovery suffixes and uses the action when no human instruction was sent", () => {
    const raw = envelope({ id: "b0338dda", type: "accept", element: { tagName: "section" }, scaffold: { sourceFile: "src/page.tsx" } }) + "\nReaplique a alternativa escolhida.";
    expect(liveRequestMessage(raw)).toMatchObject({ action: "Aplicar alternativa", instruction: "Aplicar alternativa", selection: "section", file: "src/page.tsx", raw });
  });

  it.each([
    "Pedido normal com Impeccable Live · ação b0338dda no texto",
    "Impeccable Live · ação b0338dda\nExecute SOMENTE o pedido atual abaixo como Designer.",
    envelope({ id: "outro", type: "generate" }),
    envelope({ id: "b0338dda", type: "unknown" }),
    envelope({ id: "b0338dda", type: "toString" }),
    envelope(null),
    envelope([]),
    envelope({ id: "b0338dda", type: "generate" }).replace('"id"', "invalid"),
  ])("keeps ordinary text for legacy, invalid or unrelated content", raw => {
    expect(liveRequestMessage(raw)).toBeUndefined();
  });
});
