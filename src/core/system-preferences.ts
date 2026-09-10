import { z } from "zod";

export const sleepModes = {
  off: "Desligado",
  active: "Enquanto houver agentes/chats ativos",
  open: "Enquanto Jarvis aberto",
} as const;

export const DEFAULT_TERMINAL_PREFERENCES = {
  shell: null,
  arguments: [] as string[],
  fontFamily: null,
  fontSize: 13,
};

export const terminalPreferencesSchema = z.object({
  shell: z.string().min(1).max(4096).nullable(),
  arguments: z.array(z.string().min(1).max(512)).max(16),
  fontFamily: z.string().min(1).max(160).nullable(),
  fontSize: z.number().int().min(9).max(32),
});

export const systemSnapshotSchema = z.object({
  preferences: z.object({
    preventSleep: z.enum(["off", "active", "open"]),
    notifications: z.boolean(),
    askUserTimeoutSeconds: z.number().int().min(1).max(3600),
    terminal: terminalPreferencesSchema.default(DEFAULT_TERMINAL_PREFERENCES),
  }),
  sleepInhibited: z.boolean(),
  sleepError: z.string().nullable(),
  notificationError: z.string().nullable(),
  availableTerminalShells: z.array(z.string()).default([]),
  resolvedTerminalShell: z.string().nullable().default(null),
  terminalError: z.string().nullable().default(null),
});
export type SystemSnapshot = z.infer<typeof systemSnapshotSchema>;
export type SystemPreferences = SystemSnapshot["preferences"];
export type TerminalPreferences = SystemPreferences["terminal"];

export const AUTOMATIC_TERMINAL_FONT_STACK = '"MesloLGS NF", "MesloLGS Nerd Font Mono", "Hack Nerd Font Mono", "FiraCode Nerd Font", "JetBrains Mono", monospace';

export function terminalFontFamily(font: string | null): string {
  if (!font) return AUTOMATIC_TERMINAL_FONT_STACK;
  const escaped = font.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  return `"${escaped}", "JetBrains Mono", monospace`;
}
