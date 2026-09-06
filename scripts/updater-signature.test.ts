import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { expect, it } from "vitest";
import { verifyUpdaterSignature } from "./updater-signature";

function fixture(contents: Buffer) {
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  const keyId = Buffer.from("12345678");
  const key = Buffer.concat([Buffer.from("Ed"), keyId, publicKey.export({ format: "der", type: "spki" }).subarray(-32)]);
  const digest = createHash("blake2b512").update(contents).digest();
  const signed = sign(null, digest, privateKey);
  const comment = "timestamp:1788730000\tfile:Jarvis.app.tar.gz";
  const global = sign(null, Buffer.concat([signed, Buffer.from(comment)]), privateKey);
  return {
    publicKey: Buffer.from(`untrusted comment: minisign public key\n${key.toString("base64")}\n`).toString("base64"),
    signature: Buffer.from(`untrusted comment: signature\n${Buffer.concat([Buffer.from("ED"), keyId, signed]).toString("base64")}\ntrusted comment: ${comment}\n${global.toString("base64")}\n`).toString("base64"),
  };
}
it("verifies Tauri packages and rejects tampering, replaced keys and modified trusted comments", () => {
  const bytes = Buffer.from("signed archive");
  const signed = fixture(bytes);
  expect(() => verifyUpdaterSignature(bytes, signed.signature, signed.publicKey)).not.toThrow();
  expect(() => verifyUpdaterSignature(Buffer.from("tampered"), signed.signature, signed.publicKey)).toThrow("chave pública");
  expect(() => verifyUpdaterSignature(bytes, signed.signature, fixture(bytes).publicKey)).toThrow("chave pública");
  const tampered = Buffer.from(Buffer.from(signed.signature, "base64").toString().replace("timestamp:1788730000", "timestamp:1788730001")).toString("base64");
  expect(() => verifyUpdaterSignature(bytes, tampered, signed.publicKey)).toThrow("chave pública");
  for (const invalid of ["", "not-base64", Buffer.from("missing lines").toString("base64")]) {
    expect(() => verifyUpdaterSignature(bytes, invalid, signed.publicKey)).toThrow("chave pública");
  }
});
