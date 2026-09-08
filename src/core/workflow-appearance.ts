import { z } from "zod";

export const workflowAppearanceSchema = z.object({
  icon: z.enum(["bot", "workflow", "route", "brain", "search", "code", "palette", "shield", "terminal", "wrench", "book", "sparkles", "target", "pen", "lightbulb", "rocket"]),
  color: z.enum(["blue", "green", "cyan", "yellow", "red", "purple", "neutral"]),
});
export type WorkflowAppearance = z.infer<typeof workflowAppearanceSchema>;
export const agentAppearance: WorkflowAppearance = { icon: "bot", color: "blue" };
export const flowAppearance: WorkflowAppearance = { icon: "workflow", color: "blue" };
