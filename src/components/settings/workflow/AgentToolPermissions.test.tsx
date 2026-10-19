import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import type { CustomAgent } from "@/core/workflow-catalog";
import { customAgent } from "@/test/workflow-fixtures";
import { AgentToolPermissions } from "./AgentToolPermissions";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

beforeEach(() => {
  vi.mocked(invoke).mockResolvedValue([
    { id: "generate_image", name: "generate_image", group: "Imagens", description: "Gerar imagens.", required: false, capabilities: ["read_only", "write_files", "commands"] },
    { id: "image_process", name: "image_process", group: "Imagens", description: "Preparar imagens no projeto.", required: false, capabilities: ["write_files", "commands"] },
    { id: "bash", name: "bash", group: "Projeto", description: "Executar comandos.", required: false, capabilities: ["commands"] },
  ]);
});

function ControlledPermissions({ capability, deniedTools = [] }: { capability: CustomAgent["capability"]; deniedTools?: string[] }) {
  const [agent, setAgent] = useState<CustomAgent>({ ...customAgent, capability, deniedTools });
  return <>
    <AgentToolPermissions agent={agent} disabled={false} onChange={patch => setAgent(current => ({ ...current, ...patch }))} />
    <output aria-label="Ferramentas negadas">{JSON.stringify(agent.deniedTools)}</output>
  </>;
}

it("lets a write-files agent toggle image tools while preserving its other denials and command restriction", async () => {
  const user = userEvent.setup();
  render(<ControlledPermissions capability="write_files" deniedTools={["generate_image", "bash"]} />);

  const generate = await screen.findByRole("switch", { name: "generate_image" });
  const process = screen.getByRole("switch", { name: "image_process" });
  const bash = screen.getByRole("switch", { name: "bash" });
  const denials = screen.getByRole("status", { name: "Ferramentas negadas" });
  expect(generate).not.toBeChecked();
  expect(generate).not.toHaveAttribute("aria-disabled", "true");
  expect(process).toBeChecked();
  expect(process).not.toHaveAttribute("aria-disabled", "true");
  expect(bash).not.toBeChecked();
  expect(bash).toHaveAttribute("aria-disabled", "true");

  await user.click(generate);
  expect(generate).toBeChecked();
  expect(denials).toHaveTextContent('["bash"]');

  await user.click(process);
  expect(process).not.toBeChecked();
  expect(denials).toHaveTextContent('["bash","image_process"]');

  await user.click(generate);
  expect(generate).not.toBeChecked();
  expect(denials).toHaveTextContent('["bash","image_process","generate_image"]');

  await user.click(process);
  expect(process).toBeChecked();
  expect(denials).toHaveTextContent('["bash","generate_image"]');
  expect(bash).toHaveAttribute("aria-disabled", "true");
});

it("keeps image processing and commands unavailable to a read-only agent", async () => {
  const user = userEvent.setup();
  render(<ControlledPermissions capability="read_only" />);

  const process = await screen.findByRole("switch", { name: "image_process" });
  const bash = screen.getByRole("switch", { name: "bash" });
  expect(process).not.toBeChecked();
  expect(process).toHaveAttribute("aria-disabled", "true");
  expect(bash).not.toBeChecked();
  expect(bash).toHaveAttribute("aria-disabled", "true");

  await user.click(process);
  await user.click(bash);
  expect(process).not.toBeChecked();
  expect(bash).not.toBeChecked();
  expect(screen.getByRole("status", { name: "Ferramentas negadas" })).toHaveTextContent("[]");
});
