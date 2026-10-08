import { z } from "zod";
import { customAgentSchema, customFlowSchema } from "./workflow-catalog";
import { publicationProposalSchema } from "./publication";
import { hookSchema } from "./hooks";
import { pluginPreviewSchema } from "./plugins";

const targetSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("agent"), before: customAgentSchema.nullable(), after: customAgentSchema }),
  z.object({ kind: z.literal("flow"), before: customFlowSchema.nullable(), after: customFlowSchema }),
  z.object({ kind: z.literal("publication"), after: publicationProposalSchema }),
  z.object({ kind: z.literal("mcp"), server: z.object({
    name: z.string(), transport: z.enum(["stdio", "http"]),
    command: z.string().nullable(), args: z.array(z.string()), url: z.string().nullable(),
    enabled: z.boolean(), cwd: z.string().nullable(),
    envKeys: z.array(z.string()), headerKeys: z.array(z.string()),
  }).strict() }),
  z.object({ kind: z.literal("hook"), before: hookSchema.nullable(), after: hookSchema.nullable() }).refine(target => target.before !== null || target.after !== null, "A proposta deve identificar o hook."),
  z.object({ kind: z.literal("plugin"), preview: pluginPreviewSchema }),
  z.object({ kind: z.literal("project_instructions"), path: z.literal("AGENTS.md"), before: z.string().nullable(), after: z.string().min(1) }).strict(),
]);

export const pendingAuthoringSchema = z.object({
  turnId: z.string(),
  toolId: z.string(),
  action: z.enum(["create", "update", "publish", "delete"]),
  summary: z.string(),
  catalogRevision: z.number().int().nonnegative().nullable(),
  target: targetSchema,
  agentReferences: z.array(z.object({ id: z.string(), name: z.string() })),
});

export type PendingAuthoring = z.infer<typeof pendingAuthoringSchema>;
export type McpProposalServer = Extract<PendingAuthoring["target"], { kind: "mcp" }>["server"];
export type McpAuthoringValues = { environment: Record<string, string>; headers: Record<string, string> };

export function hasMcpAuthoringValues(server: McpProposalServer, values: McpAuthoringValues): boolean {
  return server.envKeys.every(key => Object.prototype.hasOwnProperty.call(values.environment, key) && !!values.environment[key].trim())
    && server.headerKeys.every(key => Object.prototype.hasOwnProperty.call(values.headers, key) && !!values.headers[key].trim());
}
