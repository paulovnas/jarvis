import { z } from "zod";

export const webSearchConfigSchema = z.object({ accountAlias: z.string().nullable() });

const sourceSchema = z.object({
  title: z.string(),
  url: z.url().refine((url) => /^https?:\/\//i.test(url)),
});
const resultSchema = z.object({
  accountAlias: z.string(),
  model: z.string(),
  answer: z.string(),
  sources: z.array(sourceSchema),
});

export function readWebSearchResult(output: string) {
  try {
    const result = resultSchema.safeParse(JSON.parse(output));
    return result.success ? result.data : null;
  } catch {
    return null;
  }
}
