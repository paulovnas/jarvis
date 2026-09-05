import { execFileSync, spawnSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { localSigningIdentity, projectIdentifier } from "./macos-signing";

if (process.platform !== "darwin") throw new Error("Este teste requer macOS e um certificado de assinatura instalado.");
const root = fileURLToPath(new URL("../", import.meta.url));
const temporary = mkdtempSync(path.join(tmpdir(), "jarvis-signing-check-"));
const service = `com.foxtag.jarvis.signing-check.${randomUUID()}`;
const env = {
  ...process.env,
  APPLE_SIGNING_IDENTITY: localSigningIdentity().hash,
  JARVIS_SIGNING_IDENTIFIER: `${projectIdentifier(root)}.signing-check`,
};
const runner = path.join(root, "scripts/run-signed-macos.sh");
const versions = [path.join(temporary, "first build"), path.join(temporary, "second build")];
let created = false;

function invoke(binary: string, operation: string): void {
  const result = spawnSync("/bin/sh", [runner, binary, operation, service], { env, encoding: "utf8" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`Keychain check failed: ${result.stdout}${result.stderr}`);
  console.info(result.stdout.trim());
}

function signature(binary: string): { requirement: string; hash: string } {
  const result = spawnSync("/usr/bin/codesign", ["-d", "--verbose=4", "-r-", binary], { encoding: "utf8" });
  const output = result.stdout + result.stderr;
  const requirement = output.match(/^(?:# )?designated => (.+)$/m)?.[1];
  const hash = output.match(/^CDHash=(.+)$/m)?.[1];
  if (result.status !== 0 || !requirement || !hash || requirement.includes("cdhash")) {
    throw new Error("A assinatura não possui identidade estável.");
  }
  return { requirement, hash };
}

try {
  for (const [index, binary] of versions.entries()) {
    execFileSync("/usr/bin/clang", [
      "-Wall", "-Wextra", "-Werror", `-DJARVIS_BUILD=${index + 1}`,
      path.join(root, "scripts/fixtures/keychain-signing.c"),
      "-framework", "Security", "-framework", "CoreFoundation", "-o", binary,
    ]);
  }
  invoke(versions[0], "write");
  created = true;
  invoke(versions[1], "read");
  const first = signature(versions[0]);
  const second = signature(versions[1]);
  if (first.hash === second.hash || first.requirement !== second.requirement) {
    throw new Error("Os builds precisam ter conteúdos diferentes e a mesma identidade de assinatura.");
  }

  // A different identifier must still be denied: the fix must not broaden the item's ACL.
  const denied = spawnSync("/bin/sh", [runner, versions[1], "read", service], {
    env: { ...env, JARVIS_SIGNING_IDENTIFIER: `${env.JARVIS_SIGNING_IDENTIFIER}.unrelated` }, encoding: "utf8",
  });
  if (denied.status !== 1 || !/status=-\d+/.test(denied.stdout)) {
    throw new Error("A identidade diferente não foi recusada pelo Keychain conforme esperado.");
  }
  console.info("PASS: dois builds diferentes acessaram a mesma credencial sem diálogos; outra identidade foi recusada.");
} finally {
  try {
    if (created) invoke(versions[0], "delete");
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}
