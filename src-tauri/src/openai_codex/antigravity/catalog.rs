//! A model can have several upstream IDs selected by reasoning effort. Keep
//! that routing private while presenting one model in the shared selector.
use serde_json::{json, Value};
use std::collections::BTreeMap;

type Models = BTreeMap<String, Value>;

fn family(
    models: &mut Models,
    id: &str,
    name: &str,
    routes: &[(&str, &str)],
    retired: &[&str],
    mode: &str,
) {
    let routes: BTreeMap<String, Value> = routes
        .iter()
        .filter(|(_, wire)| models.contains_key(*wire))
        .map(|(effort, wire)| ((*effort).into(), json!(wire)))
        .collect();
    for wire in retired {
        models.remove(*wire);
    }
    let Some(first) = routes.values().next().and_then(Value::as_str) else {
        return;
    };
    let Some(mut metadata) = models.get(first).cloned() else {
        return;
    };
    let wires: Models = routes
        .values()
        .filter_map(Value::as_str)
        .filter_map(|wire| models.get(wire).map(|value| (wire.into(), value.clone())))
        .collect();
    for wire in wires.keys() {
        models.remove(wire);
    }
    metadata["displayName"] = json!(name);
    metadata["supportsThinking"] = json!(routes.keys().any(|effort| effort != "none"));
    metadata["_routes"] = json!(routes);
    metadata["_wire_metadata"] = json!(wires);
    metadata["_thinking_mode"] = json!(mode);
    models.insert(id.into(), metadata);
}

pub(super) fn models(payload: &Value) -> Option<Models> {
    let raw = payload["models"].as_object()?;
    let mut models: Models = raw
        .iter()
        .filter(|(id, v)| {
            v.is_object()
                && v["isInternal"] != true
                && !["chat_20706", "chat_23310", "gemini-2.5-pro"].contains(&id.as_str())
                && !id.is_empty()
                && id.len() <= 200
        })
        .map(|(id, v)| (id.clone(), v.clone()))
        .collect();
    family(
        &mut models,
        "gemini-3.1-pro",
        "Gemini 3.1 Pro",
        &[("low", "gemini-3.1-pro-low"), ("high", "gemini-pro-agent")],
        &["gemini-3.1-pro-high"],
        "budget",
    );
    family(
        &mut models,
        "gemini-3-pro",
        "Gemini 3 Pro",
        &[("low", "gemini-3-pro-low"), ("high", "gemini-3-pro-high")],
        &[],
        "level",
    );
    family(
        &mut models,
        "gemini-3.5-flash",
        "Gemini 3.5 Flash",
        &[
            ("low", "gemini-3.5-flash-extra-low"),
            ("medium", "gemini-3.5-flash-low"),
            ("high", "gemini-3-flash-agent"),
        ],
        &[],
        "budget",
    );
    family(
        &mut models,
        "gpt-oss-120b",
        "GPT-OSS 120B",
        &[
            ("low", "gpt-oss-120b-medium"),
            ("medium", "gpt-oss-120b-medium"),
            ("high", "gpt-oss-120b-medium"),
        ],
        &[],
        "budget",
    );
    family(
        &mut models,
        "claude-sonnet-4-6",
        "Claude Sonnet 4.6",
        &[
            ("low", "claude-sonnet-4-6"),
            ("medium", "claude-sonnet-4-6"),
            ("high", "claude-sonnet-4-6"),
        ],
        &["claude-sonnet-4-6-thinking"],
        "budget",
    );
    family(
        &mut models,
        "claude-opus-4-6",
        "Claude Opus 4.6",
        &[
            ("low", "claude-opus-4-6-thinking"),
            ("medium", "claude-opus-4-6-thinking"),
            ("high", "claude-opus-4-6-thinking"),
        ],
        &["claude-opus-4-6"],
        "budget",
    );
    let ids: Vec<_> = models.keys().cloned().collect();
    for id in ids {
        let Some((prefix, _)) = id.rsplit_once('-') else {
            continue;
        };
        let Some(revision) = prefix
            .strip_prefix("gemini-")
            .and_then(|s| s.strip_suffix("-flash"))
        else {
            continue;
        };
        let parts: Vec<_> = revision
            .split('.')
            .filter_map(|v| v.parse::<u32>().ok())
            .collect();
        if parts.is_empty()
            || parts[0] < 3
            || (parts[0] == 3 && parts.get(1).copied().unwrap_or(0) < 6)
        {
            continue;
        }
        let low = format!("{prefix}-low");
        let medium = format!("{prefix}-medium");
        let high = format!("{prefix}-high");
        let tiered = format!("{prefix}-tiered");
        let fallback = models.contains_key(&tiered);
        let routes = [
            (
                "low",
                if models.contains_key(&low) {
                    low.as_str()
                } else if fallback {
                    tiered.as_str()
                } else {
                    low.as_str()
                },
            ),
            (
                "medium",
                if models.contains_key(&medium) {
                    medium.as_str()
                } else if fallback {
                    tiered.as_str()
                } else {
                    medium.as_str()
                },
            ),
            (
                "high",
                if models.contains_key(&high) {
                    high.as_str()
                } else if fallback {
                    tiered.as_str()
                } else {
                    high.as_str()
                },
            ),
        ];
        family(
            &mut models,
            prefix,
            &format!("Gemini {revision} Flash"),
            &routes,
            &[],
            "level",
        );
        if models.contains_key(prefix) {
            models.remove(&tiered);
        }
    }
    // New bare/thinking pairs remain discoverable without adding a hardcoded model.
    let ids: Vec<_> = models.keys().cloned().collect();
    for id in ids {
        let Some(bare) = id.strip_suffix("-thinking") else {
            continue;
        };
        if !models.contains_key(bare) {
            continue;
        }
        let name = models[bare]["displayName"]
            .as_str()
            .unwrap_or(bare)
            .to_owned();
        family(
            &mut models,
            bare,
            &name,
            &[("none", bare), ("low", &id), ("medium", &id), ("high", &id)],
            &[],
            "budget",
        );
    }
    Some(models)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_collapses_live_tiers_and_removes_retired_routes() {
        let models=models(&json!({"models":{
            "gemini-3.7-flash-low":{"supportsThinking":true},"gemini-3.7-flash-high":{"supportsThinking":true},
            "gemini-3.1-pro-low":{},"gemini-3.1-pro-high":{},"gemini-pro-agent":{},
            "claude-sonnet-4-6":{},"claude-sonnet-4-6-thinking":{},
            "claude-new":{"displayName":"Claude New"},"claude-new-thinking":{"supportsThinking":true}
        }})).unwrap();
        assert_eq!(models.len(), 4);
        assert_eq!(
            models["gemini-3.7-flash"]["_routes"]["high"],
            "gemini-3.7-flash-high"
        );
        assert!(models["gemini-3.7-flash"]["_routes"]
            .get("medium")
            .is_none());
        assert_eq!(
            models["gemini-3.1-pro"]["_routes"]["high"],
            "gemini-pro-agent"
        );
        assert_eq!(
            models["claude-sonnet-4-6"]["_routes"]["high"],
            "claude-sonnet-4-6"
        );
        assert_eq!(models["claude-new"]["_routes"]["none"], "claude-new");
    }
}
