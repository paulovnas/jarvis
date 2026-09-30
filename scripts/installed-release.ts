import { readFileSync } from "node:fs";

export function readInstalledRelease(file: string): { version: string; notes: string } | null {
  let release: unknown;
  try { release = JSON.parse(readFileSync(file, "utf8")); }
  catch (cause) {
    if (cause && typeof cause === "object" && "code" in cause && cause.code === "ENOENT") return null;
    throw cause;
  }
  if (release === null) return null;
  if (!release || typeof release !== "object" || !("version" in release) || typeof release.version !== "string"
    || !("notes" in release) || typeof release.notes !== "string") {
    throw new Error("As notas empacotadas da versão são inválidas.");
  }
  return { version: release.version, notes: release.notes };
}
