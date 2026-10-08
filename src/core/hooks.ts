import { z } from "zod";

export const HOOK_EVENTS = ["PreToolUse", "PermissionRequest", "PostToolUse", "SessionStart", "SubagentStart", "UserPromptSubmit", "PreCompact", "PostCompact", "Stop", "SubagentStop", "Interrupt", "SessionEnd"] as const;
export const hookEventSchema = z.enum(HOOK_EVENTS);
export type HookEvent = z.infer<typeof hookEventSchema>;
export const HOOK_EVENT_LABELS: Record<HookEvent | "BeforeAgent", { label: string; description: string }> = {
  PreToolUse: { label: "Pré-uso da ferramenta", description: "Antes de executar uma ferramenta." },
  PermissionRequest: { label: "Solicitação de permissão", description: "Quando uma ferramenta solicita autorização." },
  PostToolUse: { label: "Após usar a ferramenta", description: "Depois da execução de uma ferramenta." },
  SessionStart: { label: "Início da sessão", description: "Ao iniciar a execução de uma conversa." },
  UserPromptSubmit: { label: "Envio de mensagem", description: "Quando o usuário envia uma mensagem." },
  PreCompact: { label: "Antes de compactar", description: "Antes de resumir o contexto. Disponível no runtime nativo." },
  PostCompact: { label: "Após compactar", description: "Depois de resumir o contexto. Disponível no runtime nativo." },
  Stop: { label: "Fim do turno", description: "Quando um agente conclui seu turno. Não indica o fim de todo um fluxo." },
  SubagentStart: { label: "Início do subagente", description: "Quando um agente delegado inicia uma execução." },
  SubagentStop: { label: "Fim do subagente", description: "Quando um agente delegado conclui a execução." },
  Interrupt: { label: "Interrupção", description: "Quando o usuário interrompe a execução principal." },
  SessionEnd: { label: "Encerramento da sessão", description: "Quando a sessão é encerrada." },
  BeforeAgent: { label: "Preparação do agente", description: "Instruções internas aplicadas antes de consultar o modelo." },
};

const textBytes = (max: number) => z.string().refine(value => new TextEncoder().encode(value).length <= max);
export const hookSchema = z.object({
  id: z.string().regex(/^[a-f0-9]{32}$/), name: textBytes(160).refine(value => value.trim().length > 0),
  event: hookEventSchema, command: textBytes(16_384).refine(value => value.trim().length > 0), matcher: textBytes(4096),
  timeoutSeconds: z.number().int().min(1).max(600), enabled: z.boolean(),
}).strict();
export const manualHookSchema = hookSchema;
export type Hook = z.infer<typeof hookSchema>;
export const nativeHookSchema = z.object({
  id: z.string(), name: z.string(), event: z.enum([...HOOK_EVENTS, "BeforeAgent"]), description: z.string(),
  command: z.string().nullable(), matcher: z.string().nullable(), timeoutSeconds: z.number().nullable(),
}).strict();
export type NativeHook = z.infer<typeof nativeHookSchema>;
export const hookCatalogSchema = z.object({ revision: z.number().int().nonnegative(), hooks: z.array(hookSchema), nativeHooks: z.array(nativeHookSchema), untrustedIds: z.array(z.string()).default([]) }).strict();
export type HookCatalog = z.infer<typeof hookCatalogSchema>;

export function hookError(error: unknown): string {
  if (typeof error === "object" && error !== null && "code" in error && "message" in error && typeof error.message === "string") return error.message;
  return "Não foi possível atualizar os hooks. Tente novamente.";
}
