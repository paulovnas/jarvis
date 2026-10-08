import { expect, it } from "vitest";
import { chatOptions } from "@/test/chat-fixtures";
import { customAgent, customCatalog, customFlow } from "@/test/workflow-fixtures";
import type { ModelChoice } from "@/core/provider-references";
import type { RemoteChoices } from "./client";
import { remoteModelChoice, remoteModelProblem, remoteTurnOptions } from "./remote-choices";

const primary: ModelChoice = { account: "ene", model: "sol", reasoning: "high", fallback: { executor: "claude", account: "", model: "sonnet", reasoning: null } };
const choices = (): RemoteChoices => ({ catalog: customCatalog, defaults: { "standard/builder": primary }, overrides: {}, models: [{ provider: "ene", providerKind: "openai-codex", models: [{ value: "ene/sol", label: "Sol", reasoningLevels: ["low", "high"], defaultReasoningLevel: "high" }] }, { provider: "Claude Code", executor: "claude", models: [{ value: "sonnet", label: "Sonnet", reasoningLevels: [], defaultReasoningLevel: null }] }] });

it("uses the conversation override ahead of history and leaves shared defaults intact", () => {
  const data = choices();
  data.overrides["standard/builder"] = { executor: "claude", account: "", model: "sonnet", reasoning: null };
  expect(remoteModelChoice(data, "standard", chatOptions)).toEqual(data.overrides["standard/builder"]);
  expect(data.defaults["standard/builder"]).toEqual(primary);
});

it("keeps an unavailable historical choice visible instead of replacing it silently", () => {
  const selected = remoteModelChoice(choices(), "standard", chatOptions);
  expect(selected).toMatchObject({ account: chatOptions.account, model: chatOptions.model });
  expect(remoteModelProblem(choices(), "standard", selected)).toContain("não está disponível");
});

it("starts an empty conversation with the configured model and secondary", () => {
  expect(remoteModelChoice(choices(), "standard", null)).toEqual(primary);
  expect(remoteModelProblem(choices(), "standard", primary)).toBeNull();
});

it("resolves custom agents and flow entry models before the previous chat selection", () => {
  const data = choices();
  data.catalog = { ...data.catalog, agents: [{ ...customAgent, model: primary }] };
  expect(remoteModelChoice(data, `agent:${customAgent.id}`, chatOptions)).toEqual(primary);
  expect(remoteModelChoice(data, `custom:${customFlow.id}`, chatOptions)).toEqual(primary);
});

it("rejects removed agents, flows, reasoning and secondary targets", () => {
  const data = choices();
  expect(remoteModelProblem(data, `agent:${"d".repeat(32)}`, primary)).toContain("agente não está disponível");
  expect(remoteModelProblem(data, `custom:${"d".repeat(32)}`, primary)).toContain("fluxo não está disponível");
  expect(remoteModelProblem(data, "standard", { ...primary, reasoning: "ultra" })).toContain("modelo deste chat");
  expect(remoteModelProblem(data, "standard", { ...primary, fallback: { account: "removed", model: "missing", reasoning: null } })).toContain("secundário");
});

it("blocks a planned flow with a valid coordinator when a downstream model was removed", () => {
  const data = choices();
  const missing = { account: "retired", model: "missing", reasoning: null };
  data.defaults["planned/planner"] = primary;
  data.defaults["planned/builder"] = missing;
  expect(remoteModelProblem(data, "planned", primary)).toContain("Construtor usa um modelo indisponível");
  expect(remoteModelProblem(data, "standard", primary)).toBeNull();
  data.overrides["planned/builder"] = primary;
  expect(remoteModelProblem(data, "planned", primary)).toBeNull();
  data.defaults["planned/designer"] = { ...primary, fallback: missing };
  expect(remoteModelProblem(data, "planned", primary)).toContain("Designer usa um modelo indisponível");
});

it("checks configured models of every agent used by a custom flow", () => {
  const data = choices();
  const missing = { account: "retired", model: "missing", reasoning: null };
  data.catalog = { ...data.catalog, agents: [{ ...customAgent, model: missing }] };
  expect(remoteModelProblem(data, `custom:${customFlow.id}`, primary)).toContain(`${customAgent.name} usa um modelo indisponível`);
  expect(remoteModelProblem(data, "standard", primary)).toBeNull();
  data.catalog.agents[0].model = primary;
  expect(remoteModelProblem(data, `custom:${customFlow.id}`, primary)).toBeNull();
});

it("changes next-turn scope while preserving applicable behavior and removing stale IDs", () => {
  const previous = { ...chatOptions, workflow: "custom" as const, customAgentId: customAgent.id, manualValidation: true, automaticPublication: { commit: true, push: false, pullRequest: false } };
  expect(remoteTurnOptions("planned", primary, previous)).toMatchObject({ account: "ene", model: "sol", reasoning: "high", workflow: "planned", customAgentId: null, customWorkflowId: null, manualValidation: true, automaticPublication: previous.automaticPublication, approvalMode: "manual" });
  const github = remoteTurnOptions("agent:builtin:github", primary, previous);
  expect(github.customAgentId).toBe("builtin:github");
  expect(github).not.toHaveProperty("manualValidation");
  expect(github).not.toHaveProperty("automaticPublication");
  expect(github).not.toHaveProperty("modelSelection");
});

it("restores and validates Fast from the account capability and removes it for the next Normal turn", () => {
  const data = choices();
  data.models[0].models[0].supportsFast = true;
  const fast = { ...primary, serviceTier: "priority" as const };
  data.overrides["standard/builder"] = fast;
  expect(remoteModelChoice(data, "standard", chatOptions)).toEqual(fast);
  expect(remoteModelProblem(data, "standard", fast)).toBeNull();
  const options = remoteTurnOptions("standard", fast, chatOptions);
  expect(options.serviceTier).toBe("priority");
  expect(remoteTurnOptions("standard", primary, options)).not.toHaveProperty("serviceTier");
  expect(data.defaults["standard/builder"]).toEqual(primary);
  data.models[0].models[0].supportsFast = false;
  expect(remoteModelProblem(data, "standard", fast)).toContain("Selecione Normal");
  data.models[0].models[0].supportsFast = true;
  data.models[0].providerKind = "custom";
  expect(remoteModelProblem(data, "standard", fast)).toContain("Fast");
});
