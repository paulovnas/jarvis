//! Page-scoped Chromium protocol. One observer per native browser; no remote debugging port.
mod actions;

use super::{error, AgentError, BrowserRequest, BrowserState};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{Manager, Webview};
use tauri_runtime_cef::{allocate_devtools_message_id, DevToolsProtocol, WebviewCefExt};
use tokio::sync::oneshot;

const BUFFER_LIMIT: usize = 200;
const RESULT_LIMIT: usize = 2 * 1024 * 1024;

#[derive(Default)]
pub(super) struct State(Mutex<BTreeMap<String, Arc<Session>>>);

struct Pending {
    limit: usize,
    reply: oneshot::Sender<Result<Value, AgentError>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Frame {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_id: Option<String>,
    #[serde(skip)]
    session_id: Option<String>,
    url: String,
    name: String,
    available: bool,
}

#[derive(Default)]
struct Inner {
    pending: BTreeMap<i32, Pending>,
    network: VecDeque<Value>,
    sessions: BTreeMap<String, Frame>,
    enabled_sessions: BTreeSet<String>,
    context_generation: u64,
    contexts: BTreeMap<String, i64>,
    elements: BTreeMap<String, Frame>,
}

#[derive(Default)]
pub(super) struct Session {
    inner: Mutex<Inner>,
    enabled: tokio::sync::OnceCell<()>,
}

pub(super) fn session(view: &Webview) -> Result<Arc<Session>, AgentError> {
    let state = view.app_handle().state::<BrowserState>();
    let mut sessions = state.cdp.0.lock().map_err(|_| AgentError::internal())?;
    if let Some(session) = sessions.get(view.label()) {
        return Ok(session.clone());
    }
    let session = Arc::new(Session::default());
    let observer = Arc::downgrade(&session);
    view.on_dev_tools_protocol(move |protocol| {
        if let Some(session) = observer.upgrade() {
            session.observe(protocol);
        }
    })
    .map_err(|_| error("Não foi possível conectar as ferramentas do Chromium à aba."))?;
    sessions.insert(view.label().to_owned(), session.clone());
    Ok(session)
}

pub(super) fn forget(state: &BrowserState, label: &str) {
    if let Ok(mut sessions) = state.cdp.0.lock() {
        sessions.remove(label);
    }
}

struct PendingGuard<'a> {
    session: &'a Session,
    id: i32,
}
impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut inner) = self.session.inner.lock() {
            inner.pending.remove(&self.id);
        }
    }
}

impl Session {
    pub(super) async fn enable(&self, view: &Webview) -> Result<(), AgentError> {
        self.enabled
            .get_or_try_init(|| async {
                for method in ["Runtime.enable", "Page.enable"] {
                    self.call(view, method, json!({}), None).await?;
                }
                self.call(
                    view,
                    "Network.enable",
                    json!({"maxTotalBufferSize":4_000_000,"maxResourceBufferSize":1_000_000}),
                    None,
                )
                .await?;
                Ok::<_, AgentError>(())
            })
            .await
            .map(|_| ())
    }

    async fn call(
        &self,
        view: &Webview,
        method: &str,
        params: Value,
        session_id: Option<&str>,
    ) -> Result<Value, AgentError> {
        self.request(
            view,
            method,
            params,
            session_id,
            Duration::from_secs(10),
            RESULT_LIMIT,
        )
        .await
    }

    pub(super) async fn request(
        &self,
        view: &Webview,
        method: &str,
        params: Value,
        session_id: Option<&str>,
        timeout: Duration,
        limit: usize,
    ) -> Result<Value, AgentError> {
        let id = allocate_devtools_message_id().map_err(|_| {
            error("O Chromium esgotou os identificadores de comandos. Reabra o Jarvis.")
        })?;
        let (reply, received) = oneshot::channel();
        self.inner
            .lock()
            .map_err(|_| AgentError::internal())?
            .pending
            .insert(id, Pending { limit, reply });
        let _guard = PendingGuard { session: self, id };
        let mut message = json!({"id":id,"method":method,"params":params});
        if let Some(session_id) = session_id {
            message["sessionId"] = json!(session_id);
        }
        view.send_dev_tools_message(message.to_string().as_bytes())
            .map_err(|_| error("Não foi possível enviar o comando ao Chromium."))?;
        tokio::time::timeout(timeout, received).await
            .map_err(|_| AgentError::new("browser_outcome_unknown", "O Chromium não confirmou o resultado a tempo. Inspecione a página antes de repetir uma ação."))?
            .map_err(|_| error("A aba foi fechada durante o comando."))?
    }

    fn answer(&self, id: i32, success: bool, bytes: &[u8]) {
        let pending = self
            .inner
            .lock()
            .ok()
            .and_then(|mut inner| inner.pending.remove(&id));
        if let Some(pending) = pending {
            let result = protocol_result(success, bytes, pending.limit);
            let _ = pending.reply.send(result);
        }
    }

    fn observe(&self, protocol: DevToolsProtocol) {
        match protocol {
            DevToolsProtocol::MethodResult {
                message_id,
                success,
                result,
            } => self.answer(message_id, success, &result),
            DevToolsProtocol::Message(bytes) if bytes.len() <= RESULT_LIMIT => {
                let Ok(message) = serde_json::from_slice::<Value>(&bytes) else {
                    return;
                };
                // Flattened iframe sessions retain their identity only in the raw envelope.
                if let Some(id) = message["id"].as_i64().and_then(|id| i32::try_from(id).ok()) {
                    if message["sessionId"].is_string() {
                        let success = message.get("error").is_none();
                        let value = message
                            .get(if success { "result" } else { "error" })
                            .cloned()
                            .unwrap_or_else(|| json!({}));
                        self.answer(id, success, value.to_string().as_bytes());
                    }
                } else if let Some(method) = message["method"].as_str() {
                    if let Ok(mut inner) = self.inner.lock() {
                        event(
                            &mut inner,
                            method,
                            &message["params"],
                            message["sessionId"].as_str(),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    async fn evaluate(
        &self,
        view: &Webview,
        expression: &str,
        frame: Option<&Frame>,
    ) -> Result<Value, AgentError> {
        let mut params = json!({"expression":expression,"returnByValue":true,"awaitPromise":true,"timeout":10000});
        let mut used_context = None;
        if let Some(frame) = frame {
            let (existing, generation) = {
                let inner = self.inner.lock().map_err(|_| AgentError::internal())?;
                (
                    inner.contexts.get(&frame.id).copied(),
                    inner.context_generation,
                )
            };
            let context = if let Some(context) = existing {
                context
            } else {
                let world = self
                    .call(
                        view,
                        "Page.createIsolatedWorld",
                        json!({"frameId":frame.id,"worldName":"jarvis-browser"}),
                        frame.session_id.as_deref(),
                    )
                    .await?;
                let context = world["executionContextId"]
                    .as_i64()
                    .ok_or_else(|| error("A página ainda não está pronta para inspeção."))?;
                self.cache_context(&frame.id, generation, context)?;
                context
            };
            params["contextId"] = json!(context);
            used_context = Some((&frame.id, context));
        }
        let result = self
            .call(
                view,
                "Runtime.evaluate",
                params,
                frame.and_then(|frame| frame.session_id.as_deref()),
            )
            .await;
        match result {
            Ok(result) => evaluation_result(&result),
            Err(cause) => {
                if let Some((frame, context)) = used_context {
                    self.forget_context(frame, context);
                }
                Err(cause)
            }
        }
    }

    fn cache_context(&self, frame: &str, generation: u64, context: i64) -> Result<(), AgentError> {
        let mut inner = self.inner.lock().map_err(|_| AgentError::internal())?;
        if inner.context_generation != generation {
            return Err(AgentError::new(
                "browser_stale_element",
                "A página mudou durante a inspeção. Capture um novo snapshot.",
            ));
        }
        inner.contexts.insert(frame.to_owned(), context);
        Ok(())
    }

    fn forget_context(&self, frame: &str, context: i64) {
        if let Ok(mut inner) = self.inner.lock() {
            // Another read may already have rebuilt this frame after navigation.
            if inner.contexts.get(frame) == Some(&context) {
                inner.contexts.remove(frame);
            }
        }
    }
}

fn protocol_result(success: bool, bytes: &[u8], limit: usize) -> Result<Value, AgentError> {
    if bytes.len() > limit {
        return Err(error("A resposta do Chromium excedeu o limite."));
    }
    let value: Value = if bytes.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(bytes).map_err(|_| error("Resposta inválida do Chromium."))?
    };
    if !success {
        let detail = value["message"]
            .as_str()
            .unwrap_or("O Chromium recusou este comando.");
        return Err(AgentError::new(
            "browser_command_failed",
            &detail.chars().take(1000).collect::<String>(),
        ));
    }
    Ok(value)
}

fn evaluation_result(result: &Value) -> Result<Value, AgentError> {
    if let Some(exception) = result.get("exceptionDetails") {
        let message = exception["exception"]["description"]
            .as_str()
            .or_else(|| exception["text"].as_str())
            .unwrap_or("Falha ao executar JavaScript.");
        return Err(AgentError::new(
            "browser_evaluation_failed",
            &message.chars().take(1000).collect::<String>(),
        ));
    }
    Ok(result["result"]
        .get("value")
        .cloned()
        .unwrap_or(Value::Null))
}

fn event(inner: &mut Inner, method: &str, params: &Value, session_id: Option<&str>) {
    if matches!(
        method,
        "Runtime.executionContextsCleared"
            | "Runtime.executionContextDestroyed"
            | "Page.frameNavigated"
            | "Page.navigatedWithinDocument"
    ) {
        inner.context_generation = inner.context_generation.wrapping_add(1);
        inner.contexts.clear();
        inner.elements.clear();
    }
    if method == "Target.attachedToTarget" && params["targetInfo"]["type"] == "iframe" {
        if let (Some(session), Some(id)) = (
            params["sessionId"].as_str(),
            params["targetInfo"]["targetId"].as_str(),
        ) {
            if inner.sessions.len() < 32 {
                let parent_id = session_id
                    .and_then(|parent| inner.sessions.get(parent))
                    .map(|parent| parent.id.clone());
                inner.sessions.insert(
                    session.to_owned(),
                    Frame {
                        id: id.to_owned(),
                        parent_id,
                        session_id: Some(session.to_owned()),
                        url: params["targetInfo"]["url"]
                            .as_str()
                            .unwrap_or_default()
                            .chars()
                            .take(4096)
                            .collect(),
                        name: String::new(),
                        available: true,
                    },
                );
            }
        }
    }
    if method == "Target.detachedFromTarget" {
        if let Some(session) = params["sessionId"].as_str() {
            inner.sessions.remove(session);
            inner.enabled_sessions.remove(session);
        }
        inner.context_generation = inner.context_generation.wrapping_add(1);
        inner.contexts.clear();
        inner.elements.clear();
    }
    let Some(request_id) = params["requestId"].as_str() else {
        return;
    };
    let id = session_id.map_or_else(
        || request_id.to_owned(),
        |session| format!("{session}:{request_id}"),
    );
    if id.len() > 160 {
        return;
    }
    if method == "Network.requestWillBeSent" {
        if let Some(index) = inner.network.iter().position(|item| item["id"] == id) {
            inner.network.remove(index);
        }
        inner.network.push_back(json!({"id":id,"url":params["request"]["url"].as_str().unwrap_or_default().chars().take(4096).collect::<String>(),"method":params["request"]["method"].as_str().unwrap_or_default().chars().take(40).collect::<String>(),"type":params["type"].as_str().unwrap_or_default().chars().take(40).collect::<String>()}));
        while inner.network.len() > BUFFER_LIMIT {
            inner.network.pop_front();
        }
    } else if let Some(item) = inner.network.iter_mut().find(|item| item["id"] == id) {
        if method == "Network.responseReceived" {
            if let Some(status) = params["response"]["status"].as_u64() {
                item["status"] = json!(status);
            }
        } else if method == "Network.loadingFailed" {
            item["failed"] = json!(params["errorText"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .take(500)
                .collect::<String>());
        }
    }
}

pub(super) async fn command(view: &Webview, request: &BrowserRequest) -> Result<Value, AgentError> {
    let session = session(view)?;
    session.enable(view).await?;
    match request.action.as_str() {
        "evaluate" => {
            let expression = request
                .expression
                .as_deref()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| error("Informe o JavaScript a executar."))?;
            session.evaluate(view, expression, None).await.map(bounded)
        }
        "devtools" => {
            let method = request.method.as_deref().unwrap_or_default();
            let params = request.params.clone().unwrap_or_else(|| json!({}));
            validate_method(method, &params)?;
            session.call(view, method, params, None).await.map(bounded)
        }
        "network" => {
            let inner = session.inner.lock().map_err(|_| AgentError::internal())?;
            let filter = request.filter.as_deref().unwrap_or_default().to_lowercase();
            let requests: Vec<_> = inner
                .network
                .iter()
                .filter(|item| {
                    filter.is_empty()
                        || format!(
                            "{} {} {}",
                            item["method"].as_str().unwrap_or_default(),
                            item["url"].as_str().unwrap_or_default(),
                            item["status"]
                        )
                        .to_lowercase()
                        .contains(&filter)
                })
                .cloned()
                .collect();
            let (offset, limit) = (
                request.offset.unwrap_or(0) as usize,
                request.limit.unwrap_or(30) as usize,
            );
            Ok(
                json!({"requests":requests.iter().skip(offset).take(limit).collect::<Vec<_>>(),"total":requests.len(),"offset":offset,"limit":limit}),
            )
        }
        "response_body" => {
            let id = request
                .request_id
                .as_deref()
                .ok_or_else(|| error("Informe requestId."))?;
            if !session
                .inner
                .lock()
                .map_err(|_| AgentError::internal())?
                .network
                .iter()
                .any(|item| item["id"] == id)
            {
                return Err(error(
                    "Esta requisição não está no buffer da aba. Consulte a rede novamente.",
                ));
            }
            let (session_id, request_id) = id
                .split_once(':')
                .map_or((None, id), |(session, id)| (Some(session), id));
            let response = session
                .call(
                    view,
                    "Network.getResponseBody",
                    json!({"requestId":request_id}),
                    session_id,
                )
                .await?;
            let body = response["body"].as_str().unwrap_or_default();
            Ok(
                json!({"body":body.chars().take(64000).collect::<String>(),"base64Encoded":response["base64Encoded"] == true,"truncated":body.chars().count()>64000,"totalCharacters":body.chars().count()}),
            )
        }
        "snapshot" | "wait" | "click" | "fill" | "press" => {
            actions::command(&session, view, request).await
        }
        _ => Err(error("Ação de Chromium desconhecida.")),
    }
}

fn bounded(value: Value) -> Value {
    let text = value.to_string();
    if text.len() <= 48000 {
        value
    } else {
        json!({"text":text.chars().take(48000).collect::<String>(),"truncated":true,"totalCharacters":text.chars().count()})
    }
}

fn validate_method(method: &str, params: &Value) -> Result<(), AgentError> {
    let domains = [
        "Accessibility",
        "CSS",
        "DOM",
        "DOMSnapshot",
        "Emulation",
        "Input",
        "Log",
        "Network",
        "Page",
        "Performance",
        "Runtime",
    ];
    let parts: Vec<_> = method.split('.').collect();
    let lower = method.to_ascii_lowercase();
    if parts.len() != 2
        || !domains.contains(&parts[0])
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_alphabetic()))
        || [
            "cookie",
            "downloadbehavior",
            "permissions",
            "browsercontext",
            "crash",
            "close",
            "terminateexecution",
        ]
        .iter()
        .any(|word| lower.contains(word))
        || matches!(
            method,
            "DOM.setFileInputFiles" | "Page.addScriptToEvaluateOnNewDocument"
        )
    {
        return Err(error("Este comando CDP não pertence à página controlada. Use as operações de abas do Jarvis."));
    }
    if method == "Page.navigate" {
        super::address(params["url"].as_str().unwrap_or_default())?;
        if params.get("frameId").is_some() {
            return Err(error(
                "Navegue pela aba controlada sem sobrescrever frameId.",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn protocol_observer_correlates_only_owned_request_once() {
        let session = Session::default();
        let (reply, received) = oneshot::channel();
        session
            .inner
            .lock()
            .unwrap()
            .pending
            .insert(7, Pending { limit: 100, reply });
        session.answer(8, true, br#"{"wrong":true}"#);
        assert_eq!(session.inner.lock().unwrap().pending.len(), 1);
        session.answer(7, true, br#"{"correct":true}"#);
        session.answer(7, true, br#"{"duplicate":true}"#);
        assert_eq!(received.await.unwrap().unwrap(), json!({"correct":true}));
    }

    #[tokio::test]
    async fn flattened_frame_result_retains_correlation_and_navigation_expires_snapshot_ids() {
        let session = Session::default();
        let (reply, received) = oneshot::channel();
        {
            let mut inner = session.inner.lock().unwrap();
            inner.pending.insert(7, Pending { limit: 100, reply });
            inner.contexts.insert("main".into(), 42);
            inner.elements.insert(
                "current".into(),
                Frame {
                    id: "main".into(),
                    parent_id: None,
                    session_id: None,
                    url: "https://example.test".into(),
                    name: String::new(),
                    available: true,
                },
            );
        }
        session.observe(DevToolsProtocol::Message(
            br#"{"id":7,"sessionId":"iframe-session","result":{"value":"child"}}"#.to_vec(),
        ));
        assert_eq!(received.await.unwrap().unwrap(), json!({"value":"child"}));
        session.observe(DevToolsProtocol::Message(
            br#"{"method":"Page.frameNavigated","params":{"frame":{"id":"main"}}}"#.to_vec(),
        ));
        let inner = session.inner.lock().unwrap();
        assert!(inner.contexts.is_empty());
        assert!(inner.elements.is_empty());
    }

    #[test]
    fn cancelled_protocol_request_is_retired_without_replaying_input() {
        let session = Session::default();
        let (reply, _received) = oneshot::channel();
        session
            .inner
            .lock()
            .unwrap()
            .pending
            .insert(7, Pending { limit: 100, reply });
        drop(PendingGuard {
            session: &session,
            id: 7,
        });
        assert!(session.inner.lock().unwrap().pending.is_empty());
        session.answer(7, true, b"{}");
    }

    #[test]
    fn navigation_fences_late_isolated_worlds_and_failed_reads_can_rebuild_without_replay() {
        let session = Session::default();
        let generation = session.inner.lock().unwrap().context_generation;
        session.observe(DevToolsProtocol::Message(
            br#"{"method":"Runtime.executionContextsCleared","params":{}}"#.to_vec(),
        ));
        assert!(session.cache_context("main", generation, 42).is_err());
        assert!(session.inner.lock().unwrap().contexts.is_empty());
        let generation = session.inner.lock().unwrap().context_generation;
        session.cache_context("main", generation, 43).unwrap();
        session.forget_context("main", 42);
        assert_eq!(session.inner.lock().unwrap().contexts["main"], 43);
        session.forget_context("main", 43);
        assert!(session.inner.lock().unwrap().contexts.is_empty());
        session.cache_context("main", generation, 44).unwrap();
        assert_eq!(session.inner.lock().unwrap().contexts["main"], 44);
    }

    #[test]
    fn protocol_results_reject_errors_oversized_data_and_javascript_exceptions() {
        assert!(protocol_result(false, br#"{"message":"rejected"}"#, 100).is_err());
        assert!(protocol_result(true, b"not json", 100).is_err());
        assert!(protocol_result(true, b"{}", 1).is_err());
        assert!(evaluation_result(
            &json!({"exceptionDetails":{"text":"failed"},"result":{"value":true}})
        )
        .is_err());
        assert_eq!(
            evaluation_result(&json!({"result":{"value":false}})).unwrap(),
            false
        );
    }

    #[test]
    fn network_records_are_bounded_correlated_and_skip_headers_and_bodies() {
        let mut inner = Inner::default();
        for id in 0..205 {
            event(
                &mut inner,
                "Network.requestWillBeSent",
                &json!({"requestId":id.to_string(),"request":{"url":"https://example.test","method":"GET","headers":{"Authorization":"private"},"postData":"private"}}),
                None,
            );
        }
        assert_eq!(inner.network.len(), BUFFER_LIMIT);
        assert_eq!(inner.network.front().unwrap()["id"], "5");
        event(
            &mut inner,
            "Network.responseReceived",
            &json!({"requestId":"204","response":{"status":200,"headers":{"Cookie":"private"}}}),
            None,
        );
        assert_eq!(inner.network.back().unwrap()["status"], 200);
        assert!(!serde_json::to_string(&inner.network)
            .unwrap()
            .contains("private"));
        event(
            &mut inner,
            "Network.requestWillBeSent",
            &json!({"requestId":"child","request":{"url":"https://frame.test","method":"POST"}}),
            Some("frame-session"),
        );
        assert_eq!(inner.network.back().unwrap()["id"], "frame-session:child");
    }

    #[test]
    fn page_protocol_rejects_target_management_persistence_files_and_non_web_navigation() {
        for method in [
            "Target.createTarget",
            "Browser.getVersion",
            "Network.getAllCookies",
            "DOM.setFileInputFiles",
            "Page.addScriptToEvaluateOnNewDocument",
            "Page.crash",
            "Runtime.terminateExecution",
        ] {
            assert!(validate_method(method, &json!({})).is_err(), "{method}");
        }
        assert!(validate_method("Page.navigate", &json!({"url":"file:///tmp/private"})).is_err());
        assert!(validate_method(
            "Page.navigate",
            &json!({"url":"https://example.test","frameId":"other"})
        )
        .is_err());
        assert!(validate_method("Runtime.evaluate", &json!({"expression":"true"})).is_ok());
        assert!(validate_method("Page.captureScreenshot", &json!({})).is_ok());
    }
}
