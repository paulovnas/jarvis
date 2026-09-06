import { createHash, createPublicKey, verify } from "node:crypto";

// Tauri wraps the four-line minisign signature and the two-line public key in base64.
export function verifyUpdaterSignature(contents: Buffer, signature: string, publicKey: string): void {
  try {
    const keyLines = Buffer.from(publicKey.trim(), "base64").toString("utf8").trim().split(/\r?\n/);
    const signatureLines = Buffer.from(signature.trim(), "base64").toString("utf8").trim().split(/\r?\n/);
    if (keyLines.length !== 2 || signatureLines.length !== 4 || !signatureLines[2].startsWith("trusted comment: ")) throw new Error();
    const key = Buffer.from(keyLines[1], "base64");
    const signed = Buffer.from(signatureLines[1], "base64");
    if (key.length !== 42 || signed.length !== 74 || key.subarray(0, 2).toString() !== "Ed" || !key.subarray(2, 10).equals(signed.subarray(2, 10))) throw new Error();
    const algorithm = signed.subarray(0, 2).toString();
    if (algorithm !== "ED" && algorithm !== "Ed") throw new Error();
    const message = algorithm === "ED" ? createHash("blake2b512").update(contents).digest() : contents;
    const ed25519 = createPublicKey({ key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), key.subarray(10)]), format: "der", type: "spki" });
    if (!verify(null, message, ed25519, signed.subarray(10))) throw new Error();
    const comment = Buffer.from(signatureLines[2].slice("trusted comment: ".length));
    if (!verify(null, Buffer.concat([signed.subarray(10), comment]), ed25519, Buffer.from(signatureLines[3], "base64"))) throw new Error();
  } catch { throw new Error("O pacote não corresponde à chave pública do atualizador instalada no Jarvis."); }
}
