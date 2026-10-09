import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import type { CoreActivity } from "@/core/chat";
import { AssistantWorkCollapse } from "./AssistantWorkCollapse";
import { CoreActivitySummary } from "./CoreActivitySummary";

const prepared: CoreActivity = {
  component: "open-design", action: "design_preparation", status: "applied",
  summary: "Identidade do projeto carregada", sources: ["project:DESIGN.md", "open-design:skills/design-brief"], durationMs: 3,
};
const step = { thinking: "", commentary: "Vou ajustar o layout.", tools: [], coreActivities: [prepared] };

it("groups Impeccable guidance and tool use without adding a legacy design resource", async () => {
  const user = userEvent.setup();
  render(<CoreActivitySummary steps={[{ ...step, coreActivities: [{ ...prepared, component: "impeccable", action: "design_quality", summary: "Revisão visual orientada pelo Impeccable", sources: ["project:DESIGN.md"] }], tools: [
    { id: "design", name: "design_read", status: "completed" },
    { id: "quality", name: "impeccable", status: "completed" },
  ] }]} />);
  const trigger = screen.getByRole("button", { name: /Recursos do Core/ });
  expect(trigger).toHaveTextContent("· 1");
  await user.click(trigger);
  expect(screen.getByText("Impeccable")).toBeVisible();
  expect(screen.getByText("Revisão visual orientada pelo Impeccable", { exact: false })).toBeVisible();
  expect(screen.queryByText("Open Design")).not.toBeInTheDocument();
});

it("groups the whole OpenMontage production under one Core resource", async () => {
  const user = userEvent.setup();
  render(<CoreActivitySummary steps={[{ ...step, coreActivities: [{ ...prepared, component: "openmontage", action: "production", summary: "Roteiro e mídia preparados", sources: ["project:video/checkpoint.json"] }], tools: [
    { id: "tools", name: "video_tools", status: "completed" },
    { id: "narration", name: "video_run", args: { tool: "tts" }, status: "completed" },
    { id: "music", name: "video_run", args: { tool: "music" }, status: "running" },
  ] }]} />);
  expect(screen.getByRole("button", { name: /Recursos do Core/ })).toHaveTextContent("· 1");
  await user.click(screen.getByRole("button", { name: /Recursos do Core/ }));
  expect(screen.getByText("OpenMontage")).toBeVisible();
  expect(screen.getByText("Solicitado pelo agente: 2 chamadas concluídas · 1 em andamento.")).toBeVisible();
  expect(screen.queryByText("Hyperframes")).not.toBeInTheDocument();
  expect(screen.queryByText("Audiovisual")).not.toBeInTheDocument();
});

it("shows real automatic work separately from model calls, with sources on demand", async () => {
  const user = userEvent.setup();
  render(<CoreActivitySummary steps={[{ ...step, tools: [{ id: "d1", name: "design_read", status: "completed" }] }]} />);
  const trigger = screen.getByRole("button", { name: /Recursos do Core/ });
  expect(trigger).toHaveAttribute("aria-expanded", "false");
  expect(screen.queryByText("project:DESIGN.md")).not.toBeInTheDocument();
  await user.click(trigger);
  expect(screen.getByText("Automático")).toBeVisible();
  expect(screen.getByText(/Identidade do projeto carregada/)).toBeVisible();
  expect(screen.getByText("Solicitado pelo agente: 1 chamada concluída.")).toBeVisible();
  expect(screen.getByText("project:DESIGN.md")).toBeVisible();
});

it("keeps Core warnings live and moves the summary inside completed work", async () => {
  const user = userEvent.setup();
  const work = { durationSeconds: 12, steps: [step] };
  const { rerender } = render(<AssistantWorkCollapse isStreaming work={work} />);
  expect(screen.getByText("Vou ajustar o layout.")).toBeVisible();
  const warning: CoreActivity = { ...prepared, component: "lsp", action: "post_mutation_diagnostics", status: "unavailable", summary: "O servidor não respondeu; os arquivos estão salvos.", sources: [] };
  const updated = { ...work, steps: [{ ...step, coreActivities: [prepared, warning] }] };
  rerender(<AssistantWorkCollapse isStreaming work={updated} />);
  expect(screen.getByLabelText("Há recursos com avisos")).toBeVisible();
  await user.click(screen.getByRole("button", { name: /Recursos do Core/ }));
  expect(screen.getByText(/O servidor não respondeu; os arquivos estão salvos/)).toBeVisible();
  rerender(<AssistantWorkCollapse work={updated} />);
  expect(screen.queryByRole("button", { name: /Recursos do Core/ })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Trabalhou por 12s/ }));
  expect(screen.getByRole("button", { name: /Recursos do Core/ })).toHaveAttribute("aria-expanded", "false");
});

it("aggregates long runs without claiming missing or failed Core work succeeded", async () => {
  const user = userEvent.setup();
  const receipts = Array.from({ length: 100 }, () => ({ ...prepared, status: "reused" as const }));
  render(<CoreActivitySummary steps={[{ ...step, coreActivities: receipts, tools: [{ id: "docs", name: "context7_query_docs", status: "error" }] }]} />);
  await user.click(screen.getByRole("button", { name: /Recursos do Core/ }));
  const list = screen.getByRole("list", { name: "Uso dos recursos do Core" });
  expect(list.children).toHaveLength(2);
  expect(screen.getAllByText(/Identidade do projeto carregada/)).toHaveLength(1);
  const context7 = screen.getByText("Context7").closest("li")!;
  expect(within(context7).queryByText("Automático")).not.toBeInTheDocument();
  expect(within(context7).getByText(/0 chamadas concluídas · 1 não concluída/)).toBeVisible();
  expect(screen.queryByText("Ponytail")).not.toBeInTheDocument();
});

it("does not invent activity for legacy or empty histories", () => {
  const { container } = render(<CoreActivitySummary steps={[{ thinking: "", commentary: "", tools: [] }]} />);
  expect(container).toBeEmptyDOMElement();
});

it("shows manual hook diagnostics on demand without presenting them as agent tool calls", async () => {
  const user = userEvent.setup();
  const warning: CoreActivity = { component: "manual-hooks", action: "PreToolUse", status: "issues", summary: "O hook não concluiu: tempo esgotado.", sources: [], durationMs: 0 };
  render(<CoreActivitySummary steps={[{ thinking: "", commentary: "", tools: [], coreActivities: [warning] }]} />);
  expect(screen.getByLabelText("Há recursos com avisos")).toBeVisible();
  expect(screen.queryByText(/O hook não concluiu/)).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Recursos do Core/ }));
  expect(screen.getByText("Hooks manuais")).toBeVisible();
  expect(screen.getByText("Automático")).toBeVisible();
  expect(screen.getByText(/O hook não concluiu/)).toHaveTextContent("Diagnósticos encontrados: O hook não concluiu: tempo esgotado.");
  expect(screen.queryByText(/Solicitado pelo agente/)).not.toBeInTheDocument();
});

it("keeps named plugin and hook activity inside the single Core disclosure", async () => {
  const user = userEvent.setup();
  const hook: CoreActivity = { component: "hooks", action: "PreToolUse", status: "applied", summary: "Hook concluído.", sources: [], durationMs: 4, resourceId: "quality", resourceName: "Qualidade" };
  const plugin: CoreActivity = { component: "plugins", action: "mcp_call", status: "applied", summary: "Ferramenta MCP executada.", sources: [], durationMs: 15, resourceId: "firebase-mcp", resourceName: "Firebase", pluginId: "firebase@official" };
  render(<CoreActivitySummary steps={[{ ...step, coreActivities: [prepared, hook, plugin] }]} />);
  expect(screen.getAllByRole("button", { name: /Recursos do Core/ })).toHaveLength(1);
  expect(screen.getByRole("button", { name: /Recursos do Core/ })).toHaveTextContent("· 3");
  expect(screen.queryByText("Plugins")).not.toBeInTheDocument();
  expect(screen.queryByText(/Hook concluído/)).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Recursos do Core/ }));
  const list = screen.getByRole("list", { name: "Uso dos recursos do Core" });
  expect(list.children).toHaveLength(3);
  const plugins = screen.getByText("Plugins").closest("li")!;
  expect(within(plugins).getByText(/Firebase · Aplicado:/)).toBeVisible();
  expect(within(plugins).getByText(/Plugin: firebase@official/)).toBeVisible();
  expect(within(plugins).queryByText("Automático")).not.toBeInTheDocument();
  expect(screen.getByText("Hooks")).toBeVisible();
  expect(screen.getByText(/Qualidade · Aplicado:/)).toBeVisible();
});

it("preserves different hooks, compacts repeated executions and retains an earlier plugin failure", async () => {
  const user = userEvent.setup();
  const hook: CoreActivity = { component: "hooks", action: "PreToolUse", status: "applied", summary: "Hook concluído.", sources: [], durationMs: 4, resourceId: "quality", resourceName: "Qualidade" };
  const plugin: CoreActivity = { component: "plugins", action: "mcp_call", status: "unavailable", summary: "Ferramenta MCP indisponível.", sources: [], durationMs: 15, resourceId: "firebase-mcp", resourceName: "Firebase", pluginId: "firebase@official" };
  render(<CoreActivitySummary steps={[{ ...step, coreActivities: [
    ...Array.from({ length: 50 }, () => hook),
    { ...hook, resourceId: "git", resourceName: "Proteção Git" },
    { ...hook, resourceId: "plugin-hook", resourceName: "Contexto", pluginId: "helper@local" },
    plugin, { ...plugin, status: "applied", summary: "Ferramenta MCP executada." },
  ] }]} />);
  expect(screen.getByLabelText("Há recursos com avisos")).toBeVisible();
  await user.click(screen.getByRole("button", { name: /Recursos do Core/ }));
  expect(screen.getAllByText(/Qualidade · Aplicado:/)).toHaveLength(1);
  expect(screen.getByText(/Proteção Git · Aplicado:/)).toBeVisible();
  expect(screen.getByText(/Contexto · Aplicado:/)).toBeVisible();
  expect(screen.getByText(/Plugin: helper@local/)).toBeVisible();
  expect(screen.getByText(/Firebase · Indisponível:/)).toBeVisible();
  expect(screen.getByText(/Firebase · Aplicado:/)).toBeVisible();
});

it("does not infer plugin usage from a tool name or its output", () => {
  const { container } = render(<CoreActivitySummary steps={[{ thinking: "", commentary: "", tools: [{ id: "legacy", name: "mcp__plugin_firebase_list_projects", status: "completed", output: "Plugin firebase@official available" }] }]} />);
  expect(container).toBeEmptyDOMElement();
});

it("keeps resources with missing IDs separate by name and plugin owner", async () => {
  const user = userEvent.setup();
  const hook: CoreActivity = { component: "hooks", action: "PreToolUse", status: "applied", summary: "Hook concluído.", sources: [], durationMs: 4, resourceName: "Qualidade" };
  const plugin: CoreActivity = { ...hook, component: "plugins", action: "skill_loaded", summary: "Skill carregada.", resourceName: "Design", pluginId: "first@local" };
  render(<CoreActivitySummary steps={[{ ...step, coreActivities: [hook,
    { ...hook, resourceId: null, resourceName: "Proteção Git" }, plugin,
    { ...plugin, resourceId: null, pluginId: "second@local" },
  ] }]} />);
  await user.click(screen.getByRole("button", { name: /Recursos do Core/ }));
  expect(screen.getByText(/Qualidade · Aplicado:/)).toBeVisible();
  expect(screen.getByText(/Proteção Git · Aplicado:/)).toBeVisible();
  expect(screen.getByText(/Plugin: first@local/)).toBeVisible();
  expect(screen.getByText(/Plugin: second@local/)).toBeVisible();
});

it("shows video tool calls under the managed OpenMontage resource", async () => {
  const user = userEvent.setup();
  render(<CoreActivitySummary steps={[{ thinking: "", commentary: "", tools: [{ id: "video", name: "video_run", status: "completed" }] }]} />);
  await user.click(screen.getByRole("button", { name: /Recursos do Core/ }));
  expect(screen.getByText("OpenMontage")).toBeVisible();
  expect(screen.getByText("Solicitado pelo agente: 1 chamada concluída.")).toBeVisible();
});

it("keeps structural Graft activity separate from Context-mode reductions and preserves fallback warnings", async () => {
  const user = userEvent.setup();
  const graph: CoreActivity = { component: "graft", action: "code_discovery", status: "reused", summary: "Mapa estrutural do projeto reutilizado.", sources: ["src/auth.ts"], durationMs: 4 };
  const fallback: CoreActivity = { ...graph, action: "structural_fallback", status: "unavailable", summary: "Índice indisponível; leitura nativa utilizada.", sources: [] };
  render(<CoreActivitySummary steps={[{ thinking: "", commentary: "", coreActivities: [graph, fallback], tools: [
    { id: "graft", name: "graft_find_code", status: "completed" },
    { id: "ctx", name: "ctx_search", status: "completed" },
  ] }]} />);
  await user.click(screen.getByRole("button", { name: /Recursos do Core/ }));
  const graft = screen.getByText("Graft").closest("li")!;
  expect(within(graft).getByText("Automático")).toBeVisible();
  expect(within(graft).getByText(/Mapa estrutural do projeto reutilizado/)).toBeVisible();
  expect(within(graft).getByText(/Índice indisponível; leitura nativa utilizada/)).toBeVisible();
  expect(within(graft).getByText("src/auth.ts")).toBeVisible();
  expect(within(graft).getByText("Solicitado pelo agente: 1 chamada concluída.")).toBeVisible();
  const context = screen.getByText("Context-mode").closest("li")!;
  expect(within(context).queryByText("Automático")).not.toBeInTheDocument();
  expect(within(context).getByText("Solicitado pelo agente: 1 chamada concluída.")).toBeVisible();
  expect(screen.getByLabelText("Há recursos com avisos")).toBeVisible();
});

it("separates audio generation from Hyperframes verification without claiming pending or failed calls succeeded", async () => {
  const user = userEvent.setup();
  const narration: CoreActivity = { ...prepared, component: "audiovisual", action: "narration", summary: "Narração PT-BR gerada localmente", sources: ["presentation/audio/narration.wav"] };
  render(<CoreActivitySummary steps={[{ ...step, coreActivities: [narration], tools: [
    { id: "narration", name: "video_audio", args: { action: "narrate" }, status: "completed" },
    { id: "music", name: "video_audio", args: { action: "music" }, status: "running" },
    { id: "failed", name: "video_audio", status: "error" },
    { id: "verify", name: "video_presentation", status: "completed" },
  ] }]} />);
  await user.click(screen.getByRole("button", { name: /Recursos do Core/ }));
  const audio = screen.getByText("Audiovisual").closest("li")!;
  expect(within(audio).getByText("Automático")).toBeVisible();
  expect(within(audio).getByText(/Narração PT-BR gerada localmente/)).toBeVisible();
  expect(within(audio).getByText("presentation/audio/narration.wav")).toBeVisible();
  expect(within(audio).getByText("Solicitado pelo agente: 1 chamada concluída · 1 em andamento · 1 não concluída(s).")).toBeVisible();
  const video = screen.getByText("Hyperframes").closest("li")!;
  expect(within(video).getByText("Solicitado pelo agente: 1 chamada concluída.")).toBeVisible();
  expect(within(video).queryByText("Automático")).not.toBeInTheDocument();
  expect(screen.getByLabelText("Há recursos com avisos")).toBeVisible();
});

function diagnostic(path: string, status: CoreActivity["status"], summary: string): CoreActivity {
  return { ...prepared, component: "lsp", action: "file_diagnostics", sources: [path], status, summary, fingerprint: "current" };
}

it("distinguishes a partial LSP check from code diagnostics and a failed server", async () => {
  const user = userEvent.setup();
  render(<CoreActivitySummary steps={[{ ...step, coreActivities: [
    diagnostic("pessoas.ts", "issues", "2 erros encontrados: exports ausentes."),
    diagnostic("service.ts", "pending", "Aguardando diagnósticos da versão atual."),
  ] }]} />);
  await user.click(screen.getByRole("button", { name: /Recursos do Core/ }));
  expect(screen.getByText(/Verificação parcial/)).toBeVisible();
  expect(screen.getByText(/2 erros encontrados/)).toBeVisible();
  expect(screen.getByText(/Aguardando diagnósticos/)).toBeVisible();
  expect(screen.queryByText(/Falha do servidor/)).not.toBeInTheDocument();
});

it("resolves an old warning only after a fresh successful check of the same file", async () => {
  const user = userEvent.setup();
  const warnings = [diagnostic("a.ts", "issues", "Export ausente."), diagnostic("b.ts", "unavailable", "Servidor encerrou.")];
  const renderSteps = (receipts: CoreActivity[]) => [{ ...step, coreActivities: receipts }];
  const { rerender } = render(<CoreActivitySummary steps={renderSteps(warnings)} />);
  await user.click(screen.getByRole("button", { name: /Recursos do Core/ }));
  const otherFileChecked = [...warnings, diagnostic("b.ts", "applied", "Nenhum diagnóstico encontrado.")];
  rerender(<CoreActivitySummary steps={renderSteps(otherFileChecked)} />);
  expect(screen.getByLabelText("Há recursos com avisos")).toBeVisible();
  expect(screen.getByText(/Export ausente/)).toBeVisible();
  expect(screen.getByRole("button", { name: /1 aviso resolvido/ })).toBeVisible();

  const stillPending = [...otherFileChecked, diagnostic("a.ts", "pending", "Aguardando diagnóstico."), diagnostic("a.ts", "reused", "Resultado reutilizado.")];
  rerender(<CoreActivitySummary steps={renderSteps(stillPending)} />);
  expect(screen.getByLabelText("Há recursos com avisos")).toBeVisible();
  expect(screen.getByText(/Export ausente/)).toBeVisible();

  rerender(<CoreActivitySummary steps={renderSteps([...stillPending, diagnostic("a.ts", "applied", "Nenhum diagnóstico encontrado.")])} />);
  expect(screen.queryByLabelText("Há recursos com avisos")).not.toBeInTheDocument();
  expect(screen.queryByText(/Export ausente/)).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /2 avisos resolvidos/ }));
  expect(screen.getByText(/Export ausente/)).toBeVisible();
  expect(screen.getByText(/Servidor encerrou/)).toBeVisible();
});

it("groups image processing under ComfyUI without mixing audiovisual resources", async () => {
  const user = userEvent.setup();
  render(<CoreActivitySummary steps={[{ thinking: "", commentary: "", tools: [{ id: "image-process", name: "image_process", status: "completed" }] }]} />);
  await user.click(screen.getByRole("button", { name: /Recursos do Core/ }));
  expect(screen.getByText("ComfyUI")).toBeVisible();
  expect(screen.queryByText("Hyperframes")).not.toBeInTheDocument();
  expect(screen.queryByText("Audiovisual")).not.toBeInTheDocument();
  expect(screen.getByText("Solicitado pelo agente: 1 chamada concluída.")).toBeVisible();
});

const generatedImageReceipt = { kind: "generated_image", accountAlias: "configured-images", model: "image-model", images: [{ id: "image-1", conversationId: "chat-1", name: "banner.png", mime: "image/png", kind: "image", size: 1200 }], text: "" };

it("shows the mandatory internal ComfyUI workflow when a completed generation receipt proves it", async () => {
  const user = userEvent.setup();
  render(<CoreActivitySummary steps={[{ thinking: "", commentary: "", tools: [{ id: "delegated-image", name: "generate_image", status: "completed", output: JSON.stringify({ ...generatedImageReceipt, processing: { engine: "comfyui" } }) }] }]} />);
  await user.click(screen.getByRole("button", { name: /Recursos do Core/ }));
  expect(screen.getByText("ComfyUI")).toBeVisible();
  expect(screen.getByText("Solicitado pelo agente: 1 chamada concluída.")).toBeVisible();
});

it.each([
  JSON.stringify(generatedImageReceipt),
  JSON.stringify({ ...generatedImageReceipt, processing: { engine: "other-engine" } }),
  JSON.stringify({ kind: "generated_image", processing: { engine: "comfyui" } }),
  "invalid json",
])("does not invent ComfyUI usage for a raw, legacy or invalid image receipt (%s)", output => {
  render(<CoreActivitySummary steps={[{ thinking: "", commentary: "", tools: [{ id: "raw-image", name: "generate_image", status: "completed", output }] }]} />);
  expect(screen.queryByRole("button", { name: /Recursos do Core/ })).not.toBeInTheDocument();
});
