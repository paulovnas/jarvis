import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  developmentArguments,
  localSigningIdentity,
  projectIdentifier,
  signedDevArguments,
  signingCommand,
  usesDevelopmentProfile,
} from "./macos-signing";

const projectRoot = fileURLToPath(new URL("../", import.meta.url));
const require = createRequire(import.meta.url);
let args = process.argv.slice(2);

try {
  const command = signingCommand(args);
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
    if (command === "dev") {
      process.env.JARVIS_DEV_APP_BUNDLE = "1";
      args = signedDevArguments(args, projectRoot);
    }
    console.info("Jarvis: assinatura macOS estável ativada.");
  }
  // Native addons do not reliably observe Bun's process.env mutations. Launch a fresh
  // process with an explicit environment so Rust's bundler and Cargo inherit signing.
  const cli = path.join(path.dirname(require.resolve("@tauri-apps/cli/package.json")), "tauri.js");
  const child = spawn(process.execPath, [cli, ...args], { stdio: "inherit", env: { ...process.env } });
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
