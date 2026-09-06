import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ROLE_LABELS } from "@/core/workflow";
import { beforeEach, expect, it, vi } from "vitest";
import { AgentSettings } from "./AgentSettings";
import { useAgentModels } from "@/hooks/use-agent-models";
import type { ProviderAccount } from "@/core/provider-accounts";

vi.mock("@/hooks/use-agent-models", () => ({ useAgentModels: vi.fn() }));
const save = vi.fn().mockResolvedValue(true);
const accounts: ProviderAccount[] = [{ alias:"openai-codex-personal",providerKind:"openai-codex",enabled:true,createdAt:0,email:null,accountType:"personal",modelsAvailable:true,models:[{ id:"gpt-5.6-sol",name:"GPT 5.6 Sol",reasoningLevels:["high","xhigh"],defaultReasoningLevel:"high" }] }];
beforeEach(() => { save.mockClear(); vi.mocked(useAgentModels).mockReturnValue({ data:{},error:null,saving:false,save,refresh:vi.fn() }); });

it("groups the twelve immutable agents by flow and offers useful role-specific model guidance", async () => {
  const user = userEvent.setup(); render(<AgentSettings accounts={accounts} />);
  const standard = screen.getByRole("region",{ name:"Agentes do fluxo Padrão" });
  const designer = screen.getByRole("region",{ name:"Agentes do fluxo Designer" });
  const planned = screen.getByRole("region",{ name:"Agentes do fluxo Planejado" });
  const complete = screen.getByRole("region",{ name:"Agentes do fluxo Completo" });
  expect(within(standard).getAllByRole("button",{name:/^Modelo de/})).toHaveLength(1);
  expect(within(designer).getAllByRole("button",{name:/^Modelo de/})).toHaveLength(1);
  expect(within(planned).getAllByRole("button",{name:/^Modelo de/})).toHaveLength(3);
  expect(within(complete).getAllByRole("button",{name:/^Modelo de/})).toHaveLength(7);
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
});

it("gives every agent a distinct accent within each flow", () => {
  render(<AgentSettings accounts={accounts} />);
  for (const flow of ["Padrão", "Designer", "Planejado", "Completo"]) {
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
