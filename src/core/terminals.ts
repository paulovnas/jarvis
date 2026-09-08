import { z } from "zod";

export const terminalSchema = z.object({
  id: z.string(),
  conversationId: z.string(),
  title: z.string(),
  cwd: z.string(),
  pid: z.number(),
  startedAt: z.number(),
  endedAt: z.number().nullable(),
  exitCode: z.number().nullable(),
  status: z.enum(["running", "exited", "failed"]),
  origin: z.enum(["user", "agent"]),
});

export const terminalSnapshotSchema = z.object({
  terminal: terminalSchema,
  output: z.string(),
  revision: z.number(),
  truncated: z.boolean(),
});

export const terminalOutputEventSchema = z.object({
  conversationId: z.string(),
  id: z.string(),
  data: z.string(),
  revision: z.number(),
});

export type ChatTerminal = z.infer<typeof terminalSchema>;
export type TerminalSnapshot = z.infer<typeof terminalSnapshotSchema>;
export type TerminalOutputEvent = z.infer<typeof terminalOutputEventSchema>;

export const TERMINAL_STATUS_LABELS = {
  running: "Em execução",
  exited: "Encerrado",
  failed: "Falhou",
};
