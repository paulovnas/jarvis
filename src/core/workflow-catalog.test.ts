import { expect, it } from "vitest";
import { flowOptions, flowSelection, validateGraph, workflowCatalogSchema } from "./workflow-catalog";
import { builtinAgent, customAgent, customFlow, customCatalog } from "@/test/workflow-fixtures";
import { chatOptions } from "@/test/chat-fixtures";

it("retains custom selection through persisted turn options without changing built-ins", () => {
  const selection = `custom:${customFlow.id}` as const;
  const agentSelection = `agent:${customAgent.id}` as const;
  expect(flowSelection({ ...chatOptions, ...flowOptions(selection) })).toBe(selection);
  expect(flowSelection({ ...chatOptions, ...flowOptions(agentSelection) })).toBe(agentSelection);
  expect(flowOptions(agentSelection)).toEqual({ workflow: "custom", customAgentId: customAgent.id });
  expect(flowOptions("complete")).toEqual({ workflow: "complete" });
  expect(flowSelection({ ...chatOptions, workflow: "publication" })).toBe("standard");
  expect(flowSelection()).toBe("standard");
  expect(workflowCatalogSchema.parse(customCatalog)).toEqual(customCatalog);
});

it("reads legacy identities and retains only predefined appearance choices", () => {
  const appearance = { icon: "brain", color: "cyan" };
  const catalog = { ...customCatalog, agents: [{ ...customAgent, appearance }], flows: [{ ...customFlow, appearance }] };
  expect(workflowCatalogSchema.parse(catalog)).toEqual(catalog);
  expect(workflowCatalogSchema.safeParse({ ...catalog, agents: [{ ...customAgent, appearance: { ...appearance, icon: "arbitrary" } }] }).success).toBe(false);
  expect(workflowCatalogSchema.safeParse({ ...catalog, flows: [{ ...customFlow, appearance: { ...appearance, color: "url(evil)" } }] }).success).toBe(false);
  expect(workflowCatalogSchema.parse({ ...customCatalog, agents: [{ ...customAgent, usage: undefined }] }).agents[0].usage).toBe("flow_only");
});

it("validates entry, references, termination, reachability and bounded correction loops", () => {
  expect(validateGraph(customFlow, [customAgent])).toBeNull();
  const second = { ...customFlow.steps[0], id: "d".repeat(32), onRework: customFlow.entry };
  const connected = { ...customFlow, steps: [{ ...customFlow.steps[0], next: second.id }, second] };
  expect(validateGraph(connected, [customAgent])).toBeNull();
  expect(validateGraph({ ...connected, steps: [customFlow.steps[0], second] }, [customAgent])).toMatch(/desconectados/);
  expect(validateGraph({ ...connected, steps: [connected.steps[0], { ...second, next: connected.entry }] }, [customAgent])).toMatch(/ciclo/);
  expect(validateGraph({ ...customFlow, entry: "missing" }, [customAgent])).toMatch(/inicial/);
  expect(validateGraph(customFlow, [])).toMatch(/agente/);
  expect(validateGraph(customFlow, [{ ...customAgent, usage: "solo" }])).toMatch(/Solo/);
  expect(validateGraph({ ...customFlow, steps: [{ ...customFlow.steps[0], agentId: builtinAgent.id }] }, [builtinAgent])).toBeNull();
  expect(validateGraph({ ...connected, maxSteps: 1 }, [customAgent])).toMatch(/limite/);
});
