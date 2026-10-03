import { z } from "zod";

export const generationSchema = z.object({
  outputTokens: z.number().nonnegative(),
  durationMs: z.number().nonnegative(),
  estimated: z.boolean(),
});
export type Generation = z.infer<typeof generationSchema>;

export function combineGeneration(metrics: readonly (Generation | null | undefined)[]): Generation | null {
  const measured = metrics.filter((metric): metric is Generation => Boolean(metric
    && Number.isFinite(metric.outputTokens) && metric.outputTokens >= 0
    && Number.isFinite(metric.durationMs) && metric.durationMs > 0));
  if (!measured.length) return null;
  return measured.reduce((total, metric) => ({
    outputTokens: total.outputTokens + metric.outputTokens,
    durationMs: total.durationMs + metric.durationMs,
    estimated: total.estimated || metric.estimated,
  }), { outputTokens: 0, durationMs: 0, estimated: false });
}

export function generationRate(metric: Generation | null | undefined): number | null {
  if (!metric || !Number.isFinite(metric.outputTokens) || metric.outputTokens <= 0
    || !Number.isFinite(metric.durationMs) || metric.durationMs < 100) return null;
  const rate = metric.outputTokens * 1_000 / metric.durationMs;
  return Number.isFinite(rate) && rate > 0 ? rate : null;
}
