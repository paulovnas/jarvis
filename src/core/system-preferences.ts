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
  availableTerminalFonts: z.array(z.string()).default([]),
  resolvedTerminalShell: z.string().nullable().default(null),
  terminalError: z.string().nullable().default(null),
  terminalFontError: z.string().nullable().default(null),
});
export type SystemSnapshot = z.infer<typeof systemSnapshotSchema>;
export type SystemPreferences = SystemSnapshot["preferences"];
export type TerminalPreferences = SystemPreferences["terminal"];

export const TERMINAL_FONT_PRIORITY = [
  "MesloLGS NF",
  "MesloLGS Nerd Font Mono",
  "NotoSansM Nerd Font Mono",
  "NotoMono Nerd Font Mono",
  "JetBrainsMono Nerd Font",
  "CaskaydiaCove Nerd Font Mono",
  "Hack Nerd Font Mono",
  "FiraCode Nerd Font",
  "JetBrains Mono",
] as const;

export const AUTOMATIC_TERMINAL_FONT_STACK = TERMINAL_FONT_PRIORITY.map(font => `"${font}"`).join(", ") + ", monospace";

export function resolveTerminalFont(font: string | null, available: string[]): string | null {
  if (font) return available.find(candidate => candidate.toLocaleLowerCase() === font.toLocaleLowerCase()) ?? font;
  for (const preferred of TERMINAL_FONT_PRIORITY) {
    const installed = available.find(candidate => candidate.toLocaleLowerCase() === preferred.toLocaleLowerCase());
    if (installed) return installed;
  }
  return available.find(candidate => /nerd font/i.test(candidate)) ?? available[0] ?? null;
}

export function terminalFontFamily(font: string | null): string {
  if (!font) return AUTOMATIC_TERMINAL_FONT_STACK;
  const escaped = font.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  return `"${escaped}", "JetBrains Mono", monospace`;
}
