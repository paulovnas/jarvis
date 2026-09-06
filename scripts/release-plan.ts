import { compare, parse, valid } from "semver";

export const RELEASE_REPOSITORY = "paulovnas/jarvis";
export function parseReleaseArguments(args: string[]) {
  const result = { version: args[0], dryRun: false, notesFile: undefined as string | undefined };
  const seen = new Set<string>();
  for (let i = 1; i < args.length; i++) {
    const arg = args[i];
    if (seen.has(arg)) throw new Error(`Opção repetida: ${arg}.`);
    seen.add(arg);
    if (arg === "--dry-run") { result.dryRun = true; continue; }
    if (arg === "--notes-file" && args[i + 1] && !args[i + 1].startsWith("--")) { result.notesFile = args[++i]; continue; }
    if (arg === "--target" && args[i + 1] === "aarch64-apple-darwin") { i++; continue; }
    throw new Error(`Opção inválida: ${arg}. O CI atual publica somente macOS Apple Silicon. Use --help.`);
  }
  return result;
}

export function validateCIRequest(repository: string | undefined, ref: string | undefined, tag: string, publish: boolean) {
  if (repository !== RELEASE_REPOSITORY || ref !== "refs/heads/main") throw new Error("O workflow de release só pode executar na main do repositório Jarvis.");
  if (publish && !tag) throw new Error("Informe a tag para publicar; sem tag, use o modo de validação.");
  if (tag && (!tag.startsWith("v") || releaseVersion(tag.slice(1), "0.0.0-0") !== tag.slice(1))) throw new Error("Tag de release inválida.");
}

export function notesFromTag(contents: string, version: string): string {
  const prefix = `Jarvis ${version}\n`;
  if (!contents.startsWith(prefix)) throw new Error("A anotação da tag não corresponde à versão do Jarvis.");
  return contents.slice(prefix.length).trim();
}
export function releaseVersion(input: string, current: string): string {
  const version = valid(input);
  if (!version || input !== version || parse(version)!.build.length) throw new Error("Use uma versão SemVer sem v, como 0.8.0-beta.2 ou 0.8.0.");
  if (compare(version, current) < 0) throw new Error("A versão não pode ser menor que a atual.");
  return version;
}
export function targetPlatforms(target: string): string[] {
  if (target === "universal-apple-darwin") return ["darwin-aarch64", "darwin-x86_64"];
  if (target === "aarch64-apple-darwin") return ["darwin-aarch64"];
  if (target === "x86_64-apple-darwin") return ["darwin-x86_64"];
  throw new Error("Target de release não suportado. Use aarch64, x86_64 ou universal-apple-darwin.");
}
export function releaseManifest(version: string, target: string, archive: string, signature: string, notes: string, date = new Date()) {
  if (!signature.trim()) throw new Error("Assinatura de atualização ausente.");
  const url = `https://github.com/${RELEASE_REPOSITORY}/releases/download/v${version}/${encodeURIComponent(archive)}`;
  return { version, notes, pub_date: date.toISOString(), platforms: Object.fromEntries(targetPlatforms(target).map(platform => [platform, { signature: signature.trim(), url }])) };
}
export function replaceCargoVersion(contents: string, version: string, lock = false): string {
  const pattern = lock ? /(\[\[package\]\]\nname = "jarvis"\nversion = ")[^"]+("\n)/ : /(\[package\]\nname = "jarvis"\nversion = ")[^"]+("\n)/;
  if (!pattern.test(contents)) throw new Error("Não foi possível localizar a versão Rust do Jarvis.");
  return contents.replace(pattern, `$1${version}$2`);
}
