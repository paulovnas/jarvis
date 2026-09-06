import { z } from "zod";

export const usageWindowSchema = z.object({
  id: z.string(), group: z.string(), thirdParty: z.boolean(), label: z.string(),
  durationSeconds: z.number().positive().nullable(),
  remainingPercent: z.number().min(0).max(100).nullable(),
  resetsAt: z.number().nullable(),
});
export const accountUsageSchema = z.object({
  alias: z.string(), fetchedAt: z.number().nullable(), email: z.string().nullable(), plan: z.string().nullable(),
  windows: z.array(usageWindowSchema), error: z.string().nullable(),
  resetCredits: z.object({ availableCount: z.number().int().nonnegative(), expirations: z.array(z.number().nullable()), detailsAvailable: z.boolean() }).nullable(),
});
export type AccountUsage = z.infer<typeof accountUsageSchema>;
export type UsageWindow = z.infer<typeof usageWindowSchema>;
export function aliasSuffix(alias: string) { return alias.replace(/^(openai-codex|antigravity)-/, ""); }
export function planLabel(plan: string | null | undefined, accountType: string) {
  if (plan) {
    const value = plan.toLowerCase();
    if (value.includes("business")) return "Business";
    if (value.includes("enterprise")) return "Enterprise";
    const known: Record<string, string> = { plus: "Plus", pro: "Pro", free: "Grátis", team: "Team", prolite: "Pro Lite", "g1-pro-tier": "Google AI Pro", "g1-ultra-tier": "Google AI Ultra", "free-tier": "Grátis" };
    return known[value] ?? plan;
  }
  return ({ personal: "Pessoal", enterprise: "Enterprise" } as Record<string, string>)[accountType] ?? "Não informado";
}
export function remainingTime(at: number | null, now: number) {
  if (at === null) return null;
  if (at <= now) return "agora";
  const minutes = Math.ceil((at - now) / 60_000);
  const days = Math.floor(minutes / 1440), hours = Math.floor(minutes % 1440 / 60), mins = minutes % 60;
  return days ? `${days}d${hours ? ` ${hours}h` : ""}` : hours ? `${hours}h${mins ? ` ${mins}m` : ""}` : `${mins}m`;
}
export function quotaColor(remaining: number | null) {
  return remaining === null ? "var(--muted-foreground)" : remaining <= 10 ? "#e06c75" : remaining <= 30 ? "#e5c07b" : "#98c379";
}
export function quotaPercent(remaining: number | null) { return remaining === null ? "—" : `${Math.round(remaining)}%`; }
