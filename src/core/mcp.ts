import { z } from "zod";

export const mcpCheckSchema = z.object({ toolCount: z.number().int().nonnegative(), error: z.string().nullable() });
export const mcpServersSchema = z.array(z.object({
  id: z.string(), name: z.string(), kind: z.enum(["local", "remote"]),
  enabled: z.boolean(), configured: z.boolean(), revision: z.number().int(), lastCheck: mcpCheckSchema.nullable(),
}));
export type McpServer = z.infer<typeof mcpServersSchema>[number];
export const MCP_TEMPLATE = JSON.stringify({ context7: { type: "local", command: ["npx", "-y", "@upstash/context7-mcp", "--api-key", "YOUR_API_KEY"], enabled: true } }, null, 2);

export function validateMcpJson(raw: string): string | null {
  try {
    const value: unknown = JSON.parse(raw);
    return !value || typeof value !== "object" || Array.isArray(value) || Object.keys(value).length !== 1
      ? "Informe exatamente um MCP nomeado, sem a chave externa mcp." : null;
  } catch { return "JSON inválido. Confira as aspas, vírgulas e chaves."; }
}
