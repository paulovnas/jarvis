import { z } from "zod";

export const attachmentSchema = z.object({
  id: z.string(), conversationId: z.string(), name: z.string(), mime: z.string(),
  size: z.number().nonnegative(), kind: z.enum(["image", "document"]),
});
export type Attachment = z.infer<typeof attachmentSchema>;
const visionResultSchema = z.object({ accountAlias: z.string(), model: z.string(), analysis: z.string() });
export function readVisionResult(output: string) {
  try { const parsed = visionResultSchema.safeParse(JSON.parse(output)); return parsed.success ? parsed.data : null; }
  catch { return null; }
}
export function uploadFile(file: File): Promise<{ name: string; data: string }> {
  if (!file.size || file.size > 20 * 1024 * 1024) return Promise.reject(new Error("Cada arquivo deve ter entre 1 byte e 20 MB."));
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("Não foi possível ler o arquivo."));
    reader.onload = () => typeof reader.result === "string" ? resolve({ name: file.name, data: reader.result.slice(reader.result.indexOf(",") + 1) }) : reject(new Error("Arquivo inválido."));
    reader.readAsDataURL(file);
  });
}
