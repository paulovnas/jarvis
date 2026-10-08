//! Small, durable receipts for work performed by the host rather than the model.
use super::ComponentId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "CoreActivityStatus"))]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Applied,
    Reused,
    Unavailable,
    Pending,
    Issues,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivityComponent {
    #[serde(rename = "manual-hooks")]
    ManualHooks,
    #[serde(rename = "hooks")]
    Hooks,
    #[serde(rename = "plugins")]
    Plugins,
    #[serde(untagged)]
    Core(ComponentId),
}

impl From<ComponentId> for ActivityComponent {
    fn from(component: ComponentId) -> Self {
        Self::Core(component)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "CoreActivity"))]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    #[cfg_attr(
        test,
        ts(
            type = "\"context-mode\" | \"ponytail\" | \"beads\" | \"open-design\" | \"context7\" | \"lsp\" | \"hyperframes\" | \"audiovisual\" | \"comfyui\" | \"graft\" | \"manual-hooks\" | \"hooks\" | \"plugins\""
        )
    )]
    pub component: ActivityComponent,
    pub action: String,
    pub status: Status,
    pub summary: String,
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[cfg_attr(test, ts(type = "number"))]
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub resource_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub resource_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub plugin_id: Option<String>,
}

impl Activity {
    pub fn new(component: ComponentId, action: &str, summary: &str) -> Self {
        Self {
            component: component.into(),
            action: action.into(),
            status: Status::Applied,
            summary: summary.into(),
            sources: Vec::new(),
            fingerprint: None,
            duration_ms: 0,
            resource_id: None,
            resource_name: None,
            plugin_id: None,
        }
    }

    pub fn unavailable(component: ComponentId, action: &str, summary: &str) -> Self {
        Self {
            status: Status::Unavailable,
            ..Self::new(component, action, summary)
        }
    }

    #[cfg(test)]
    pub fn manual_hook(action: &str, summary: &str) -> Self {
        Self {
            component: ActivityComponent::ManualHooks,
            action: action.into(),
            status: Status::Issues,
            summary: summary.into(),
            sources: Vec::new(),
            fingerprint: None,
            duration_ms: 0,
            resource_id: None,
            resource_name: None,
            plugin_id: None,
        }
    }

    pub(crate) fn hook(
        action: &str,
        summary: &str,
        id: &str,
        name: &str,
        plugin_id: Option<String>,
    ) -> Self {
        Self::resource(
            ActivityComponent::Hooks,
            action,
            summary,
            id,
            name,
            plugin_id,
        )
    }

    pub(crate) fn plugin(
        action: &str,
        summary: &str,
        id: &str,
        name: &str,
        plugin_id: &str,
    ) -> Self {
        Self::resource(
            ActivityComponent::Plugins,
            action,
            summary,
            id,
            name,
            Some(plugin_id.into()),
        )
    }

    fn resource(
        component: ActivityComponent,
        action: &str,
        summary: &str,
        id: &str,
        name: &str,
        plugin_id: Option<String>,
    ) -> Self {
        Self {
            component,
            action: action.into(),
            status: Status::Applied,
            summary: summary.into(),
            sources: Vec::new(),
            fingerprint: None,
            duration_ms: 0,
            resource_id: Some(id.into()),
            resource_name: Some(name.into()),
            plugin_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn resource_activity_round_trips_optional_identity_without_changing_legacy_receipts() {
        let hook = Activity::hook(
            "PreToolUse",
            "Hook executado.",
            "hook-1",
            "Guard",
            Some("guard@local".into()),
        );
        let value = serde_json::to_value(&hook).unwrap();
        assert_eq!(value["component"], "hooks");
        assert_eq!(value["resourceId"], "hook-1");
        assert_eq!(value["resourceName"], "Guard");
        assert_eq!(value["pluginId"], "guard@local");
        let restored: Activity = serde_json::from_value(value).unwrap();
        assert_eq!(restored.plugin_id.as_deref(), Some("guard@local"));
        let plugin = Activity::plugin(
            "skill_loaded",
            "Skill carregada.",
            "skill-1",
            "guidance",
            "guard@local",
        );
        let restored: Activity =
            serde_json::from_value(serde_json::to_value(plugin).unwrap()).unwrap();
        assert_eq!(restored.component, ActivityComponent::Plugins);
        let legacy = serde_json::to_value(Activity::new(
            ComponentId::ContextMode,
            "prepare",
            "Preparado",
        ))
        .unwrap();
        for field in ["resourceId", "resourceName", "pluginId"] {
            assert!(legacy.get(field).is_none());
        }
        let mut nullable = legacy;
        nullable["pluginId"] = Value::Null;
        let restored: Activity = serde_json::from_value(nullable).unwrap();
        assert!(restored.plugin_id.is_none());
    }

    #[test]
    fn core_receipts_retain_their_existing_wire_format() {
        for component in ComponentId::ALL {
            let activity = Activity::new(component, "prepare", "Referências preparadas");
            let value = serde_json::to_value(&activity).unwrap();
            assert_eq!(value["component"], serde_json::to_value(component).unwrap());
            let restored: Activity = serde_json::from_value(value).unwrap();
            assert_eq!(restored.component, ActivityComponent::Core(component));
            assert_eq!(restored.status, Status::Applied);
        }
    }

    #[test]
    fn manual_hook_diagnostics_round_trip_without_becoming_installable_cores() {
        let activity = Activity::manual_hook("PreToolUse", "O hook não concluiu: tempo esgotado.");
        let value = serde_json::to_value(&activity).unwrap();
        assert_eq!(
            value,
            json!({
                "component": "manual-hooks",
                "action": "PreToolUse",
                "status": "issues",
                "summary": "O hook não concluiu: tempo esgotado.",
                "sources": [],
                "durationMs": 0
            })
        );
        let restored: Activity = serde_json::from_value(value).unwrap();
        assert_eq!(restored.component, ActivityComponent::ManualHooks);
        assert_eq!(restored.status, Status::Issues);
        assert!(serde_json::from_value::<ComponentId>(json!("manual-hooks")).is_err());
        assert!(serde_json::from_value::<ActivityComponent>(json!("unknown")).is_err());
    }
}
