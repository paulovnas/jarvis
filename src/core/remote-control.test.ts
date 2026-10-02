import { expect, it } from "vitest";
import { pairingForAddress, remoteControlError, remoteStatusSchema } from "./remote-control";

it("keeps the pairing secret in the fragment and accepts only advertised addresses", () => {
  const status = remoteStatusSchema.parse({ enabled: true, running: true, port: 47731, urls: ["http://192.168.1.2:47731/", "http://10.0.0.2:47731/"], pairingUrl: "http://192.168.1.2:47731/#pair=secret", pairingExpiresAt: 123, devices: [], error: null });
  expect(pairingForAddress(status, status.urls[1])).toBe("http://10.0.0.2:47731/#pair=secret");
  expect(pairingForAddress(status, "https://untrusted.example/")).toBeNull();
  expect(pairingForAddress({ ...status, pairingUrl: null }, status.urls[0])).toBeNull();
});

it("shows structured service errors without displaying serialized secrets", () => {
  expect(remoteControlError({ code: "remote_bind", message: "Porta indisponível", token: "secret" })).toBe("Porta indisponível");
  expect(remoteControlError(null)).toContain("Verifique o status");
});
