import { appendFileSync, copyFileSync, mkdirSync, readdirSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import { prerelease } from "semver";
import { command, configuration, optionalRelease, root } from "./release-common";
import { artifactNames, desktopManifest, notesFromTag, RELEASE_REPOSITORY, releaseTargets, supportedTarget, targetPlatforms, validateCIRequest } from "./release-plan";
import { verifyReleaseArtifacts } from "./release-artifacts";

const tag = process.env.RELEASE_TAG ?? "";
const publish = process.env.RELEASE_PUBLISH === "true";
const artifactDirectory = path.join(root, "release-artifacts");
function output(name: string, value: string) {
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `${name}=${value}\n`);
}
function verifyTag() {
  const { config } = configuration();
  if (tag && tag !== `v${config.version}`) throw new Error("A versão no checkout difere da tag solicitada.");
  const sha = command("git", ["rev-parse", "HEAD"], true);
  if (tag && command("git", ["rev-parse", `refs/tags/${tag}^{commit}`], true) !== sha) throw new Error("A tag não aponta para o commit compilado.");
  if (process.env.RELEASE_SHA && process.env.RELEASE_SHA !== sha) throw new Error("O commit mudou entre a compilação e a publicação.");
  return { config, sha };
}
function prepare() {
  validateCIRequest(process.env.GITHUB_REPOSITORY, process.env.GITHUB_REF, tag, publish);
  if (tag) {
    const commit = command("git", ["rev-parse", `refs/tags/${tag}^{commit}`], true);
    command("git", ["merge-base", "--is-ancestor", commit, "HEAD"], true);
    command("git", ["checkout", "--detach", commit]);
  }
  const { config, sha } = verifyTag();
  if (publish && optionalRelease(tag)?.isDraft === false) throw new Error("Esta versão já foi publicada e não será substituída.");
  output("sha", sha);
  output("version", config.version);
  console.info(`Compilar Jarvis ${config.version} (${sha.slice(0, 7)}) · ${publish ? "publicação" : "validação sem publicação"}`);
}
function stage() {
  const { config, sha } = verifyTag();
  const releaseTarget = supportedTarget(process.env.RELEASE_TARGET);
  const bundle = path.join(root, "src-tauri/target", releaseTarget, "release/bundle");
  mkdirSync(artifactDirectory, { recursive: true });
  const { archive, signature } = artifactNames(config.version, releaseTarget);
  if (releaseTarget === "x86_64-pc-windows-msvc") {
    const nsis = path.join(bundle, "nsis");
    const installers = readdirSync(nsis).filter(name => name.endsWith("-setup.exe") && name.includes(`_${config.version}_`));
    if (installers.length !== 1) throw new Error("Não foi possível identificar o instalador Windows da versão.");
    copyFileSync(path.join(nsis, installers[0]), path.join(artifactDirectory, archive));
    copyFileSync(path.join(nsis, `${installers[0]}.sig`), path.join(artifactDirectory, signature));
  } else {
    const app = path.join(bundle, "macos", `${config.productName}.app`);
    command("codesign", ["--verify", "--deep", "--strict", app]);
    // Verify the actual signer, not just that the bundle has any valid signature.
    command("codesign", ["-d", `--extract-certificates=${path.join(process.env.RUNNER_TEMP ?? artifactDirectory, "jarvis-cert-")}`, app], true);
    const certPath = path.join(process.env.RUNNER_TEMP ?? artifactDirectory, "jarvis-cert-0");
    const fingerprint = command("openssl", ["x509", "-inform", "DER", "-in", certPath, "-noout", "-fingerprint", "-sha1"], true).split("=").at(-1)?.replaceAll(":", "").trim();
    if (!process.env.APPLE_SIGNING_IDENTITY || fingerprint?.toLowerCase() !== process.env.APPLE_SIGNING_IDENTITY.toLowerCase()) throw new Error("O build usa outra identidade Apple.");
    const version = config.version;
    const prefix = `Jarvis_${version}_${releaseTarget}`;
    const dmg = readdirSync(path.join(bundle, "dmg")).filter(name => name.endsWith(".dmg") && name.includes(`_${version}_`));
    if (dmg.length !== 1) throw new Error("Não foi possível identificar o DMG da versão.");
    command("codesign", ["--verify", "--strict", path.join(bundle, "dmg", dmg[0])]);
    copyFileSync(path.join(bundle, "dmg", dmg[0]), path.join(artifactDirectory, `${prefix}.dmg`));
    copyFileSync(`${app}.tar.gz`, path.join(artifactDirectory, archive));
    // Tauri may lowercase the signature filename even when productName is capitalized.
    const signatureName = readdirSync(path.join(bundle, "macos")).filter(name => name.toLowerCase() === `${config.productName}.app.tar.gz.sig`.toLowerCase());
    if (signatureName.length !== 1) throw new Error("Assinatura do atualizador ausente ou ambígua.");
    copyFileSync(path.join(bundle, "macos", signatureName[0]), path.join(artifactDirectory, signature));
  }
  writeFileSync(path.join(artifactDirectory, `build-${releaseTarget}.json`), JSON.stringify({ version: config.version, sha, target: releaseTarget }) + "\n");
  verifyReleaseArtifacts(artifactDirectory, config.version, sha, releaseTarget, config.plugins.updater.pubkey);
  command("git", ["diff", "--exit-code", "HEAD"]);
  console.info(`Artefatos de ${releaseTarget} e assinatura do atualizador verificados.`);
}
function verifyAssets() {
  const { config, sha } = verifyTag();
  return { version: config.version, platforms: releaseTargets.map(target => verifyReleaseArtifacts(artifactDirectory, config.version, sha, target, config.plugins.updater.pubkey)) };
}
function publishArtifacts() {
  validateCIRequest(process.env.GITHUB_REPOSITORY, process.env.GITHUB_REF, tag, publish);
  if (!publish) throw new Error("Publicação desativada; use o modo de validação.");
  const assets = verifyAssets();
  const existing = optionalRelease(tag);
  if (existing?.isDraft === false) throw new Error("A release já está pública. Nenhum arquivo será substituído.");
  const beta = prerelease(assets.version) !== null;
  if (!existing) {
    const notes = notesFromTag(command("git", ["for-each-ref", "--format=%(contents)", `refs/tags/${tag}`], true) + "\n", assets.version);
    const notesFile = path.join(artifactDirectory, "notes.md");
    writeFileSync(notesFile, notes);
    command("gh", ["release", "create", tag, "--repo", RELEASE_REPOSITORY, "--verify-tag", "--draft", "--title", `Jarvis ${assets.version}`, ...(beta ? ["--prerelease"] : []), ...(notes ? ["--notes-file", notesFile] : ["--generate-notes"])]);
  }
  const release = optionalRelease(tag);
  if (!release?.isDraft) throw new Error("A release deixou de ser um rascunho antes do upload.");
  const manifest = desktopManifest(assets.version, assets.platforms, release.body);
  const manifests = ["latest.json"];
  writeFileSync(path.join(artifactDirectory, "latest.json"), JSON.stringify(manifest, null, 2) + "\n");
  for (const platform of releaseTargets.flatMap(targetPlatforms)) {
    const name = `latest-${platform}.json`;
    manifests.push(name);
    writeFileSync(path.join(artifactDirectory, name), JSON.stringify({ ...manifest, platforms: { [platform]: manifest.platforms[platform] } }, null, 2) + "\n");
  }
  const files = [...assets.platforms.flatMap(asset => asset.names), ...manifests];
  command("gh", ["release", "upload", tag, "--repo", RELEASE_REPOSITORY, "--clobber", ...files.map(name => path.join(artifactDirectory, name))]);
  // Publish both platforms atomically from one draft, after every required upload.
  const remote = JSON.parse(command("gh", ["release", "view", tag, "--repo", RELEASE_REPOSITORY, "--json", "assets,isDraft"], true)) as { isDraft: boolean; assets: { name: string; size: number }[] };
  if (!remote.isDraft || files.some(name => !remote.assets.some(asset => asset.name === name && asset.size === statSync(path.join(artifactDirectory, name)).size))) throw new Error("O upload está incompleto; o rascunho foi preservado.");
  command("gh", ["release", "edit", tag, "--repo", RELEASE_REPOSITORY, "--draft=false", `--latest=${!beta}`]);
  console.info(`Publicado: https://github.com/${RELEASE_REPOSITORY}/releases/tag/${tag}`);
}
try {
  const action = process.argv[2];
  if (action === "prepare") prepare();
  else if (action === "stage") stage();
  else if (action === "publish") publishArtifacts();
  else if (action === "verify") { verifyAssets(); console.info("Artefatos e assinatura válidos; nenhuma release foi publicada."); }
  else throw new Error("Use prepare, stage, verify ou publish.");
} catch (error) { console.error(error instanceof Error ? error.message : "Falha na publicação pelo CI."); process.exitCode = 1; }
