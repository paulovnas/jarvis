//! Read-only native progress projection and two conversation-owned UI intents.
use super::{address, evaluate, label, BrowserState};
use crate::agent::AgentError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

const ADAPTER: &str = include_str!("../../../resources/impeccable/jarvis-live-adapter.js");

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Projection {
    conversation_id: String,
    tab_id: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    session_ids: Vec<String>,
    state: String,
    phase: String,
    action_id: Option<String>,
    error: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    step: Option<usize>,
    #[serde(default)]
    activity: Option<String>,
    #[serde(default)]
    elapsed_ms: u64,
    #[serde(default)]
    queue_count: usize,
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    feedback: Option<Feedback>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Intent {
    request_id: String,
    action: Action,
    conversation_id: String,
    tab_id: String,
    action_id: Option<String>,
    revision: u64,
}
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Action {
    Cancel,
    Resume,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Feedback {
    request_id: String,
    error: Option<String>,
}
struct Entry {
    origin: String,
    data: Mutex<Projection>,
    pending: Mutex<Option<(String, Instant)>>,
    seen: Mutex<VecDeque<String>>,
}
impl Entry {
    fn admit(&self, request_id: &str) -> bool {
        let Ok(mut pending) = self.pending.lock() else {
            return false;
        };
        let Ok(mut seen) = self.seen.lock() else {
            return false;
        };
        if pending.is_some() || seen.iter().any(|previous| previous == request_id) {
            return false;
        }
        seen.push_back(request_id.into());
        if seen.len() > 32 {
            seen.pop_front();
        }
        *pending = Some((request_id.into(), Instant::now()));
        true
    }
}
#[derive(Default)]
pub(super) struct State {
    entries: Mutex<BTreeMap<String, Arc<Entry>>>,
}

fn local(value: &str) -> bool {
    address(value).ok().is_some_and(|url| match url.host() {
        Some(url::Host::Domain(host)) => host == "localhost",
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    })
}
pub(crate) fn same_origin(initial: &str, current: &str) -> bool {
    match (address(initial), address(current)) {
        (Ok(initial), Ok(current)) => {
            local(initial.as_str()) && initial.origin() == current.origin()
        }
        _ => false,
    }
}
fn valid(intent: &Intent, current: &Projection) -> bool {
    !intent.request_id.is_empty()
        && intent.request_id.len() <= 128
        && intent.conversation_id == current.conversation_id
        && current.tab_id.as_deref() == Some(&intent.tab_id)
        && intent.action_id == current.action_id
        && intent.revision <= current.revision
        && match intent.action {
            Action::Cancel => {
                matches!(current.phase.as_str(), "waiting" | "running")
                    && current.action_id.is_some()
            }
            Action::Resume => {
                current.phase == "blocked" || current.state == "setup" || current.error.is_some()
            }
        }
}
fn bounded(value: &mut Option<String>, limit: usize) {
    if let Some(text) = value {
        *text = text.chars().take(limit).collect();
    }
}

pub(crate) fn design_changed(app: &tauri::AppHandle, conversation: &str, tab: &str) {
    let state = app.state::<BrowserState>();
    let Ok(owned) = state.tab(app, conversation, tab) else {
        return;
    };
    let Some(view) = app.get_webview(&label(tab)) else {
        return;
    };
    if view
        .url()
        .ok()
        .is_some_and(|url| same_origin(&owned.url, url.as_str()))
    {
        let _ = view.eval("window.dispatchEvent(new Event('jarvis-live:design-changed'))");
    }
}

/// Called only by the native Live owner. No arbitrary page or renderer payload is accepted.
pub(crate) fn project(app: &tauri::AppHandle, status: &Value) {
    let Ok(mut projection) = serde_json::from_value::<Projection>(status.clone()) else {
        return;
    };
    let state = app.state::<BrowserState>();
    let Ok(mut entries) = state.live.entries.lock() else {
        return;
    };
    let existing = entries.iter().find_map(|(id, entry)| {
        entry
            .data
            .lock()
            .ok()
            .filter(|data| data.conversation_id == projection.conversation_id)
            .map(|_| id.clone())
    });
    if projection.state == "off" {
        if let Some(id) = existing {
            entries.remove(&id);
            if let Some(view) = app.get_webview(&label(&id)) {
                let _ = view.eval("window.__JARVIS_IMPECCABLE_LIVE__?.dispose()");
            }
        }
        return;
    }
    let Some(tab) = projection.tab_id.clone().or(existing) else {
        return;
    };
    if state.tab(app, &projection.conversation_id, &tab).is_err() {
        return;
    }
    let Some(origin) = projection.url.as_deref().filter(|url| local(url)) else {
        return;
    };
    let origin = origin.to_owned();
    projection.session_ids = status["tasks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|task| !matches!(task["status"].as_str(), Some("cancelled" | "paused")))
        .filter_map(|task| task["actionId"].as_str().map(str::to_owned))
        .collect();
    projection.tab_id = Some(tab.clone());
    bounded(&mut projection.model, 160);
    bounded(&mut projection.provider, 80);
    bounded(&mut projection.activity, 200);
    bounded(&mut projection.error, 1000);
    if let Some(entry) = entries.get(&tab) {
        if let Ok(mut data) = entry.data.lock() {
            projection.revision = data.revision.saturating_add(1);
            projection.feedback = data.feedback.clone();
            *data = projection;
        }
        return;
    }
    projection.revision = 1;
    let entry = Arc::new(Entry {
        origin,
        data: Mutex::new(projection),
        pending: Mutex::new(None),
        seen: Mutex::new(VecDeque::new()),
    });
    entries.insert(tab.clone(), Arc::clone(&entry));
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        pump(&app, &tab, &entry).await;
    });
}

async fn pump(app: &tauri::AppHandle, tab: &str, entry: &Arc<Entry>) {
    loop {
        let owned = app
            .state::<BrowserState>()
            .live
            .entries
            .lock()
            .ok()
            .is_some_and(|entries| {
                entries
                    .get(tab)
                    .is_some_and(|value| Arc::ptr_eq(value, entry))
            });
        if !owned {
            break;
        }
        let Some(view) = app.get_webview(&label(tab)) else {
            break;
        };
        let Some(mut data) = entry.data.lock().ok().map(|data| data.clone()) else {
            break;
        };
        let Ok(owned_tab) = app
            .state::<BrowserState>()
            .tab(app, &data.conversation_id, tab)
        else {
            break;
        };
        let current_url = view.url().ok();
        let Some(current_url) = current_url.filter(|url| same_origin(&entry.origin, url.as_str()))
        else {
            // OAuth may leave the site temporarily. Retain ownership, without projecting into it.
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        };
        if owned_tab.url != current_url.as_str() {
            super::page_changed(app, &data.conversation_id, tab, |tab| {
                tab.url = current_url.to_string();
            });
        }
        crate::core::impeccable_live::follow_page(
            app,
            &data.conversation_id,
            tab,
            current_url.as_str(),
        );
        data.url = Some(current_url.to_string());
        if let Ok(mut pending) = entry.pending.lock() {
            if pending
                .as_ref()
                .is_some_and(|(_, time)| time.elapsed() >= Duration::from_secs(25))
            {
                let request_id = pending.take().map(|value| value.0).unwrap_or_default();
                data.feedback = Some(Feedback {
                    request_id,
                    error: Some("O Jarvis não confirmou a ação. Tente novamente pelo chat.".into()),
                });
                if let Ok(mut saved) = entry.data.lock() {
                    saved.feedback = data.feedback.clone();
                }
            }
        }
        let script = format!("(() => {{ if (location.origin !== {}) return []; if (!window.__JARVIS_IMPECCABLE_LIVE__) {{ {ADAPTER} }} const adapter = window.__JARVIS_IMPECCABLE_LIVE__; adapter.update({}); return adapter.drain(); }})()", json!(current_url.origin().ascii_serialization()), json!(data));
        if let Ok(value) = evaluate(&view, script).await {
            if let Some(request) = value
                .as_array()
                .filter(|requests| requests.len() <= 1)
                .and_then(|requests| requests.first())
            {
                if let Ok(intent) = serde_json::from_value::<Intent>(request.clone()) {
                    let current = entry.data.lock().ok().map(|data| data.clone());
                    if current
                        .as_ref()
                        .is_some_and(|current| valid(&intent, current))
                    {
                        let admitted = entry.admit(&intent.request_id);
                        if admitted {
                            let _ = app.emit_to(
                                tauri::EventTarget::webview("main"),
                                "impeccable-live:action",
                                intent,
                            );
                        }
                    } else if let Some(current) = current.filter(|current| {
                        current.conversation_id == intent.conversation_id
                            && current.tab_id.as_deref() == Some(&intent.tab_id)
                    }) {
                        if let Ok(mut data) = entry.data.lock() {
                            data.feedback = Some(Feedback { request_id: intent.request_id, error: Some("Esta solicitação Live mudou. Use os controles da alteração atual.".into()) });
                            data.revision = current.revision.saturating_add(1);
                        }
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    if let Ok(mut entries) = app.state::<BrowserState>().live.entries.lock() {
        if entries
            .get(tab)
            .is_some_and(|value| Arc::ptr_eq(value, entry))
        {
            entries.remove(tab);
        }
    }
}

/// The trusted main renderer acknowledges only a previously admitted toolbar intent.
#[tauri::command]
pub fn impeccable_live_action_feedback(
    app: tauri::AppHandle,
    conversation_id: String,
    tab_id: String,
    request_id: String,
    error: Option<String>,
) -> Result<(), AgentError> {
    let state = app.state::<BrowserState>();
    let entries = state
        .live
        .entries
        .lock()
        .map_err(|_| AgentError::internal())?;
    let entry = entries
        .get(&tab_id)
        .ok_or_else(|| super::error("A aba Live já foi fechada."))?;
    let mut pending = entry.pending.lock().map_err(|_| AgentError::internal())?;
    let mut data = entry.data.lock().map_err(|_| AgentError::internal())?;
    if data.conversation_id != conversation_id {
        return Err(AgentError::cancelled());
    }
    if !pending
        .as_ref()
        .is_some_and(|pending| pending.0 == request_id)
    {
        return Err(AgentError::cancelled());
    }
    pending.take();
    data.feedback = Some(Feedback {
        request_id,
        error: error.map(|text| text.chars().take(600).collect()),
    });
    data.revision = data.revision.saturating_add(1);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn current() -> Projection {
        serde_json::from_value(json!({"conversationId":"c1","tabId":"b1","state":"ready","phase":"running","actionId":"a1","error":null,"revision":4})).unwrap()
    }
    fn intent() -> Intent {
        serde_json::from_value(json!({"conversationId":"c1","tabId":"b1","actionId":"a1","revision":4,"requestId":"request-1","action":"cancel"})).unwrap()
    }
    #[test]
    fn toolbar_can_only_control_current_local_live_action() {
        let mut request = intent();
        let mut state = current();
        assert!(valid(&request, &state));
        request.tab_id = "other".into();
        assert!(!valid(&request, &state));
        request.tab_id = "b1".into();
        request.conversation_id = "other".into();
        assert!(!valid(&request, &state));
        request.conversation_id = "c1".into();
        request.action_id = Some("old".into());
        assert!(!valid(&request, &state));
        request.action_id = Some("a1".into());
        request.action = Action::Resume;
        assert!(!valid(&request, &state));
        state.phase = "blocked".into();
        assert!(valid(&request, &state));
        request.revision = 5;
        assert!(!valid(&request, &state));
        assert!(local("http://localhost:3001/"));
        assert!(local("http://[::1]:3001/"));
        assert!(!local("https://example.com"));
    }
    #[test]
    fn projection_does_not_expose_arbitrary_native_fields_or_page_arguments() {
        let mut value = json!(current());
        value["options"] = json!({"account":"secret"});
        value["user"] = json!("private text");
        let projection: Projection = serde_json::from_value(value).unwrap();
        let safe = json!(projection);
        assert!(safe.get("options").is_none());
        assert!(safe.get("user").is_none());
        let mut request = json!(intent());
        request["options"] = json!({"model":"unexpected"});
        assert!(serde_json::from_value::<Intent>(request).is_err());
    }
    #[test]
    fn toolbar_intents_are_admitted_once_without_overlapping_requests() {
        let entry = Entry {
            origin: "http://localhost:3001/login".into(),
            data: Mutex::new(current()),
            pending: Mutex::new(None),
            seen: Mutex::new(VecDeque::new()),
        };
        assert!(entry.admit("request-1"));
        assert!(!entry.admit("request-2"));
        entry.pending.lock().unwrap().take();
        assert!(!entry.admit("request-1"));
        assert!(entry.admit("request-2"));
    }
    #[test]
    fn navigation_keeps_the_site_scope_across_paths_queries_and_auth_return() {
        let site = "http://localhost:3001/login";
        assert!(same_origin(
            site,
            "http://localhost:3001/dashboard?from=today#chart"
        ));
        assert!(!same_origin(site, "https://accounts.google.com/login"));
        assert!(same_origin(site, "http://localhost:3001/history"));
        for other in [
            "http://localhost:3002/dashboard",
            "https://localhost:3001/dashboard",
            "http://127.0.0.1:3001/dashboard",
        ] {
            assert!(!same_origin(site, other));
        }
    }
}
