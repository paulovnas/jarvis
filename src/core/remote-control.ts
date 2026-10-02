import { z } from "zod";

export const remoteStatusSchema = z.object({
  enabled: z.boolean(),
  running: z.boolean(),
  port: z.number().int().positive().nullable(),
  urls: z.array(z.string().url()),
  pairingUrl: z.string().url().nullable(),
  pairingExpiresAt: z.number().nonnegative().nullable(),
  devices: z.array(z.object({
    id: z.string(),
    name: z.string(),
    connectedAt: z.number().nonnegative(),
    lastSeenAt: z.number().nonnegative(),
  })),
  error: z.string().nullable(),
});

export type RemoteStatus = z.infer<typeof remoteStatusSchema>;

export function remoteControlError(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error !== null && "message" in error && typeof error.message === "string") return error.message;
  return "Não foi possível atualizar o acesso remoto. Verifique o status e tente novamente.";
}

export function pairingForAddress(status: RemoteStatus, address: string): string | null {
  if (!status.pairingUrl || !status.urls.includes(address)) return null;
  const pairing = new URL(status.pairingUrl);
  const target = new URL(address);
  target.hash = pairing.hash;
  return target.toString();
}
