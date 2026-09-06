import { compare, parse, valid } from "semver";

export const RELEASE_REPOSITORY = "paulovnas/jarvis";
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
