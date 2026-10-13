//! Native absence evidence never relies on the engine's lossy status summary.
use super::*;
use std::{
    io::{ErrorKind, Read},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    path::Component,
};

const MAX_ROOTS: usize = 256;
const MAX_FILES: usize = 1024;
const MAX_JOURNAL: u64 = 2 * 1024 * 1024;
const MAX_SCAN: u64 = 32 * 1024 * 1024;
const IGNORED: &[&str] = &[
    "node_modules",
    "dist",
    "build",
    "coverage",
    "vendor",
    "vendors",
];

pub(super) struct Request {
    pub(super) args: Vec<String>,
    pub(super) inspection: Value,
}

pub(super) fn request(
    home: &Path,
    project: &Path,
    conversation: &str,
    args: &[String],
    current_root: Option<&Path>,
    active_roots: &[PathBuf],
) -> Result<Request, CoreError> {
    let project = project.canonicalize()?;
    let target = requested_target(&project, args)?;
    let mut normalized = Vec::new();
    let mut values = args.iter();
    while let Some(arg) = values.next() {
        if arg == "--target" || arg == "-t" {
            let path = values.next().ok_or_else(invalid)?;
            normalized.extend(["--target".into(), path.clone()]);
        } else if let Some(path) = arg.strip_prefix("--target=") {
            normalized.extend(["--target".into(), path.into()]);
        } else {
            normalized.push(arg.clone());
        }
    }
    let mut args = normalized;
    let mut roots = vec![project.clone()];
    let mut inspection = json!({
        "confirmedInactive": false,
        "projectRoot": project,
        "conversationId": conversation,
        "target": target,
        "appRoots": [],
    });
    let inspected = (|| {
        if target.is_none() {
            if let Some(owned) = current_root {
                add_root(&project, &mut roots, owned)?;
                args.extend(["--target".into(), owned.to_string_lossy().into_owned()]);
            }
        }
        let home = home.canonicalize()?;
        let owner_path = ownership_path(&home, &project);
        checked_path(&home, &owner_path)?;
        let owner = read_record(&owner_path)?
            .map(serde_json::from_value::<Ownership>)
            .transpose()
            .map_err(|_| invalid())?;
        if let Some(owner) = &owner {
            if owner.project_root != project
                || owner.root.canonicalize()? != owner.root
                || owner.identity.0 != owner.root.join(".impeccable/live/server.json")
                || owner.pid == 0
                || owner.identity.1 == 0
                || owner.identity.2.is_empty()
                || owner.identity.2.len() > 128
            {
                return Err(invalid());
            }
            add_root(&project, &mut roots, &owner.root)?;
            if target.is_none() && current_root.is_none() && owner.conversation_id == conversation {
                args.extend(["--target".into(), owner.root.to_string_lossy().into_owned()]);
            }
        }
        if let Some(target) = &target {
            let mut directory = if target.is_dir() {
                target.as_path()
            } else {
                target.parent().ok_or_else(invalid)?
            };
            loop {
                add_root(&project, &mut roots, directory)?;
                if directory == project {
                    break;
                }
                directory = directory.parent().ok_or_else(invalid)?;
            }
            // Explicit targets never inherit another app from the repo pointer.
            roots.retain(|root| target.starts_with(root));
        } else {
            if let Some(pointer) =
                read_checked(&project, &project.join(".impeccable/live/app-root.json"))?
            {
                let entries = if let Some(entries) = pointer.get("appRoots") {
                    entries
                        .as_array()
                        .ok_or_else(invalid)?
                        .iter()
                        .map(|entry| entry.get("appRoot"))
                        .collect::<Vec<_>>()
                } else {
                    vec![pointer.get("appRoot")]
                };
                if entries.len() > 16 {
                    return Err(invalid());
                }
                for entry in entries {
                    let root = entry.and_then(Value::as_str).ok_or_else(invalid)?;
                    add_root(&project, &mut roots, &project.join(root))?;
                }
            }
            let mut scanned_directories = 0;
            discover(&project, &project, 2, &mut roots, &mut scanned_directories)?;
        }
        for root in active_roots {
            if target
                .as_ref()
                .is_none_or(|target| target.starts_with(root))
            {
                add_root(&project, &mut roots, root)?;
                return Err(error(
                    "Uma sessão Live deste projeto ainda está ativa no Jarvis.",
                ));
            }
        }
        check_owner_lock(&home, &owner_path.with_extension("lock"))?;
        if let Some(owner) = &owner {
            if roots.contains(&owner.root) {
                if let Some(info) = read_checked(&project, &owner.identity.0)? {
                    if info["pid"].as_u64() != Some(u64::from(owner.pid))
                        || info["port"].as_u64() != Some(u64::from(owner.identity.1))
                        || info["token"].as_str() != Some(owner.identity.2.as_str())
                    {
                        return Err(error(
                            "A identidade do helper Live mudou; a inspeção é inconclusiva.",
                        ));
                    }
                }
                helper_absent(owner.pid, owner.identity.1)?;
            }
        }
        let mut scanned = 0;
        for root in &roots {
            for relative in [".impeccable/live/server.json", ".impeccable-live.json"] {
                if let Some(info) = read_checked(&project, &root.join(relative))? {
                    let pid = info["pid"]
                        .as_u64()
                        .and_then(|pid| u32::try_from(pid).ok())
                        .filter(|pid| *pid > 0)
                        .ok_or_else(invalid)?;
                    let port = info["port"]
                        .as_u64()
                        .and_then(|port| u16::try_from(port).ok())
                        .filter(|port| *port > 0)
                        .ok_or_else(invalid)?;
                    info["token"]
                        .as_str()
                        .filter(|token| !token.is_empty() && token.len() <= 128)
                        .ok_or_else(invalid)?;
                    helper_absent(pid, port)?;
                }
            }
            for relative in [".impeccable/live/sessions", ".impeccable-live/sessions"] {
                sessions(&project, &root.join(relative), &mut scanned)?;
            }
            if tasks::read(root, &home)?.iter().any(|task| {
                !matches!(
                    task.status,
                    tasks::Status::Completed | tasks::Status::Cancelled
                )
            }) {
                return Err(error(
                    "O Jarvis ainda tem um pedido Live pendente neste aplicativo.",
                ));
            }
        }
        Ok(())
    })();
    inspection["appRoots"] = json!(roots);
    match inspected {
        Ok(()) => inspection["confirmedInactive"] = json!(true),
        Err(cause) => inspection["error"] = json!(cause.message),
    }
    Ok(Request { args, inspection })
}

fn invalid() -> CoreError {
    error("O estado nativo Live é inválido ou ambíguo; a inspeção não confirmou ausência.")
}

fn checked_path(root: &Path, path: &Path) -> Result<Option<fs::Metadata>, CoreError> {
    let mut current = root.to_path_buf();
    let mut found = None;
    for component in path.strip_prefix(root).map_err(|_| invalid())?.components() {
        let Component::Normal(part) = component else {
            return Err(invalid());
        };
        current.push(part);
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(cause) if cause.kind() == ErrorKind::NotFound => return Ok(None),
            Err(cause) => return Err(cause.into()),
        };
        if metadata.is_symlink() || !current.canonicalize()?.starts_with(root) {
            return Err(invalid());
        }
        found = Some(metadata);
    }
    Ok(found)
}

fn read_checked(root: &Path, path: &Path) -> Result<Option<Value>, CoreError> {
    checked_path(root, path)?;
    read_record(path)
}

fn add_root(project: &Path, roots: &mut Vec<PathBuf>, root: &Path) -> Result<(), CoreError> {
    let root = root.canonicalize()?;
    if !root.is_dir() || !root.starts_with(project) {
        return Err(invalid());
    }
    if !roots.contains(&root) {
        if roots.len() >= MAX_ROOTS {
            return Err(invalid());
        }
        roots.push(root);
    }
    Ok(())
}

fn discover(
    project: &Path,
    directory: &Path,
    depth: usize,
    roots: &mut Vec<PathBuf>,
    scanned: &mut usize,
) -> Result<(), CoreError> {
    for (index, entry) in fs::read_dir(directory)?.enumerate() {
        if index >= MAX_FILES {
            return Err(invalid());
        }
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().ok_or_else(invalid)?;
        if name.starts_with('.') || IGNORED.contains(&name) || !entry.file_type()?.is_dir() {
            continue;
        }
        *scanned += 1;
        if *scanned > MAX_ROOTS {
            return Err(invalid());
        }
        let path = entry.path();
        if !path.canonicalize()?.starts_with(project) {
            return Err(invalid());
        }
        for relative in [
            ".impeccable/live",
            ".impeccable-live.json",
            ".impeccable-live/sessions",
        ] {
            if checked_path(project, &path.join(relative))?.is_some() {
                add_root(project, roots, &path)?;
            }
        }
        if depth > 1 {
            discover(project, &path, depth - 1, roots, scanned)?;
        }
    }
    Ok(())
}

fn check_owner_lock(home: &Path, path: &Path) -> Result<(), CoreError> {
    let Some(metadata) = checked_path(home, path)? else {
        return Ok(());
    };
    if !metadata.is_file() {
        return Err(invalid());
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    fs2::FileExt::try_lock_shared(&file)
        .map_err(|_| error("Outro Jarvis ainda está gerenciando o Live deste projeto."))?;
    Ok(())
}

fn helper_absent(pid: u32, port: u16) -> Result<(), CoreError> {
    if !process_absent(pid)? {
        return Err(error("O processo do helper Live ainda está ativo; uma falha de resposta não confirma ausência."));
    }
    match TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        Duration::from_millis(200),
    ) {
        Err(cause) if cause.kind() == ErrorKind::ConnectionRefused => Ok(()),
        Ok(_) => Err(error(
            "A porta registrada do helper Live ainda está ocupada; a inspeção é inconclusiva.",
        )),
        Err(_) => Err(error(
            "Não foi possível confirmar o encerramento do helper Live.",
        )),
    }
}

#[cfg(unix)]
fn process_absent(pid: u32) -> Result<bool, CoreError> {
    let pid = i32::try_from(pid)
        .ok()
        .filter(|pid| *pid > 0)
        .ok_or_else(invalid)?;
    // SAFETY: signal zero only checks the positive recorded PID; it never sends a signal.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return Ok(false);
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => Ok(true),
        Some(libc::EPERM) => Ok(false),
        _ => Err(error(
            "Não foi possível verificar o processo do helper Live.",
        )),
    }
}

#[cfg(windows)]
fn process_absent(pid: u32) -> Result<bool, CoreError> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, STILL_ACTIVE},
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    if pid == 0 {
        return Err(invalid());
    }
    // SAFETY: the positive PID is queried without mutation, and every opened handle is closed.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return if GetLastError() == ERROR_INVALID_PARAMETER {
                Ok(true)
            } else {
                Err(error(
                    "Não foi possível verificar o processo do helper Live.",
                ))
            };
        }
        let mut code = 0;
        let found = GetExitCodeProcess(handle, &mut code);
        CloseHandle(handle);
        if found == 0 {
            return Err(error(
                "Não foi possível verificar o processo do helper Live.",
            ));
        }
        Ok(code != STILL_ACTIVE as u32)
    }
}

#[cfg(not(any(unix, windows)))]
fn process_absent(_pid: u32) -> Result<bool, CoreError> {
    Err(error(
        "A verificação nativa do processo Live não está disponível nesta plataforma.",
    ))
}

fn sessions(project: &Path, directory: &Path, scanned: &mut u64) -> Result<(), CoreError> {
    let Some(metadata) = checked_path(project, directory)? else {
        return Ok(());
    };
    if !metadata.is_dir() {
        return Err(invalid());
    }
    for (index, entry) in fs::read_dir(directory)?.enumerate() {
        if index >= MAX_FILES {
            return Err(invalid());
        }
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().ok_or_else(invalid)?;
        if let Some(id) = name.strip_suffix(".jsonl") {
            let (sequence, phase, size) = terminal_journal(project, &entry.path(), id, scanned)?;
            if let Some(snapshot) =
                read_checked(project, &directory.join(format!("{id}.snapshot.json")))?
            {
                if snapshot["id"] != id
                    || snapshot["phase"] != phase
                    || snapshot["__journalBytes"].as_u64() != Some(size)
                    || snapshot["__nextSeq"].as_u64() != sequence.checked_add(1)
                {
                    return Err(invalid());
                }
            }
        } else if let Some(id) = name.strip_suffix(".snapshot.json") {
            if checked_path(project, &directory.join(format!("{id}.jsonl")))?.is_none() {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

fn terminal_journal(
    project: &Path,
    path: &Path,
    id: &str,
    scanned: &mut u64,
) -> Result<(u64, &'static str, u64), CoreError> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(invalid());
    }
    let metadata = checked_path(project, path)?.ok_or_else(invalid)?;
    if !metadata.is_file() || metadata.len() > MAX_JOURNAL {
        return Err(invalid());
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_JOURNAL + 1).read_to_end(&mut bytes)?;
    *scanned += bytes.len() as u64;
    if bytes.len() as u64 != metadata.len() || *scanned > MAX_SCAN {
        return Err(invalid());
    }
    let content = std::str::from_utf8(&bytes).map_err(|_| invalid())?;
    let mut previous = 0;
    let mut last = None;
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        if line.len() > 256 * 1024 {
            return Err(invalid());
        }
        let record: Value = serde_json::from_str(line).map_err(|_| invalid())?;
        let sequence = record["seq"]
            .as_u64()
            .filter(|sequence| *sequence > previous)
            .ok_or_else(invalid)?;
        let kind = record["type"].as_str().ok_or_else(invalid)?;
        if record["id"] != id || record["event"]["id"] != id || record["event"]["type"] != kind {
            return Err(invalid());
        }
        previous = sequence;
        // shortcut: only a final canonical completion proves inactivity; richer terminal replay belongs to the engine.
        last = match kind {
            "complete" => Some("completed"),
            "discarded" => Some("discarded"),
            _ => None,
        };
    }
    let phase = last.ok_or_else(|| {
        error("Uma sessão Live ainda está pendente ou não confirmou seu encerramento.")
    })?;
    Ok((previous, phase, metadata.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        _directory: tempfile::TempDir,
        home: PathBuf,
        project: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let home = directory.path().join("home");
            let project = directory.path().join("project");
            fs::create_dir_all(&home).unwrap();
            fs::create_dir_all(project.join("app")).unwrap();
            Self {
                home: home.canonicalize().unwrap(),
                project: project.canonicalize().unwrap(),
                _directory: directory,
            }
        }

        fn inspect(&self, args: &[String]) -> Request {
            request(&self.home, &self.project, "chat", args, None, &[]).unwrap()
        }

        fn record(&self, path: &str, value: &Value) {
            let path = self.project.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, value.to_string()).unwrap();
        }

        fn owner(&self) {
            let app = self.project.join("app");
            let record = Ownership {
                conversation_id: "chat".into(),
                project_root: self.project.clone(),
                root: app.clone(),
                identity: (
                    app.join(".impeccable/live/server.json"),
                    8400,
                    "private".into(),
                ),
                pid: std::process::id(),
                executable: None,
                pending_id: None,
                url: None,
                design_path: None,
                product_path: None,
                restart: false,
            };
            let path = ownership_path(&self.home, &self.project);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, serde_json::to_vec(&record).unwrap()).unwrap();
        }

        fn journal(&self, relative: &str, kinds: &[&str]) -> PathBuf {
            let path = self.project.join(relative).join("action.jsonl");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let content = kinds
                .iter()
                .enumerate()
                .map(|(index, kind)| {
                    format!("{}\n", json!({"seq":index + 1,"id":"action","type":kind,"event":{"id":"action","type":kind}}))
                })
                .collect::<String>();
            fs::write(&path, content).unwrap();
            path
        }
    }

    #[test]
    fn empty_native_stores_confirm_the_project_and_conversation_scope() {
        let fixture = Fixture::new();
        let inspected = fixture.inspect(&[]).inspection;
        assert_eq!(inspected["confirmedInactive"], true);
        assert_eq!(inspected["projectRoot"], json!(fixture.project));
        assert_eq!(inspected["conversationId"], "chat");
        assert_eq!(inspected["target"], Value::Null);
        assert_eq!(inspected["appRoots"], json!([fixture.project]));
        assert!(inspected.get("error").is_none());
    }

    #[test]
    fn a_living_helper_with_bad_auth_never_confirms_absence() {
        let fixture = Fixture::new();
        for relative in [".impeccable/live/server.json", ".impeccable-live.json"] {
            fixture.record(
                relative,
                &json!({"pid":std::process::id(),"port":8400,"token":"wrong-auth-private"}),
            );
            let inspected = fixture.inspect(&[]).inspection;
            assert_eq!(inspected["confirmedInactive"], false);
            assert!(inspected["error"].as_str().is_some());
            assert!(!inspected.to_string().contains("wrong-auth-private"));
            fs::remove_file(fixture.project.join(relative)).unwrap();
        }
    }

    #[test]
    fn malformed_helper_and_pointer_records_are_inconclusive() {
        let fixture = Fixture::new();
        for relative in [
            ".impeccable/live/server.json",
            ".impeccable/live/app-root.json",
        ] {
            let path = fixture.project.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "{broken").unwrap();
            assert_eq!(fixture.inspect(&[]).inspection["confirmedInactive"], false);
            fs::remove_file(&path).unwrap();
        }
    }

    #[test]
    fn corrupt_or_unreadable_session_records_are_inconclusive() {
        let fixture = Fixture::new();
        for relative in [".impeccable/live/sessions", ".impeccable-live/sessions"] {
            let path = fixture.journal(relative, &["generate", "complete"]);
            fs::write(&path, "not-json\n").unwrap();
            let inspected = fixture.inspect(&[]).inspection;
            assert_eq!(inspected["confirmedInactive"], false);
            assert!(inspected["error"].as_str().is_some());
            fs::remove_file(&path).unwrap();
            fs::create_dir(&path).unwrap();
            assert_eq!(fixture.inspect(&[]).inspection["confirmedInactive"], false);
            fs::remove_dir(&path).unwrap();
        }
    }

    #[test]
    fn completed_history_is_preserved_but_variants_ready_remain_active() {
        let fixture = Fixture::new();
        for relative in [".impeccable/live/sessions", ".impeccable-live/sessions"] {
            let path = fixture.journal(relative, &["generate", "complete"]);
            let original = fs::read(&path).unwrap();
            assert_eq!(fixture.inspect(&[]).inspection["confirmedInactive"], true);
            assert_eq!(fs::read(&path).unwrap(), original);
            fixture.journal(relative, &["generate", "agent_done"]);
            assert_eq!(fixture.inspect(&[]).inspection["confirmedInactive"], false);
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn requested_target_is_retained_and_other_apps_do_not_replace_its_scope() {
        let fixture = Fixture::new();
        fs::write(fixture.project.join("app/page.tsx"), "export default null").unwrap();
        fixture.record(
            ".impeccable/live/app-root.json",
            &json!({"version":2,"appRoots":[{"appRoot":fixture.project.join("other")}]}),
        );
        fixture.record(
            "other/.impeccable/live/server.json",
            &json!({"pid":std::process::id(),"port":8400,"token":"private"}),
        );
        for target in ["app", "app/page.tsx"] {
            let inspected = fixture.inspect(&[format!("--target={target}")]);
            assert_eq!(inspected.args, vec!["--target", target]);
            assert_eq!(
                inspected.inspection["target"],
                json!(fixture.project.join(target).canonicalize().unwrap())
            );
            assert_eq!(inspected.inspection["confirmedInactive"], true);
            assert!(!inspected.inspection["appRoots"]
                .as_array()
                .unwrap()
                .contains(&json!(fixture.project.join("other"))));
        }
    }

    #[test]
    fn an_app_root_pointer_includes_the_child_helper_in_unscoped_inspection() {
        let fixture = Fixture::new();
        fixture.record(
            ".impeccable/live/app-root.json",
            &json!({"version":2,"appRoots":[{"appRoot":fixture.project.join("app")}]}),
        );
        fixture.record(
            "app/.impeccable/live/server.json",
            &json!({"pid":std::process::id(),"port":8400,"token":"private"}),
        );
        let inspected = fixture.inspect(&[]).inspection;
        assert_eq!(inspected["confirmedInactive"], false);
        assert!(inspected["appRoots"]
            .as_array()
            .unwrap()
            .contains(&json!(fixture.project.join("app"))));
    }

    #[test]
    fn bounded_discovery_catches_a_child_helper_without_a_pointer() {
        let fixture = Fixture::new();
        fixture.record(
            "app/.impeccable/live/server.json",
            &json!({"pid":std::process::id(),"port":8400,"token":"private"}),
        );
        assert_eq!(fixture.inspect(&[]).inspection["confirmedInactive"], false);
    }

    #[test]
    fn a_corrupt_snapshot_cannot_hide_behind_a_completed_journal() {
        let fixture = Fixture::new();
        let path = fixture.journal(".impeccable/live/sessions", &["generate", "complete"]);
        fs::write(path.with_file_name("action.snapshot.json"), "{broken").unwrap();
        assert_eq!(fixture.inspect(&[]).inspection["confirmedInactive"], false);
    }

    #[test]
    fn owner_admission_lock_is_inspected_without_creating_or_replacing_it() {
        let fixture = Fixture::new();
        let path = ownership_path(&fixture.home, &fixture.project).with_extension("lock");
        assert!(!path.exists());
        assert_eq!(fixture.inspect(&[]).inspection["confirmedInactive"], true);
        assert!(!path.exists());
        let lock = lock_owner(&fixture.home, &fixture.project).unwrap();
        assert_eq!(fixture.inspect(&[]).inspection["confirmedInactive"], false);
        drop(lock);
        assert_eq!(fixture.inspect(&[]).inspection["confirmedInactive"], true);
        assert_eq!(fs::read(path).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn requested_target_normalizes_flags_and_rejects_inconsistent_scope() {
        let fixture = Fixture::new();
        let expected = fixture.project.join("app");
        for args in [
            vec!["--target".into(), "app".into()],
            vec!["--target=app".into()],
            vec!["-t".into(), "app".into()],
        ] {
            assert_eq!(
                requested_target(&fixture.project, &args).unwrap(),
                Some(expected.clone())
            );
        }
        assert_eq!(requested_target(&fixture.project, &[]).unwrap(), None);
        assert!(requested_target(
            &fixture.project,
            &["--target=app".into(), "--target=.".into()]
        )
        .is_err());
        assert!(requested_target(&fixture.project, &["--target=../home".into()]).is_err());
    }

    #[test]
    fn unrelated_status_arguments_are_forwarded_unchanged() {
        let fixture = Fixture::new();
        let args = vec![
            "--json".into(),
            "--compact".into(),
            "-t".into(),
            "app".into(),
            "--verbose=1".into(),
        ];
        let inspected = fixture.inspect(&args);
        assert_eq!(
            inspected.args,
            vec!["--json", "--compact", "--target", "app", "--verbose=1"]
        );
        assert_eq!(inspected.inspection["confirmedInactive"], true);
        assert_eq!(
            requested_target(&fixture.project, &args).unwrap(),
            Some(fixture.project.join("app"))
        );
        let opaque = vec!["--json".into(), "--compact".into()];
        assert_eq!(fixture.inspect(&opaque).args, opaque);
    }

    #[test]
    fn the_current_owned_app_target_is_passed_to_engine_status() {
        let fixture = Fixture::new();
        let app = fixture.project.join("app");
        let inspected = request(
            &fixture.home,
            &fixture.project,
            "chat",
            &[],
            Some(&app),
            std::slice::from_ref(&app),
        )
        .unwrap();
        assert_eq!(
            inspected.args,
            vec!["--target".to_owned(), app.to_string_lossy().into_owned()]
        );
        assert_eq!(inspected.inspection["confirmedInactive"], false);
        assert_eq!(inspected.inspection["target"], Value::Null);
    }

    #[test]
    fn a_saved_living_helper_cannot_be_hidden_by_a_missing_server_record() {
        let fixture = Fixture::new();
        fixture.owner();
        let inspected = fixture.inspect(&[]);
        assert_eq!(inspected.inspection["confirmedInactive"], false);
        assert_eq!(inspected.inspection["target"], Value::Null);
        assert_eq!(
            inspected.args,
            vec![
                "--target".to_owned(),
                fixture.project.join("app").to_string_lossy().into_owned()
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_redirected_saved_app_root_is_inconclusive() {
        let fixture = Fixture::new();
        fixture.owner();
        let app = fixture.project.join("app");
        let moved = fixture.project.join("moved-app");
        fs::rename(&app, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, app).unwrap();
        assert_eq!(fixture.inspect(&[]).inspection["confirmedInactive"], false);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_session_stores_do_not_confirm_absence() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.project.join(".impeccable/live")).unwrap();
        std::os::unix::fs::symlink(
            fixture.project.join("app"),
            fixture.project.join(".impeccable/live/sessions"),
        )
        .unwrap();
        assert_eq!(fixture.inspect(&[]).inspection["confirmedInactive"], false);
    }
}
