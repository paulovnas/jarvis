import { act, render, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import userEvent from "@testing-library/user-event";
import { ROLE_LABELS } from "@/core/workflow";
import { beforeEach, expect, it, vi } from "vitest";
import { AgentSettings } from "./AgentSettings";
import { useAgentModels } from "@/hooks/use-agent-models";
import type { ProviderAccount } from "@/core/provider-accounts";

vi.mock("@/hooks/use-agent-models", () => ({ useAgentModels: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
const save = vi.fn().mockResolvedValue(true);
const accounts: ProviderAccount[] = [{ alias:"openai-codex-personal",providerKind:"openai-codex",enabled:true,createdAt:0,email:null,accountType:"personal",modelsAvailable:true,models:[{ id:"gpt-5.6-sol",name:"GPT 5.6 Sol",reasoningLevels:["high","xhigh"],defaultReasoningLevel:"high" }] }];
beforeEach(() => { call.mockReset(); save.mockClear(); vi.mocked(useAgentModels).mockReturnValue({ data:{},error:null,saving:false,save,refresh:vi.fn() }); });

it("groups the thirteen immutable agents by flow and offers useful role-specific model guidance", async () => {
  const user = userEvent.setup(); render(<AgentSettings accounts={accounts} />);
  const standard = screen.getByRole("region",{ name:"Agentes do fluxo Padrão" });
  const designer = screen.getByRole("region",{ name:"Agentes do fluxo Designer" });
  const planned = screen.getByRole("region",{ name:"Agentes do fluxo Planejado" });
  const complete = screen.getByRole("region",{ name:"Agentes do fluxo Completo" });
  const publication = screen.getByRole("region",{ name:"Agentes do fluxo Publicação" });
  expect(within(standard).getAllByRole("button",{name:/^Modelo de/})).toHaveLength(1);
  expect(within(designer).getAllByRole("button",{name:/^Modelo de/})).toHaveLength(1);
  expect(within(planned).getAllByRole("button",{name:/^Modelo de/})).toHaveLength(3);
  expect(within(complete).getAllByRole("button",{name:/^Modelo de/})).toHaveLength(7);
  expect(within(publication).getAllByRole("button",{name:/^Modelo de/})).toHaveLength(1);
  expect(within(publication).getByText(/Prepara commits, pull requests e merges/i)).toBeInTheDocument();
  expect(within(complete).getByText(/Transforma o plano em uma especificação executável/)).toBeInTheDocument();
  expect(within(complete).getByText(/Revisa a implementação em um contexto independente/)).toBeInTheDocument();
  await user.hover(screen.getByRole("button",{name:"Como escolher o modelo de Revisor no fluxo Completo"}));
  expect(await screen.findByText(/análise crítica independente/)).toHaveTextContent("GPT 5.6 Sol com Extra alto");
  expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
});

it("identifica o provedor escolhido pelo sufixo ao lado do modelo", () => {
  vi.mocked(useAgentModels).mockReturnValue({ data: { "planned/planner": { account: "openai-codex-personal", model: "gpt-5.6-sol", reasoning: "high" } }, error: null, saving: false, save, refresh: vi.fn() });
  render(<AgentSettings accounts={accounts} />);
  const group = screen.getByRole("region", { name: "Agentes do fluxo Planejado" });
  expect(within(group).getByTitle("openai-codex-personal")).toHaveTextContent("personal");
  expect(within(group).getByRole("button", { name: "Modelo de Planejador no fluxo Planejado" })).toHaveTextContent("GPT 5.6 Sol");
});

it("saves the selected provider, model and supported effort only for the chosen agent and flow", async () => {
  const user = userEvent.setup(); render(<AgentSettings accounts={accounts} />);
  screen.getByRole("button",{name:"Modelo de Revisor no fluxo Completo"}).focus(); await user.keyboard("{Enter}");
  (await screen.findByRole("menuitem",{name:"openai-codex-personal"})).focus(); await user.keyboard("{ArrowRight}");
  (await screen.findByRole("menuitem",{name:"GPT 5.6 Sol"})).focus(); await user.keyboard("{ArrowRight}");
  const group = await screen.findByRole("group",{name:"Raciocínio"});
  expect(within(group).queryByRole("menuitem",{name:"Médio"})).not.toBeInTheDocument();
  await user.click(within(group).getByRole("menuitem",{name:"Extra alto"}));
  expect(save).toHaveBeenCalledWith("complete","reviewer",{account:"openai-codex-personal",model:"gpt-5.6-sol",reasoning:"xhigh"});
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(call).not.toHaveBeenCalled();
});

it("opens the chosen agent instructions on demand as read-only formatted Markdown", async () => {
  let resolve!: (value: unknown) => void;
  call.mockImplementationOnce(() => new Promise(done => { resolve = done; }));
  const user = userEvent.setup(); render(<AgentSettings accounts={accounts} />);
  expect(call).not.toHaveBeenCalled();
  const trigger = screen.getByRole("button", { name: "Ver instruções de Designer no fluxo Planejado" });
  trigger.focus(); await user.keyboard("{Enter}");
  const dialog = screen.getByRole("dialog", { name: "Designer Planejado" });
  expect(within(dialog).getByRole("status", { name: "Carregando instruções do agente" })).toBeInTheDocument();
  expect(call).toHaveBeenCalledExactlyOnceWith("get_agent_instructions", { flow: "planned", role: "designer" });
  await act(async () => resolve([{ title: "Papel do agente", content: "Preserve a **identidade visual**.\n\n- Revise os componentes\n- Consulte `design.md`" }]));
  await within(dialog).findByText("identidade visual", { selector: "strong" });
  expect(within(dialog).getByRole("heading", { name: "Papel do agente" })).toBeVisible();
  expect(within(dialog).getAllByRole("listitem")).toHaveLength(2);
  expect(within(dialog).getByText("design.md", { selector: "code" })).toBeInTheDocument();
  expect(within(dialog).queryByRole("textbox")).not.toBeInTheDocument();
  expect(within(dialog).queryByRole("button", { name: /salvar|editar/i })).not.toBeInTheDocument();
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(trigger).toHaveFocus();
  expect(save).not.toHaveBeenCalled();
});

it("lets the user retry an instruction load failure without changing the model", async () => {
  call.mockRejectedValueOnce(new Error("offline"));
  const user = userEvent.setup(); render(<AgentSettings accounts={accounts} />);
  await user.click(screen.getByRole("button", { name: "Ver instruções de Revisor no fluxo Completo" }));
  const dialog = screen.getByRole("dialog", { name: "Revisor Completo" });
  await within(dialog).findByRole("alert");
  call.mockResolvedValueOnce([{ title: "Papel do agente", content: "Revisão independente." }]);
  await user.click(within(dialog).getByRole("button", { name: "Tentar novamente" }));
  await within(dialog).findByText("Revisão independente.");
  expect(save).not.toHaveBeenCalled();
});

it("gives every agent a distinct accent within each flow", () => {
  render(<AgentSettings accounts={accounts} />);
  for (const flow of ["Padrão", "Designer", "Planejado", "Completo", "Publicação"]) {
    const group = screen.getByRole("region", { name: `Agentes do fluxo ${flow}` });
    const colors = Object.values(ROLE_LABELS).flatMap(label => {
      const name = within(group).queryByText(label, { selector: "[data-slot=card-title]" });
      return name ? [getComputedStyle(name).color] : [];
    });
    expect(colors.length).toBeGreaterThan(0);
    expect(new Set(colors).size).toBe(colors.length);
  }
});

it("represents loading with the card structure", () => {
  vi.mocked(useAgentModels).mockReturnValue({ data:null,error:null,saving:false,save,refresh:vi.fn() });
  render(<AgentSettings accounts={accounts} />);
  expect(screen.getByRole("status",{name:"Carregando modelos dos agentes"})).toBeInTheDocument();
});
