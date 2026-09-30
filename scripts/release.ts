import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { command, configuration, optionalRelease, read, releaseEnvironment, releaseWorkflow, requiredSecrets, root, versionFiles } from "./release-common";
import { localValidationTrailer, parseReleaseArguments, releaseVersion, RELEASE_REPOSITORY, replaceCargoVersion, requireLocalValidation } from "./release-plan";

function verifySource(sha: string) {
  if (command("git", ["rev-parse", "HEAD"], true) !== sha
    || command("git", ["status", "--porcelain"], true)
    || command("git", ["branch", "--show-current"], true) !== "main") {
    throw new Error("O código mudou durante a validação. Nenhuma tag ou publicação será enviada; reexecute o release após revisar as alterações.");
  }
}

function main() {
  const args = process.argv.slice(2).filter(arg => arg !== "--");
  if (!args.length || args.includes("--help")) {
    console.info("Uso: bun run release 1.8.3 [--notes-file arquivo.md] [--dry-run]\nPrepara a versão e executa os checks completos localmente antes de criar/enviar a tag. O Actions compila, assina e publica os instaladores. Requer Bun, Git, gh autenticado, Rust/Clippy e as dependências nativas da plataforma; as chaves de assinatura ficam no CI.");
    return;
  }
  const options = parseReleaseArguments(args);
  const { pkg, config } = configuration();
  const version = releaseVersion(options.version, pkg.version);
  const tag = `v${version}`;
  if (options.dryRun) {
    console.info(`${tag} → ${RELEASE_REPOSITORY}\nmacOS Apple Silicon + Windows x64 + Linux x64\nVersão → checks locais (bun run check, Clippy e testes Rust) → tag do commit validado → push → Actions: DMG/NSIS/DEB/AppImage e atualizadores assinados → publicação conjunta.\nNenhum arquivo foi alterado ou publicado.`);
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
  let notes = options.notesFile ? readFileSync(path.resolve(options.notesFile), "utf8").trim() : "";
  if (optionalRelease(tag)?.isDraft === false) throw new Error("Esta versão já foi publicada. Escolha uma versão maior.");
  command("git", ["fetch", "origin", "main", "--tags"]);
  const sourceCommit = command("git", ["rev-parse", "HEAD"], true);
  const taggedCommit = command("git", ["tag", "--list", tag], true) ? command("git", ["rev-list", "-n", "1", tag], true) : null;
  if (taggedCommit && taggedCommit !== sourceCommit) throw new Error("A tag existente aponta para outro commit. Não será sobrescrita; reexecute o workflow da tag no Actions.");
  const remoteCommit = command("git", ["rev-parse", "origin/main"], true);
  // Also allow retry after local checks failed, before the tag was created.
  const retryPush = sourceCommit !== remoteCommit && pkg.version === version
    && command("git", ["log", "-1", "--format=%s"], true) === `chore(release): ${tag}`
    && command("git", ["rev-parse", "HEAD^"], true) === remoteCommit;
  if (sourceCommit !== remoteCommit && !retryPush) throw new Error("Sincronize main com origin/main antes de publicar.");
  if (taggedCommit && notes) throw new Error("A tag já existe; reenvie sem --notes-file para preservar as notas originais.");
  if (!taggedCommit && !options.notesFile) {
    notes = command("gh", ["api", `repos/${RELEASE_REPOSITORY}/releases/generate-notes`, "--method", "POST", "-f", `tag_name=${tag}`, "-f", `target_commitish=${remoteCommit}`, "--jq", ".body"], true);
  }
  if (!taggedCommit && !notes.trim()) throw new Error("As notas da nova versão estão vazias. Informe um arquivo com --notes-file.");
  if (pkg.version !== version) {
    pkg.version = version; config.version = version;
    writeFileSync(path.join(root, "package.json"), JSON.stringify(pkg, null, 2) + "\n");
    writeFileSync(path.join(root, "src-tauri/tauri.conf.json"), JSON.stringify(config, null, 2) + "\n");
    for (const file of versionFiles.slice(2)) writeFileSync(path.join(root, file), replaceCargoVersion(read(file), version, file.endsWith(".lock")));
    command("git", ["add", "--", ...versionFiles]);
    command("git", ["diff", "--cached", "--check"]);
    command("git", ["commit", "-m", `chore(release): ${tag}`]);
  }
  const releaseCommit = command("git", ["rev-parse", "HEAD"], true);
  verifySource(releaseCommit);
  if (taggedCommit) {
    requireLocalValidation(command("git", ["cat-file", "tag", `refs/tags/${tag}`], true), releaseCommit);
  } else {
    console.info(`Validando localmente ${tag} (${releaseCommit.slice(0, 7)}) antes do envio.`);
    const env = { ...process.env, CI: "true" };
    command("bun", ["install", "--frozen-lockfile"], false, env);
    command("bun", ["run", "check"], false, env);
    command("cargo", ["clippy", "--locked", "--manifest-path", "src-tauri/Cargo.toml", "--all-targets", "--", "-D", "warnings"], false, env);
    command("cargo", ["test", "--locked", "--manifest-path", "src-tauri/Cargo.toml", "--", "--test-threads=2"], false, env);
    verifySource(releaseCommit);
  }
  if (!taggedCommit) {
    const temp = mkdtempSync(path.join(tmpdir(), "jarvis-release-notes-"));
    try {
      const file = path.join(temp, "notes.md");
      writeFileSync(file, `Jarvis ${version}\n\n${notes}\n\n${localValidationTrailer}${releaseCommit}\n`);
      command("git", ["tag", "--no-sign", "-a", tag, releaseCommit, "--cleanup=verbatim", "--file", file]);
    } finally { rmSync(temp, { recursive: true, force: true }); }
  }
  verifySource(releaseCommit);
  command("git", ["push", "--atomic", "origin", `${releaseCommit}:refs/heads/main`, `refs/tags/${tag}`]);
  command("gh", ["workflow", "run", releaseWorkflow, "--repo", RELEASE_REPOSITORY, "--ref", "main", "-f", `tag=${tag}`, "-f", "publish=true"]);
  console.info(`Checks locais concluídos. Release enviado ao CI: https://github.com/${RELEASE_REPOSITORY}/actions/workflows/${releaseWorkflow}\nO GitHub publicará ${tag} após compilar e verificar os instaladores assinados. Para acompanhar: gh run list --workflow ${releaseWorkflow}`);
}
try { main(); } catch (error) { console.error(error instanceof Error ? error.message : "Não foi possível iniciar o release."); process.exitCode = 1; }
