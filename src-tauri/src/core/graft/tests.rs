use super::*;

async fn query(graft: &Graft, signal: &watch::Receiver<bool>, name: &str, args: Value) -> Value {
    serde_json::from_str(&graft.execute(name, &args, signal.clone()).await.unwrap()).unwrap()
}

#[test]
fn receipts_do_not_claim_missing_stale_or_partial_graphs_are_reused() {
    use super::super::activity::Status;
    assert_eq!(
        receipt(&json!({"freshness":{"missing":true}})).0,
        Status::Pending
    );
    assert_eq!(
        receipt(&json!({"freshness":{"current":false}})).0,
        Status::Pending
    );
    assert_eq!(
        receipt(&json!({"freshness":{"current":true},"coverage":{"partial":true}})).0,
        Status::Issues
    );
    assert_eq!(
        receipt(&json!({"freshness":{"current":true,"rebuilt":true}})).0,
        Status::Applied
    );
    assert_eq!(
        receipt(&json!({"freshness":{"current":true,"rebuilt":false}})).0,
        Status::Reused
    );
}

#[test]
fn structural_tools_expose_provider_ready_function_contracts() {
    let tools = definitions();
    assert_eq!(tools.len(), 6);
    for tool in &tools {
        assert_eq!(tool["type"], "function", "{}", tool["name"]);
        assert_eq!(tool["strict"], false, "{}", tool["name"]);
        assert_eq!(tool["parameters"]["type"], "object");
        assert_eq!(tool["parameters"]["additionalProperties"], false);
    }
    let find = tools
        .iter()
        .find(|tool| tool["name"] == "graft_find_code")
        .unwrap();
    assert_eq!(find["parameters"]["required"], json!(["query"]));
    assert!(find["parameters"]["properties"].get("limit").is_some());
    let map = tools
        .iter()
        .find(|tool| tool["name"] == "graft_repo_map")
        .unwrap();
    assert_eq!(map["parameters"]["required"], json!([]));
}

#[test]
fn structural_tools_validate_scope_limits_and_types() {
    assert_eq!(definitions().len(), 6);
    for (name, args) in [
        ("graft_file_api", json!({"file":"src/lib.ts"})),
        (
            "graft_trace_calls",
            json!({"symbol":"Thing.call","depth":"all"}),
        ),
        (
            "graft_find_all",
            json!({"pattern":"Thing","fixed":true,"in":"src"}),
        ),
        ("graft_check_freshness", json!({})),
    ] {
        assert!(validate_args(name, &args).is_ok());
    }
    for (name, args) in [
        ("unknown", json!({})),
        ("graft_find_code", json!({"query":"hello","limit":11})),
        ("graft_find_code", json!({"query":"hello","limit":"5"})),
        ("graft_file_api", json!({"file":"../outside.ts"})),
        ("graft_file_api", json!({"file":"/outside.ts"})),
        ("graft_file_api", json!({"file":"C:\\outside.ts"})),
        ("graft_find_all", json!({"pattern":"","fixed":true})),
        ("graft_repo_map", json!({"max_dirs":999})),
        ("graft_trace_calls", json!({"symbol":"x","depth":0})),
        ("graft_check_freshness", json!({"root":"other"})),
    ] {
        assert!(validate_args(name, &args).is_err(), "{name}: {args}");
    }
}
#[test]
fn graph_cache_is_private_and_separate_per_checkout() {
    let home = tempfile::tempdir().unwrap();
    let a = storage(home.path(), Path::new("/checkout/a")).unwrap();
    let b = storage(home.path(), Path::new("/checkout/b")).unwrap();
    assert_ne!(a, b);
    assert!(a.starts_with(fs::canonicalize(crate::data_dir::root(home.path())).unwrap()));
    assert!(!a.starts_with("/checkout/a"));
    assert_eq!(a, storage(home.path(), Path::new("/checkout/a")).unwrap());
}
#[cfg(unix)]
#[test]
fn redirected_private_cache_is_rejected() {
    let home = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    fs::create_dir(crate::data_dir::root(home.path())).unwrap();
    std::os::unix::fs::symlink(
        other.path(),
        crate::data_dir::root(home.path()).join("graft"),
    )
    .unwrap();
    assert!(storage(home.path(), Path::new("/checkout")).is_err());
    assert_eq!(fs::read_dir(other.path()).unwrap().count(), 0);
}
#[tokio::test]
async fn cancellation_breaks_singleflight_wait_without_releasing_another_call() {
    let cache = tempfile::tempdir().unwrap();
    let (sender, signal) = watch::channel(false);
    let held = lock(cache.path(), signal.clone()).await.unwrap();
    let pending = lock(cache.path(), signal.clone());
    let cancel = async {
        tokio::task::yield_now().await;
        sender.send(true).unwrap();
    };
    let (result, _) = tokio::join!(pending, cancel);
    assert_eq!(result.unwrap_err().code, "cancelled");
    sender.send(false).unwrap();
    assert!(tokio::time::timeout(
        Duration::from_millis(80),
        lock(cache.path(), signal.clone())
    )
    .await
    .is_err());
    drop(held);
    assert!(lock(cache.path(), signal).await.is_ok());
}
#[tokio::test]
async fn projectless_chat_spawns_nothing_and_cancelled_open_propagates() {
    let (_sender, signal) = watch::channel(false);
    let inactive = Graft::inactive();
    assert!(inactive.definitions().is_empty());
    assert!(inactive
        .prepare("generate image", signal.clone())
        .await
        .unwrap()
        .is_empty());
    assert!(inactive.take_activity().is_empty());
    let (_sender, cancelled) = watch::channel(true);
    assert_eq!(
        Graft::open(Path::new("/missing"), Path::new("/missing"), cancelled)
            .await
            .err()
            .unwrap()
            .code,
        "cancelled"
    );
    let missing = Graft::open(Path::new("/missing"), Path::new("/missing"), signal)
        .await
        .unwrap();
    assert!(missing.definitions().is_empty());
    assert_eq!(
        missing.take_activity()[0].status,
        super::super::activity::Status::Unavailable
    );
}
#[tokio::test]
#[ignore = "Downloads managed Graft + Node and verifies actual portable parsers"]
async fn native_graft_installation_and_structural_lifecycle() {
    let home = tempfile::tempdir().unwrap();
    super::super::install::install(
        home.path(),
        ComponentId::Graft,
        |stage| eprintln!("Graft: {stage}"),
        |_| {},
    )
    .await
    .unwrap();
    let root = home.path().join("code");
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("math.ts"),
        "export function add(a:number,b:number) { return a+b; }\n",
    )
    .unwrap();
    fs::write(
        root.join("main.ts"),
        "import {add} from './math'; export function calculate() { return add(1,2); }\n",
    )
    .unwrap();
    let (_sender, signal) = watch::channel(false);
    let graft = Graft::open(home.path(), &root, signal.clone())
        .await
        .unwrap();
    let cold = query(&graft, &signal, "graft_repo_map", json!({})).await;
    assert_eq!(cold["result"]["totals"]["files"], 2);
    assert_eq!(cold["freshness"]["rebuilt"], true);
    let callers = query(
        &graft,
        &signal,
        "graft_trace_calls",
        json!({"symbol":"add"}),
    )
    .await;
    assert!(callers.to_string().contains("calculate"));
    assert_eq!(callers["freshness"]["rebuilt"], false);
    let modified = fs::metadata(root.join("math.ts"))
        .unwrap()
        .modified()
        .unwrap();
    fs::write(
        root.join("math.ts"),
        "export function sum(a:number,b:number) { return a+b; }\n",
    )
    .unwrap();
    fs::File::options()
        .write(true)
        .open(root.join("math.ts"))
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    let drift = query(&graft, &signal, "graft_check_freshness", json!({})).await;
    assert_eq!(drift["freshness"]["current"], false);
    let api = query(&graft, &signal, "graft_file_api", json!({"file":"math.ts"})).await;
    assert!(api.to_string().contains("sum"));
    assert!(!api.to_string().contains("\"name\":\"add\""));
    fs::remove_file(root.join("main.ts")).unwrap();
    let map = query(&graft, &signal, "graft_repo_map", json!({})).await;
    assert_eq!(map["result"]["totals"]["files"], 1);
    assert_eq!(
        fs::read_dir(&root).unwrap().count(),
        1,
        "No project hooks/config/cache created"
    );
    let references = query(
        &graft,
        &signal,
        "graft_find_all",
        json!({"pattern":"sum", "fixed":true}),
    )
    .await;
    assert_eq!(references["result"]["totalHits"], 1);
    fs::write(
        root.join("long.ts"),
        format!(
            "export function unicodeExample() {{ return '{}'; }}",
            "雪".repeat(3500)
        ),
    )
    .unwrap();
    let excerpt = query(
        &graft,
        &signal,
        "graft_find_code",
        json!({"query":"unicodeExample"}),
    )
    .await;
    assert_eq!(excerpt["result"]["hits"][0]["excerpt"]["truncated"], true);
    assert!(excerpt.to_string().len() < 60000);
    let renamed = root.join("renamed.ts");
    fs::rename(root.join("math.ts"), &renamed).unwrap();
    let api = query(
        &graft,
        &signal,
        "graft_file_api",
        json!({"file":"renamed.ts"}),
    )
    .await;
    assert_eq!(api["result"]["file"], "renamed.ts");
}
