import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { prerelease } from "semver";
import { RELEASE_REPOSITORY, releaseManifest, releaseVersion, replaceCargoVersion, targetPlatforms } from "./release-plan";

const root = fileURLToPath(new URL("../", import.meta.url));
const args = process.argv.slice(2).filter(arg => arg !== "--");
const read = (file: string) => readFileSync(path.join(root, file), "utf8");
const versions = ["package.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"];
const option = (name: string) => { const i = args.indexOf(name); if (i < 0) return undefined; if (!args[i + 1] || args[i + 1].startsWith("--")) throw new Error(`Informe o valor de ${name}.`); return args[i + 1]; };

function command(program: string, params: string[], capture = false, env: NodeJS.ProcessEnv = process.env): string {
  const result = spawnSync(program, params, { cwd: root, env, encoding: "utf8", stdio: capture ? ["ignore", "pipe", "pipe"] : "inherit" });
  if (result.error || result.status !== 0) throw new Error(`Falhou: ${program} ${params.join(" ")}${capture && result.stderr ? `\n${result.stderr.trim()}` : ""}`);
  return result.stdout?.trim() ?? "";
}
function optionalRelease(tag: string): { isDraft: boolean } | null {
  const result = spawnSync("gh", ["release", "view", tag, "--repo", RELEASE_REPOSITORY, "--json", "isDraft"], { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  if (result.status === 0) return JSON.parse(result.stdout) as { isDraft: boolean };
  if (/release not found|not found|HTTP 404/i.test(result.stderr)) return null;
  throw new Error("Não foi possível consultar o release no GitHub. Confira gh auth status.");
}

async function main() {
  if (!args.length || args.includes("--help")) {
    console.info("Uso: bun run release 0.8.0-beta.2 [--notes-file arquivo.md] [--target universal-apple-darwin] [--dry-run]\nValida, compila, assina, cria o commit/tag da versão e publica o release no GitHub. Execute com o código já commitado e a árvore limpa."); return;
  }
  for (let i = 1; i < args.length; i++) {
    if (args[i] === "--dry-run") continue;
    if (args[i] === "--target" || args[i] === "--notes-file") { option(args[i]); i++; continue; }
    throw new Error(`Opção desconhecida: ${args[i]}. Use --help.`);
  }
  const packageJson = JSON.parse(read("package.json")) as { version: string };
  const config = JSON.parse(read("src-tauri/tauri.conf.json")) as { version: string; plugins: { updater: { pubkey: string } } };
  for (const file of versions.slice(2)) {
    if (replaceCargoVersion(read(file), packageJson.version, file.endsWith(".lock")) !== read(file)) throw new Error(`A versão em ${file} difere de package.json.`);
  }
  if (config.version !== packageJson.version) throw new Error("As versões em tauri.conf.json e package.json diferem.");
  const version = releaseVersion(args[0], packageJson.version);
  const tag = `v${version}`;
  const target = option("--target") ?? (process.arch === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin");
  const platforms = targetPlatforms(target);
  const notes = option("--notes-file");
  const beta = prerelease(version) !== null;
  if (args.includes("--dry-run")) {
    console.info(`${tag} → ${RELEASE_REPOSITORY}\n${beta ? "Pré-release" : "Release estável"} · ${platforms.join(", ")}\nQuality gates → build macOS assinado → commit/tag → push → release em rascunho → manifestos → publicação.\nNenhum arquivo foi alterado ou publicado.`); return;
  }
  if (process.platform !== "darwin") throw new Error("O comando de distribuição atual gera instaladores macOS. Execute em um Mac.");
  if (command("git", ["status", "--porcelain"], true)) throw new Error("Faça commit das alterações antes de gerar um release.");
  if (command("git", ["branch", "--show-current"], true) !== "main") throw new Error("Gere o release a partir da branch main.");
  command("gh", ["auth", "status"]);
  const repo = JSON.parse(command("gh", ["repo", "view", "--json", "nameWithOwner,isPrivate"], true)) as { nameWithOwner: string; isPrivate: boolean };
  if (repo.nameWithOwner !== RELEASE_REPOSITORY || repo.isPrivate) throw new Error("O release exige o repositório público paulovnas/jarvis.");
  if (notes && !existsSync(path.resolve(notes))) throw new Error("Arquivo de notas não encontrado.");
  const existing = optionalRelease(tag);
  if (existing && !existing.isDraft) throw new Error("Esta versão já foi publicada. Escolha uma versão maior.");
  command("git", ["fetch", "origin", "main", "--tags"]);
  const sourceCommit = command("git", ["rev-parse", "HEAD"], true);
  if (sourceCommit !== command("git", ["rev-parse", "origin/main"], true)) throw new Error("Sincronize main com origin/main antes de publicar.");
  const key = process.env.TAURI_SIGNING_PRIVATE_KEY ?? path.join(homedir(), ".jarvis/release/updater.key");
  if (!existsSync(key) || !statSync(key).isFile()) throw new Error("Chave de atualização ausente. Restaure ~/.jarvis/release/updater.key; não gere outra chave para instalações existentes.");
  if (!existsSync(`${key}.pub`) || readFileSync(`${key}.pub`, "utf8").trim() !== config.plugins.updater.pubkey) throw new Error("A chave pública não corresponde à configurada no Jarvis.");
  // Skip Finder AppleScript decoration so packaging never requires GUI automation.
  const env = { ...process.env, CI: "true", TAURI_BUNDLER_DMG_IGNORE_CI: "false", TAURI_SIGNING_PRIVATE_KEY: key, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "" };
  packageJson.version = version; config.version = version;
  writeFileSync(path.join(root, "package.json"), JSON.stringify(packageJson, null, 2) + "\n");
  writeFileSync(path.join(root, "src-tauri/tauri.conf.json"), JSON.stringify(config, null, 2) + "\n");
  for (const file of versions.slice(2)) writeFileSync(path.join(root, file), replaceCargoVersion(read(file), version, file.endsWith(".lock")));
  command("bun", ["run", "check"]);
  command("cargo", ["clippy", "--manifest-path", "src-tauri/Cargo.toml", "--all-targets", "--", "-D", "warnings"]);
  command("cargo", ["test", "--manifest-path", "src-tauri/Cargo.toml"]);
  command("bun", ["run", "tauri", "build", "--target", target, "--bundles", "app,dmg", "--config", path.join(root, "src-tauri/tauri.release.conf.json")], false, env);
  const bundle = path.join(root, "src-tauri/target", target, "release/bundle");
  const archiveSource = path.join(bundle, "macos/jarvis.app.tar.gz");
  const signature = readFileSync(`${archiveSource}.sig`, "utf8").trim();
  command("codesign", ["--verify", "--deep", "--strict", path.join(bundle, "macos/jarvis.app")]);
  const dmgs = readdirSync(path.join(bundle, "dmg")).filter(file => file.endsWith(".dmg") && file.includes(`_${version}_`));
  if (dmgs.length !== 1) throw new Error("Não foi possível identificar o instalador DMG desta versão.");
  if (command("git", ["rev-parse", "HEAD"], true) !== sourceCommit || command("git", ["branch", "--show-current"], true) !== "main") throw new Error("O checkout mudou durante o build. Revise antes de publicar.");
  const dirty = command("git", ["diff", "HEAD", "--name-only"], true).split("\n").filter(Boolean);
  if (dirty.some(file => !versions.includes(file)) || command("git", ["ls-files", "--others", "--exclude-standard"], true)) throw new Error("Outros arquivos mudaram durante o build. Revise antes de publicar.");
  if (dirty.length) {
    command("git", ["add", "--", ...versions]); command("git", ["diff", "--cached", "--check"]);
    command("git", ["commit", "-m", `chore(release): ${tag}`]);
  }
  const tagExists = command("git", ["tag", "--list", tag], true);
  if (tagExists) {
    if (command("git", ["rev-list", "-n", "1", tag], true) !== command("git", ["rev-parse", "HEAD"], true)) throw new Error("A tag existente aponta para outro commit. Não será sobrescrita.");
  } else command("git", ["tag", "-a", tag, "-m", `Jarvis ${version}`]);
  command("git", ["push", "--atomic", "origin", "main", `refs/tags/${tag}`]);
  const temp = mkdtempSync(path.join(tmpdir(), "jarvis-release-"));
  try {
    const archive = `jarvis_${version}_${target}.app.tar.gz`;
    copyFileSync(archiveSource, path.join(temp, archive));
    copyFileSync(`${archiveSource}.sig`, path.join(temp, `${archive}.sig`));
    const dmg = `jarvis_${version}_${target}.dmg`;
    copyFileSync(path.join(bundle, "dmg", dmgs[0]), path.join(temp, dmg));
    if (!existing) command("gh", ["release", "create", tag, "--repo", RELEASE_REPOSITORY, "--verify-tag", "--draft", "--title", `Jarvis ${version}`, ...(beta ? ["--prerelease"] : []), ...(notes ? ["--notes-file", path.resolve(notes)] : ["--generate-notes"])]);
    const { body, isDraft } = JSON.parse(command("gh", ["release", "view", tag, "--repo", RELEASE_REPOSITORY, "--json", "body,isDraft"], true)) as { body: string; isDraft: boolean };
    if (!isDraft) throw new Error("O release foi publicado durante o build. Seus arquivos não serão substituídos.");
    const manifest = JSON.stringify(releaseManifest(version, target, archive, signature, body), null, 2) + "\n";
    for (const name of ["latest.json", ...platforms.map(platform => `latest-${platform}.json`)]) writeFileSync(path.join(temp, name), manifest);
    command("gh", ["release", "upload", tag, "--repo", RELEASE_REPOSITORY, "--clobber", ...readdirSync(temp).map(file => path.join(temp, file))]);
    command("gh", ["release", "edit", tag, "--repo", RELEASE_REPOSITORY, "--draft=false", `--latest=${!beta}`]);
    console.info(`Publicado: https://github.com/${RELEASE_REPOSITORY}/releases/tag/${tag}`);
  } finally { rmSync(temp, { recursive: true, force: true }); }
}
main().catch((error: unknown) => { console.error(error instanceof Error ? error.message : "Não foi possível publicar o release."); process.exitCode = 1; });
