use super::*;

#[test]
fn accepts_web_addresses_and_rejects_privileged_schemes_and_credentials() {
    assert_eq!(
        address("localhost:5173/test").unwrap().as_str(),
        "http://localhost:5173/test"
    );
    assert_eq!(address("https://example.com").unwrap().scheme(), "https");
    for value in [
        "",
        "file:///C:/secret",
        "tauri://localhost",
        "javascript:alert(1)",
        "https://user:pass@example.com",
        "data:text/html,test",
    ] {
        assert!(address(value).is_err(), "accepted {value}");
    }
}

#[test]
fn read_only_agents_cannot_navigate_or_interact() {
    let read = definitions(crate::agent::Mode::Plan);
    let names: Vec<_> = read
        .iter()
        .filter_map(|value| value["name"].as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "browser_list",
            "browser_snapshot",
            "browser_console",
            "browser_screenshot"
        ]
    );
    for tool in definitions(crate::agent::Mode::Build) {
        let name = tool["name"].as_str().unwrap();
        assert_eq!(
            crate::agent::tools::needs_approval(name),
            !names.contains(&name)
        );
    }
}

#[test]
fn bounds_reject_nan_negative_and_oversized_surfaces() {
    let good = Viewport {
        x: 50.,
        y: 120.,
        width: 900.,
        height: 600.,
    };
    assert!(good.valid());
    assert!(!Viewport {
        x: f64::NAN,
        ..good.clone()
    }
    .valid());
    assert!(!Viewport {
        width: 0.,
        ..good.clone()
    }
    .valid());
    assert!(!Viewport {
        y: -1.,
        ..good.clone()
    }
    .valid());
    assert!(!Viewport {
        height: 20000.,
        ..good
    }
    .valid());
}

#[test]
fn requests_reject_unknown_fields_instead_of_accepting_arbitrary_javascript() {
    assert!(serde_json::from_value::<BrowserRequest>(
        json!({"action":"snapshot", "id":"tab", "script":"evil()"})
    )
    .is_err());
}
