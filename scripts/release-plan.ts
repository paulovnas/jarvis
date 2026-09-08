import { compare, parse, valid } from "semver";

export const RELEASE_REPOSITORY = "paulovnas/jarvis";
export const releaseTargets = ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"] as const;
export type ReleaseTarget = typeof releaseTargets[number];

export function supportedTarget(target: string | undefined): ReleaseTarget {
  const result = releaseTargets.find(value => value === target);
  if (!result) throw new Error("Target de release ausente ou não suportado.");
  return result;
}

export function artifactNames(version: string, target: ReleaseTarget) {
  const prefix = `Jarvis_${version}_${target}`;
  const archive = target === "x86_64-pc-windows-msvc" ? `${prefix}-setup.exe` : `${prefix}.app.tar.gz`;
  return { archive, signature: `${archive}.sig`, names: [...(target === "aarch64-apple-darwin" ? [`${prefix}.dmg`] : []), archive, `${archive}.sig`] };
}
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
    throw new Error(`Opção inválida: ${arg}. O CI publica macOS Apple Silicon e Windows x64 juntos. Use --help.`);
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
  if (!version || input !== version || parse(version)!.build.length) throw new Error("Use uma versão SemVer sem v, como 0.8.4-beta ou 0.8.4.");
  if (compare(version, current) < 0) throw new Error("A versão não pode ser menor que a atual.");
  return version;
}
export function targetPlatforms(target: string): string[] {
  if (target === "universal-apple-darwin") return ["darwin-aarch64", "darwin-x86_64"];
  if (target === "aarch64-apple-darwin") return ["darwin-aarch64"];
  if (target === "x86_64-apple-darwin") return ["darwin-x86_64"];
  if (target === "x86_64-pc-windows-msvc") return ["windows-x86_64"];
  throw new Error("Target de release não suportado. Use aarch64, x86_64 ou universal-apple-darwin.");
}
export function desktopManifest(version: string, assets: { target: ReleaseTarget; archive: string; signature: string }[], notes: string, date = new Date()) {
  if (assets.length !== releaseTargets.length || releaseTargets.some(target => assets.filter(asset => asset.target === target).length !== 1)) {
    throw new Error("A publicação exige exatamente um pacote de cada plataforma.");
  }
  const manifests = assets.map(asset => releaseManifest(version, asset.target, asset.archive, asset.signature, notes, date));
  return { ...manifests[0], platforms: Object.assign({}, ...manifests.map(manifest => manifest.platforms)) as Record<string, { signature: string; url: string }> };
}
export function releaseManifest(version: string, target: string, archive: string, signature: string, notes: string, date = new Date()) {
  if (!signature.trim()) throw new Error("Assinatura de atualização ausente.");
  const url = `https://github.com/${RELEASE_REPOSITORY}/releases/download/v${version}/${encodeURIComponent(archive)}`;
  return { version, notes, pub_date: date.toISOString(), platforms: Object.fromEntries(targetPlatforms(target).map(platform => [platform, { signature: signature.trim(), url }])) };
}
export function replaceCargoVersion(contents: string, version: string, lock = false): string {
  const pattern = lock ? /(\[\[package\]\]\r?\nname = "jarvis"\r?\nversion = ")[^"]+("\r?\n)/ : /(\[package\]\r?\nname = "jarvis"\r?\nversion = ")[^"]+("\r?\n)/;
  if (!pattern.test(contents)) throw new Error("Não foi possível localizar a versão Rust do Jarvis.");
  return contents.replace(pattern, `$1${version}$2`);
}
