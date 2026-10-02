use super::*;

fn running() -> Inner {
    let mut inner = Inner {
        config: Config {
            enabled: true,
            ..Config::default()
        },
        port: Some(DEFAULT_PORT),
        urls: vec![format!("http://192.168.1.8:{DEFAULT_PORT}")],
        ..Inner::default()
    };
    inner.rotate_pairing(1_000).unwrap();
    inner
}
fn headers(cookie: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::HOST,
        format!("192.168.1.8:{DEFAULT_PORT}").parse().unwrap(),
    );
    headers.insert(
        header::ORIGIN,
        format!("http://192.168.1.8:{DEFAULT_PORT}")
            .parse()
            .unwrap(),
    );
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    if let Some(cookie) = cookie {
        headers.insert(
            header::COOKIE,
            format!("{COOKIE}={cookie}").parse().unwrap(),
        );
    }
    headers
}
#[test]
fn changing_network_updates_pairing_and_host_allowlist_without_revoking_devices() {
    let mut inner = running();
    let token = inner.pairing.as_ref().unwrap().token.clone();
    let (cookie, _) = inner
        .pair(
            PairInput {
                token,
                name: "Celular".into(),
            },
            1_001,
        )
        .unwrap();
    let pairing = inner.pairing.as_ref().unwrap().token.clone();
    assert!(inner.update_interfaces([
        "10.0.0.8".parse().unwrap(),
        "10.0.0.8".parse().unwrap(),
        "8.8.8.8".parse().unwrap(),
        "::1".parse().unwrap(),
    ]));
    assert_eq!(
        inner.urls,
        [
            format!("http://10.0.0.8:{DEFAULT_PORT}"),
            format!("http://127.0.0.1:{DEFAULT_PORT}"),
        ]
    );
    assert_eq!(
        inner.status().pairing_url.unwrap(),
        format!("http://10.0.0.8:{DEFAULT_PORT}/#pair={pairing}")
    );
    let old_headers = headers(Some(&cookie));
    let peer = "10.0.0.9".parse().unwrap();
    assert!(!valid_request(&inner, peer, &old_headers, &Method::POST));
    let mut current = old_headers;
    current.insert(
        header::HOST,
        format!("10.0.0.8:{DEFAULT_PORT}").parse().unwrap(),
    );
    current.insert(
        header::ORIGIN,
        format!("http://10.0.0.8:{DEFAULT_PORT}").parse().unwrap(),
    );
    assert!(valid_request(&inner, peer, &current, &Method::POST));
    assert!(inner.session(&current, 1_002).is_some());
    assert!(!inner.update_interfaces(["10.0.0.8".parse().unwrap()]));
    inner.close();
    assert!(!inner.update_interfaces(["192.168.1.8".parse().unwrap()]));
    assert!(inner.urls.is_empty());
}
#[test]
fn pairing_is_single_use_expires_and_session_does_not_expose_credentials() {
    let mut inner = running();
    let token = inner.pairing.as_ref().unwrap().token.clone();
    let (cookie, metadata) = inner
        .pair(
            PairInput {
                token: token.clone(),
                name: "Celular".into(),
            },
            1_001,
        )
        .unwrap();
    assert_eq!(metadata["name"], "Celular");
    assert!(metadata.get("token").is_none());
    assert!(inner
        .pair(
            PairInput {
                token,
                name: "Outro".into()
            },
            1_002
        )
        .is_err());
    assert!(inner.session(&headers(Some(&cookie)), 1_003).is_some());
    let token = inner.pairing.as_ref().unwrap().token.clone();
    assert!(inner
        .pair(
            PairInput {
                token,
                name: "Outro".into()
            },
            1_001 + PAIR_TTL
        )
        .is_err());
    let status = serde_json::to_value(inner.status()).unwrap();
    assert!(!status.to_string().contains(&cookie));
    assert!(!inner.devices.contains_key(&cookie));
    assert!(inner
        .session(&headers(Some(&cookie)), 1_001 + SESSION_TTL)
        .is_none());
}
#[test]
fn revocation_and_disable_invalidate_sessions_without_runtime_access() {
    let mut inner = running();
    let token = inner.pairing.as_ref().unwrap().token.clone();
    let (cookie, _) = inner
        .pair(
            PairInput {
                token,
                name: "Celular".into(),
            },
            1_001,
        )
        .unwrap();
    let (key, revoked) = inner.session(&headers(Some(&cookie)), 1_002).unwrap();
    let device = inner.devices.remove(&key).unwrap();
    device.revoked.send(true).unwrap();
    assert!(*revoked.borrow());
    assert!(inner.session(&headers(Some(&cookie)), 1_003).is_none());
    let token = inner.pairing.as_ref().unwrap().token.clone();
    let (cookie, _) = inner
        .pair(
            PairInput {
                token,
                name: "Outro".into(),
            },
            1_004,
        )
        .unwrap();
    inner.close();
    assert!(inner.port.is_none());
    assert!(inner.pairing.is_none());
    assert!(inner.session(&headers(Some(&cookie)), 1_005).is_none());
}
#[test]
fn host_origin_peer_and_json_checks_prevent_cross_origin_and_rebinding() {
    let inner = running();
    let peer = "192.168.1.9".parse().unwrap();
    let valid = headers(None);
    assert!(valid_request(&inner, peer, &valid, &Method::POST));
    let mut wrong = valid.clone();
    wrong.insert(header::HOST, "attacker.test:47731".parse().unwrap());
    assert!(!valid_request(&inner, peer, &wrong, &Method::POST));
    wrong = valid.clone();
    wrong.insert(header::ORIGIN, "https://attacker.test".parse().unwrap());
    assert!(!valid_request(&inner, peer, &wrong, &Method::GET));
    wrong = valid.clone();
    wrong.remove(header::ORIGIN);
    assert!(!valid_request(&inner, peer, &wrong, &Method::POST));
    wrong = valid.clone();
    wrong.insert(header::CONTENT_TYPE, "text/plain".parse().unwrap());
    assert!(!valid_request(&inner, peer, &wrong, &Method::POST));
    assert!(!valid_request(
        &inner,
        "8.8.8.8".parse().unwrap(),
        &valid,
        &Method::POST
    ));
    assert!(!valid_request(
        &inner,
        "::1".parse().unwrap(),
        &valid,
        &Method::POST
    ));
}
#[test]
fn remote_assets_never_resolve_desktop_files_or_traversal() {
    assert_eq!(asset_path("/").unwrap().0, "remote.html");
    assert!(asset_path("/assets/remote-Ab12.js").is_some());
    assert!(asset_path("/fonts/Roboto.woff2").is_some());
    for path in [
        "/index.html",
        "/src/main.tsx",
        "/assets/../index.html",
        "/assets/%2e%2e/index.html",
        "/assets/a\\b.js",
        "/assets/app.js.map",
        "/fonts/data.json",
        "//remote.html",
    ] {
        assert!(asset_path(path).is_none(), "{path}");
    }
}
#[test]
fn cookies_and_rpc_catalog_cannot_authorize_arbitrary_tauri_calls() {
    assert!(session_key(&headers(None)).is_none());
    let token = "ab".repeat(32);
    let mut duplicate = headers(Some(&token));
    duplicate.append(header::COOKIE, format!("{COOKIE}={token}").parse().unwrap());
    assert!(session_key(&duplicate).is_none());
    for method in [
        "get_provider_accounts",
        "run_shell",
        "save_system_preferences",
        "revoke_remote_device",
        "browser_command",
    ] {
        assert_eq!(mutation(method), None);
    }
    assert_eq!(mutation("message"), Some(true));
    assert_eq!(mutation("history"), Some(false));
}
#[test]
fn configuration_is_opt_in_and_validated_before_loading() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("remote.json");
    assert!(!Config::load(&path).unwrap().enabled);
    let config = Config {
        version: 1,
        enabled: true,
        port: 40000,
    };
    config.save(&path).unwrap();
    assert!(Config::load(&path).unwrap().enabled);
    fs::write(&path, br#"{"version":1,"enabled":true,"port":0}"#).unwrap();
    assert!(Config::load(&path).is_err());
    #[cfg(unix)]
    {
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(temp.path(), &path).unwrap();
        assert!(Config::load(&path).is_err());
    }
}
#[test]
fn rate_limits_pairing_and_bounds_untrusted_peer_storage() {
    let mut inner = running();
    let peer = "192.168.1.9".parse().unwrap();
    for _ in 0..10 {
        assert!(inner.rate(peer, true, 1_000));
    }
    assert!(!inner.rate(peer, true, 1_001));
    assert!(inner.rate(peer, true, 61_001));
}
#[tokio::test]
async fn action_result_is_replayed_and_revocation_interrupts_pending_response() {
    let (result, receive) = watch::channel(Some(success(Value::Null)));
    let (_revoke, revoked) = watch::channel(false);
    let response = wait_action(receive, revoked).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        success(Value::Null)
    );
    let (revoke, revoked) = watch::channel(false);
    let waiting = wait_action(result.subscribe(), revoked);
    revoke.send(true).unwrap();
    assert_eq!(waiting.await.status(), StatusCode::UNAUTHORIZED);
}
