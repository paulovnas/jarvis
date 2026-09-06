import { z } from "zod";

export const sleepModes = {
  off: "Desligado",
  active: "Enquanto houver agentes/chats ativos",
  open: "Enquanto Jarvis aberto",
} as const;

export const systemSnapshotSchema = z.object({
  preferences: z.object({ preventSleep: z.enum(["off", "active", "open"]), notifications: z.boolean() }),
  sleepInhibited: z.boolean(),
  sleepError: z.string().nullable(),
  notificationError: z.string().nullable(),
});
export type SystemSnapshot = z.infer<typeof systemSnapshotSchema>;
export type SystemPreferences = SystemSnapshot["preferences"];
