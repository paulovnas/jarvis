import { appendFileSync, copyFileSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import { prerelease } from "semver";
import { command, configuration, optionalRelease, releaseTarget, root } from "./release-common";
import { notesFromTag, RELEASE_REPOSITORY, releaseManifest, validateCIRequest } from "./release-plan";
import { verifyUpdaterSignature } from "./updater-signature";

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
  const bundle = path.join(root, "src-tauri/target", releaseTarget, "release/bundle");
  const app = path.join(bundle, "macos", `${config.productName}.app`);
  command("codesign", ["--verify", "--deep", "--strict", app]);
  // Verify the actual signer, not just that the bundle has any valid signature.
  const certificate = command("codesign", ["-d", "--extract-certificates", path.join(process.env.RUNNER_TEMP ?? artifactDirectory, "jarvis-cert-"), app], true);
  void certificate;
  const certPath = path.join(process.env.RUNNER_TEMP ?? artifactDirectory, "jarvis-cert-0");
  const fingerprint = command("openssl", ["x509", "-inform", "DER", "-in", certPath, "-noout", "-fingerprint", "-sha1"], true).split("=").at(-1)?.replaceAll(":", "").trim();
  if (!process.env.APPLE_SIGNING_IDENTITY || fingerprint?.toLowerCase() !== process.env.APPLE_SIGNING_IDENTITY.toLowerCase()) throw new Error("O build usa outra identidade Apple.");
  const version = config.version;
  const prefix = `Jarvis_${version}_${releaseTarget}`;
  mkdirSync(artifactDirectory, { recursive: true });
  const dmg = readdirSync(path.join(bundle, "dmg")).filter(name => name.endsWith(".dmg") && name.includes(`_${version}_`));
  if (dmg.length !== 1) throw new Error("Não foi possível identificar o DMG da versão.");
  command("codesign", ["--verify", "--strict", path.join(bundle, "dmg", dmg[0])]);
  copyFileSync(path.join(bundle, "dmg", dmg[0]), path.join(artifactDirectory, `${prefix}.dmg`));
  copyFileSync(`${app}.tar.gz`, path.join(artifactDirectory, `${prefix}.app.tar.gz`));
  // Tauri may lowercase the signature filename even when productName is capitalized.
  const signatureName = readdirSync(path.join(bundle, "macos")).filter(name => name.toLowerCase() === `${config.productName}.app.tar.gz.sig`.toLowerCase());
  if (signatureName.length !== 1) throw new Error("Assinatura do atualizador ausente ou ambígua.");
  copyFileSync(path.join(bundle, "macos", signatureName[0]), path.join(artifactDirectory, `${prefix}.app.tar.gz.sig`));
  writeFileSync(path.join(artifactDirectory, "build.json"), JSON.stringify({ version, sha, target: releaseTarget }) + "\n");
  verifyAssets();
  command("git", ["diff", "--exit-code", "HEAD"]);
  console.info("Aplicativo, DMG e assinatura do atualizador verificados.");
}
function verifyAssets() {
  const { config, sha } = verifyTag();
  const metadata = JSON.parse(readFileSync(path.join(artifactDirectory, "build.json"), "utf8")) as { version: string; sha: string; target: string };
  if (metadata.version !== config.version || metadata.sha !== sha || metadata.target !== releaseTarget) throw new Error("Os artefatos pertencem a outra versão, commit ou arquitetura.");
  const prefix = `Jarvis_${config.version}_${releaseTarget}`;
  const names = [`${prefix}.dmg`, `${prefix}.app.tar.gz`, `${prefix}.app.tar.gz.sig`];
  for (const name of names) if (!statSync(path.join(artifactDirectory, name)).isFile() || statSync(path.join(artifactDirectory, name)).size === 0) throw new Error(`Artefato ausente: ${name}`);
  const archive = names[1];
  const signature = readFileSync(path.join(artifactDirectory, names[2]), "utf8").trim();
  verifyUpdaterSignature(readFileSync(path.join(artifactDirectory, archive)), signature, config.plugins.updater.pubkey);
  return { version: config.version, archive, signature, names };
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
  const manifest = JSON.stringify(releaseManifest(assets.version, releaseTarget, assets.archive, assets.signature, release.body), null, 2) + "\n";
  const manifests = ["latest.json", "latest-darwin-aarch64.json"];
  for (const name of manifests) writeFileSync(path.join(artifactDirectory, name), manifest);
  const files = [...assets.names, ...manifests];
  command("gh", ["release", "upload", tag, "--repo", RELEASE_REPOSITORY, "--clobber", ...files.map(name => path.join(artifactDirectory, name))]);
  // A draft is only promoted once all five required assets have finished uploading.
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
