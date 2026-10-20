import { z } from "zod";
import { attachmentSchema } from "./attachments";

export const pixelArtResultSchema = z.object({
  kind: z.literal("pixel_art"),
  jobId: z.string().min(1),
  callId: z.string().optional(),
  conversationId: z.string().optional(),
  revision: z.number().int().nonnegative().default(0),
  manual: z.boolean().default(false),
  action: z.string(),
  status: z.enum(["running", "completed", "blocked", "failed", "cancelled"]),
  phase: z.string(),
  progress: z.object({
    completed: z.number().nonnegative(),
    total: z.number().positive(),
    unit: z.string(),
  }).refine(value => value.completed <= value.total).nullish(),
  logs: z.array(z.string()).default([]),
  logPath: z.string().nullish(),
  images: z.array(attachmentSchema.extend({ kind: z.literal("image") })).default([]),
  artifacts: z.array(z.object({ name: z.string(), path: z.string(), mime: z.string(), previewError: z.string().optional() })).default([]),
  error: z.string().nullish(),
  nextAction: z.string().nullish(),
});
export type PixelArtResult = z.infer<typeof pixelArtResultSchema>;

export function readPixelArtResult(output?: string): PixelArtResult | null {
  if (!output) return null;
  try {
    const parsed = pixelArtResultSchema.safeParse(JSON.parse(output));
    return parsed.success ? parsed.data : null;
  } catch { return null; }
}
