import { z } from "zod";
import { pendingAuthoringSchema } from "./authoring";
import {
  agentStepSchema,
  agentToolSchema,
  agentTurnSchema,
  compactionEventSchema,
  contextInfoSchema,
  directTaskSchema,
  fileChangeSchema,
  historyWindowSchema,
  pendingApprovalSchema,
  queuedMessageSchema,
  retryStatusSchema,
  usageSchema,
  type AgentStep,
  type AgentTurn,
  type ChatSnapshot,
} from "./chat";
import { pendingQuestionSchema } from "./questions";
import { hasValidHistoryWindow, historyWindow, mergeChat } from "./chat-history";
import {
  IPC_PROTOCOL_VERSION,
  type AgentEventBatch as GeneratedAgentEventBatch,
  type ChatSubscription as GeneratedChatSubscription,
} from "@/generated/ipc";

const startedItemSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("step"), stepIndex: z.number().int().nonnegative(), step: agentStepSchema }),
  z.object({ type: z.literal("tool"), stepIndex: z.number().int().nonnegative(), tool: agentToolSchema }),
]);
const snapshotStateSchema = z.object({
  compacting: z.boolean(), activeTurnId: z.string().nullable(), pendingApproval: pendingApprovalSchema.nullable(),
  pendingQuestion: pendingQuestionSchema.nullable(), pendingAuthoring: pendingAuthoringSchema.nullable(),
  queuedMessages: z.array(queuedMessageSchema), context: contextInfoSchema,
  compactions: z.array(compactionEventSchema), fileChanges: z.array(fileChangeSchema), history: historyWindowSchema,
});
const agentEventSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("turnStarted"), turn: agentTurnSchema }),
  z.object({ type: z.literal("turnTimingUpdated"), turnId: z.string(), durationMs: z.number().nonnegative(), activeSince: z.number().nonnegative().nullable() }),
  z.object({ type: z.literal("itemStarted"), item: startedItemSchema }),
  z.object({
    type: z.literal("itemDelta"), stepIndex: z.number().int().nonnegative(),
    textAppend: z.string(), summaryAppend: z.string(), textReplace: z.string().optional(), summaryReplace: z.string().optional(),
    durationMs: z.number().nonnegative(), retry: retryStatusSchema.nullable(), usage: usageSchema.nullable(),
    coreActivities: agentStepSchema.shape.coreActivities,
  }),
  z.object({ type: z.literal("itemCompleted"), stepIndex: z.number().int().nonnegative(), tool: agentToolSchema }),
  z.object({ type: z.literal("tasksUpdated"), tasks: z.array(directTaskSchema) }),
  z.object({ type: z.literal("approvalRequested"), approval: pendingApprovalSchema.nullable() }),
  z.object({ type: z.literal("stateChanged"), state: snapshotStateSchema }),
  z.object({ type: z.literal("turnCompleted"), turn: agentTurnSchema }),
]);

export const agentEventBatchSchema = z.object({
  protocolVersion: z.number().int().nonnegative().default(0),
  conversationId: z.string(), baseRevision: z.number().int().nonnegative().nullable(),
  revision: z.number().int().nonnegative(), events: z.array(agentEventSchema),
});
export type AgentEventBatch = GeneratedAgentEventBatch & z.infer<typeof agentEventBatchSchema>;
export const chatSubscriptionSchema = z.object({
  protocolVersion: z.number().int().nonnegative(),
  reset: z.boolean(),
  snapshot: z.unknown().nullable(),
  batches: z.array(agentEventBatchSchema),
});
export type ChatSubscription = GeneratedChatSubscription & z.infer<typeof chatSubscriptionSchema>;

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
      const total = historyWindow(snapshot).total + (current?.id === event.turn.id ? 0 : 1);
      return { ...snapshot, turns: [event.turn], history: { start: total - 1, total }, activeTurnId: event.turn.id, latestOptions: event.turn.options };
    }
    case "turnCompleted":
      return replaceLatestTurn(snapshot, turn => turn.id === event.turn.id ? event.turn : turn);
    case "turnTimingUpdated":
      return replaceLatestTurn(snapshot, turn => turn.id === event.turnId ? { ...turn, durationMs: event.durationMs, activeSince: event.activeSince } : turn);
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
        coreActivities: event.coreActivities ?? step.coreActivities,
        durationMs: event.durationMs, retry: event.retry, usage: event.usage,
      })));
    case "itemCompleted":
      return replaceLatestTurn(snapshot, turn => replaceStep(turn, event.stepIndex, step => ({
        ...step,
        tools: step.tools.map(tool => tool.id === event.tool.id ? event.tool : tool),
      })));
    case "tasksUpdated":
      return replaceLatestTurn(snapshot, turn => ({ ...turn, tasks: event.tasks }));
    case "approvalRequested":
      return { ...snapshot, pendingApproval: event.approval };
    case "stateChanged": {
      const total = event.state.history.total;
      return { ...snapshot, ...event.state, history: { start: Math.max(0, total - snapshot.turns.length), total } };
    }
  }
}

export function applyAgentEventBatch(
  current: ChatSnapshot | null,
  batch: AgentEventBatch,
): { snapshot: ChatSnapshot | null; needsResync: boolean } {
  if (!current || current.conversationId !== batch.conversationId) {
    return { snapshot: current, needsResync: true };
  }
  if (batch.protocolVersion > IPC_PROTOCOL_VERSION) {
    return { snapshot: current, needsResync: true };
  }
  if (!hasValidHistoryWindow(current)) return { snapshot: current, needsResync: true };
  if (batch.revision <= current.revision) return { snapshot: current, needsResync: false };
  if (batch.baseRevision !== current.revision) return { snapshot: current, needsResync: true };
  // Native events describe only the latest turn. Its index is not the start
  // of the renderer's paginated history, and older visible turns are not live.
  const window = historyWindow(current);
  const latest = window.start + current.turns.length === window.total ? current.turns[current.turns.length - 1] : undefined;
  const tail: ChatSnapshot = {
    ...current,
    turns: latest ? [latest] : [],
    history: { start: window.total - (latest ? 1 : 0), total: window.total },
    navigation: undefined,
  };
  const snapshot = batch.events.reduce(applyEvent, tail);
  return { snapshot: mergeChat(current, { ...snapshot, revision: batch.revision }), needsResync: false };
}
