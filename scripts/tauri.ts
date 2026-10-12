import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { prepareImpeccable } from "./prepare-impeccable";
import { ensureCefTauriCli } from "./tauri-cef-cli";
import {
  developmentArguments,
  localSigningIdentity,
  projectIdentifier,
  signingCommand,
  usesDevelopmentProfile,
} from "./macos-signing";

const projectRoot = fileURLToPath(new URL("../", import.meta.url));
let args = process.argv.slice(2);

try {
  const command = signingCommand(args);
  if (command) {
    const targetIndex = args.indexOf("--target");
    await prepareImpeccable({ target: targetIndex >= 0 ? args[targetIndex + 1] : undefined });
  }
  const development = usesDevelopmentProfile(args);
  if (development) {
    process.env.JARVIS_RUNTIME_PROFILE = "development";
    args = developmentArguments(args, projectRoot);
  }
  if (process.platform === "darwin" && command) {
    const identity = localSigningIdentity();
    // Scope the certificate to this invocation, including Tauri's bundler and Cargo children.
    process.env.APPLE_SIGNING_IDENTITY = identity.hash;
    process.env.JARVIS_SIGNING_IDENTIFIER = projectIdentifier(projectRoot, development);
    // The pinned CEF CLI builds, bundles and signs the complete development app,
    // including Chromium helpers. The old Cargo runner would create an incomplete bundle.
    process.env.JARVIS_CEF_RUNTIME = "1";
    console.info("Jarvis: assinatura macOS estável ativada.");
  }
  const cli = await ensureCefTauriCli(projectRoot);
  const child = spawn(cli, args, { stdio: "inherit", env: { ...process.env } });
  const interrupt = () => { child.kill("SIGINT"); };
  const terminate = () => { child.kill("SIGTERM"); };
  process.on("SIGINT", interrupt);
  process.on("SIGTERM", terminate);
  child.on("error", (error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
  child.on("close", (code, signal) => {
    process.removeListener("SIGINT", interrupt);
    process.removeListener("SIGTERM", terminate);
    process.exitCode = code ?? (signal === "SIGINT" ? 130 : 1);
  });
} catch (error) {
  console.error(error instanceof Error ? error.message : "Não foi possível executar o Tauri.");
  process.exitCode = 1;
}
