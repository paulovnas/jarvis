import { z } from "zod";
import { libraryError } from "./library";

export function browserError(cause: unknown) { return libraryError(cause, "Não foi possível concluir a ação no navegador."); }

export const browserTabSchema = z.object({ id: z.string(), conversationId: z.string(), title: z.string(), url: z.string(), loading: z.boolean() });
export const browserSnapshotSchema = z.object({ tabs: browserTabSchema.array(), activeId: z.string().nullable(), backend: z.enum(["embedded", "extension"]).optional(), extensionError: z.string().nullable().optional() });
export const browserConsoleSchema = z.object({ logs: z.array(z.object({ level: z.string(), text: z.string(), time: z.number() })) });
export const browserExtensionStatusSchema = z.object({ state: z.enum(["listening", "connected", "error"]), endpoint: z.string(), profileLabel: z.string().nullable(), extensionVersion: z.string().nullable(), error: z.string().nullable() });
export const browserExtensionSetupSchema = z.object({ path: z.string(), connectionCode: z.string() });
export const browserDiscoverySchema = z.object({ tabs: z.array(z.object({ id: z.string(), title: z.string(), url: z.string(), owned: z.boolean() })) });
export const browserNetworkSchema = z.object({ requests: z.array(z.object({ id: z.string(), method: z.string(), url: z.string(), status: z.number().optional(), type: z.string().optional(), failed: z.string().optional() })), total: z.number(), offset: z.number(), limit: z.number() });
export type BrowserTab = z.infer<typeof browserTabSchema>;
export type BrowserSnapshot = z.infer<typeof browserSnapshotSchema>;
export type BrowserLog = z.infer<typeof browserConsoleSchema>["logs"][number];
export type BrowserRequest = { action: "open" | "select" | "close" | "navigate" | "back" | "forward" | "reload" | "console" | "screenshot" | "discover" | "attach" | "network"; id?: string | null; url?: string; filter?: string; offset?: number; limit?: number };
export type BrowserExtensionStatus = z.infer<typeof browserExtensionStatusSchema>;
export const EMPTY_BROWSER: BrowserSnapshot = { tabs: [], activeId: null };

export function browserAddress(value: string): string {
  const input = value.trim();
  if (!input || input.length > 4096) throw new Error("Informe um endereço HTTP ou HTTPS.");
  const url = new URL(input.includes("://") ? input : `http://${input}`);
  if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) throw new Error("Use um endereço HTTP ou HTTPS sem credenciais na URL.");
  return url.href;
}

export function browserOccluded(document: Document): boolean {
  // Passive tooltips must not hide the native page just because the user hovers.
  return Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"], [role="alertdialog"], [role="menu"], [role="listbox"], [data-slot="popover-content"]'))
    .some(node => !node.hasAttribute("data-closed") && !node.hidden && node.getAttribute("aria-hidden") !== "true" && getComputedStyle(node).display !== "none");
}
