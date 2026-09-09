const ANTIGRAVITY: &str = r#"
Antigravity execution policy: keep the work evidence-driven and tool-efficient. Use Context-mode retrieval before broad repeated reads, use LSP definition/reference/symbol tools for code navigation when the project has a language server, and prefer apply_patch for one coherent multi-file mutation. Reuse existing browser snapshots, screenshots and indexed excerpts until the underlying page or question changes. If a preferred tool is unavailable, state the limitation once and use the smallest reliable fallback; do not loop on the same request.
"#;

const GEMINI: &str = r#"
Gemini operational guidance: translate the request into a short sequence of observable outcomes, then act on the strongest available evidence. Before another exploratory read, search, screenshot or navigation, check whether Context-mode or the current tool history already contains the answer. After a focused implementation and relevant checks succeed, stop exploring and deliver the result.
"#;

pub(super) fn append(target: &mut String, antigravity: bool, model: &str) {
    if !antigravity {
        return;
    }
    target.push_str(ANTIGRAVITY);
    if model.to_ascii_lowercase().contains("gemini") {
        target.push_str(GEMINI);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antigravity_gemini_receives_both_operational_overlays() {
        let mut prompt = "base".to_owned();
        append(&mut prompt, true, "gemini-3.1-pro");
        assert!(prompt.contains("Antigravity execution policy"));
        assert!(prompt.contains("Gemini operational guidance"));
        assert!(prompt.contains("apply_patch"));
        assert!(prompt.contains("Context-mode"));
    }

    #[test]
    fn antigravity_third_party_only_receives_provider_overlay() {
        let mut prompt = String::new();
        append(&mut prompt, true, "claude-sonnet-4.6");
        assert!(prompt.contains("Antigravity execution policy"));
        assert!(!prompt.contains("Gemini operational guidance"));
    }

    #[test]
    fn other_providers_keep_the_shared_prompt_unchanged() {
        let mut prompt = "shared cacheable prefix".to_owned();
        append(&mut prompt, false, "gemini-custom");
        assert_eq!(prompt, "shared cacheable prefix");
    }
}
