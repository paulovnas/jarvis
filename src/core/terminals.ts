import { z } from "zod";

export const terminalSchema = z.object({
  id: z.string(),
  projectId: z.string(),
  conversationId: z.string().nullable(),
  title: z.string(),
  cwd: z.string(),
  pid: z.number(),
  startedAt: z.number(),
  endedAt: z.number().nullable(),
  exitCode: z.number().nullable(),
  status: z.enum(["running", "exited", "failed"]),
  origin: z.enum(["user", "agent"]),
  command: z.string().nullable().optional(),
});

export const terminalSnapshotSchema = z.object({
  terminal: terminalSchema,
  output: z.string(),
  revision: z.number(),
  truncated: z.boolean(),
});

export const terminalOutputEventSchema = z.object({
  projectId: z.string(),
  id: z.string(),
  data: z.string(),
  revision: z.number(),
});

export const terminalProjectActivitySchema = z.object({
  projectId: z.string(),
  count: z.number().int().positive(),
});

export type ProjectTerminal = z.infer<typeof terminalSchema>;
export type TerminalSnapshot = z.infer<typeof terminalSnapshotSchema>;
export type TerminalOutputEvent = z.infer<typeof terminalOutputEventSchema>;
export type TerminalProjectActivity = z.infer<typeof terminalProjectActivitySchema>;

export const TERMINAL_STATUS_LABELS = {
  running: "Em execução",
  exited: "Encerrado",
  failed: "Falhou",
};
