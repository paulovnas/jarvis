import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";

export interface SigningIdentity {
  hash: string;
  name: string;
}

export function selectSigningIdentity(output: string, requested?: string): SigningIdentity {
  const identities = Array.from(
    output.matchAll(/^\s*\d+\)\s+([A-Fa-f0-9]{40})\s+"([^"]+)"\s*$/gm),
    (match) => ({ hash: match[1], name: match[2] }),
  );
  if (requested) {
    const matches = identities.filter(
      ({ hash, name }) => hash.toLowerCase() === requested.toLowerCase() || name === requested,
    );
    if (matches.length === 1) return matches[0];
    throw new Error(
      "APPLE_SIGNING_IDENTITY deve identificar exatamente um certificado válido instalado. " +
      "Use security find-identity -v -p codesigning e informe o hash do certificado.",
    );
  }
  const candidates = identities.filter(({ name }) =>
    /^(Apple Development|Mac Developer|Developer ID Application|Apple Distribution|3rd Party Mac Developer Application): /.test(name),
  );
  if (candidates.length === 1) return candidates[0];
  throw new Error(
    candidates.length === 0
      ? "Nenhum certificado de assinatura Apple disponível. Instale um certificado Apple Development para os builds locais do Jarvis."
      : "Há mais de um certificado de assinatura disponível. Defina APPLE_SIGNING_IDENTITY com o hash escolhido para manter a identidade do Jarvis entre builds.",
  );
}

export function localSigningIdentity(requested = process.env.APPLE_SIGNING_IDENTITY): SigningIdentity {
  return selectSigningIdentity(
    execFileSync("/usr/bin/security", ["find-identity", "-v", "-p", "codesigning"], { encoding: "utf8" }),
    requested,
  );
}

export function signingCommand(args: string[]): "dev" | "build" | "bundle" | undefined {
  const separator = args.indexOf("--");
  const options = separator < 0 ? args : args.slice(0, separator);
  if (options.some((arg) => ["--help", "-h", "help", "--version", "-V"].includes(arg))) return;
  return options.find((arg) => arg === "dev" || arg === "build" || arg === "bundle");
}

export function usesDevelopmentProfile(args: string[]): boolean {
  const command = signingCommand(args);
  if (command === "dev") return true;
  const separator = args.indexOf("--");
  const options = separator < 0 ? args : args.slice(0, separator);
  return (command === "build" || command === "bundle") && options.includes("--debug");
}

export function developmentArguments(args: string[], projectRoot: string): string[] {
  if (!usesDevelopmentProfile(args)) return args;
  const separator = args.indexOf("--");
  const index = separator < 0 ? args.length : separator;
  const config = path.join(projectRoot, "src-tauri/tauri.dev.conf.json");
  return [...args.slice(0, index), "--config", config, ...args.slice(index)];
}

export function signedDevArguments(args: string[], projectRoot: string): string[] {
  const separator = args.indexOf("--");
  const options = separator < 0 ? args : args.slice(0, separator);
  if (options.some((arg) => arg === "--runner" || arg.startsWith("--runner=") || /^-r/.test(arg))) {
    throw new Error("O modo dev assinado usa o runner Cargo do Jarvis; remova a opção --runner.");
  }
  // Cargo's array syntax preserves spaces in checkout paths. A string runner does not.
  const runner = JSON.stringify(["/bin/sh", path.posix.join(projectRoot, "scripts/run-signed-macos.sh")]);
  const config = JSON.stringify({
    build: {
      runner: {
        cmd: "cargo",
        args: [
          "--config", `target.aarch64-apple-darwin.runner = ${runner}`,
          "--config", `target.x86_64-apple-darwin.runner = ${runner}`,
        ],
      },
    },
  });
  // Tauri's first separator starts Cargo arguments; the second starts application arguments.
  const index = separator < 0 ? args.length : separator;
  return [...args.slice(0, index), "--config", config, ...args.slice(index)];
}

export function projectIdentifier(projectRoot: string, development = false): string {
  const filename = development ? "tauri.dev.conf.json" : "tauri.conf.json";
  const config: unknown = JSON.parse(readFileSync(path.join(projectRoot, "src-tauri", filename), "utf8"));
  if (typeof config !== "object" || config === null || !("identifier" in config) || typeof config.identifier !== "string") {
    throw new Error("O identificador do aplicativo está ausente em tauri.conf.json.");
  }
  return config.identifier;
}
