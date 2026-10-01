import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { builtinGithubAgent, builtinImageAgent, builtinVideoAgent, customAgent, customFlow } from "@/test/workflow-fixtures";
import { FlowPicker } from "./FlowPicker";

it("offers the six Jarvis flows with descriptions and commits only the chosen flow", async () => {
  const user = userEvent.setup(), change = vi.fn();
  render(<FlowPicker value="standard" onChange={change} />);
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
  const list = await screen.findByRole("listbox", { name: "Opções de fluxo e agente" });
  expect(within(list).getAllByRole("option")).toHaveLength(6);
  expect(within(list).getByText("Fluxos Jarvis")).toBeVisible();
  const designer = within(list).getByRole("option", { name: "Designer" });
  expect(within(designer).getByText("Implementação especializada em design e frontend.")).toBeVisible();
  await user.click(designer);
  expect(change).toHaveBeenCalledExactlyOnceWith("designer");
});
it("blocks selection during execution", () => {
  render(<FlowPicker value="complete" onChange={vi.fn()} disabled />);
  expect(screen.getByRole("button", { name: "Selecionar fluxo" })).toBeDisabled();
});

it("selects a saved custom flow without changing the six Jarvis choices", async () => {
  const user = userEvent.setup(), change = vi.fn();
  render(<FlowPicker value="standard" onChange={change} customFlows={[customFlow]} />);
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
  expect(await screen.findAllByRole("option")).toHaveLength(7);
  await user.click(screen.getByRole("option", { name: "Meu fluxo" }));
  expect(change).toHaveBeenCalledExactlyOnceWith(`custom:${customFlow.id}`);
});

it("uses the saved flow icon and color in the composer and list", async () => {
  const user = userEvent.setup();
  render(<FlowPicker value={`custom:${customFlow.id}`} onChange={vi.fn()} customFlows={[{ ...customFlow, appearance: { color: "green", icon: "rocket" } }]} />);
  const trigger = screen.getByRole("button", { name: "Selecionar fluxo" });
  expect(trigger.querySelector("svg.lucide-rocket")).toHaveStyle({ color: "var(--color-onedark-green)" });
  await user.click(trigger);
  expect((await screen.findByRole("option", { name: "Meu fluxo" })).querySelector("svg.lucide-rocket")).toBeInTheDocument();
});

it("offers Solo and Mixed custom agents and hides flow-only agents", async () => {
  const user = userEvent.setup(), change = vi.fn();
  const solo = { ...customAgent, id: "c".repeat(32), name: "Analista solo", usage: "solo" as const };
  const flowOnly = { ...customAgent, id: "d".repeat(32), name: "Agente interno", usage: "flow_only" as const };
  render(<FlowPicker value="standard" onChange={change} customAgents={[customAgent, solo, flowOnly]} />);
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
  expect(within(await screen.findByRole("listbox")).getByText("Agentes personalizados")).toBeVisible();
  expect(screen.getByRole("option", { name: customAgent.name })).toBeVisible();
  expect(screen.getByRole("option", { name: solo.name })).toBeVisible();
  expect(screen.queryByRole("option", { name: flowOnly.name })).not.toBeInTheDocument();
  await user.click(screen.getByRole("option", { name: solo.name }));
  expect(change).toHaveBeenCalledExactlyOnceWith(`agent:${solo.id}`);
});

it("offers the built-in GitHub agent for direct conversations", async () => {
  const user = userEvent.setup(), change = vi.fn();
  render(<FlowPicker value="standard" onChange={change} builtinAgents={[builtinGithubAgent]} />);
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
  await user.click(await screen.findByRole("option", { name: "GitHub" }));
  expect(change).toHaveBeenCalledExactlyOnceWith("agent:builtin:github");
});

it("searches names and descriptions and handles no matching options", async () => {
  const user = userEvent.setup(), change = vi.fn();
  render(<FlowPicker value="standard" onChange={change} customFlows={[customFlow]} customAgents={[customAgent]} />);
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
  const search = await screen.findByRole("combobox", { name: "Buscar fluxo ou agente" });
  await waitFor(() => expect(search).toHaveFocus());
  await user.type(search, "Meu fluxo");
  expect(screen.getAllByRole("option")).toHaveLength(1);
  expect(screen.getByRole("option", { name: customFlow.name })).toBeVisible();
  await user.clear(search);
  await user.type(search, "evidências");
  expect(screen.getAllByRole("option")).toHaveLength(1);
  expect(screen.getByRole("option", { name: customAgent.name })).toBeVisible();
  await user.clear(search);
  await user.type(search, "nenhuma correspondência");
  expect(screen.queryByRole("option")).not.toBeInTheDocument();
  expect(screen.getByText("Nenhum fluxo ou agente encontrado.")).toBeVisible();
  expect(screen.getByRole("button", { name: "Ver detalhes" })).toBeDisabled();
  await waitFor(() => expect(search).not.toHaveAttribute("aria-activedescendant"));
  expect(screen.getByRole("listbox")).not.toHaveAttribute("aria-activedescendant");
  expect(change).not.toHaveBeenCalled();
});

it("filters flows and agents while keeping native and custom groups distinct", async () => {
  const user = userEvent.setup(), change = vi.fn();
  render(<FlowPicker value="standard" onChange={change} customFlows={[customFlow]} customAgents={[customAgent]} builtinAgents={[builtinGithubAgent]} />);
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
  const list = await screen.findByRole("listbox");
  expect(within(list).getAllByRole("option")).toHaveLength(9);
  for (const heading of ["Fluxos Jarvis", "Fluxos personalizados", "Agentes Jarvis", "Agentes personalizados"]) expect(within(list).getByText(heading)).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Fluxos" }));
  expect(within(list).getAllByRole("option")).toHaveLength(7);
  expect(within(list).queryByText("Agentes Jarvis")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Agentes" }));
  expect(within(list).getAllByRole("option")).toHaveLength(2);
  expect(within(list).getByText("Agentes Jarvis")).toBeVisible();
  expect(within(list).getByText("Agentes personalizados")).toBeVisible();
  expect(within(list).queryByText("Fluxos Jarvis")).not.toBeInTheDocument();
  const search = screen.getByRole("combobox", { name: "Buscar fluxo ou agente" });
  await user.click(search);
  await user.keyboard("{ArrowDown}");
  const highlighted = within(list).getByRole("option", { selected: true });
  expect([builtinGithubAgent.name, customAgent.name]).toContain(highlighted.getAttribute("aria-label"));
  expect(search).toHaveAttribute("aria-activedescendant", highlighted.id);
  await user.click(screen.getByRole("button", { name: "Agentes" }));
  await user.keyboard("{Enter}");
  expect(change).not.toHaveBeenCalled();
  expect(list).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Todos" }));
  expect(within(list).getAllByRole("option")).toHaveLength(9);
  expect(change).not.toHaveBeenCalled();
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("listbox")).not.toBeInTheDocument());
  await waitFor(() => expect(screen.getByRole("button", { name: "Selecionar fluxo" })).toHaveFocus());
});

it("previews hovered and keyboard-highlighted options without committing until Enter", async () => {
  const user = userEvent.setup(), change = vi.fn();
  render(<FlowPicker value="standard" onChange={change} />);
  const trigger = screen.getByRole("button", { name: "Selecionar fluxo" });
  await user.click(trigger);
  const search = await screen.findByRole("combobox", { name: "Buscar fluxo ou agente" });
  await waitFor(() => expect(search).toHaveFocus());
  await user.keyboard("{ArrowDown}");
  expect(screen.getByRole("option", { name: "Designer" })).toHaveAttribute("aria-selected", "true");
  expect(screen.getByRole("option", { name: "Padrão" })).toHaveAttribute("aria-current", "true");
  expect(screen.getByLabelText("Detalhes da opção em destaque")).toHaveTextContent("Implementação especializada em design e frontend.");
  await user.hover(screen.getByRole("option", { name: "Completo" }));
  expect(screen.getByRole("option", { name: "Completo" })).toHaveAttribute("aria-selected", "true");
  expect(screen.getByLabelText("Detalhes da opção em destaque")).toHaveTextContent("Uma equipe da investigação à revisão.");
  expect(change).not.toHaveBeenCalled();
  await user.keyboard("{ArrowUp}{Enter}");
  expect(change).toHaveBeenCalledExactlyOnceWith("planned");
  await waitFor(() => expect(screen.queryByRole("listbox")).not.toBeInTheDocument());
  await waitFor(() => expect(trigger).toHaveFocus());
});

it("restores focus on Escape and opens the committed selection in a large catalog", async () => {
  const user = userEvent.setup(), change = vi.fn();
  const flows = Array.from({ length: 30 }, (_, index) => ({ ...customFlow, id: `flow-${index}`, name: `Fluxo ${index}` }));
  render(<FlowPicker value="custom:flow-26" onChange={change} customFlows={flows} />);
  const trigger = screen.getByRole("button", { name: "Selecionar fluxo" });
  trigger.focus();
  await user.keyboard("{ArrowDown}");
  const selected = await screen.findByRole("option", { name: "Fluxo 26" });
  expect(screen.getAllByRole("option")).toHaveLength(36);
  expect(selected).toHaveAttribute("aria-selected", "true");
  expect(selected).toHaveAttribute("aria-current", "true");
  const search = screen.getByRole("combobox", { name: "Buscar fluxo ou agente" });
  await waitFor(() => expect(search).toHaveAttribute("aria-activedescendant", selected.id));
  await user.type(search, "Designer");
  await user.keyboard("{Escape}");
  await waitFor(() => expect(trigger).toHaveFocus());
  expect(change).not.toHaveBeenCalled();
  await user.click(trigger);
  expect(screen.getByRole("combobox", { name: "Buscar fluxo ou agente" })).toHaveValue("");
  expect(screen.getByRole("option", { name: "Fluxo 26" })).toHaveAttribute("aria-selected", "true");
});

it("shows the full highlighted description through details without selecting it", async () => {
  const user = userEvent.setup(), change = vi.fn();
  const description = "Uma descrição longa para explicar o fluxo.\nSegunda linha com instruções completas para consulta.";
  render(<FlowPicker value={`custom:${customFlow.id}`} onChange={change} customFlows={[{ ...customFlow, description }]} />);
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
  await user.click(await screen.findByRole("button", { name: "Ver detalhes" }));
  const dialog = await screen.findByRole("dialog", { name: "Meu fluxo" });
  expect(within(dialog).getByText(description, { collapseWhitespace: false })).toBeVisible();
  expect(change).not.toHaveBeenCalled();
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Meu fluxo" })).not.toBeInTheDocument());
  expect(screen.getByRole("listbox")).toBeVisible();
});

it("keeps an unavailable selection explicit and offers valid replacements", async () => {
  const user = userEvent.setup(), change = vi.fn();
  render(<FlowPicker value="custom:missing" onChange={change} />);
  const trigger = screen.getByRole("button", { name: "Selecionar fluxo" });
  expect(trigger).toHaveTextContent("Opção indisponível");
  await user.click(trigger);
  const options = await screen.findAllByRole("option");
  expect(options.every(option => !option.hasAttribute("aria-current"))).toBe(true);
  expect(change).not.toHaveBeenCalled();
  await user.click(screen.getByRole("option", { name: "Padrão" }));
  expect(change).toHaveBeenCalledExactlyOnceWith("standard");
});

it("offers the video flow and generator as distinct native choices with audiovisual guidance", async () => {
  const user = userEvent.setup(), change = vi.fn();
  render(<FlowPicker value="standard" onChange={change} builtinAgents={[builtinVideoAgent]} />);
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
  expect(await screen.findByRole("option", { name: "Vídeo" })).toBeVisible();
  expect(screen.getByText(/Apresentações de projetos com animação, narração, música/)).toBeVisible();
  await user.click(screen.getByRole("option", { name: "Gerador de vídeos" }));
  expect(change).toHaveBeenCalledExactlyOnceWith("agent:builtin:video");
});

it("distinguishes the image flow and its mixed specialist from the video flow", async () => {
  const user = userEvent.setup(), change = vi.fn();
  render(<FlowPicker value="standard" onChange={change} builtinAgents={[builtinImageAgent, builtinVideoAgent]} />);
  await user.click(screen.getByRole("button", { name: "Selecionar fluxo" }));
  expect(await screen.findByRole("option", { name: "Imagens" })).toBeVisible();
  expect(screen.getByRole("option", { name: "Vídeo" })).toBeVisible();
  await user.click(screen.getByRole("option", { name: "Gerador de imagens" }));
  expect(change).toHaveBeenCalledExactlyOnceWith("agent:builtin:image_generator");
});
