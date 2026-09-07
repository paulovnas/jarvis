import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { command, configuration, optionalRelease, read, releaseEnvironment, releaseTarget, releaseWorkflow, requiredSecrets, root, versionFiles } from "./release-common";
import { parseReleaseArguments, releaseVersion, RELEASE_REPOSITORY, replaceCargoVersion } from "./release-plan";

function main() {
  const args = process.argv.slice(2).filter(arg => arg !== "--");
  if (!args.length || args.includes("--help")) {
    console.info("Uso: bun run release 0.8.4-beta [--notes-file arquivo.md] [--dry-run]\nPrepara o commit/tag e inicia a validação, compilação assinada e publicação no GitHub Actions. Funciona em macOS, Windows e Linux; não requer chaves locais nem Rust.");
    return;
  }
  const options = parseReleaseArguments(args);
  const { pkg, config } = configuration();
  const version = releaseVersion(options.version, pkg.version);
  const tag = `v${version}`;
  if (options.dryRun) {
    console.info(`${tag} → ${RELEASE_REPOSITORY}\nmacOS Apple Silicon (${releaseTarget})\nCommit/tag → push → GitHub Actions: verificações → build assinado → DMG/atualizador → publicação.\nNenhum arquivo foi alterado ou publicado.`);
    return;
  }
  if (command("git", ["status", "--porcelain"], true)) throw new Error("Faça commit das alterações antes de gerar um release.");
  if (command("git", ["branch", "--show-current"], true) !== "main") throw new Error("Gere o release a partir da branch main.");
  command("gh", ["auth", "status"]);
  const repo = JSON.parse(command("gh", ["repo", "view", "--json", "nameWithOwner,isPrivate"], true)) as { nameWithOwner: string; isPrivate: boolean };
  if (repo.nameWithOwner !== RELEASE_REPOSITORY || repo.isPrivate) throw new Error(`O release exige o repositório público ${RELEASE_REPOSITORY}.`);
  command("gh", ["workflow", "view", releaseWorkflow, "--repo", RELEASE_REPOSITORY], true);
  const secrets = JSON.parse(command("gh", ["secret", "list", "--repo", RELEASE_REPOSITORY, "--env", releaseEnvironment, "--json", "name"], true)) as { name: string }[];
  const missing = requiredSecrets.filter(name => !secrets.some(secret => secret.name === name));
  if (missing.length) throw new Error(`Configure os secrets do ambiente ${releaseEnvironment}: ${missing.join(", ")}.`);
  if (options.notesFile && !existsSync(path.resolve(options.notesFile))) throw new Error("Arquivo de notas não encontrado.");
  const notes = options.notesFile ? readFileSync(path.resolve(options.notesFile), "utf8").trim() : "";
  if (optionalRelease(tag)?.isDraft === false) throw new Error("Esta versão já foi publicada. Escolha uma versão maior.");
  command("git", ["fetch", "origin", "main", "--tags"]);
  const sourceCommit = command("git", ["rev-parse", "HEAD"], true);
  const taggedCommit = command("git", ["tag", "--list", tag], true) ? command("git", ["rev-list", "-n", "1", tag], true) : null;
  if (taggedCommit && taggedCommit !== sourceCommit) throw new Error("A tag existente aponta para outro commit. Não será sobrescrita; reexecute o workflow da tag no Actions.");
  const remoteCommit = command("git", ["rev-parse", "origin/main"], true);
  // Retry the same release after an interrupted atomic push, without replaying other local work.
  const retryPush = taggedCommit === sourceCommit && pkg.version === version
    && command("git", ["log", "-1", "--format=%s"], true) === `chore(release): ${tag}`
    && command("git", ["rev-parse", "HEAD^"], true) === remoteCommit;
  if (sourceCommit !== remoteCommit && !retryPush) throw new Error("Sincronize main com origin/main antes de publicar.");
  if (taggedCommit && notes) throw new Error("A tag já existe; reenvie sem --notes-file para preservar as notas originais.");
  if (pkg.version !== version) {
    pkg.version = version; config.version = version;
    writeFileSync(path.join(root, "package.json"), JSON.stringify(pkg, null, 2) + "\n");
    writeFileSync(path.join(root, "src-tauri/tauri.conf.json"), JSON.stringify(config, null, 2) + "\n");
    for (const file of versionFiles.slice(2)) writeFileSync(path.join(root, file), replaceCargoVersion(read(file), version, file.endsWith(".lock")));
    command("git", ["add", "--", ...versionFiles]);
    command("git", ["diff", "--cached", "--check"]);
    command("git", ["commit", "-m", `chore(release): ${tag}`]);
  }
  if (!taggedCommit) {
    const temp = mkdtempSync(path.join(tmpdir(), "jarvis-release-notes-"));
    try {
      const file = path.join(temp, "notes.md");
      writeFileSync(file, `Jarvis ${version}\n\n${notes}\n`);
      command("git", ["tag", "-a", tag, "--file", file]);
    } finally { rmSync(temp, { recursive: true, force: true }); }
  }
  command("git", ["push", "--atomic", "origin", "main", `refs/tags/${tag}`]);
  command("gh", ["workflow", "run", releaseWorkflow, "--repo", RELEASE_REPOSITORY, "--ref", "main", "-f", `tag=${tag}`, "-f", "publish=true"]);
  console.info(`Release enviado ao CI: https://github.com/${RELEASE_REPOSITORY}/actions/workflows/${releaseWorkflow}\nO GitHub publicará ${tag} após todas as verificações e assinaturas. Para acompanhar: gh run list --workflow ${releaseWorkflow}`);
}
try { main(); } catch (error) { console.error(error instanceof Error ? error.message : "Não foi possível iniciar o release."); process.exitCode = 1; }
