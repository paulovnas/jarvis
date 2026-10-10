import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmod, copyFile, lstat, mkdir, mkdtemp, readFile, rename, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { enginePlatform, IMPECCABLE_CLI_VERSION, IMPECCABLE_ENGINE_VERSION, IMPECCABLE_SOURCE_COMMIT, nativeEnginePatches, type EnginePlatform, type NativeEngineReceipt } from "./prepare-impeccable";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const REPOSITORY = "https://github.com/pbakaus/impeccable.git";
const TARGETS: Record<EnginePlatform, string> = {
  "darwin-arm64": "aarch64-apple-darwin", "darwin-x64": "x86_64-apple-darwin",
  "linux-arm64": "aarch64-unknown-linux-gnu", "linux-x64": "x86_64-unknown-linux-gnu",
  "windows-x64": "x86_64-pc-windows-msvc",
};
interface BuildOptions { target: string; destination: string; offline?: boolean }
export interface NativeStage { directory: string; target: EnginePlatform; triple: string; binary: string }

function digest(bytes: Uint8Array): string { return createHash("sha256").update(bytes).digest("hex"); }
function git(args: string[], cwd?: string): Buffer {
  return execFileSync("git", args, { cwd, stdio: ["ignore", "pipe", "pipe"], maxBuffer: 100 * 1024 * 1024 });
}

/** Archive a pinned commit into an app-owned temporary checkout. Never patch docs/. */
export async function stageImpeccableNative(target: string, offline = false): Promise<NativeStage> {
  const platform = enginePlatform(target);
  const directory = await mkdtemp(path.join(os.tmpdir(), "jarvis-impeccable-native-"));
  await chmod(directory, 0o700);
  let source = path.join(ROOT, "docs/impeccable");
  try {
    if (git(["-C", source, "rev-parse", `${IMPECCABLE_SOURCE_COMMIT}^{commit}`]).toString().trim() !== IMPECCABLE_SOURCE_COMMIT) throw new Error("Pinned source unavailable");
  } catch (error) {
    if (offline) throw new Error("O código-fonte fixado do Impeccable não está disponível offline.", { cause: error });
    source = path.join(directory, ".source.git");
    git(["init", "--bare", source]);
    git(["-C", source, "fetch", "--depth=1", REPOSITORY, IMPECCABLE_SOURCE_COMMIT]);
    if (git(["-C", source, "rev-parse", "FETCH_HEAD"]).toString().trim() !== IMPECCABLE_SOURCE_COMMIT) throw new Error("O commit recebido do Impeccable diverge do snapshot fixado.", { cause: error });
  }
  const archive = git(["-C", source, "archive", "--format=tar", IMPECCABLE_SOURCE_COMMIT]);
  execFileSync("tar", ["-xf", "-", "-C", directory], { input: archive, stdio: ["pipe", "ignore", "pipe"], maxBuffer: 8 * 1024 * 1024 });
  const patches = await nativeEnginePatches();
  for (const patch of patches) {
    const bytes = await readFile(path.join(ROOT, patch.path));
    if (digest(bytes) !== patch.sha256) throw new Error("O patch nativo mudou durante a preparação.");
    execFileSync("git", ["apply", "--check", "-"], { cwd: directory, input: bytes, stdio: ["pipe", "ignore", "pipe"] });
    execFileSync("git", ["apply", "-"], { cwd: directory, input: bytes, stdio: ["pipe", "ignore", "pipe"] });
  }
  const triple = TARGETS[platform];
  const binary = path.join(directory, "target", triple, "release", platform === "windows-x64" ? "impeccable.exe" : "impeccable");
  await writeFile(path.join(directory, "jarvis-stage.json"), JSON.stringify({ sourceCommit: IMPECCABLE_SOURCE_COMMIT, target: platform, patches }, null, 2) + "\n", { mode: 0o600 });
  return { directory, target: platform, triple, binary };
}

/** Publish only a binary built from the prepared, pinned source and patch set. */
export async function publishImpeccableNative(stage: NativeStage, destination: string): Promise<{ enginePath: string }> {
  const triple = TARGETS[stage.target];
  const expectedBinary = path.join(stage.directory, "target", triple, "release", stage.target === "windows-x64" ? "impeccable.exe" : "impeccable");
  if (stage.triple !== triple || stage.binary !== expectedBinary) throw new Error("O engine não pertence à plataforma e ao staging preparados.");
  const metadata = await lstat(stage.directory);
  if (!metadata.isDirectory() || metadata.isSymbolicLink()) throw new Error("Staging nativo inválido.");
  const manifest: unknown = JSON.parse(await readFile(path.join(stage.directory, "jarvis-stage.json"), "utf8"));
  const patches = await nativeEnginePatches();
  if (!manifest || typeof manifest !== "object" || !("sourceCommit" in manifest) || !("target" in manifest) || !("patches" in manifest)
    || manifest.sourceCommit !== IMPECCABLE_SOURCE_COMMIT || manifest.target !== stage.target || JSON.stringify(manifest.patches) !== JSON.stringify(patches)) {
    throw new Error("O staging nativo não corresponde ao código e patches atuais.");
  }
  const binaryMetadata = await lstat(stage.binary);
  if (!binaryMetadata.isFile() || binaryMetadata.isSymbolicLink()) throw new Error("O engine compilado não é um arquivo regular.");
  const receipt: NativeEngineReceipt = {
    schemaVersion: 1, engineVersion: IMPECCABLE_ENGINE_VERSION, cliVersion: IMPECCABLE_CLI_VERSION,
    sourceCommit: IMPECCABLE_SOURCE_COMMIT, target: stage.target, patches, engineSha256: digest(await readFile(stage.binary)),
  };
  const bin = path.join(destination, "bin");
  await mkdir(bin, { recursive: true });
  if ((await lstat(bin)).isSymbolicLink()) throw new Error("A pasta do engine não pode ser um link simbólico.");
  const enginePath = path.join(bin, stage.target === "windows-x64" ? "impeccable.exe" : "impeccable");
  const temporary = `${enginePath}.${process.pid}.tmp`;
  await copyFile(stage.binary, temporary);
  if (stage.target !== "windows-x64") await chmod(temporary, 0o755);
  await rename(temporary, enginePath);
  const receiptPath = path.join(bin, "jarvis-engine.json");
  await writeFile(`${receiptPath}.${process.pid}.tmp`, JSON.stringify(receipt, null, 2) + "\n", { mode: 0o600 });
  await rename(`${receiptPath}.${process.pid}.tmp`, receiptPath);
  return { enginePath };
}

export async function buildImpeccableNativeEngine(options: BuildOptions): Promise<{ enginePath: string }> {
  const stage = await stageImpeccableNative(options.target, options.offline);
  // Use an already configured toolchain; the upstream file requests wasm and
  // optional components that a native host build neither needs nor installs.
  const toolchain = process.env.RUSTUP_TOOLCHAIN ?? execFileSync("rustup", ["show", "active-toolchain"], { cwd: ROOT, encoding: "utf8" }).trim().split(/\s+/)[0];
  if (!toolchain) throw new Error("Nenhuma toolchain Rust configurada para o engine nativo.");
  const env: NodeJS.ProcessEnv = { ...process.env, RUSTUP_TOOLCHAIN: toolchain, CARGO_BUILD_JOBS: "1", CARGO_INCREMENTAL: "0", CARGO_TARGET_DIR: path.join(stage.directory, "target") };
  delete env.RUSTFLAGS;
  execFileSync("cargo", ["build", "--release", "--locked", "-p", "impeccable", "--target", stage.triple], { cwd: stage.directory, env, stdio: "inherit" });
  return publishImpeccableNative(stage, options.destination);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const value = (flag: string): string | undefined => {
    const index = args.indexOf(flag);
    if (index < 0) return undefined;
    const result = args[index + 1];
    if (!result || result.startsWith("--")) throw new Error(`Informe um valor após ${flag}.`);
    return result;
  };
  try {
    const target = enginePlatform(value("--target"));
    const destination = value("--destination") ?? path.join(ROOT, "src-tauri/resources/impeccable");
    if (args.includes("--stage-only")) console.info(JSON.stringify(await stageImpeccableNative(target, args.includes("--offline"))));
    else if (args.includes("--publish")) {
      const directory = value("--stage");
      if (!directory) throw new Error("Informe o staging com --stage.");
      const triple = TARGETS[target];
      const binary = path.join(directory, "target", triple, "release", target === "windows-x64" ? "impeccable.exe" : "impeccable");
      console.info(JSON.stringify(await publishImpeccableNative({ directory, target, triple, binary }, destination)));
    } else console.info(JSON.stringify(await buildImpeccableNativeEngine({ target, destination, offline: args.includes("--offline") })));
  } catch (error) {
    console.error(error instanceof Error ? error.message : "Não foi possível preparar o engine Impeccable nativo.");
    process.exitCode = 1;
  }
}
