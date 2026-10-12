import { spawn } from "node:child_process";
import { access, mkdir, readFile, stat } from "node:fs/promises";
import { constants } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";

// The app runtime, CLI and bundler must come from the same CEF-capable revision.
export const TAURI_CEF_REVISION = "9d1a5b176e337c8395362a4e7c99bac99b36a1d1";
export const TAURI_CEF_REPOSITORY = "https://github.com/tauri-apps/tauri";

type Installer = (command: string, args: string[], options: { cwd: string; env: NodeJS.ProcessEnv }) => Promise<void>;

function installedPackage(value: unknown, binary: string): boolean {
  if (typeof value !== "object" || value === null || !("installs" in value)) return false;
  const installs = value.installs;
  if (typeof installs !== "object" || installs === null) return false;
  const source = `(git+${TAURI_CEF_REPOSITORY}?rev=${TAURI_CEF_REVISION}#${TAURI_CEF_REVISION})`;
  return Object.entries(installs).some(([name, install]: [string, unknown]) =>
    name.startsWith("tauri-cli ") && name.endsWith(source) &&
    typeof install === "object" && install !== null && "bins" in install &&
    Array.isArray(install.bins) && install.bins.includes(binary),
  );
}

async function ready(directory: string, binary: string): Promise<boolean> {
  try {
    const executable = path.join(directory, "bin", binary);
    if (!(await stat(executable)).isFile()) return false;
    await access(executable, constants.X_OK);
    return installedPackage(JSON.parse(await readFile(path.join(directory, ".crates2.json"), "utf8")) as unknown, binary);
  } catch { return false; }
}

const install: Installer = (command, args, options) => new Promise((resolve, reject) => {
  const child = spawn(command, args, { ...options, stdio: "inherit" });
  const interrupt = () => { child.kill("SIGINT"); };
  const terminate = () => { child.kill("SIGTERM"); };
  const cleanup = () => { process.removeListener("SIGINT", interrupt); process.removeListener("SIGTERM", terminate); };
  process.on("SIGINT", interrupt);
  process.on("SIGTERM", terminate);
  child.once("error", cause => { cleanup(); reject(cause); });
  child.once("close", (code, signal) => {
    cleanup();
    if (code === 0) resolve();
    else reject(new Error(`A instalação do CLI Chromium falhou (${signal ?? code ?? "sem código"}).`));
  });
});

export async function ensureCefTauriCli(projectRoot: string, options: { directory?: string; installer?: Installer; platform?: NodeJS.Platform } = {}): Promise<string> {
  const directory = options.directory ?? path.join(homedir(), ".cache", "jarvis", "tauri-cli", TAURI_CEF_REVISION);
  const binary = (options.platform ?? process.platform) === "win32" ? "cargo-tauri.exe" : "cargo-tauri";
  if (!await ready(directory, binary)) {
    console.info("Jarvis: preparando o CLI fixado para o Chromium integrado.");
    await mkdir(directory, { recursive: true });
    await (options.installer ?? install)("cargo", ["install", "--git", TAURI_CEF_REPOSITORY, "--rev", TAURI_CEF_REVISION, "--locked", "--root", directory, "--bin", "cargo-tauri", "--force", "tauri-cli"], {
      cwd: projectRoot,
      env: { ...process.env, CARGO_TARGET_DIR: path.join(directory, "target") },
    });
    if (!await ready(directory, binary)) throw new Error("O CLI instalado não corresponde à revisão Chromium fixada. O CLI Tauri padrão não será utilizado.");
  }
  return path.join(directory, "bin", binary);
}
