import { spawnSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import path from "node:path";
import { localSigningIdentity } from "./macos-signing";
import { configuration, releaseEnvironment, root } from "./release-common";
import { RELEASE_REPOSITORY } from "./release-plan";

// Sensitive input only crosses stdin/environment; subprocess diagnostics are deliberately redacted.
function run(program: string, args: string[], input?: string, env = process.env): string {
  const result = spawnSync(program, args, { cwd: root, input, env, encoding: "utf8", stdio: ["pipe", "pipe", "pipe"] });
  if (result.error || result.status !== 0) throw new Error(`Falhou: ${program} ${args.slice(0, 2).join(" ")}. Código ${result.status ?? "indisponível"}; saída omitida para proteger credenciais.`);
  return result.stdout;
}
function secret(name: string, value: string) {
  run("gh", ["secret", "set", name, "--repo", RELEASE_REPOSITORY, "--env", releaseEnvironment], value);
  console.info(`Secret configurado: ${name}`);
}
function main() {
  if (process.platform !== "darwin") throw new Error("Execute a configuração inicial no Mac que possui o certificado Apple.");
  const { config } = configuration();
  const keyPath = process.env.TAURI_SIGNING_PRIVATE_KEY ?? path.join(homedir(), ".jarvis/release/updater.key");
  if (readFileSync(`${keyPath}.pub`, "utf8").trim() !== config.plugins.updater.pubkey) throw new Error("A chave pública local difere da instalada no aplicativo.");
  const key = readFileSync(keyPath, "utf8").trim();
  const identity = localSigningIdentity();
  const temp = mkdtempSync(path.join(tmpdir(), "jarvis-ci-secrets-"));
  try {
    const password = randomBytes(32).toString("hex");
    const certificate = path.join(temp, "identity.p12");
    console.info("Exportando somente o certificado de assinatura do Jarvis. O macOS pode solicitar autorização do Keychain.");
    run("swift", ["scripts/export-release-identity.swift"], undefined, { ...process.env, JARVIS_EXPORT_IDENTITY: identity.hash, JARVIS_EXPORT_PASSWORD: password, JARVIS_EXPORT_PATH: certificate });
    // Only initialize a new environment. Never loosen existing protection rules.
    const endpoint = `repos/${RELEASE_REPOSITORY}/environments/${releaseEnvironment}`;
    const lookup = spawnSync("gh", ["api", endpoint], { encoding: "utf8" });
    if (lookup.status !== 0) {
      if (!/HTTP 404/.test(lookup.stderr)) throw new Error("Não foi possível consultar o ambiente de publicação.");
      run("gh", ["api", "--method", "PUT", endpoint, "--input", "-"], JSON.stringify({ deployment_branch_policy: { protected_branches: false, custom_branch_policies: true } }));
      run("gh", ["api", "--method", "POST", `${endpoint}/deployment-branch-policies`, "--input", "-"], JSON.stringify({ name: "main", type: "branch" }));
    }
    const environment = JSON.parse(run("gh", ["api", endpoint])) as { deployment_branch_policy: { custom_branch_policies: boolean } | null };
    const policies = JSON.parse(run("gh", ["api", `${endpoint}/deployment-branch-policies`])) as { branch_policies: { name: string; type: string }[] };
    if (!environment.deployment_branch_policy?.custom_branch_policies || policies.branch_policies.length !== 1
      || policies.branch_policies[0].name !== "main" || policies.branch_policies[0].type !== "branch") {
      throw new Error("O ambiente macos-release precisa permitir exclusivamente a branch main. Revise suas regras no GitHub.");
    }
    secret("TAURI_SIGNING_PRIVATE_KEY", key);
    if (process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD) secret("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD);
    secret("APPLE_CERTIFICATE", readFileSync(certificate).toString("base64"));
    secret("APPLE_CERTIFICATE_PASSWORD", password);
    secret("APPLE_SIGNING_IDENTITY", identity.hash);
    console.info("Assinaturas preservadas nos secrets de macos-release. Os arquivos locais continuam disponíveis como backup.");
  } finally { rmSync(temp, { recursive: true, force: true }); }
}
try { main(); } catch (error) { console.error(error instanceof Error ? error.message : "Falha ao configurar secrets."); process.exitCode = 1; }
