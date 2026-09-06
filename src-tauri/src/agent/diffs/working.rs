use super::*;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use tokio::io::AsyncReadExt;

const MAX_FILE: u64 = 1024 * 1024;

fn failure() -> AgentError {
    AgentError::new("diff_unavailable", "Não foi possível conferir as alterações pendentes da sessão.")
}

// No shell, hooks, external diff drivers or optional index writes. Bound both
// process lifetime and output, including repositories with unusual filenames.
async fn git(root: &Path, args: &[&str]) -> Result<(bool, String), AgentError> {
    let mut child = tokio::process::Command::new("git")
        .arg("--no-optional-locks").arg("--literal-pathspecs").arg("-C").arg(root)
        .args(args).env("GIT_TERMINAL_PROMPT", "0")
        .stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true)
        .spawn().map_err(|_| failure())?;
    let mut stdout = child.stdout.take().ok_or_else(failure)?.take(MAX_FILE + 1);
    let mut bytes = vec![];
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        stdout.read_to_end(&mut bytes).await.map_err(|_| failure())?;
        if bytes.len() as u64 > MAX_FILE { return Err(failure()); }
        let status = child.wait().await.map_err(|_| failure())?;
        Ok((status.success(), String::from_utf8(bytes).map_err(|_| failure())?))
    }).await.map_err(|_| failure())?;
    result
}

pub(super) struct Repository { root: PathBuf, head: Option<String> }

impl Repository {
    pub async fn open(root: &Path) -> Result<Option<Self>, AgentError> {
        let (ok, path) = git(root, &["rev-parse", "--show-toplevel"]).await?;
        if !ok { return Ok(None); }
        let (has_head, head) = git(root, &["rev-parse", "--verify", "HEAD"]).await?;
        Ok(Some(Self { root: PathBuf::from(path.trim_end_matches('\n')), head: has_head.then(|| head.trim().to_owned()) }))
    }

    pub async fn contents(&self, root: &Path, path: &str) -> Result<Option<String>, AgentError> {
        let Some(head) = &self.head else { return Ok(None); };
        let absolute = root.join(path);
        let relative = absolute.strip_prefix(&self.root).map_err(|_| failure())?.to_str().ok_or_else(failure)?;
        let (found, listing) = git(&self.root, &["ls-tree", "-z", head, "--", relative]).await?;
        if !found { return Err(failure()); }
        if listing.is_empty() { return Ok(None); }
        if !listing.starts_with("100") { return Err(failure()); }
        let (ok, text) = git(&self.root, &["show", &format!("{head}:{relative}")]).await?;
        if !ok || text.contains('\0') { return Err(failure()); }
        Ok(Some(text))
    }
}

async fn current(root: &Path, path: &str) -> Result<Option<String>, AgentError> {
    let relative = Path::new(path);
    if relative.components().any(|part| !matches!(part, std::path::Component::Normal(_))) { return Err(failure()); }
    let mut absolute = root.to_path_buf();
    for part in relative.components() {
        absolute.push(part);
        match tokio::fs::symlink_metadata(&absolute).await {
            Ok(meta) if meta.file_type().is_symlink() => return Err(failure()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(failure()),
            _ => {}
        }
    }
    let file = tokio::fs::File::open(absolute).await.map_err(|_| failure())?;
    if !file.metadata().await.map_err(|_| failure())?.is_file() { return Err(failure()); }
    let mut bytes = vec![];
    file.take(MAX_FILE + 1).read_to_end(&mut bytes).await.map_err(|_| failure())?;
    if bytes.len() as u64 > MAX_FILE || bytes.contains(&0) { return Err(failure()); }
    String::from_utf8(bytes).map(Some).map_err(|_| failure())
}

fn diff<'a>(before: &'a str, after: &'a str) -> TextDiff<'a, 'a, 'a, str> {
    TextDiff::configure().timeout(Duration::from_millis(300)).diff_lines(before, after)
}

// Map session-owned lines to the live revisions. Intersecting by line (rather
// than by dirty filename) excludes pre-existing edits and partial commits.
fn project_lines(before: &str, after: &str, owned: &HashSet<usize>) -> HashSet<usize> {
    diff(before, after).iter_all_changes().filter_map(|change| {
        (change.tag() == ChangeTag::Equal && owned.contains(&change.old_index()?)).then_some(change.new_index()?)
    }).collect()
}

pub(super) fn pending(file: &FileRevision, head: Option<&str>, live: Option<&str>) -> FileRevision {
    let empty_created = file.before.is_none() && file.after.as_deref() == Some("") && head.is_none() && live == Some("");
    let empty_deleted = file.before.as_deref() == Some("") && file.after.is_none() && head == Some("") && live.is_none();
    let before = file.before.as_deref().unwrap_or("");
    let after = file.after.as_deref().unwrap_or("");
    let head = head.unwrap_or("");
    let live = live.unwrap_or("");
    let mut added = HashSet::new();
    let mut removed = HashSet::new();
    let surviving: HashSet<_> = diff(after, live).iter_all_changes()
        .filter(|change| change.tag() == ChangeTag::Equal).filter_map(|change| change.old_index()).collect();
    let session_diff = diff(before, after);
    for op in session_diff.ops() {
        // An overwritten replacement no longer belongs to this session. Pure
        // deletions remain attributable without requiring an inserted line.
        let survives = op.new_range().is_empty() || op.new_range().any(|index| surviving.contains(&index));
        for change in session_diff.iter_changes(op) {
            match change.tag() {
                ChangeTag::Insert => { if let Some(index) = change.new_index() { added.insert(index); } }
                ChangeTag::Delete if survives => { if let Some(index) = change.old_index() { removed.insert(index); } }
                _ => {}
            }
        }
    }
    let added = project_lines(after, live, &added);
    let removed = project_lines(before, head, &removed);
    let mut baseline = String::new();
    for change in diff(head, live).iter_all_changes() {
        let owned = match change.tag() {
            ChangeTag::Insert => change.new_index().is_some_and(|index| added.contains(&index)),
            ChangeTag::Delete => change.old_index().is_some_and(|index| removed.contains(&index)),
            ChangeTag::Equal => false,
        };
        if change.tag() == ChangeTag::Equal || (change.tag() == ChangeTag::Insert && !owned) || (change.tag() == ChangeTag::Delete && owned) {
            baseline.push_str(change.value());
        }
    }
    let mut result = FileRevision::new(file.path.clone(), Some(baseline), Some(live.to_owned()), "conversation");
    // Empty-file creation/deletion is still a change, despite zero changed lines.
    if empty_created { result.before = None; }
    if empty_deleted { result.after = None; }
    // Stable and JS-safe; invalidates an open diff after external edits/commits.
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    result.before.hash(&mut hash); result.after.hash(&mut hash);
    result.revision = hash.finish() & ((1_u64 << 53) - 1);
    result
}

pub(super) async fn files(session: &Session, only: Option<&str>) -> Result<Vec<FileRevision>, AgentError> {
    let revisions: Vec<_> = session.data.lock().map_err(|_| AgentError::internal())?.extras.files.values()
        .filter(|file| file.base != "unknown" && only.is_none_or(|path| path == file.path)).cloned().collect();
    if revisions.is_empty() { return Ok(vec![]); }
    let repository = Repository::open(&session.root).await?;
    let mut result = vec![];
    for revision in revisions {
        let live = current(&session.root, &revision.path).await?;
        let head = if let Some(repo) = &repository { repo.contents(&session.root, &revision.path).await? } else { revision.before.clone() };
        let filtered = pending(&revision, head.as_deref(), live.as_deref());
        if filtered.changed() { result.push(filtered); }
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
