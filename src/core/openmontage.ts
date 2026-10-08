import { z } from "zod";

export const openMontageConfigurationSchema = z.object({
  allowPaidTools: z.boolean(),
  allowModelDownloads: z.boolean(),
  credentials: z.array(z.object({ key: z.string().min(1), label: z.string().min(1), secret: z.boolean(), configured: z.boolean() })),
  optionalPackages: z.array(z.object({ id: z.string().min(1), label: z.string().min(1), installed: z.boolean() })),
});

export type OpenMontageConfiguration = z.infer<typeof openMontageConfigurationSchema>;
