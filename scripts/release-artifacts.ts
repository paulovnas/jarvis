import { readFileSync, statSync } from "node:fs";
import path from "node:path";
import { artifactNames, type ReleaseTarget } from "./release-plan";
import { verifyUpdaterSignature } from "./updater-signature";

export function verifyReleaseArtifacts(directory: string, version: string, sha: string, target: ReleaseTarget, publicKey: string) {
  const metadata: unknown = JSON.parse(readFileSync(path.join(directory, `build-${target}.json`), "utf8"));
  if (!metadata || typeof metadata !== "object" || !("version" in metadata) || metadata.version !== version
    || !("sha" in metadata) || metadata.sha !== sha || !("target" in metadata) || metadata.target !== target) {
    throw new Error("Os artefatos pertencem a outra versão, commit ou arquitetura.");
  }
  const { archive, signature: signatureFile, names } = artifactNames(version, target);
  for (const name of names) {
    const file = statSync(path.join(directory, name));
    if (!file.isFile() || file.size === 0) throw new Error(`Artefato vazio ou inválido: ${name}`);
  }
  const signature = readFileSync(path.join(directory, signatureFile), "utf8").trim();
  verifyUpdaterSignature(readFileSync(path.join(directory, archive)), signature, publicKey);
  return { version, target, archive, signature, names };
}
