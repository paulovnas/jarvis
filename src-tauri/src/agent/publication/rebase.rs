//! Resume only rebases started by the native publication executor.
use super::*;

#[derive(Serialize, Deserialize)]
struct Checkpoint {
    branch: String,
    before: String,
    onto: String,
    reference: String,
    conflicts: Vec<String>,
    #[serde(default)]
    lease: Option<PushLease>,
    #[serde(default)]
    completed: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct PushLease {
    destination: String,
    expected: String,
}

fn git_path(directory: &Path, name: &str) -> Result<PathBuf, AgentError> {
    let value = git(directory, ["rev-parse", "--git-path", name])?;
    let path = PathBuf::from(value.trim());
    Ok(if path.is_absolute() {
        path
    } else {
        directory.join(path)
    })
}

fn save(directory: &Path, checkpoint: &Checkpoint) -> Result<(), AgentError> {
    let path = git_path(directory, "jarvis-publication-rebase.json")?;
    let failure = || {
        error(
            "publication_sync_recovery",
            "Não foi possível salvar o checkpoint do rebase.",
        )
    };
    let mut file = tempfile::NamedTempFile::new_in(path.parent().ok_or_else(failure)?)
        .map_err(|_| failure())?;
    serde_json::to_writer(file.as_file_mut(), checkpoint).map_err(|_| failure())?;
    file.as_file_mut().sync_all().map_err(|_| failure())?;
    file.persist(path).map_err(|_| failure())?;
    Ok(())
}

fn optional(directory: &Path) -> Result<Option<Checkpoint>, AgentError> {
    let bytes = match std::fs::read(git_path(directory, "jarvis-publication-rebase.json")?) {
        Ok(bytes) => bytes,
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(AgentError::storage()),
    };
    serde_json::from_slice(&bytes).map(Some).map_err(|_| {
        error(
            "publication_sync_recovery",
            "O checkpoint do rebase está inválido.",
        )
    })
}

fn load(directory: &Path) -> Result<Checkpoint, AgentError> {
    optional(directory)?.ok_or_else(|| error("publication_sync_recovery", "Este rebase não possui um checkpoint de publicação do Jarvis. Inspecione a operação existente antes de alterá-la."))
}

fn push_destination(directory: &Path) -> Result<Option<String>, AgentError> {
    let urls = git(
        directory,
        ["remote", "get-url", "--push", "--all", "origin"],
    )?;
    let mut urls = urls.lines();
    Ok(urls
        .next()
        .filter(|_| urls.next().is_none())
        .map(str::to_owned))
}

fn remote_head(
    directory: &Path,
    destination: &str,
    branch: &str,
) -> Result<Option<String>, AgentError> {
    let reference = format!("refs/heads/{branch}");
    let output = git(directory, ["ls-remote", "--refs", destination, &reference])?;
    Ok(output.lines().find_map(|line| {
        let (oid, name) = line.split_once('\t')?;
        (name == reference).then(|| oid.to_owned())
    }))
}

pub(super) fn push_lease(directory: &Path, branch: &str) -> Result<Option<String>, AgentError> {
    let Some(checkpoint) = optional(directory)? else {
        return Ok(None);
    };
    let (Some(lease), Some(completed)) = (checkpoint.lease, checkpoint.completed) else {
        return Ok(None);
    };
    if checkpoint.branch != branch
        || push_destination(directory)?.as_deref() != Some(lease.destination.as_str())
        || rebase_in_progress(directory)?
        || !is_ancestor(directory, &completed, "HEAD")?
    {
        return Ok(None);
    }
    Ok(Some(format!(
        "--force-with-lease=refs/heads/{branch}:{}",
        lease.expected
    )))
}

pub(super) fn complete(directory: &Path) -> Result<(), AgentError> {
    let mut checkpoint = load(directory)?;
    checkpoint.completed = Some(git(directory, ["rev-parse", "HEAD"])?);
    save(directory, &checkpoint)
}

pub(super) fn pushed(directory: &Path, branch: &str) -> Result<(), AgentError> {
    if optional(directory)?.is_some_and(|checkpoint| checkpoint.branch == branch) {
        clear(directory)?;
    }
    Ok(())
}

pub(super) fn checkpoint(
    directory: &Path,
    before: &str,
    onto: &str,
    reference: &str,
) -> Result<(), AgentError> {
    if rebase_in_progress(directory)? {
        return Err(error(
            "publication_sync_recovery",
            "Já existe um rebase em andamento. Resolva-o antes de iniciar outro.",
        ));
    }
    let branch = current_branch(directory)?;
    // A lease is valid only if the old remote history was included before our
    // rebase. A tracking ref refreshed later must never renew this permission.
    let lease = if let Some(destination) = push_destination(directory)? {
        // Failure to inspect the push endpoint prevents lease recovery, but
        // must not prevent a local rebase using the fetched integration branch.
        match remote_head(directory, &destination, &branch).ok().flatten() {
            Some(expected)
                if is_ancestor(directory, &expected, before).unwrap_or(false)
                    || push_lease(directory, &branch)?.as_deref()
                        == Some(
                            format!("--force-with-lease=refs/heads/{branch}:{expected}").as_str(),
                        ) =>
            {
                Some(PushLease {
                    destination,
                    expected,
                })
            }
            _ => None,
        }
    } else {
        None
    };
    save(
        directory,
        &Checkpoint {
            branch,
            before: before.into(),
            onto: onto.into(),
            reference: reference.into(),
            conflicts: vec![],
            lease,
            completed: None,
        },
    )
}

pub(super) fn clear(directory: &Path) -> Result<(), AgentError> {
    match std::fs::remove_file(git_path(directory, "jarvis-publication-rebase.json")?) {
        Ok(()) => Ok(()),
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(error("publication_sync_recovery", "O rebase terminou, mas o checkpoint não pôde ser removido. Verifique o estado antes de repetir.")),
    }
}

pub(super) fn conflict(directory: &Path, details: &str) -> Result<AgentError, AgentError> {
    let mut checkpoint = load(directory)?;
    checkpoint.conflicts = git_paths(directory, ["diff", "--name-only", "--diff-filter=U", "-z"])?
        .into_iter()
        .collect();
    save(directory, &checkpoint)?;
    Ok(error("publication_sync_conflict", &format!("Rebase pausado para resolução, preservando os commits e a implementação. Arquivos em conflito: {}. Inspecione os dois lados, corrija os arquivos com apply_patch e proponha sync=rebase_continue com files contendo os caminhos resolvidos, commitMessage=null e sem outras operações. Use sync=rebase_abort para restaurar a branch anterior. Não repita commit/push. {}", checkpoint.conflicts.join(", "), bounded(details))))
}

pub(super) fn validate(
    directory: &Path,
    proposal: &RepositoryProposal,
) -> Result<String, AgentError> {
    let checkpoint = load(directory)?;
    let metadata = git_path(directory, "rebase-merge")?;
    let read = |name| {
        std::fs::read_to_string(metadata.join(name))
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    if read("orig-head") != checkpoint.before
        || read("onto") != checkpoint.onto
        || read("head-name") != format!("refs/heads/{}", checkpoint.branch)
    {
        return Err(error(
            "publication_sync_recovery",
            "O estado Git mudou desde o checkpoint. Não é seguro repetir a continuação do rebase.",
        ));
    }
    if proposal.commit_message.is_some()
        || proposal.reset.is_some()
        || proposal.branch.is_some()
        || proposal.push != PushMode::None
        || proposal.pull_request.is_some()
        || (proposal.sync == SyncMode::RebaseAbort && !proposal.files.is_empty())
    {
        return Err(error("invalid_publication_proposal", "Continue ou aborte o rebase separadamente. Na continuação, informe apenas os arquivos resolvidos; publique após o rebase terminar."));
    }
    for file in &proposal.files {
        safe_relative(file, "Um arquivo resolvido")?;
        if !checkpoint.conflicts.contains(file) {
            return Err(error(
                "publication_staged_scope",
                "Inclua somente arquivos do conflito registrado pelo Jarvis.",
            ));
        }
    }
    Ok(checkpoint.branch)
}

pub(super) fn finish(directory: &Path, proposal: &RepositoryProposal) -> Result<(), AgentError> {
    validate(directory, proposal)?;
    if proposal.sync == SyncMode::RebaseAbort {
        git(directory, ["rebase", "--abort"])?;
        // The restored HEAD can still include an earlier, unpublished native
        // rebase. Keep its lease bound to that restored history.
        return complete(directory);
    }
    if !proposal.files.is_empty() {
        let mut check = vec![
            OsString::from("diff"),
            OsString::from("--check"),
            OsString::from("--"),
        ];
        check.extend(proposal.files.iter().map(|path| literal_pathspec(path)));
        git(directory, check)?;
        let mut add = vec![
            OsString::from("add"),
            OsString::from("--all"),
            OsString::from("--"),
        ];
        add.extend(proposal.files.iter().map(|path| literal_pathspec(path)));
        git(directory, add)?;
    }
    let output = run(
        directory,
        "git",
        [
            "-c",
            "core.editor=true",
            "-c",
            "commit.gpgsign=false",
            "rebase",
            "--continue",
        ],
    )?;
    if !output.status.success() {
        return Err(conflict(
            directory,
            &String::from_utf8_lossy(&output.stderr),
        )?);
    }
    complete(directory)
}
