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
            "browser_wait",
            "browser_console",
            "browser_screenshot",
            "browser_discover",
            "browser_network",
            "browser_response_body"
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
fn extension_tools_explain_firefox_input_and_debugger_capabilities() {
    let tools = definitions(crate::agent::Mode::Build);
    let description = |name: &str| {
        tools.iter().find(|tool| tool["name"] == name).unwrap()["description"]
            .as_str()
            .unwrap()
    };
    assert!(description("browser_click").contains("Firefox uses DOM activation"));
    assert!(description("browser_devtools").contains("Firefox does not support CDP"));
    assert!(description("browser_evaluate").contains("CSP"));
    for name in [
        "browser_discover",
        "browser_snapshot",
        "browser_network",
        "browser_response_body",
    ] {
        assert!(!description(name).contains("Chromium"));
    }
}

#[test]
fn screenshot_save_path_is_optional_in_build_and_rejected_by_the_read_only_catalog() {
    for mode in [crate::agent::Mode::Plan, crate::agent::Mode::Build] {
        let tools = definitions(mode);
        let screenshot = tools
            .iter()
            .find(|tool| tool["name"] == "browser_screenshot")
            .unwrap();
        let schema = jsonschema::validator_for(&screenshot["parameters"]).unwrap();
        assert!(schema.is_valid(&json!({"id":"tab"})));
        assert_eq!(
            schema.is_valid(&json!({"id":"tab","savePath":"videos/demo/assets/page.png"})),
            mode == crate::agent::Mode::Build
        );
        assert_eq!(screenshot["parameters"]["required"], json!(["id"]));
    }
}

#[test]
fn screenshot_contract_supports_video_assets_without_replacing_dom_discovery_or_reuse() {
    for mode in [crate::agent::Mode::Plan, crate::agent::Mode::Build] {
        let tools = definitions(mode);
        let screenshot = tools
            .iter()
            .find(|tool| tool["name"] == "browser_screenshot")
            .unwrap();
        let description = screenshot["description"].as_str().unwrap();
        assert!(description.contains("requested product/UI assets for a video"));
        assert!(description.contains("Prefer snapshot for text and element discovery"));
        assert!(description.contains("when visual analysis is needed"));
        assert!(description.contains("reuse captures until the page changes"));
    }
    assert!(EFFICIENCY.contains("intentional product/UI captures needed by the requested video"));
    assert!(EFFICIENCY.contains("truthful product evidence and assets for the requested video"));
    assert!(EFFICIENCY.contains("Prefer a DOM snapshot for text, element discovery and behavior"));
    assert!(EFFICIENCY.contains("does not require a new capture of an unchanged page"));
    assert!(EFFICIENCY.contains("Never replace an unavailable browser with shell automation"));
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

#[test]
fn provider_compatible_schemas_accept_current_or_semantic_targets_and_reject_invalid_fields() {
    let tools = definitions(crate::agent::Mode::Build);
    for tool in &tools {
        assert_eq!(tool["strict"], false);
        let schema = tool["parameters"].to_string();
        for keyword in ["oneOf", "anyOf", "allOf", "not", "const"] {
            assert!(!schema.contains(&format!("\"{keyword}\":")));
        }
    }
    for (name, args, valid) in [
        (
            "browser_click",
            json!({"id":"tab","element":"element-1"}),
            true,
        ),
        (
            "browser_click",
            json!({"id":"tab","locator":{"role":"button","name":"Save"}}),
            true,
        ),
        (
            "browser_click",
            json!({"id":"tab","locator":{"css":"button"}}),
            false,
        ),
        (
            "browser_click",
            json!({"id":"tab","locator":{"text":1}}),
            false,
        ),
        (
            "browser_click",
            json!({"id":"tab","locator":{"text":"x".repeat(201)}}),
            false,
        ),
        ("browser_click", json!({"locator":{"text":"Save"}}), false),
        (
            "browser_fill",
            json!({"id":"tab","locator":{"label":"Email"},"text":""}),
            true,
        ),
        (
            "browser_press",
            json!({"id":"tab","locator":{"testId":"submit"},"key":"Enter"}),
            true,
        ),
        ("browser_wait", json!({"id":"tab","state":"ready"}), true),
        (
            "browser_wait",
            json!({"id":"tab","state":"detached","element":"element-1"}),
            true,
        ),
        (
            "browser_wait",
            json!({"id":"tab","state":"visible","locator":{"text":"Saved"},"timeoutMs":0}),
            true,
        ),
        ("browser_wait", json!({"id":"tab","state":"stable"}), false),
        ("browser_wait", json!({"id":"tab"}), false),
        (
            "browser_wait",
            json!({"id":"tab","state":"ready","timeoutMs":15001}),
            false,
        ),
        (
            "browser_wait",
            json!({"id":"tab","state":"ready","timeoutMs":1.5}),
            false,
        ),
    ] {
        let tool = tools.iter().find(|tool| tool["name"] == name).unwrap();
        let validator = jsonschema::validator_for(&tool["parameters"]).unwrap();
        assert_eq!(validator.is_valid(&args), valid, "{name}: {args}");
    }
}
