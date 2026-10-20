use super::*;

fn state(root: &Path) -> DiagnosticsState {
    DiagnosticsState::new(root.to_owned(), "2.0.7".into(), "a".repeat(32))
}

#[test]
fn detailed_errors_survive_restart_with_multiline_stacks_and_redacted_secrets() {
    let root = tempfile::tempdir().unwrap();
    let first = state(root.path());
    first.begin();
    let stack = format!(
        "Error: failure\n{}",
        "  at native_handler (src/handler.rs:42)\n".repeat(300)
    );
    let mut record = Record::new(&first, EventKind::Runtime, Level::Error);
    record.operation = Some("jarvis_propose_flow".into());
    record.category = Some("tool_arguments_invalid".into());
    record.correlation_id = correlation("original-tool-call");
    record.details = Some(Details::new(
        "Invalid agentId; Authorization: Bearer private-value; api_key=sk-private; https://u:p@example.test/path?token=private-query",
        Some(&stack),
    ));
    first.record(record);
    let restarted = state(root.path());
    let page = restarted
        .log_page(Query {
            level: Some(Level::Error),
            ..Query::default()
        })
        .unwrap();
    assert_eq!(page.total, 1);
    let event = &page.events[0];
    let details = event.details.as_ref().unwrap();
    assert_eq!(details.stack.as_deref(), Some(stack.as_str()));
    assert!(!details.truncated);
    assert_eq!(event.operation.as_deref(), Some("jarvis_propose_flow"));
    let stored = fs::read_to_string(log_path(&first.inner.root, 0)).unwrap();
    for secret in ["private-value", "sk-private", "private-query", "u:p@"] {
        assert!(!stored.contains(secret), "persisted {secret}");
    }
    assert!(stored.contains("[redigido]"));
}

#[test]
fn logs_filter_and_page_without_replaying_operations() {
    let root = tempfile::tempdir().unwrap();
    let state = state(root.path());
    for (timestamp, level) in [(1, Level::Info), (2, Level::Warning), (3, Level::Error)] {
        let mut record = Record::new(&state, EventKind::Runtime, level);
        record.timestamp = timestamp;
        record.details = Some(Details::new("workflow schema", None));
        state.record(record);
    }
    let page = state
        .log_page(Query {
            limit: Some(1),
            before: Some(3),
            query: Some("SCHEMA".into()),
            ..Query::default()
        })
        .unwrap();
    assert_eq!(page.total, 3);
    assert_eq!(page.next_offset, Some(1));
    assert_eq!(page.events[0].timestamp, 3);
    let next = state
        .log_page(Query {
            offset: 1,
            limit: Some(1),
            before: Some(page.before),
            ..Query::default()
        })
        .unwrap();
    assert_eq!(next.events[0].level, Level::Warning);
    assert_eq!(
        state
            .log_page(Query {
                since: Some(3),
                ..Query::default()
            })
            .unwrap()
            .total,
        1
    );
    assert!(state
        .log_page(Query {
            limit: Some(51),
            ..Query::default()
        })
        .is_err());
}

#[test]
fn cleanup_requires_confirmation_preserves_markers_and_continues_logging() {
    let root = tempfile::tempdir().unwrap();
    let state = state(root.path());
    state.begin();
    fs::write(state.inner.root.join("unrelated.jsonl"), "preserve").unwrap();
    assert!(clear(&state, false).is_err());
    assert!(log_path(&state.inner.root, 0).exists());
    let result = clear(&state, true).unwrap();
    assert!(result.freed_bytes > 0);
    assert_eq!(state.log_storage(false).unwrap(), (0, 0));
    assert!(state.inner.root.join(MARKER).exists());
    assert_eq!(
        fs::read_to_string(state.inner.root.join("unrelated.jsonl")).unwrap(),
        "preserve"
    );
    state.record(Record::new(&state, EventKind::Runtime, Level::Warning));
    assert_eq!(state.log_page(Query::default()).unwrap().total, 1);
}

#[test]
fn oversize_details_are_explicit_and_invalid_injected_details_are_excluded() {
    let root = tempfile::tempdir().unwrap();
    let state = state(root.path());
    state.begin();
    let details = Details::new(
        &"á".repeat(MAX_MESSAGE_BYTES),
        Some(&"s".repeat(MAX_STACK_BYTES + 1)),
    );
    assert!(details.truncated);
    assert!(details.valid());
    assert!(details.message.len() <= MAX_MESSAGE_BYTES);
    let mut record = Record::new(&state, EventKind::Runtime, Level::Error);
    record.details = Some(Details {
        message: "token=must-not-escape".into(),
        stack: None,
        truncated: false,
    });
    let mut file = append_file(&log_path(&state.inner.root, 0)).unwrap();
    writeln!(file, "{}", serde_json::to_string(&record).unwrap()).unwrap();
    assert_eq!(
        state
            .log_page(Query {
                level: Some(Level::Error),
                ..Query::default()
            })
            .unwrap()
            .total,
        0
    );
}

#[test]
fn rich_records_keep_the_existing_rotation_budget() {
    let root = tempfile::tempdir().unwrap();
    let state = state(root.path());
    let mut record = Record::new(&state, EventKind::Runtime, Level::Warning);
    record.details = Some(Details::new("failure", Some(&"frame\n".repeat(100))));
    for _ in 0..30 {
        write_record(&state.inner.root, &record, 4096, 3).unwrap();
    }
    let bytes: u64 = (0..3)
        .map(|i| fs::metadata(log_path(&state.inner.root, i)).unwrap().len())
        .sum();
    assert!(bytes <= 4096 * 3);
    assert!(!log_path(&state.inner.root, 3).exists());
}

#[test]
fn invalid_cleanup_entries_are_rejected_before_removing_any_logs() {
    let root = tempfile::tempdir().unwrap();
    let state = state(root.path());
    state.begin();
    fs::create_dir(log_path(&state.inner.root, 1)).unwrap();
    assert!(clear(&state, true).is_err());
    assert!(log_path(&state.inner.root, 0).is_file());
    assert!(state.inner.root.join(MARKER).is_file());
}

#[test]
fn diagnostic_write_failures_are_counted_instead_of_claimed_as_success() {
    let root = tempfile::tempdir().unwrap();
    let state = state(root.path());
    ensure_directory(&state.inner.root).unwrap();
    fs::create_dir(log_path(&state.inner.root, 0)).unwrap();
    state.record(Record::new(&state, EventKind::Runtime, Level::Error));
    assert_eq!(state.inner.dropped_records.load(Ordering::Relaxed), 1);
}
