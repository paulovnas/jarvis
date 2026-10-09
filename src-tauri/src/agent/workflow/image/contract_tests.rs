use super::super::{contracts, Flow, Role};

#[test]
fn image_generation_reuses_confirmed_project_evidence_before_discovery() {
    for id in ["main", "child"] {
        let prompt = contracts::prompt(Flow::ImageGenerator, Role::ImageGenerator, id);
        assert!(prompt.contains(
            "Reuse confirmed product facts, brand decisions, assets and attachment IDs from this conversation or handoff"
        ));
        assert!(prompt.contains(
            "refresh only missing, stale or conflicting evidence that affects the requested output"
        ));
        assert!(prompt.contains(
            "Once the required evidence is available, create the requested asset without restarting discovery"
        ));
    }
}

#[test]
fn image_generation_requires_supported_delivery_without_permission_bypasses() {
    for id in ["main", "child"] {
        let prompt = contracts::prompt(Flow::ImageGenerator, Role::ImageGenerator, id);
        assert!(prompt.contains(
            "Do not offer HTML, PDF or shell rendering unless the current native tool catalog actually supports the requested output"
        ));
        assert!(prompt.contains("A project MCP board organizes work; it is not a renderer"));
        assert!(
            prompt.contains("Do not bypass unavailable file tools by changing the live app DOM")
        );
        assert!(prompt.contains("without repeating an uncertain published action"));
        assert!(prompt.contains(
            "If an optional format or export is unsupported, still deliver the supported native image"
        ));
    }
    for flow in [Flow::ImageGenerator, Flow::Custom] {
        for tool in ["generate_image", "image_process", "read_attachment"] {
            assert!(Role::ImageGenerator.allows(flow, tool, true), "{tool}");
        }
        for tool in ["write", "edit", "apply_patch", "bash", "ctx_exec"] {
            assert!(!Role::ImageGenerator.allows(flow, tool, true), "{tool}");
        }
    }
}
