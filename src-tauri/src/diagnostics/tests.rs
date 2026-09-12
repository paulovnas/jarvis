use super::*;

fn state(root: &Path, run: char) -> DiagnosticsState {
    DiagnosticsState::new(
        root.to_path_buf(),
        "0.9.10-beta".into(),
        run.to_string().repeat(32),
    )
}

fn all_records(state: &DiagnosticsState) -> Vec<Record> {
    state
        .records()
        .unwrap()
        .into_iter()
        .flat_map(|(_, records)| records)
        .collect()
}

#[test]
fn startup_marker_reports_only_the_previous_unclean_run() {
    let directory = tempfile::tempdir().unwrap();
    let first = state(directory.path(), 'a');
    first.begin();

    let second = state(directory.path(), 'b');
    second.begin();
    assert_eq!(
        all_records(&second)
            .iter()
            .filter(|record| record.event == EventKind::AbruptShutdown)
            .count(),
        1
    );
    assert_eq!(
        all_records(&second)
            .iter()
            .find(|record| record.event == EventKind::AbruptShutdown)
            .and_then(|record| record.previous_run_id.as_deref()),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );

    second.finish(ShutdownReason::UserExit);
    let third = state(directory.path(), 'c');
    third.begin();
    assert_eq!(
        all_records(&third)
            .iter()
            .filter(|record| record.event == EventKind::AbruptShutdown)
            .count(),
        1
    );
}

#[test]
fn log_rotation_keeps_a_fixed_number_of_regular_files() {
    let directory = tempfile::tempdir().unwrap();
    let state = state(directory.path(), 'a');
    ensure_directory(&state.inner.root).unwrap();
    let record = Record::new(&state, EventKind::Startup, Level::Info);
    for _ in 0..20 {
        write_record(&state.inner.root, &record, 512, 3).unwrap();
    }
    assert!(log_path(&state.inner.root, 0).is_file());
    assert!(log_path(&state.inner.root, 1).is_file());
    assert!(log_path(&state.inner.root, 2).is_file());
    assert!(!log_path(&state.inner.root, 3).exists());
}

#[test]
fn provider_metadata_accepts_only_bounded_identifiers() {
    let valid = ProviderMetadata::new(
        Some(429),
        Some("rate_limit_exceeded"),
        Some("req_01J-safe:value"),
    );
    assert_eq!(valid.http_status, Some(429));
    assert_eq!(valid.upstream_code.as_deref(), Some("rate_limit_exceeded"));
    assert_eq!(valid.request_id.as_deref(), Some("req_01J-safe:value"));

    let invalid = ProviderMetadata::new(
        Some(999),
        Some("private key invalid"),
        Some("Bearer secret-value"),
    );
    assert_eq!(invalid.http_status, None);
    assert_eq!(invalid.upstream_code, None);
    assert_eq!(invalid.request_id, None);
}

#[test]
fn summary_distinguishes_required_failure_categories_without_private_content() {
    let directory = tempfile::tempdir().unwrap();
    let state = state(directory.path(), 'a');
    state.begin();

    let mut update = Record::new(&state, EventKind::CleanShutdown, Level::Info);
    update.shutdown_reason = Some(ShutdownReason::Update);
    state.record(update);
    state.record(Record::new(&state, EventKind::Panic, Level::Error));
    state.record(Record::new(
        &state,
        EventKind::SingleInstanceConflict,
        Level::Warning,
    ));
    let mut storage = Record::new(&state, EventKind::StorageFailure, Level::Error);
    storage.operation = Some("session_journal".into());
    storage.correlation_id = correlation("conversation-private");
    state.record(storage);
    let mut provider = Record::new(&state, EventKind::ProviderRefusal, Level::Error);
    provider.provider = Some("custom".into());
    provider.category = Some("provider_limit".into());
    provider.http_status = Some(429);
    provider.upstream_code = Some("rate_limit_exceeded".into());
    provider.request_id = Some("req-safe".into());
    provider.correlation_id = correlation("conversation-private");
    state.record(provider);

    let summary = state.summary().unwrap();
    assert!(summary
        .recent_events
        .iter()
        .any(|event| event.event == EventKind::Panic));
    assert!(summary
        .recent_events
        .iter()
        .any(|event| event.event == EventKind::SingleInstanceConflict));
    assert!(summary
        .recent_events
        .iter()
        .any(|event| event.event == EventKind::StorageFailure));
    assert!(summary
        .recent_events
        .iter()
        .any(|event| event.event == EventKind::ProviderRefusal));
    assert!(summary.copyable.contains("reason=Update"));
    assert!(summary.copyable.contains("http=429"));
    assert!(!summary.copyable.contains("conversation-private"));
}

#[test]
fn export_reparses_logs_and_drops_unknown_or_unsafe_records() {
    use std::io::Read as _;

    let directory = tempfile::tempdir().unwrap();
    let state = state(directory.path(), 'a');
    state.begin();
    let path = log_path(&state.inner.root, 0);
    let mut file = OpenOptions::new().append(true).open(path).unwrap();
    writeln!(
        file,
        "{{\"prompt\":\"TOP SECRET\",\"response\":\"private\"}}"
    )
    .unwrap();
    writeln!(file, "{{\"schemaVersion\":1,\"timestamp\":1,\"level\":\"error\",\"event\":\"provider_refusal\",\"runId\":\"{}\",\"appVersion\":\"0.9.10-beta\",\"os\":\"macos\",\"arch\":\"aarch64\",\"pid\":1,\"requestId\":\"Bearer secret\"}}", "a".repeat(32)).unwrap();
    drop(file);

    let destination = directory.path().join("Jarvis-diagnostico.zip");
    let result = state.export(&destination).unwrap();
    assert!(result.bytes > 0);
    let mut archive = zip::ZipArchive::new(File::open(destination).unwrap()).unwrap();
    let mut contents = String::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        entry.read_to_string(&mut contents).unwrap();
    }
    assert!(contents.contains("jarvis-diagnostics"));
    assert!(
        contents.contains("\"event\": \"startup\"") || contents.contains("\"event\":\"startup\"")
    );
    assert!(!contents.contains("TOP SECRET"));
    assert!(!contents.contains("private"));
    assert!(!contents.contains("prompt"));
    assert!(!contents.contains("response"));
    assert!(!contents.contains("Bearer"));
}
