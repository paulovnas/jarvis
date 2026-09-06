import { z } from "zod";

export const processSchema = z.object({
  id: z.string(), conversationId: z.string(), title: z.string(), command: z.string(), cwd: z.string(), pid: z.number(),
  startedAt: z.number(), endedAt: z.number().nullable(), exitCode: z.number().nullable(),
  status: z.enum(["running", "stopping", "stopped", "exited", "failed"]),
});
export type ChatProcess = z.infer<typeof processSchema>;
export const processRunning = (process: ChatProcess) => process.status === "running" || process.status === "stopping";
export const PROCESS_LABELS = { running: "Em execução", stopping: "Parando", stopped: "Parado", exited: "Encerrado", failed: "Falhou" };
