use super::{diffs::FileRevision, tools, AgentError, Mode};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::{self, Permissions},
    io::Write,
    path::{Component, Path, PathBuf},
};
use tokio::sync::watch;

const MAX_PATCH_BYTES: usize = 4 * 1024 * 1024;
const MAX_FILE_BYTES: usize = 1024 * 1024;
const MAX_FILES: usize = 64;
const MAX_CHUNKS: usize = 512;

fn error(message: impl Into<String>) -> AgentError {
    AgentError::new("patch_error", &message.into())
}

pub(super) fn definition() -> Value {
    tools::definition(
        "apply_patch",
        "Apply one transactional multi-file patch in Build mode. Validate every hunk before writing. Format: *** Begin Patch, then *** Add File, *** Update File (optional *** Move to), or *** Delete File sections, ending with *** End Patch. Update lines use space for context, - for removal and + for addition.",
        json!({
            "patchText":{
                "type":"string",
                "minLength":1,
                "maxLength":MAX_PATCH_BYTES,
                "description":"Complete patch text between *** Begin Patch and *** End Patch."
            }
        }),
        &["patchText"],
    )
}

#[derive(Debug)]
enum Hunk {
    Add {
        path: String,
        content: String,
    },
    Delete {
        path: String,
    },
    Update {
        path: String,
        move_to: Option<String>,
        chunks: Vec<Chunk>,
    },
}

impl Hunk {
    fn paths(&self) -> impl Iterator<Item = &str> {
        let (source, destination) = match self {
            Self::Add { path, .. } | Self::Delete { path } => (path.as_str(), None),
            Self::Update { path, move_to, .. } => (path.as_str(), move_to.as_deref()),
        };
        std::iter::once(source).chain(destination)
    }
}

#[derive(Debug)]
struct Chunk {
    old_lines: Vec<String>,
    new_lines: Vec<String>,
    anchor: Option<String>,
    end_of_file: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChangeKind {
    Add,
    Update,
    Delete,
    Move,
}

struct PlannedChange {
    kind: ChangeKind,
    source: PathBuf,
    target: PathBuf,
    source_relative: String,
    target_relative: String,
    before: Option<String>,
    after: Option<String>,
    permissions: Option<Permissions>,
}

struct StagedChange {
    plan: PlannedChange,
    temp: Option<PathBuf>,
    backup: Option<PathBuf>,
    backup_moved: bool,
    target_written: bool,
}

pub(super) struct Outcome {
    pub(super) output: String,
    pub(super) revisions: Vec<FileRevision>,
    pub(super) changed_paths: Vec<String>,
    pub(super) diagnostic_paths: Vec<String>,
}

pub(super) fn target_paths(args: &Value) -> Result<Vec<String>, AgentError> {
    let patch = patch_text(args)?;
    let hunks = parse(patch)?;
    Ok(hunks
        .iter()
        .flat_map(Hunk::paths)
        .map(str::to_owned)
        .collect())
}

pub(super) async fn execute(
    root: &Path,
    args: &Value,
    mode: Mode,
    signal: watch::Receiver<bool>,
) -> Result<Outcome, AgentError> {
    if mode == Mode::Plan {
        return Err(error("O modo Plan não permite aplicar patches."));
    }
    let patch = patch_text(args)?.to_owned();
    let root = root.to_path_buf();
    tauri::async_runtime::spawn_blocking(move || apply(&root, &patch, &signal))
        .await
        .map_err(|_| AgentError::internal())?
}

fn patch_text(args: &Value) -> Result<&str, AgentError> {
    let patch = args["patchText"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| error("Informe o conteúdo completo do patch."))?;
    if patch.len() > MAX_PATCH_BYTES {
        return Err(error("O patch excede o limite de 4 MiB."));
    }
    Ok(patch)
}

fn parse(patch: &str) -> Result<Vec<Hunk>, AgentError> {
    let normalized = patch.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<_> = normalized.split('\n').collect();
    let first = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .ok_or_else(|| error("O patch está vazio."))?;
    let last = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .ok_or_else(|| error("O patch está vazio."))?;
    if lines[first].trim() != "*** Begin Patch" || lines[last].trim() != "*** End Patch" {
        return Err(error(
            "Use os marcadores *** Begin Patch e *** End Patch no início e no fim.",
        ));
    }
    let mut hunks = Vec::new();
    let mut chunks = 0;
    let mut index = first + 1;
    while index < last {
        if lines[index].trim().is_empty() {
            index += 1;
            continue;
        }
        if let Some(path) = lines[index].strip_prefix("*** Add File:") {
            let path = checked_header_path(path)?;
            index += 1;
            let mut content = Vec::new();
            while index < last && !lines[index].starts_with("*** ") {
                let line = lines[index]
                    .strip_prefix('+')
                    .ok_or_else(|| error("Linhas de um arquivo novo devem começar com '+'."))?;
                content.push(line);
                index += 1;
            }
            let mut content = content.join("\n");
            if !content.is_empty() {
                content.push('\n');
            }
            hunks.push(Hunk::Add { path, content });
        } else if let Some(path) = lines[index].strip_prefix("*** Delete File:") {
            hunks.push(Hunk::Delete {
                path: checked_header_path(path)?,
            });
            index += 1;
        } else if let Some(path) = lines[index].strip_prefix("*** Update File:") {
            let path = checked_header_path(path)?;
            index += 1;
            let move_to = if index < last {
                if let Some(destination) = lines[index].strip_prefix("*** Move to:") {
                    index += 1;
                    Some(checked_header_path(destination)?)
                } else {
                    None
                }
            } else {
                None
            };
            let mut updates = Vec::new();
            while index < last && !is_file_header(lines[index]) {
                if lines[index].trim().is_empty() {
                    index += 1;
                    continue;
                }
                let header = lines[index]
                    .strip_prefix("@@")
                    .ok_or_else(|| error("Cada trecho de atualização deve começar com '@@'."))?;
                let anchor = header.trim().trim_end_matches("@@").trim();
                let anchor = (!anchor.is_empty()).then(|| anchor.to_owned());
                index += 1;
                let mut old_lines = Vec::new();
                let mut new_lines = Vec::new();
                let mut changed = false;
                let mut end_of_file = false;
                while index < last
                    && !lines[index].starts_with("@@")
                    && !is_file_header(lines[index])
                {
                    if lines[index] == "*** End of File" {
                        end_of_file = true;
                        index += 1;
                        break;
                    }
                    let line = lines[index];
                    let Some(marker) = line.as_bytes().first().copied() else {
                        return Err(error(
                            "Linhas vazias em uma atualização devem ser escritas como uma linha de contexto ' ', adição '+' ou remoção '-'.",
                        ));
                    };
                    let content = line[1..].to_owned();
                    match marker {
                        b' ' => {
                            old_lines.push(content.clone());
                            new_lines.push(content);
                        }
                        b'-' => {
                            old_lines.push(content);
                            changed = true;
                        }
                        b'+' => {
                            new_lines.push(content);
                            changed = true;
                        }
                        _ => {
                            return Err(error(
                                "Use ' ', '-' ou '+' no início de cada linha do trecho.",
                            ))
                        }
                    }
                    index += 1;
                }
                if !changed {
                    return Err(error("Um trecho de atualização não contém alterações."));
                }
                chunks += 1;
                if chunks > MAX_CHUNKS {
                    return Err(error("O patch excede o limite de 512 trechos."));
                }
                updates.push(Chunk {
                    old_lines,
                    new_lines,
                    anchor,
                    end_of_file,
                });
            }
            if updates.is_empty() {
                return Err(error(
                    "Uma atualização precisa conter ao menos um trecho '@@'.",
                ));
            }
            hunks.push(Hunk::Update {
                path,
                move_to,
                chunks: updates,
            });
        } else {
            return Err(error(format!(
                "Seção de patch desconhecida: {}",
                lines[index]
            )));
        }
        if hunks.len() > MAX_FILES {
            return Err(error("O patch excede o limite de 64 operações de arquivo."));
        }
    }
    if hunks.is_empty() {
        return Err(error("O patch não contém alterações."));
    }
    Ok(hunks)
}

fn checked_header_path(value: &str) -> Result<String, AgentError> {
    let path = value.trim();
    if path.is_empty() || path.chars().any(char::is_control) {
        return Err(error("Um caminho do patch está vazio ou é inválido."));
    }
    Ok(path.to_owned())
}

fn is_file_header(line: &str) -> bool {
    line.starts_with("*** Add File:")
        || line.starts_with("*** Update File:")
        || line.starts_with("*** Delete File:")
}

fn apply(root: &Path, patch: &str, signal: &watch::Receiver<bool>) -> Result<Outcome, AgentError> {
    if *signal.borrow() {
        return Err(AgentError::cancelled());
    }
    let hunks = parse(patch)?;
    let plans = plan(root, hunks)?;
    if *signal.borrow() {
        return Err(AgentError::cancelled());
    }
    let mut created_directories = Vec::new();
    if let Err(cause) = create_parent_directories(root, &plans, &mut created_directories) {
        remove_empty_directories(&created_directories);
        return Err(cause);
    }
    let mut staged = match stage(plans) {
        Ok(staged) => staged,
        Err(cause) => {
            remove_empty_directories(&created_directories);
            return Err(cause);
        }
    };
    if *signal.borrow() {
        cleanup_staged(&staged);
        remove_empty_directories(&created_directories);
        return Err(AgentError::cancelled());
    }
    if let Err(cause) = commit(&mut staged) {
        let rollback = rollback(&mut staged);
        cleanup_staged(&staged);
        remove_empty_directories(&created_directories);
        return match rollback {
            Ok(()) => Err(cause),
            Err(rollback) => Err(error(format!(
                "{} A restauração também falhou: {rollback}",
                cause.message
            ))),
        };
    }
    let mut cleanup_warnings = Vec::new();
    for change in &staged {
        if let Some(backup) = &change.backup {
            if backup.exists() && fs::remove_file(backup).is_err() {
                cleanup_warnings.push(format!(
                    "backup temporário não removido em {}",
                    backup.display()
                ));
            }
        }
    }
    let outcome = outcome(&staged, cleanup_warnings);
    sync_directories(&staged);
    Ok(outcome)
}

fn plan(root: &Path, hunks: Vec<Hunk>) -> Result<Vec<PlannedChange>, AgentError> {
    if fs::canonicalize(root).ok().as_deref() != Some(root) || !root.is_dir() {
        return Err(error("A pasta original do projeto não está disponível."));
    }
    let mut plans = Vec::with_capacity(hunks.len());
    for hunk in hunks {
        let planned = match hunk {
            Hunk::Add { path, content } => {
                ensure_size(&content)?;
                let target = safe_path(root, &path)?;
                require_missing(&target, "adicionar")?;
                let relative = relative_path(root, &target)?;
                PlannedChange {
                    kind: ChangeKind::Add,
                    source: target.clone(),
                    target,
                    source_relative: relative.clone(),
                    target_relative: relative,
                    before: None,
                    after: Some(content),
                    permissions: None,
                }
            }
            Hunk::Delete { path } => {
                let source = safe_path(root, &path)?;
                let (before, permissions) = read_existing(&source)?;
                let relative = relative_path(root, &source)?;
                PlannedChange {
                    kind: ChangeKind::Delete,
                    source: source.clone(),
                    target: source,
                    source_relative: relative.clone(),
                    target_relative: relative,
                    before: Some(before),
                    after: None,
                    permissions: Some(permissions),
                }
            }
            Hunk::Update {
                path,
                move_to,
                chunks,
            } => {
                let source = safe_path(root, &path)?;
                let (before, permissions) = read_existing(&source)?;
                let after = apply_chunks(&path, &before, &chunks)?;
                ensure_size(&after)?;
                if after == before && move_to.is_none() {
                    return Err(error(format!(
                        "A atualização de '{path}' não altera o arquivo."
                    )));
                }
                let (kind, target, target_relative) = if let Some(destination) = move_to {
                    let target = safe_path(root, &destination)?;
                    if target == source {
                        return Err(error("A origem e o destino da movimentação são iguais."));
                    }
                    require_missing(&target, "mover")?;
                    let relative = relative_path(root, &target)?;
                    (ChangeKind::Move, target, relative)
                } else {
                    (
                        ChangeKind::Update,
                        source.clone(),
                        relative_path(root, &source)?,
                    )
                };
                let source_relative = relative_path(root, &source)?;
                PlannedChange {
                    kind,
                    source,
                    target,
                    source_relative,
                    target_relative,
                    before: Some(before),
                    after: Some(after),
                    permissions: Some(permissions),
                }
            }
        };
        plans.push(planned);
    }
    validate_collisions(&plans)?;
    Ok(plans)
}

fn safe_path(root: &Path, value: &str) -> Result<PathBuf, AgentError> {
    let supplied = Path::new(value);
    let relative = if supplied.is_absolute() {
        supplied
            .strip_prefix(root)
            .map_err(|_| error("Todos os caminhos do patch devem permanecer dentro do projeto."))?
    } else {
        supplied
    };
    let mut path = root.to_path_buf();
    let mut count = 0;
    for component in relative.components() {
        match component {
            Component::CurDir => continue,
            Component::Normal(part) => {
                path.push(part);
                count += 1;
            }
            _ => {
                return Err(error(
                    "Caminhos do patch não aceitam '..', prefixos ou escapes do projeto.",
                ))
            }
        }
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(error("Links simbólicos não são aceitos pelo patch."))
            }
            Ok(_) => {}
            Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(error("Não foi possível validar um caminho do patch.")),
        }
    }
    if count == 0 {
        return Err(error(
            "O patch precisa apontar para um arquivo dentro do projeto.",
        ));
    }
    Ok(path)
}

fn relative_path(root: &Path, path: &Path) -> Result<String, AgentError> {
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .map_err(|_| error("Caminho do patch fora do projeto."))
}

fn read_existing(path: &Path) -> Result<(String, Permissions), AgentError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| error(format!("Arquivo não encontrado: {}", path.display())))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(error(format!(
            "O destino precisa ser um arquivo comum: {}",
            path.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() > 1 {
            return Err(error(
                "Arquivos com hard links não podem ser alterados pelo patch.",
            ));
        }
    }
    let text = tools::read_text(path)?;
    Ok((text, metadata.permissions()))
}

fn require_missing(path: &Path, action: &str) -> Result<(), AgentError> {
    match fs::symlink_metadata(path) {
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(error(format!(
            "Não é possível {action}: o destino já existe em {}.",
            path.display()
        ))),
    }
}

fn ensure_size(content: &str) -> Result<(), AgentError> {
    if content.len() > MAX_FILE_BYTES {
        Err(error("Cada arquivo do patch pode ter no máximo 1 MiB."))
    } else {
        Ok(())
    }
}

fn validate_collisions(plans: &[PlannedChange]) -> Result<(), AgentError> {
    let mut owners: HashMap<&Path, usize> = HashMap::new();
    for (index, plan) in plans.iter().enumerate() {
        let paths: Vec<&Path> = if plan.source == plan.target {
            vec![&plan.source]
        } else {
            vec![&plan.source, &plan.target]
        };
        for path in paths {
            if owners.insert(path, index).is_some() {
                return Err(error(format!(
                    "O patch usa o mesmo caminho em mais de uma operação: {}",
                    path.display()
                )));
            }
        }
    }
    let paths: Vec<_> = owners.keys().copied().collect();
    for (index, left) in paths.iter().enumerate() {
        for right in paths.iter().skip(index + 1) {
            if left.starts_with(right) || right.starts_with(left) {
                return Err(error(
                    "O patch contém caminhos de arquivo que se sobrepõem como pasta e conteúdo.",
                ));
            }
        }
    }
    Ok(())
}

struct TextLayout {
    bom: bool,
    ending: &'static str,
    trailing: bool,
    lines: Vec<String>,
}

impl TextLayout {
    fn read(value: &str) -> Self {
        let (bom, value) = value
            .strip_prefix('\u{feff}')
            .map_or((false, value), |value| (true, value));
        let ending = if value.contains("\r\n") {
            "\r\n"
        } else if value.contains('\r') && !value.contains('\n') {
            "\r"
        } else {
            "\n"
        };
        let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
        let trailing = normalized.ends_with('\n');
        let mut lines: Vec<_> = normalized.split('\n').map(str::to_owned).collect();
        if trailing {
            lines.pop();
        }
        Self {
            bom,
            ending,
            trailing,
            lines,
        }
    }

    fn render(self) -> String {
        let mut result = self.lines.join(self.ending);
        if self.trailing {
            result.push_str(self.ending);
        }
        if self.bom {
            result.insert(0, '\u{feff}');
        }
        result
    }
}

fn apply_chunks(path: &str, original: &str, chunks: &[Chunk]) -> Result<String, AgentError> {
    let mut layout = TextLayout::read(original);
    let mut cursor = 0;
    for chunk in chunks {
        let mut search_start = cursor;
        if let Some(anchor) = &chunk.anchor {
            search_start = layout
                .lines
                .iter()
                .enumerate()
                .skip(cursor)
                .find(|(_, line)| line.contains(anchor))
                .map(|(index, _)| index)
                .ok_or_else(|| {
                    error(format!(
                        "Contexto '@@ {anchor}' não encontrado em '{path}'."
                    ))
                })?;
        }
        let at = if chunk.old_lines.is_empty() {
            if chunk.end_of_file {
                layout.lines.len()
            } else if chunk.anchor.is_some() {
                search_start + 1
            } else {
                return Err(error(format!(
                    "Uma inserção em '{path}' precisa de contexto ou do marcador *** End of File."
                )));
            }
        } else {
            find_unique_sequence(
                &layout.lines,
                &chunk.old_lines,
                search_start,
                chunk.end_of_file,
            )
            .ok_or_else(|| {
                error(format!(
                    "O trecho original não foi encontrado de forma única em '{path}'. Leia o arquivo novamente."
                ))
            })?
        };
        let end = at + chunk.old_lines.len();
        layout.lines.splice(at..end, chunk.new_lines.clone());
        cursor = at + chunk.new_lines.len();
    }
    Ok(layout.render())
}

fn find_unique_sequence(
    lines: &[String],
    pattern: &[String],
    start: usize,
    end_of_file: bool,
) -> Option<usize> {
    if pattern.is_empty() || pattern.len() > lines.len() {
        return None;
    }
    if end_of_file {
        let at = lines.len() - pattern.len();
        return (at >= start && lines[at..] == pattern[..]).then_some(at);
    }
    let matches: Vec<_> = (start..=lines.len() - pattern.len())
        .filter(|at| lines[*at..*at + pattern.len()] == pattern[..])
        .take(2)
        .collect();
    (matches.len() == 1).then_some(matches[0])
}

fn create_parent_directories(
    root: &Path,
    plans: &[PlannedChange],
    created: &mut Vec<PathBuf>,
) -> Result<(), AgentError> {
    let mut parents: Vec<_> = plans
        .iter()
        .filter_map(|plan| plan.after.as_ref().and(plan.target.parent()))
        .collect();
    parents.sort();
    parents.dedup();
    for parent in parents {
        let relative = parent
            .strip_prefix(root)
            .map_err(|_| error("Pasta de destino fora do projeto."))?;
        let mut current = root.to_path_buf();
        for component in relative.components() {
            let Component::Normal(part) = component else {
                return Err(error("Pasta de destino inválida."));
            };
            current.push(part);
            match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
                Ok(_) => return Err(error("Um componente do destino não é uma pasta comum.")),
                Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => {
                    fs::create_dir(&current)
                        .map_err(|_| error("Não foi possível criar uma pasta do patch."))?;
                    created.push(current.clone());
                }
                Err(_) => return Err(error("Não foi possível verificar uma pasta do patch.")),
            }
        }
    }
    Ok(())
}

fn stage(plans: Vec<PlannedChange>) -> Result<Vec<StagedChange>, AgentError> {
    let mut staged = Vec::with_capacity(plans.len());
    for plan in plans {
        match stage_one(plan) {
            Ok(change) => staged.push(change),
            Err(cause) => {
                cleanup_staged(&staged);
                return Err(cause);
            }
        }
    }
    Ok(staged)
}

fn stage_one(plan: PlannedChange) -> Result<StagedChange, AgentError> {
    let temp = if let Some(after) = &plan.after {
        let parent = plan
            .target
            .parent()
            .ok_or_else(|| error("Destino do patch sem pasta."))?;
        let mut file = tempfile::Builder::new()
            .prefix(".jarvis-patch-")
            .tempfile_in(parent)
            .map_err(|_| error("Não foi possível preparar um arquivo temporário."))?;
        if let Some(permissions) = &plan.permissions {
            file.as_file()
                .set_permissions(permissions.clone())
                .map_err(|_| error("Não foi possível preservar as permissões do arquivo."))?;
        }
        file.write_all(after.as_bytes())
            .and_then(|()| file.as_file().sync_all())
            .map_err(|_| error("Não foi possível preparar o conteúdo do patch."))?;
        let (_, path) = file
            .keep()
            .map_err(|_| error("Não foi possível manter o arquivo temporário do patch."))?;
        Some(path)
    } else {
        None
    };
    let backup = match plan.before.is_some() {
        true => match backup_path(&plan.source) {
            Ok(path) => Some(path),
            Err(cause) => {
                if let Some(temp) = &temp {
                    let _ = fs::remove_file(temp);
                }
                return Err(cause);
            }
        },
        false => None,
    };
    Ok(StagedChange {
        plan,
        temp,
        backup,
        backup_moved: false,
        target_written: false,
    })
}

fn backup_path(source: &Path) -> Result<PathBuf, AgentError> {
    let parent = source
        .parent()
        .ok_or_else(|| error("Arquivo do patch sem pasta."))?;
    for _ in 0..8 {
        let id = crate::library::new_id().map_err(|_| AgentError::internal())?;
        let candidate = parent.join(format!(".jarvis-patch-backup-{id}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(error("Não foi possível reservar um backup para o patch."))
}

fn commit(staged: &mut [StagedChange]) -> Result<(), AgentError> {
    for change in staged {
        match change.plan.kind {
            ChangeKind::Add => {
                require_missing(&change.plan.target, "adicionar")?;
                move_temp(change)?;
            }
            ChangeKind::Update | ChangeKind::Move => {
                move_to_backup(change)?;
                require_missing(&change.plan.target, "gravar").and_then(|()| move_temp(change))?;
            }
            ChangeKind::Delete => move_to_backup(change)?,
        }
    }
    Ok(())
}

fn move_to_backup(change: &mut StagedChange) -> Result<(), AgentError> {
    let backup = change.backup.as_ref().ok_or_else(AgentError::internal)?;
    fs::rename(&change.plan.source, backup).map_err(|_| {
        error(format!(
            "Não foi possível preparar a substituição de {}.",
            change.plan.source.display()
        ))
    })?;
    change.backup_moved = true;
    Ok(())
}

fn move_temp(change: &mut StagedChange) -> Result<(), AgentError> {
    let temp = change.temp.as_ref().ok_or_else(AgentError::internal)?;
    fs::rename(temp, &change.plan.target).map_err(|_| {
        error(format!(
            "Não foi possível gravar {}.",
            change.plan.target.display()
        ))
    })?;
    change.temp = None;
    change.target_written = true;
    Ok(())
}

fn rollback(staged: &mut [StagedChange]) -> Result<(), String> {
    let mut failures = Vec::new();
    for change in staged.iter_mut().rev() {
        if change.target_written && fs::remove_file(&change.plan.target).is_err() {
            failures.push(format!("remover {}", change.plan.target.display()));
        }
        if change.backup_moved {
            if let Some(backup) = &change.backup {
                if fs::rename(backup, &change.plan.source).is_err() {
                    failures.push(format!("restaurar {}", change.plan.source.display()));
                }
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join(", "))
    }
}

fn cleanup_staged(staged: &[StagedChange]) {
    for change in staged {
        if let Some(temp) = &change.temp {
            let _ = fs::remove_file(temp);
        }
    }
}

fn remove_empty_directories(created: &[PathBuf]) {
    for directory in created.iter().rev() {
        let _ = fs::remove_dir(directory);
    }
}

#[cfg(unix)]
fn sync_directories(staged: &[StagedChange]) {
    let mut directories: Vec<_> = staged
        .iter()
        .flat_map(|change| [change.plan.source.parent(), change.plan.target.parent()])
        .flatten()
        .collect();
    directories.sort();
    directories.dedup();
    for directory in directories {
        let _ = fs::File::open(directory).and_then(|file| file.sync_all());
    }
}

#[cfg(not(unix))]
fn sync_directories(_: &[StagedChange]) {}

fn outcome(staged: &[StagedChange], warnings: Vec<String>) -> Outcome {
    let mut summary = Vec::with_capacity(staged.len());
    let mut revisions = Vec::new();
    let mut changed_paths = Vec::new();
    let mut diagnostic_paths = Vec::new();
    for change in staged {
        match change.plan.kind {
            ChangeKind::Add => {
                summary.push(format!("A {}", change.plan.target_relative));
                revisions.push(FileRevision::new(
                    change.plan.target_relative.clone(),
                    None,
                    change.plan.after.clone(),
                    "conversation",
                ));
                changed_paths.push(change.plan.target_relative.clone());
                diagnostic_paths.push(change.plan.target_relative.clone());
            }
            ChangeKind::Update => {
                summary.push(format!("M {}", change.plan.source_relative));
                revisions.push(FileRevision::new(
                    change.plan.source_relative.clone(),
                    change.plan.before.clone(),
                    change.plan.after.clone(),
                    "conversation",
                ));
                changed_paths.push(change.plan.source_relative.clone());
                diagnostic_paths.push(change.plan.source_relative.clone());
            }
            ChangeKind::Delete => {
                summary.push(format!("D {}", change.plan.source_relative));
                revisions.push(FileRevision::new(
                    change.plan.source_relative.clone(),
                    change.plan.before.clone(),
                    None,
                    "conversation",
                ));
                changed_paths.push(change.plan.source_relative.clone());
            }
            ChangeKind::Move => {
                summary.push(format!(
                    "R {} -> {}",
                    change.plan.source_relative, change.plan.target_relative
                ));
                revisions.push(FileRevision::new(
                    change.plan.source_relative.clone(),
                    change.plan.before.clone(),
                    None,
                    "conversation",
                ));
                revisions.push(FileRevision::new(
                    change.plan.target_relative.clone(),
                    None,
                    change.plan.after.clone(),
                    "conversation",
                ));
                changed_paths.extend([
                    change.plan.source_relative.clone(),
                    change.plan.target_relative.clone(),
                ]);
                diagnostic_paths.push(change.plan.target_relative.clone());
            }
        }
    }
    let mut output = format!(
        "Patch aplicado em {} operação(ões):\n{}",
        staged.len(),
        summary.join("\n")
    );
    if !warnings.is_empty() {
        output.push_str(&format!("\nAvisos de limpeza: {}", warnings.join("; ")));
    }
    Outcome {
        output,
        revisions,
        changed_paths,
        diagnostic_paths,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tests::Fixture;

    fn args(patch: &str) -> Value {
        json!({"patchText":patch})
    }

    #[tokio::test]
    async fn mixed_patch_is_atomic_and_preserves_existing_layout() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.root.join("src")).unwrap();
        let update = fixture.root.join("src/update.txt");
        fs::write(&update, "first\r\nold\r\nlast\r\n").unwrap();
        fs::write(fixture.root.join("move.txt"), "before\n").unwrap();
        fs::write(fixture.root.join("delete.txt"), "gone\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&update, fs::Permissions::from_mode(0o640)).unwrap();
        }
        let patch = r#"*** Begin Patch
*** Update File: src/update.txt
@@
 first
-old
+new
 last
*** Add File: src/added.txt
+created
*** Update File: move.txt
*** Move to: nested/moved.txt
@@
-before
+after
*** Delete File: delete.txt
*** End Patch"#;
        let (_send, signal) = watch::channel(false);
        let result = execute(&fixture.root, &args(patch), Mode::Build, signal)
            .await
            .unwrap();
        assert_eq!(
            fs::read_to_string(&update).unwrap(),
            "first\r\nnew\r\nlast\r\n"
        );
        assert_eq!(
            fs::read_to_string(fixture.root.join("src/added.txt")).unwrap(),
            "created\n"
        );
        assert_eq!(
            fs::read_to_string(fixture.root.join("nested/moved.txt")).unwrap(),
            "after\n"
        );
        assert!(!fixture.root.join("move.txt").exists());
        assert!(!fixture.root.join("delete.txt").exists());
        assert_eq!(result.revisions.len(), 5);
        assert_eq!(result.changed_paths.len(), 5);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(update).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
    }

    #[tokio::test]
    async fn invalid_late_hunk_changes_no_files() {
        let fixture = Fixture::new();
        fs::write(fixture.root.join("first.txt"), "original\n").unwrap();
        fs::write(fixture.root.join("second.txt"), "actual\n").unwrap();
        let patch = r#"*** Begin Patch
*** Update File: first.txt
@@
-original
+changed
*** Update File: second.txt
@@
-stale
+wrong
*** End Patch"#;
        let (_send, signal) = watch::channel(false);
        assert!(execute(&fixture.root, &args(patch), Mode::Build, signal)
            .await
            .is_err());
        assert_eq!(
            fs::read_to_string(fixture.root.join("first.txt")).unwrap(),
            "original\n"
        );
        assert_eq!(
            fs::read_to_string(fixture.root.join("second.txt")).unwrap(),
            "actual\n"
        );
    }

    #[tokio::test]
    async fn rejects_escape_symlink_collisions_and_plan_mode() {
        let fixture = Fixture::new();
        let (_send, signal) = watch::channel(false);
        let escape = args("*** Begin Patch\n*** Add File: ../outside.txt\n+x\n*** End Patch");
        assert!(execute(&fixture.root, &escape, Mode::Build, signal.clone())
            .await
            .is_err());
        let collision = args(
            "*** Begin Patch\n*** Add File: same.txt\n+one\n*** Add File: same.txt\n+two\n*** End Patch",
        );
        assert!(
            execute(&fixture.root, &collision, Mode::Build, signal.clone())
                .await
                .is_err()
        );
        assert!(!fixture.root.join("same.txt").exists());
        let plan = args("*** Begin Patch\n*** Add File: plan.txt\n+x\n*** End Patch");
        assert!(execute(&fixture.root, &plan, Mode::Plan, signal.clone())
            .await
            .is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&fixture.root, fixture.root.join("link")).unwrap();
            let symlink = args("*** Begin Patch\n*** Add File: link/file.txt\n+x\n*** End Patch");
            assert!(execute(&fixture.root, &symlink, Mode::Build, signal)
                .await
                .is_err());
        }
    }

    #[test]
    fn ambiguous_update_requires_fresh_context() {
        let chunks = vec![Chunk {
            old_lines: vec!["same".into()],
            new_lines: vec!["different".into()],
            anchor: None,
            end_of_file: false,
        }];
        assert!(apply_chunks("file.txt", "same\nsame\n", &chunks).is_err());
    }

    #[test]
    fn extracts_all_paths_for_instruction_preflight() {
        let paths = target_paths(&args(
            "*** Begin Patch\n*** Update File: source.txt\n*** Move to: folder/target.txt\n@@\n-old\n+new\n*** End Patch",
        ))
        .unwrap();
        assert_eq!(paths, ["source.txt", "folder/target.txt"]);
    }
}
