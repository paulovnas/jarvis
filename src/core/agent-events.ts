import { z } from "zod";
import { pendingAuthoringSchema } from "./authoring";
import {
  agentStepSchema,
  agentToolSchema,
  agentTurnSchema,
  compactionEventSchema,
  contextInfoSchema,
  fileChangeSchema,
  historyWindowSchema,
  queuedMessageSchema,
  retryStatusSchema,
  usageSchema,
  type AgentStep,
  type AgentTurn,
  type ChatSnapshot,
} from "./chat";
import { pendingQuestionSchema } from "./questions";

const startedItemSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("step"), stepIndex: z.number().int().nonnegative(), step: agentStepSchema }),
  z.object({ type: z.literal("tool"), stepIndex: z.number().int().nonnegative(), tool: agentToolSchema }),
]);
const snapshotStateSchema = z.object({
  compacting: z.boolean(), activeTurnId: z.string().nullable(), pendingApproval: agentToolSchema.nullable(),
  pendingQuestion: pendingQuestionSchema.nullable(), pendingAuthoring: pendingAuthoringSchema.nullable(),
  queuedMessages: z.array(queuedMessageSchema), context: contextInfoSchema,
  compactions: z.array(compactionEventSchema), fileChanges: z.array(fileChangeSchema), history: historyWindowSchema,
});
const agentEventSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("turnStarted"), turn: agentTurnSchema }),
  z.object({ type: z.literal("itemStarted"), item: startedItemSchema }),
  z.object({
    type: z.literal("itemDelta"), stepIndex: z.number().int().nonnegative(),
    textAppend: z.string(), summaryAppend: z.string(), textReplace: z.string().optional(), summaryReplace: z.string().optional(),
    durationMs: z.number().nonnegative(), retry: retryStatusSchema.nullable(), usage: usageSchema.nullable(),
  }),
  z.object({ type: z.literal("itemCompleted"), stepIndex: z.number().int().nonnegative(), tool: agentToolSchema }),
  z.object({ type: z.literal("approvalRequested"), tool: agentToolSchema.nullable() }),
  z.object({ type: z.literal("stateChanged"), state: snapshotStateSchema }),
  z.object({ type: z.literal("turnCompleted"), turn: agentTurnSchema }),
]);

export const agentEventBatchSchema = z.object({
  conversationId: z.string(), baseRevision: z.number().int().nonnegative().nullable(),
  revision: z.number().int().nonnegative(), events: z.array(agentEventSchema),
});
export type AgentEventBatch = z.infer<typeof agentEventBatchSchema>;

function replaceLatestTurn(snapshot: ChatSnapshot, change: (turn: AgentTurn) => AgentTurn): ChatSnapshot {
  if (!snapshot.turns.length) return snapshot;
  const turns = snapshot.turns.slice();
  turns[turns.length - 1] = change(turns[turns.length - 1]);
  return { ...snapshot, turns };
}

function replaceStep(turn: AgentTurn, index: number, change: (step: AgentStep) => AgentStep): AgentTurn {
  const current = turn.steps[index];
  if (!current) return turn;
  const steps = turn.steps.slice();
  steps[index] = change(current);
  return { ...turn, steps };
}

function applyEvent(snapshot: ChatSnapshot, event: z.infer<typeof agentEventSchema>): ChatSnapshot {
  switch (event.type) {
    case "turnStarted": {
      const current = snapshot.turns[snapshot.turns.length - 1];
      const turns = current?.id === event.turn.id
        ? [...snapshot.turns.slice(0, -1), event.turn]
        : [...snapshot.turns, event.turn].slice(-60);
      return { ...snapshot, turns, activeTurnId: event.turn.id, latestOptions: event.turn.options };
    }
    case "turnCompleted":
      return replaceLatestTurn(snapshot, turn => turn.id === event.turn.id ? event.turn : turn);
    case "itemStarted": {
      const item = event.item;
      return replaceLatestTurn(snapshot, turn => {
        if (item.type === "step") {
          const steps = turn.steps.slice();
          steps[item.stepIndex] = item.step;
          return { ...turn, steps };
        }
        const startedTool = item.tool;
        return replaceStep(turn, item.stepIndex, step => {
          const index = step.tools.findIndex(tool => tool.id === startedTool.id);
          const tools = step.tools.slice();
          if (index >= 0) tools[index] = startedTool;
          else tools.push(startedTool);
          return { ...step, tools };
        });
      });
    }
    case "itemDelta":
      return replaceLatestTurn(snapshot, turn => replaceStep(turn, event.stepIndex, step => ({
        ...step,
        text: event.textReplace ?? `${step.text}${event.textAppend}`,
        summary: event.summaryReplace ?? `${step.summary}${event.summaryAppend}`,
        durationMs: event.durationMs, retry: event.retry, usage: event.usage,
      })));
    case "itemCompleted":
      return replaceLatestTurn(snapshot, turn => replaceStep(turn, event.stepIndex, step => ({
        ...step,
        tools: step.tools.map(tool => tool.id === event.tool.id ? event.tool : tool),
      })));
    case "approvalRequested":
      return { ...snapshot, pendingApproval: event.tool };
    case "stateChanged":
      return { ...snapshot, ...event.state };
  }
}

export function applyAgentEventBatch(
  current: ChatSnapshot | null,
  batch: AgentEventBatch,
): { snapshot: ChatSnapshot | null; needsResync: boolean } {
  if (!current || current.conversationId !== batch.conversationId) {
    return { snapshot: current, needsResync: true };
  }
  if (batch.revision <= current.revision) return { snapshot: current, needsResync: false };
  if (batch.baseRevision !== current.revision) return { snapshot: current, needsResync: true };
  const snapshot = batch.events.reduce(applyEvent, current);
  return { snapshot: { ...snapshot, revision: batch.revision }, needsResync: false };
}
