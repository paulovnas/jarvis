import { z } from "zod";

const appearanceColorSchema = z.enum(["blue", "green", "cyan", "yellow", "red", "purple", "neutral"]);
const workflowIconSchema = z.enum(["bot", "workflow", "route", "brain", "search", "code", "palette", "shield", "terminal", "wrench", "book", "sparkles", "target", "pen", "lightbulb", "rocket"]);
const projectIconSchema = z.enum(["bot", "workflow", "route", "brain", "search", "code", "palette", "shield", "terminal", "wrench", "book", "sparkles", "target", "pen", "lightbulb", "rocket", "folder", "folder-code", "package", "database", "globe", "app-window"]);

export const workflowAppearanceSchema = z.object({
  icon: workflowIconSchema,
  color: appearanceColorSchema,
});
export const projectAppearanceSchema = z.object({ icon: projectIconSchema, color: appearanceColorSchema });
export type WorkflowAppearance = z.infer<typeof workflowAppearanceSchema>;
export type ProjectAppearance = z.infer<typeof projectAppearanceSchema>;
export type IdentityAppearance = WorkflowAppearance | ProjectAppearance;
export const agentAppearance: WorkflowAppearance = { icon: "bot", color: "blue" };
export const flowAppearance: WorkflowAppearance = { icon: "workflow", color: "blue" };
export const projectAppearance: ProjectAppearance = { icon: "folder", color: "cyan" };
