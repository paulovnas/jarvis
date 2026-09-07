import { z } from "zod";
import { attachmentSchema } from "./attachments";

const resultSchema = z.object({
  kind: z.literal("generated_image"), accountAlias: z.string(), model: z.string(),
  images: z.array(attachmentSchema.extend({ kind: z.literal("image") })).min(1).max(4), text: z.string(),
});
export function readGeneratedImages(output?: string) {
  if (!output) return null;
  try { const result = resultSchema.safeParse(JSON.parse(output)); return result.success ? result.data : null; }
  catch { return null; }
}
