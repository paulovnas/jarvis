import { z } from "zod";
import { libraryError } from "./library";

export function browserError(cause: unknown) { return libraryError(cause, "Não foi possível concluir a ação no navegador."); }

export const browserTabSchema = z.object({ id: z.string(), conversationId: z.string(), title: z.string(), url: z.string(), loading: z.boolean() });
export const browserSnapshotSchema = z.object({ tabs: browserTabSchema.array(), activeId: z.string().nullable() });
export const browserConsoleSchema = z.object({ logs: z.array(z.object({ level: z.string(), text: z.string(), time: z.number() })) });
export type BrowserTab = z.infer<typeof browserTabSchema>;
export type BrowserSnapshot = z.infer<typeof browserSnapshotSchema>;
export type BrowserLog = z.infer<typeof browserConsoleSchema>["logs"][number];
export type BrowserRequest = { action: "open" | "select" | "close" | "navigate" | "back" | "forward" | "reload" | "console" | "screenshot"; id?: string | null; url?: string };
export const EMPTY_BROWSER: BrowserSnapshot = { tabs: [], activeId: null };

export function browserAddress(value: string): string {
  const input = value.trim();
  if (!input || input.length > 4096) throw new Error("Informe um endereço HTTP ou HTTPS.");
  const url = new URL(input.includes("://") ? input : `http://${input}`);
  if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) throw new Error("Use um endereço HTTP ou HTTPS sem credenciais na URL.");
  return url.href;
}

export function browserOccluded(document: Document): boolean {
  return Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"], [role="alertdialog"], [role="menu"], [role="listbox"], [data-slot="popover-content"], [data-slot="tooltip-content"]'))
    .some(node => !node.hasAttribute("data-closed") && !node.hidden && node.getAttribute("aria-hidden") !== "true" && getComputedStyle(node).display !== "none");
}
