use super::*;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Barrier,
    },
    thread,
};

fn catalog(cost: Value) -> Value {
    json!({"opencode-go": {"models": {
        "bunny-free": {"name": "Space Bunny Free", "cost": cost}
    }}})
}

fn active() -> Value {
    json!({"data": [{"id": "bunny-free"}]})
}

fn expected() -> Vec<FreeModel> {
    vec![FreeModel {
        id: "bunny-free".into(),
        name: "Space Bunny Free".into(),
    }]
}

#[test]
fn accepts_only_explicit_zero_prices_and_deduplicates_available_ids() {
    let metadata = json!({"opencode-go": {"models": {
        "bunny-free": {"name": "Space Bunny Free", "cost": {"input": 0, "output": 0, "cache_read": 0, "cache_write": 0}},
        "longcat-free": {"name": "LongCat Free", "cost": {"input": 0.0, "output": 0.0}},
        "paid-free": {"name": "Misleading Free", "cost": {"input": 0.15, "output": 0.6}},
        "removed-free": {"name": "Removed Free", "cost": {"input": 0, "output": 0}}
    }}});
    let models = parse(
        &json!({"data": [{"id": "bunny-free"}, {"id": "paid-free"}, {"id": "longcat-free"}, {"id": "bunny-free"}, {"id": "unknown"}]}),
        &metadata,
    )
    .unwrap();
    assert_eq!(
        models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        ["longcat-free", "bunny-free"]
    );
}

#[test]
fn missing_invalid_or_unknown_price_dimensions_never_mean_free() {
    for cost in [
        Value::Null,
        json!({}),
        json!({"input": 0}),
        json!({"input": "0", "output": 0}),
        json!({"input": 0, "output": false}),
        json!({"input": 0, "output": 0, "cache_read": null}),
        json!({"input": 0, "output": 0, "cache_write": 0.1}),
        json!({"input": 0, "output": 0, "cache_read": -1}),
        json!({"input": 0, "output": 0, "per_request": 0.1}),
        json!({"input": 0, "output": 0, "future_price": 0}),
        json!({"input": 0, "output": 0, "tiers": null}),
        json!({"input": 0, "output": 0, "context_over_200k": null}),
    ] {
        assert!(
            parse(&active(), &catalog(cost.clone())).unwrap().is_empty(),
            "{cost}"
        );
    }
}

#[test]
fn checks_cache_and_prices_in_every_context_tier() {
    let free = json!({
        "input": 0, "output": 0,
        "tiers": [{"input": 0, "output": 0, "cache_read": 0, "tier": {"type": "context", "size": 200000}}],
        "context_over_200k": {"input": 0, "output": 0, "cache_write": 0}
    });
    assert_eq!(
        parse(&active(), &catalog(free.clone())).unwrap(),
        expected()
    );
    for pointer in [
        "/tiers/0/input",
        "/tiers/0/output",
        "/tiers/0/cache_read",
        "/context_over_200k/input",
        "/context_over_200k/output",
        "/context_over_200k/cache_write",
    ] {
        let mut paid = free.clone();
        *paid.pointer_mut(pointer).unwrap() = json!(0.01);
        assert!(
            parse(&active(), &catalog(paid)).unwrap().is_empty(),
            "{pointer}"
        );
    }
}

#[test]
fn malformed_tiers_and_missing_prices_are_excluded() {
    for tier in [
        json!({"input": 0, "output": 0}),
        json!({"output": 0, "tier": {"type": "context", "size": 1000}}),
        json!({"input": 0, "output": 0, "tier": {"type": "unknown", "size": 1000}}),
        json!({"input": 0, "output": 0, "tier": {"type": "context", "size": null}}),
        json!({"input": 0, "output": 0, "tier": {"type": "context", "size": 0}}),
        json!({"input": 0, "output": 0, "tier": {"type": "context", "size": 1000}, "request": 0}),
    ] {
        let cost = json!({"input": 0, "output": 0, "tiers": [tier]});
        assert!(parse(&active(), &catalog(cost)).unwrap().is_empty());
    }
}

#[test]
fn invalid_catalog_shape_returns_an_error_instead_of_confirming_no_free_models() {
    assert!(parse(
        &json!({"data": null}),
        &catalog(json!({"input": 0, "output": 0}))
    )
    .is_err());
    assert!(parse(&active(), &json!({"opencode": {"models": {}}})).is_err());
    assert_eq!(
        parse(
            &json!({"data": []}),
            &catalog(json!({"input": 0, "output": 0}))
        )
        .unwrap(),
        Vec::<FreeModel>::new()
    );
}

fn server(replies: Vec<(u16, String)>) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let worker = thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, body) in replies {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            requests.push(String::from_utf8(request).unwrap());
            write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
        requests
    });
    (url, worker)
}

#[test]
fn public_http_reads_availability_and_official_prices_without_credentials() {
    let (url, worker) = server(vec![
        (200, active().to_string()),
        (
            200,
            catalog(json!({"input": 0, "output": 0, "cache_read": 0})).to_string(),
        ),
    ]);
    assert_eq!(
        fetch_from(
            &client().unwrap(),
            &format!("{url}/models"),
            &format!("{url}/api.json")
        )
        .unwrap(),
        expected()
    );
    let requests = worker.join().unwrap();
    assert!(requests[0].starts_with("GET /models HTTP/1.1"));
    assert!(requests[1].starts_with("GET /api.json HTTP/1.1"));
    assert!(requests.iter().all(|request| {
        let request = request.to_ascii_lowercase();
        !request.contains("authorization:") && !request.contains("x-api-key:")
    }));
}

#[test]
fn http_failures_are_safe_and_never_return_partial_or_stale_prices() {
    for (status, body) in [
        (503, "private gateway details"),
        (200, "invalid JSON private content"),
    ] {
        let (url, worker) = server(vec![(status, body.into())]);
        let error = fetch_from(&client().unwrap(), &url, &url).unwrap_err();
        assert!(!error.message.contains("private"));
        worker.join().unwrap();
    }
}

#[test]
fn cache_coalesces_concurrent_public_requests() {
    let cache = Arc::new(Mutex::new(None));
    let calls = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(4));
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let cache = cache.clone();
            let calls = calls.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                cached_models(&cache, || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(20));
                    Ok(expected())
                })
                .unwrap()
            })
        })
        .collect();
    for worker in workers {
        assert_eq!(worker.join().unwrap(), expected());
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn expired_cache_is_replaced_and_failed_refresh_does_not_reuse_free_prices() {
    let cache = Mutex::new(Some(Cached {
        checked_at: Instant::now() - FRESH_FOR,
        result: Ok(expected()),
    }));
    assert_eq!(
        cached_models(&cache, || Err(unavailable())),
        Err(unavailable())
    );
    assert_eq!(
        cached_models(&cache, || panic!("error refresh should be coalesced")),
        Err(unavailable())
    );
    cache.lock().unwrap().as_mut().unwrap().checked_at = Instant::now() - RETRY_AFTER;
    assert_eq!(
        cached_models(&cache, || Ok(Vec::new())).unwrap(),
        Vec::<FreeModel>::new()
    );
}

#[test]
fn ipc_response_contains_only_model_id_and_name() {
    assert_eq!(
        serde_json::to_value(expected()).unwrap(),
        json!([{"id": "bunny-free", "name": "Space Bunny Free"}])
    );
}
