import { execFileSync } from "node:child_process";
import { access, appendFile, mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { constants } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { ensureCefTauriCli } from "./tauri-cef-cli";

// export-cef-dir selects the native distribution used to generate these exact bindings.
export const CEF_BINDINGS_VERSION = "151.8.1";
// The build metadata selects the native distribution pinned in Cargo.lock.
const CEF_PACKAGE_VERSION = `${CEF_BINDINGS_VERSION}+151.3.24`;
type Run = (program: string, args: string[], options: { cwd: string; env: NodeJS.ProcessEnv }) => Promise<void>;
type Options = {
  home?: string; platform?: NodeJS.Platform; arch?: string; env?: NodeJS.ProcessEnv;
  envFile: string; pathFile?: string; installCli?: boolean; run?: Run;
};
const execute: Run = async (program, args, options) => { execFileSync(program, args, { ...options, stdio: "inherit" }); };

async function installedExporter(directory: string, binary: string): Promise<boolean> {
  try {
    const executable = path.join(directory, "bin", binary);
    if (!(await stat(executable)).isFile()) return false;
    await access(executable, constants.X_OK);
    const value = JSON.parse(await readFile(path.join(directory, ".crates2.json"), "utf8")) as unknown;
    if (typeof value !== "object" || !value || !("installs" in value) || typeof value.installs !== "object" || !value.installs) return false;
    return Object.entries(value.installs).some(([name, record]: [string, unknown]) => name.startsWith(`export-cef-dir ${CEF_PACKAGE_VERSION} (`) && typeof record === "object" && record !== null && "bins" in record && Array.isArray(record.bins) && record.bins.includes(binary));
  } catch { return false; }
}

export async function prepareCefCi(projectRoot: string, options: Options): Promise<string> {
  const platform = options.platform ?? process.platform;
  if (!["darwin", "win32", "linux"].includes(platform)) throw new Error("Plataforma não suportada pelo Chromium integrado.");
  const arch = options.arch ?? process.arch;
  const cache = path.join(options.home ?? homedir(), ".cache", "jarvis");
  const directory = path.join(cache, "cef", CEF_BINDINGS_VERSION, `${platform}-${arch}`);
  const marker = path.join(directory, platform === "darwin" ? "Chromium Embedded Framework.framework/Chromium Embedded Framework" : platform === "win32" ? "libcef.dll" : "libcef.so");
  const receipt = path.join(directory, ".jarvis-cef.json");
  let ready = false;
  try {
    const value = JSON.parse(await readFile(receipt, "utf8")) as unknown;
    const library = await stat(marker);
    ready = library.isFile() && library.size > 0 && typeof value === "object" && value !== null && "version" in value && value.version === CEF_BINDINGS_VERSION && "platform" in value && value.platform === platform && "arch" in value && value.arch === arch;
  } catch { /* A partial cache is re-exported before it can reach the linker. */ }
  if (!ready) {
    const exporter = path.join(cache, "cef-exporter", CEF_BINDINGS_VERSION);
    const binary = platform === "win32" ? "export-cef-dir.exe" : "export-cef-dir";
    const run = options.run ?? execute;
    const env = { ...(options.env ?? process.env), CARGO_TARGET_DIR: path.join(exporter, "target") };
    await mkdir(exporter, { recursive: true });
    if (!await installedExporter(exporter, binary)) {
      await run("cargo", ["install", "export-cef-dir", "--version", `=${CEF_BINDINGS_VERSION}`, "--locked", "--root", exporter, "--bin", "export-cef-dir", "--force"], { cwd: projectRoot, env });
      if (!await installedExporter(exporter, binary)) throw new Error("O exportador CEF instalado não corresponde à versão fixada.");
    }
    await run(path.join(exporter, "bin", binary), [directory], { cwd: projectRoot, env });
    const library = await stat(marker);
    if (!library.isFile() || library.size === 0) throw new Error("A distribuição CEF exportada está incompleta.");
    await writeFile(receipt, JSON.stringify({ version: CEF_BINDINGS_VERSION, platform, arch }));
  }
  const env = options.env ?? process.env;
  const variables = [`CEF_PATH=${directory}`];
  if (platform === "darwin") variables.push(`DYLD_FALLBACK_LIBRARY_PATH=${[env.DYLD_FALLBACK_LIBRARY_PATH, directory, path.join(directory, "Chromium Embedded Framework.framework", "Libraries")].filter(Boolean).join(":")}`);
  if (platform === "linux") variables.push(`LD_LIBRARY_PATH=${[directory, env.LD_LIBRARY_PATH].filter(Boolean).join(":")}`);
  if (platform === "win32") {
    if (!options.pathFile) throw new Error("GITHUB_PATH ausente para o carregamento do CEF no Windows.");
    await appendFile(options.pathFile, `${directory}\n`);
  }
  await appendFile(options.envFile, `${variables.join("\n")}\n`);
  if (options.installCli) await ensureCefTauriCli(projectRoot);
  return directory;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (!process.env.GITHUB_ENV) throw new Error("GITHUB_ENV ausente; este preparador é exclusivo do CI.");
    await prepareCefCi(fileURLToPath(new URL("../", import.meta.url)), { envFile: process.env.GITHUB_ENV, pathFile: process.env.GITHUB_PATH, installCli: process.argv.includes("--cli") });
  } catch (cause) {
    console.error(cause instanceof Error ? cause.message : "Não foi possível preparar o Chromium no CI.");
    process.exitCode = 1;
  }
}
