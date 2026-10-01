import { z } from "zod";
import { attachmentSchema } from "./attachments";

const resultSchema = z.object({
  kind: z.literal("generated_image"), accountAlias: z.string(), model: z.string(),
  images: z.array(attachmentSchema.extend({ kind: z.literal("image") })).min(1).max(4), text: z.string(),
  processing: z.object({ engine: z.string().optional(), images: z.array(z.object({ width: z.number().int().positive(), height: z.number().int().positive() }).nullable()).min(1).max(4).optional() }).nullish(),
});
export function readGeneratedImages(output?: string) {
  if (!output) return null;
  try { const result = resultSchema.safeParse(JSON.parse(output)); return result.success ? result.data : null; }
  catch { return null; }
}
