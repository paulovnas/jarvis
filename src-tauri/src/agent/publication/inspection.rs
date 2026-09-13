//! Bounded, read-only repository discovery for publication and GitHub questions.
use super::*;
use std::{collections::VecDeque, fs};
use tokio::sync::watch;

const MAX_DIRECTORIES: usize = 2_000;
const MAX_REPOSITORIES: usize = 24;

pub(in crate::agent) fn definition() -> Value {
    tools::definition("jarvis_inspect_publication", "Inspect Git publication state without mutations or network requests. Returns independent repository roots, branch/upstream, changed files, diff summaries, whitespace issues and available package scripts in one call. Start here for publication; inspect only the needed diffs next. Omit paths for bounded discovery, or provide known repository roots relative to the project to refresh them. Truncation is reported explicitly; results are evidence, not publication approval.", json!({"paths":{"type":"array","items":{"type":"string","minLength":1},"minItems":1,"maxItems":MAX_REPOSITORIES}}), &[])
}

pub(in crate::agent) async fn inspect(
    root: &Path,
    args: &Value,
    signal: watch::Receiver<bool>,
) -> Result<String, AgentError> {
    if *signal.borrow() {
        return Err(AgentError::cancelled());
    }
    tools::scoped(root, ".", false)?;
    let (paths, truncated) = if let Some(paths) = args["paths"].as_array() {
        if paths.is_empty() || paths.len() > MAX_REPOSITORIES {
            return Err(error(
                "publication_inspection",
                "Informe de 1 a 24 repositórios.",
            ));
        }
        (
            paths
                .iter()
                .map(|path| {
                    path.as_str().map(str::to_owned).ok_or_else(|| {
                        error("publication_inspection", "Cada path deve ser um texto.")
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            false,
        )
    } else {
        let root = root.to_path_buf();
        tauri::async_runtime::spawn_blocking(move || discover(&root))
            .await
            .map_err(|_| AgentError::internal())??
    };
    let mut repositories = Vec::new();
    for path in paths {
        let result = match inspect_repository(root, &path, &signal).await {
            Ok(value) => value,
            Err(cause) if cause.code == "cancelled" => return Err(cause),
            Err(cause) => json!({"path":path,"error":{"code":cause.code,"message":cause.message}}),
        };
        repositories.push(result);
    }
    Ok(json!({"repositories":repositories,"discoveryTruncated":truncated,"guidance":"Each path is an independent repository. Do not treat nested repositories as files to stage in a parent repository. Reuse this state, inspect the relevant diffs and propose only the requested operations. If discovery is truncated, use known paths or list the remaining project directories."}).to_string())
}

fn discover(root: &Path) -> Result<(Vec<String>, bool), AgentError> {
    let mut pending = VecDeque::from([root.to_path_buf()]);
    let mut repositories = Vec::new();
    let mut visited = 0;
    let mut truncated = false;
    while let Some(directory) = pending.pop_front() {
        visited += 1;
        if visited > MAX_DIRECTORIES || repositories.len() >= MAX_REPOSITORIES {
            truncated = true;
            break;
        }
        if fs::symlink_metadata(directory.join(".git")).is_ok_and(|meta| !meta.is_symlink()) {
            let relative = directory
                .strip_prefix(root)
                .map_err(|_| AgentError::internal())?;
            repositories.push(if relative.as_os_str().is_empty() {
                ".".into()
            } else {
                relative.to_string_lossy().replace('\\', "/")
            });
        }
        let entries = fs::read_dir(&directory).map_err(|_| {
            error(
                "publication_inspection",
                "Não foi possível inspecionar uma pasta do projeto.",
            )
        })?;
        let mut children = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|_| {
                error(
                    "publication_inspection",
                    "Não foi possível listar uma pasta do projeto.",
                )
            })?;
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') || tools::ignored_discovery_directory(&name)
            {
                continue;
            }
            if entry
                .file_type()
                .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink())
            {
                if visited + pending.len() + children.len() >= MAX_DIRECTORIES {
                    truncated = true;
                    break;
                }
                children.push(entry.path());
            }
        }
        children.sort();
        pending.extend(children);
    }
    repositories.sort();
    Ok((repositories, truncated))
}

fn excerpt(text: &str, limit: usize) -> Value {
    json!({"text":text.chars().take(limit).collect::<String>(),"truncated":text.chars().count() > limit})
}

async fn git_output(
    directory: &Path,
    args: &[&str],
    signal: &watch::Receiver<bool>,
) -> Result<(bool, String), AgentError> {
    let mut command = crate::background::tokio_command("git");
    crate::mcp::executable::configure(&mut command, false);
    command
        .args(["--no-optional-locks", "-c", "core.fsmonitor=false"])
        .args(args)
        .current_dir(directory)
        .env("GIT_TERMINAL_PROMPT", "0");
    let mut child = crate::agent::shell::spawn_process(command)
        .map_err(|_| error("publication_inspection", "Não foi possível iniciar o Git."))?;
    let stdout = tokio::spawn(tools::capture(
        child.stdout().take().ok_or_else(AgentError::internal)?,
    ));
    let stderr = tokio::spawn(tools::capture(
        child.stderr().take().ok_or_else(AgentError::internal)?,
    ));
    let mut signal = signal.clone();
    let status = tokio::select! {
        _ = crate::agent::cancelled(&mut signal) => Err(AgentError::cancelled()),
        result = tokio::time::timeout(std::time::Duration::from_secs(15), child.wait()) => {
            match result {
                Ok(Ok(status)) => Ok(status.success()),
                _ => Err(error("publication_inspection_timeout", "A inspeção Git não terminou em 15 segundos. Tente novamente somente este repositório.")),
            }
        }
    };
    if status.is_err() {
        let _ = Box::into_pin(child.kill()).await;
    }
    let _ = child.wait().await;
    let (stdout, stderr) =
        tokio::join!(tools::finish_capture(stdout), tools::finish_capture(stderr));
    Ok((status?, format!("{stdout}{stderr}")))
}

async fn git_text(
    directory: &Path,
    args: &[&str],
    signal: &watch::Receiver<bool>,
) -> Result<String, AgentError> {
    let (success, text) = git_output(directory, args, signal).await?;
    if success {
        Ok(text.trim().to_owned())
    } else {
        Err(error("publication_inspection", &bounded(&text)))
    }
}

async fn inspect_repository(
    root: &Path,
    path: &str,
    signal: &watch::Receiver<bool>,
) -> Result<Value, AgentError> {
    safe_relative(path, "A pasta do repositório")?;
    let directory = tools::scoped(root, path, false)?;
    if !directory.is_dir() {
        return Err(error(
            "publication_inspection",
            "O caminho não é uma pasta.",
        ));
    }
    let top = git_text(&directory, &["rev-parse", "--show-toplevel"], signal).await?;
    if fs::canonicalize(top.trim()).ok().as_ref() != Some(&directory) {
        return Err(error(
            "publication_inspection",
            "Informe a raiz exata de um repositório Git.",
        ));
    }
    let status = git_text(
        &directory,
        &["status", "--short", "--branch", "--untracked-files=normal"],
        signal,
    )
    .await?;
    let remotes = git_text(&directory, &["remote"], signal).await?;
    let unstaged = git_text(
        &directory,
        &["diff", "--no-ext-diff", "--no-textconv", "--stat"],
        signal,
    )
    .await?;
    let staged = git_text(
        &directory,
        &[
            "diff",
            "--cached",
            "--no-ext-diff",
            "--no-textconv",
            "--stat",
        ],
        signal,
    )
    .await?;
    let mut whitespace = Vec::new();
    for cached in [false, true] {
        let mut args = vec!["diff", "--no-ext-diff", "--no-textconv", "--check"];
        if cached {
            args.push("--cached");
        }
        let (passed, details) = git_output(&directory, &args, signal).await?;
        whitespace.push(json!({"staged":cached,"passed":passed,"details":excerpt(&details,1500)}));
    }
    let scripts = directory.join("package.json");
    let scripts = fs::symlink_metadata(&scripts)
        .ok()
        .filter(|meta| meta.is_file() && !meta.is_symlink() && meta.len() <= 1024 * 1024)
        .and_then(|_| fs::read_to_string(&scripts).ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|manifest| {
            manifest["scripts"]
                .as_object()
                .map(|scripts| scripts.keys().take(40).cloned().collect::<Vec<_>>())
        })
        .unwrap_or_default();
    Ok(
        json!({"path":path,"status":excerpt(&status,6000),"remotes":remotes.lines().take(20).collect::<Vec<_>>(),"unstagedDiff":excerpt(&unstaged,3000),"stagedDiff":excerpt(&staged,3000),"whitespaceChecks":whitespace,"packageScripts":scripts,"hasProjectInstructions":directory.join("AGENTS.md").is_file()}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tests::Fixture;

    #[tokio::test]
    async fn inspection_finds_independent_nested_repositories_without_staging() {
        let (_send, signal) = watch::channel(false);
        let fixture = Fixture::new();
        for path in [".", "frontend", "backend"] {
            let directory = fixture.root.join(path);
            fs::create_dir_all(&directory).unwrap();
            git(&directory, ["init", "--quiet"]).unwrap();
            fs::write(directory.join("pending.txt"), "pending\n").unwrap();
        }
        fs::create_dir_all(fixture.root.join("node_modules/ignored/.git")).unwrap();
        let result: Value = serde_json::from_str(
            &inspect(&fixture.root, &json!({}), signal.clone())
                .await
                .unwrap(),
        )
        .unwrap();
        let repositories = result["repositories"].as_array().unwrap();
        assert_eq!(repositories.len(), 3);
        assert_eq!(result["discoveryTruncated"], false);
        for repository in repositories {
            assert!(repository.get("error").is_none(), "{repository}");
            assert!(repository["status"]["text"]
                .as_str()
                .unwrap()
                .contains("pending.txt"));
            assert_eq!(repository["stagedDiff"]["text"], "");
        }
        let result: Value = serde_json::from_str(
            &inspect(&fixture.root, &json!({"paths":["../outside"]}), signal)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(result["repositories"][0].get("error").is_some());
    }
}
