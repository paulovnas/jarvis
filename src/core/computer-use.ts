import { z } from "zod";
import { libraryError } from "./library";

const identitySchema = z.object({ bundleId: z.string().min(1), bundlePath: z.string().min(1), executablePath: z.string().min(1) });
const accessSchema = z.enum(["observe", "control"]);
export const computerModeSchema = z.enum(["manual", "yolo"]);
export type ComputerMode = z.infer<typeof computerModeSchema>;
export const computerModeDescriptions: Record<ComputerMode, string> = {
  manual: "Você autoriza os aplicativos nesta conversa e revisa cada ação de controle.",
  yolo: "O agente pode observar e controlar os aplicativos, incluindo o Jarvis, sem autorizações por aplicativo ou ação. Vale para todos os chats.",
};
const boundsSchema = z.object({ x: z.number().finite(), y: z.number().finite(), width: z.number().nonnegative(), height: z.number().nonnegative() });
export const computerCapabilitiesSchema = z.object({
  platform: z.string(), supported: z.boolean(), accessibility: z.boolean(), screenCapture: z.boolean(), inputPosting: z.boolean(),
  captureSupported: z.boolean(), sessionStatus: z.string(), reason: z.string().nullable(),
});
export const computerAppSchema = z.object({
  appId: z.string().min(1), name: z.string(), identity: identitySchema, processId: z.number().int().positive(), launchId: z.string().min(1),
  windows: z.array(z.object({ id: z.number().int().nonnegative(), title: z.string(), bounds: boundsSchema })),
});
export const computerSnapshotSchema = z.object({
  mode: computerModeSchema.default("manual"),
  capabilities: computerCapabilitiesSchema,
  grants: z.array(z.object({ id: z.string().min(1), identity: identitySchema, access: accessSchema })),
  targets: z.array(z.object({
    id: z.string().min(1), appId: z.string().min(1), appName: z.string(), processId: z.number().int().positive(),
    windowId: z.number().int().nonnegative(), windowTitle: z.string(), access: accessSchema,
    generation: z.number().int().nonnegative(), revision: z.number().int().nonnegative(),
  })),
  activity: z.object({
    operationId: z.string(), targetId: z.string().nullable(), tool: z.string(),
    phase: z.enum(["observing", "prepared", "inputSent", "observed", "uncertain", "stopped", "error"]),
    message: z.string(), timestamp: z.number().nonnegative(),
  }).nullable(),
  stopped: z.boolean(),
});
export const computerUpdateSchema = z.object({ conversationId: z.string(), snapshot: computerSnapshotSchema });
export type ComputerApp = z.infer<typeof computerAppSchema>;
export type ComputerSnapshot = z.infer<typeof computerSnapshotSchema>;
export type ComputerAccess = z.infer<typeof accessSchema>;
export type ComputerPermission = "accessibility" | "screenCapture" | "inputPosting";
export const computerAccessLabels: Record<ComputerAccess, string> = { observe: "Observar", control: "Observar e controlar" };
export const computerInputTools = ["computer_click", "computer_type", "computer_press", "computer_scroll", "computer_drag"];
export function computerUseError(cause: unknown) {
  return cause instanceof z.ZodError ? "O estado do uso do computador é inválido. Atualize para tentar novamente." : libraryError(cause, "Não foi possível atualizar o uso do computador.");
}
