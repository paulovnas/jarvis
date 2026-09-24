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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "CoreActivity"))]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    #[cfg_attr(
        test,
        ts(
            type = "\"context-mode\" | \"ponytail\" | \"beads\" | \"open-design\" | \"context7\" | \"lsp\""
        )
    )]
    pub component: ComponentId,
    pub action: String,
    pub status: Status,
    pub summary: String,
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[cfg_attr(test, ts(type = "number"))]
    pub duration_ms: u64,
}

impl Activity {
    pub fn new(component: ComponentId, action: &str, summary: &str) -> Self {
        Self {
            component,
            action: action.into(),
            status: Status::Applied,
            summary: summary.into(),
            sources: Vec::new(),
            fingerprint: None,
            duration_ms: 0,
        }
    }

    pub fn unavailable(component: ComponentId, action: &str, summary: &str) -> Self {
        Self {
            status: Status::Unavailable,
            ..Self::new(component, action, summary)
        }
    }
}
