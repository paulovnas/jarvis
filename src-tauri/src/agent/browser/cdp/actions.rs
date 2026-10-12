//! The native transport reuses the extension's isolated-world DOM and input guards.
use super::{error, evaluation_result, AgentError, BrowserRequest, Frame, Session};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};
use tauri::Webview;

const PAGE: &str = include_str!("../page-cdp.js");
const MAX_FRAME_SESSIONS: usize = 32;

fn auto_attach() -> Value {
    json!({"autoAttach":true,"waitForDebuggerOnStart":false,"flatten":true,"filter":[{"type":"iframe","exclude":false},{"exclude":true}]})
}

fn unvisited_sessions(
    sessions: &BTreeMap<String, Frame>,
    visited: &BTreeSet<String>,
) -> Vec<Frame> {
    sessions
        .iter()
        .filter(|(id, _)| !visited.contains(*id))
        .take(MAX_FRAME_SESSIONS.saturating_sub(visited.len()))
        .map(|(_, frame)| frame.clone())
        .collect()
}

#[derive(Clone)]
struct FrameOwner {
    object_id: String,
    session_id: Option<String>,
}

fn frame<'a>(frames: &'a [Frame], id: Option<&str>) -> Result<&'a Frame, AgentError> {
    let frame = frames
        .iter()
        .find(|frame| {
            id.map_or(
                frame.parent_id.is_none() && frame.session_id.is_none(),
                |id| frame.id == id,
            )
        })
        .filter(|frame| frame.available)
        .ok_or_else(|| error("O frame foi substituído. Capture um novo snapshot."))?;
    if !frame.url.is_empty()
        && !frame.url.starts_with("about:blank")
        && !frame.url.starts_with("about:srcdoc")
    {
        super::super::address(&frame.url)?;
    }
    Ok(frame)
}

fn visit_tree(
    tree: &Value,
    session_id: Option<&str>,
    parent: Option<&str>,
    depth: usize,
    frames: &mut BTreeMap<String, Frame>,
) {
    if depth > 16 || frames.len() >= 100 {
        return;
    }
    let data = &tree["frame"];
    let Some(id) = data["id"].as_str().filter(|id| id.len() <= 160) else {
        return;
    };
    let previous = frames.get(id);
    let frame = Frame {
        id: id.to_owned(),
        parent_id: data["parentId"]
            .as_str()
            .or(parent)
            .map(str::to_owned)
            .or_else(|| previous.and_then(|frame| frame.parent_id.clone())),
        session_id: session_id
            .map(str::to_owned)
            .or_else(|| previous.and_then(|frame| frame.session_id.clone())),
        url: data["url"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(4096)
            .collect(),
        name: data["name"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(500)
            .collect(),
        available: true,
    };
    frames.insert(id.to_owned(), frame);
    if let Some(children) = tree["childFrames"].as_array() {
        for child in children {
            visit_tree(child, session_id, Some(id), depth + 1, frames);
        }
    }
}

impl Session {
    async fn frames(&self, view: &Webview) -> Result<Vec<Frame>, AgentError> {
        self.call(view, "Target.setAutoAttach", auto_attach(), None)
            .await?;
        let tree = self
            .call(view, "Page.getFrameTree", json!({}), None)
            .await?;
        let mut frames = BTreeMap::new();
        let mut visited = BTreeSet::new();
        visit_tree(&tree["frameTree"], None, None, 0, &mut frames);
        loop {
            // Attaching a child can reveal another out-of-process iframe. Expand
            // after its acknowledgement, not just from the initial root snapshot.
            let sessions = unvisited_sessions(
                &self
                    .inner
                    .lock()
                    .map_err(|_| AgentError::internal())?
                    .sessions,
                &visited,
            );
            if sessions.is_empty() {
                break;
            }
            for session in sessions {
                if let Some(id) = &session.session_id {
                    visited.insert(id.clone());
                }
                let result = async {
                    if let Some(id) = session.session_id.as_deref() {
                        let enabled = self
                            .inner
                            .lock()
                            .map_err(|_| AgentError::internal())?
                            .enabled_sessions
                            .contains(id);
                        if !enabled {
                            for method in ["Runtime.enable", "Page.enable", "Network.enable"] {
                                self.call(view, method, json!({}), Some(id)).await?;
                            }
                            self.call(view, "Target.setAutoAttach", auto_attach(), Some(id))
                                .await?;
                            self.inner
                                .lock()
                                .map_err(|_| AgentError::internal())?
                                .enabled_sessions
                                .insert(id.to_owned());
                        }
                    }
                    self.call(
                        view,
                        "Page.getFrameTree",
                        json!({}),
                        session.session_id.as_deref(),
                    )
                    .await
                }
                .await;
                if let Ok(tree) = result {
                    visit_tree(
                        &tree["frameTree"],
                        session.session_id.as_deref(),
                        session.parent_id.as_deref(),
                        0,
                        &mut frames,
                    );
                } else {
                    let parent = frames
                        .get(&session.id)
                        .and_then(|frame| frame.parent_id.clone())
                        .or_else(|| session.parent_id.clone());
                    frames.insert(
                        session.id.clone(),
                        Frame {
                            parent_id: parent,
                            available: false,
                            ..session
                        },
                    );
                }
            }
        }
        if frames.is_empty() {
            return Err(error(
                "A página ainda não informou seus frames. Aguarde a navegação.",
            ));
        }
        Ok(frames.into_values().collect())
    }

    async fn page(&self, view: &Webview, frame: &Frame, args: Value) -> Result<Value, AgentError> {
        self.evaluate(view, &format!("({PAGE}).page({args})"), Some(frame))
            .await
    }

    fn target_frame(
        &self,
        frames: &[Frame],
        request: &BrowserRequest,
    ) -> Result<Frame, AgentError> {
        let element_frame = if let Some(element) = &request.element {
            Some(self.inner.lock().map_err(|_| AgentError::internal())?.elements.get(element).cloned().ok_or_else(|| AgentError::new("browser_stale_element","O elemento não pertence ao snapshot atual. Capture um novo snapshot antes de agir."))?)
        } else {
            None
        };
        if element_frame
            .as_ref()
            .is_some_and(|frame| request.frame_id.as_ref().is_some_and(|id| *id != frame.id))
        {
            return Err(error("O elemento não pertence ao frame selecionado."));
        }
        frame(
            frames,
            element_frame
                .as_ref()
                .map(|frame| frame.id.as_str())
                .or(request.frame_id.as_deref()),
        )
        .cloned()
    }

    async fn owner(
        &self,
        view: &Webview,
        owner: &FrameOwner,
        action: &str,
        point: &Value,
    ) -> Result<Value, AgentError> {
        let result = self.call(view,"Runtime.callFunctionOn",json!({"objectId":owner.object_id,"functionDeclaration":format!("function(args) {{ return ({PAGE}).frameOwner.call(this,args); }}"),"arguments":[{"value":{"action":action,"x":point["x"],"y":point["y"]}}],"returnByValue":true}),owner.session_id.as_deref()).await?;
        evaluation_result(&result)
    }

    async fn release_owners(&self, view: &Webview, owners: &[FrameOwner]) {
        for owner in owners {
            let _ = self
                .call(
                    view,
                    "Runtime.releaseObject",
                    json!({"objectId":owner.object_id}),
                    owner.session_id.as_deref(),
                )
                .await;
        }
    }

    async fn pointer(
        &self,
        view: &Webview,
        frames: &[Frame],
        initial_frame: &Frame,
        initial: &Value,
    ) -> Result<(Value, Vec<FrameOwner>), AgentError> {
        let mut current = initial_frame.clone();
        let mut point = initial.clone();
        let mut owners = Vec::new();
        let result = async {
            for _ in 0..16 {
                let Some(parent_id) = current.parent_id.as_deref() else {
                    return Ok(point);
                };
                let parent = frame(frames, Some(parent_id))?;
                self.page(view, parent, json!({"action":"inspect"})).await?;
                let node = self
                    .call(
                        view,
                        "DOM.getFrameOwner",
                        json!({"frameId":current.id}),
                        parent.session_id.as_deref(),
                    )
                    .await?;
                let context = self
                    .inner
                    .lock()
                    .map_err(|_| AgentError::internal())?
                    .contexts
                    .get(&parent.id)
                    .copied();
                let resolved = self
                    .call(
                        view,
                        "DOM.resolveNode",
                        json!({"backendNodeId":node["backendNodeId"],"executionContextId":context}),
                        parent.session_id.as_deref(),
                    )
                    .await?;
                let object_id = resolved["object"]["objectId"]
                    .as_str()
                    .ok_or_else(|| error("O navegador não informou o elemento do frame."))?
                    .to_owned();
                let owner = FrameOwner {
                    object_id,
                    session_id: parent.session_id.clone(),
                };
                owners.push(owner.clone());
                point = self.owner(view, &owner, "point", &point).await?;
                if point["ready"] != true {
                    return Ok(point);
                }
                current = parent.clone();
            }
            Err(error(
                "A profundidade de frames excedeu o limite de inspeção.",
            ))
        }
        .await;
        match result {
            Ok(point) if point["ready"] == true => {
                let mut result = initial.clone();
                result["x"] = point["x"].clone();
                result["y"] = point["y"].clone();
                Ok((result, owners))
            }
            Ok(point) => {
                self.release_owners(view, &owners).await;
                Ok((point, Vec::new()))
            }
            Err(cause) => {
                self.release_owners(view, &owners).await;
                Err(cause)
            }
        }
    }

    async fn wait_page(
        &self,
        view: &Webview,
        target: &Frame,
        frames: &[Frame],
        request: &BrowserRequest,
        prepare: bool,
    ) -> Result<(Value, Vec<FrameOwner>), AgentError> {
        let start = Instant::now();
        let timeout = Duration::from_millis(u64::from(request.timeout_ms.unwrap_or(5000)));
        let mut last = Value::Null;
        for _ in 0..160 {
            // Navigation clears the host registry; never silently resolve a stale snapshot ID.
            if let Some(element) = &request.element {
                if !self
                    .inner
                    .lock()
                    .map_err(|_| AgentError::internal())?
                    .elements
                    .contains_key(element)
                {
                    return Err(AgentError::new(
                        "browser_stale_element",
                        "O elemento expirou durante a navegação. Capture um novo snapshot.",
                    ));
                }
            }
            let args = if prepare {
                json!({"action":"prepare","mode":request.action,"element":request.element,"locator":request.locator})
            } else {
                json!({"action":"wait","state":request.state,"element":request.element,"locator":request.locator})
            };
            last = self.page(view, target, args).await?;
            if last["ready"] == true {
                if prepare && request.action == "click" {
                    let (point, owners) = self.pointer(view, frames, target, &last).await?;
                    if point["ready"] == true {
                        return Ok((point, owners));
                    }
                    last = point;
                } else {
                    return Ok((last, Vec::new()));
                }
            }
            if terminal_preparation_error(last["code"].as_str().unwrap_or_default()) {
                return Err(AgentError::new(
                    last["code"].as_str().unwrap_or("browser_action_timeout"),
                    last["reason"]
                        .as_str()
                        .unwrap_or("O alvo não está disponível para esta ação."),
                ));
            }
            if start.elapsed() >= timeout {
                break;
            }
            tokio::time::sleep(
                Duration::from_millis(100).min(timeout.saturating_sub(start.elapsed())),
            )
            .await;
        }
        Err(AgentError::new("browser_action_timeout",&format!("A condição não foi atendida: {}. Nenhum input foi enviado; inspecione a página ou ajuste o alvo.",last["reason"].as_str().unwrap_or("página ainda não pronta"))))
    }

    async fn interact(
        &self,
        view: &Webview,
        request: &BrowserRequest,
        frames: &[Frame],
        target: &Frame,
    ) -> Result<Value, AgentError> {
        let (prepared, owners) = self.wait_page(view, target, frames, request, true).await?;
        let token = prepared["token"]
            .as_str()
            .ok_or_else(|| error("O navegador não confirmou um alvo de interação."))?;
        let mut guarded = false;
        let mut dispatched = false;
        let result=async {
            match request.action.as_str() {
                "click"=>{
                    let (x,y)=point(&prepared)?;
                    let deadline=Instant::now()+Duration::from_secs(3);
                    for owner in &owners {
                        let guard=self.owner(view,owner,"guard",&json!({})).await?;
                        require_ready(&guard)?;
                    }
                    let guard=self.page(view,target,json!({"action":"guard","element":token})).await?;
                    require_ready(&guard)?; guarded=true;
                    if Instant::now()>=deadline {return Err(error("A proteção do alvo expirou antes do clique. Nenhum input foi enviado."));}
                    dispatched=true;
                    for kind in ["mousePressed","mouseReleased"] {
                        self.request(view,"Input.dispatchMouseEvent",json!({"type":kind,"x":x,"y":y,"button":"left","clickCount":1}),None,deadline.saturating_duration_since(Instant::now()).max(Duration::from_millis(1)),super::RESULT_LIMIT).await?;
                    }
                    let finished=self.page(view,target,json!({"action":"finish","element":token})).await?; guarded=false;
                    if finished["blocked"]==true || finished.get("code").is_some() {return Err(error("O alvo mudou durante o clique."));}
                    for owner in &owners {
                        let finished=self.owner(view,owner,"finish",&json!({})).await?;
                        if finished["blocked"]==true || finished["ready"]!=true {return Err(error("O frame mudou durante o clique."));}
                    }
                }
                "fill"=>{
                    let verified=self.page(view,target,json!({"action":"verify","element":token})).await?;require_ready(&verified)?;
                    dispatched=true;
                    let text=request.text.as_deref().unwrap_or_default();
                    if prepared["tag"]=="select" {
                        let selected=self.page(view,target,json!({"action":"select","element":token,"text":text})).await?;
                        if selected["ok"]!=true {return Err(error(selected["reason"].as_str().unwrap_or("Não foi possível selecionar a opção.")));}
                    } else {
                        self.call(view,"Input.dispatchKeyEvent",json!({"type":"keyDown","key":"a","code":"KeyA","windowsVirtualKeyCode":65,"modifiers":2,"commands":["selectAll"]}),None).await?;
                        self.call(view,"Input.dispatchKeyEvent",json!({"type":"keyUp","key":"a","code":"KeyA","windowsVirtualKeyCode":65,"modifiers":2}),None).await?;
                        let verified=self.page(view,target,json!({"action":"verify","element":token})).await?;require_ready(&verified)?;
                        if text.is_empty() {
                            self.key(view,"Backspace","Backspace",8,None).await?;
                        } else {self.call(view,"Input.insertText",json!({"text":text}),None).await?;}
                    }
                }
                "press"=>{
                    let (key,code,value,text)=key(request.key.as_deref().unwrap_or_default())?;
                    let verified=self.page(view,target,json!({"action":"verify","element":token})).await?;require_ready(&verified)?;
                    dispatched=true;self.key(view,key,code,value,text).await?;
                }
                _=>return Err(error("Ação de interação inválida.")),
            }
            Ok(json!({"ok":true,"dispatched":true,"frameId":target.id,"instructions":"Input was dispatched once. Confirm the expected outcome with browser_wait or snapshot; never blindly repeat an action."}))
        }.await;
        if guarded {
            let _ = self
                .page(view, target, json!({"action":"finish","element":token}))
                .await;
        }
        for owner in &owners {
            let _ = self.owner(view, owner, "finish", &json!({})).await;
        }
        self.release_owners(view, &owners).await;
        if dispatched && result.is_err() {
            Err(AgentError::new("browser_outcome_unknown","Uma ação foi enviada, mas seu resultado não foi confirmado. Inspecione a página antes de repetir."))
        } else {
            result
        }
    }

    async fn key(
        &self,
        view: &Webview,
        key: &str,
        code: &str,
        value: u8,
        text: Option<&str>,
    ) -> Result<(), AgentError> {
        let mut params =
            json!({"type":"keyDown","key":key,"code":code,"windowsVirtualKeyCode":value});
        if let Some(text) = text {
            params["text"] = json!(text);
        }
        self.call(view, "Input.dispatchKeyEvent", params, None)
            .await?;
        self.call(
            view,
            "Input.dispatchKeyEvent",
            json!({"type":"keyUp","key":key,"code":code,"windowsVirtualKeyCode":value}),
            None,
        )
        .await?;
        Ok(())
    }
}

fn require_ready(value: &Value) -> Result<(), AgentError> {
    if value["ready"] == true {
        Ok(())
    } else {
        Err(AgentError::new(
            value["code"].as_str().unwrap_or("browser_stale_element"),
            value["reason"]
                .as_str()
                .unwrap_or("O alvo mudou antes da ação."),
        ))
    }
}

fn terminal_preparation_error(code: &str) -> bool {
    matches!(
        code,
        "browser_ambiguous_element"
            | "browser_stale_element"
            | "browser_element_not_editable"
            | "browser_element_not_focusable"
            | "browser_hit_test_unavailable"
            | "browser_frame_transform_unsupported"
            | "browser_focus_changed"
            | "browser_snapshot_truncated"
            | "browser_invalid_request"
    )
}

fn point(prepared: &Value) -> Result<(f64, f64), AgentError> {
    let x = prepared["x"]
        .as_f64()
        .filter(|value| value.is_finite() && *value >= 0.);
    let y = prepared["y"]
        .as_f64()
        .filter(|value| value.is_finite() && *value >= 0.);
    x.zip(y)
        .ok_or_else(|| error("O navegador não confirmou coordenadas válidas para o clique."))
}

fn key(value: &str) -> Result<(&str, &str, u8, Option<&str>), AgentError> {
    Ok(match value {
        "Enter" => ("Enter", "Enter", 13, Some("\r")),
        "Tab" => ("Tab", "Tab", 9, None),
        "Escape" => ("Escape", "Escape", 27, None),
        "ArrowLeft" => ("ArrowLeft", "ArrowLeft", 37, None),
        "ArrowUp" => ("ArrowUp", "ArrowUp", 38, None),
        "ArrowRight" => ("ArrowRight", "ArrowRight", 39, None),
        "ArrowDown" => ("ArrowDown", "ArrowDown", 40, None),
        _ => return Err(error("Tecla não suportada.")),
    })
}

pub(super) async fn command(
    session: &Session,
    view: &Webview,
    request: &BrowserRequest,
) -> Result<Value, AgentError> {
    let frames = session.frames(view).await?;
    let target = session.target_frame(&frames, request)?;
    match request.action.as_str() {
        "snapshot" => {
            let generation = crate::library::new_id()?;
            let mut result=session.page(view,&target,json!({"action":"snapshot","generation":generation,"offset":request.offset.unwrap_or(0),"limit":request.limit.unwrap_or(100)})).await?;
            let mut inner = session.inner.lock().map_err(|_| AgentError::internal())?;
            inner.elements.clear();
            if let Some(elements) = result["elements"].as_array() {
                for element in elements {
                    if let Some(id) = element["id"].as_str() {
                        inner.elements.insert(id.to_owned(), target.clone());
                    }
                }
            }
            result["frameId"] = json!(target.id);
            result["frames"] = json!(frames);
            Ok(result)
        }
        "wait" => {
            let start = Instant::now();
            session
                .wait_page(view, &target, &frames, request, false)
                .await?;
            Ok(
                json!({"ready":true,"state":request.state,"frameId":target.id,"elapsedMs":start.elapsed().as_millis()}),
            )
        }
        "click" | "fill" | "press" => session.interact(view, request, &frames, &target).await,
        _ => Err(error("Ação de página inválida.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_out_of_process_frames_are_discovered_after_child_attach_without_revisiting() {
        let session = Session::default();
        let attach = |parent: Option<&str>, id: &str, frame_id: &str| {
            let mut message = json!({"method":"Target.attachedToTarget","params":{"sessionId":id,"targetInfo":{"targetId":frame_id,"type":"iframe","url":"https://frame.test"}}});
            if let Some(parent) = parent {
                message["sessionId"] = json!(parent);
            }
            session.observe(super::super::DevToolsProtocol::Message(
                message.to_string().into_bytes(),
            ));
        };
        let mut frames = BTreeMap::new();
        visit_tree(
            &json!({"frame":{"id":"root","url":"https://example.test"},"childFrames":[{"frame":{"id":"child","url":"https://frame.test"}}]}),
            None,
            None,
            0,
            &mut frames,
        );
        let mut visited = BTreeSet::new();
        attach(None, "child-session", "child");
        let children = unvisited_sessions(&session.inner.lock().unwrap().sessions, &visited);
        assert_eq!(children.len(), 1);
        visited.insert("child-session".into());
        visit_tree(
            &json!({"frame":{"id":"child","url":"https://frame.test"}}),
            Some("child-session"),
            None,
            0,
            &mut frames,
        );
        // This event arrives only after setAutoAttach is sent to the child session.
        attach(Some("child-session"), "grandchild-session", "grandchild");
        let grandchildren = unvisited_sessions(&session.inner.lock().unwrap().sessions, &visited);
        assert_eq!(grandchildren.len(), 1);
        assert_eq!(grandchildren[0].parent_id.as_deref(), Some("child"));
        visited.insert("grandchild-session".into());
        visit_tree(
            &json!({"frame":{"id":"grandchild","url":"https://nested.test"}}),
            grandchildren[0].session_id.as_deref(),
            grandchildren[0].parent_id.as_deref(),
            0,
            &mut frames,
        );
        let frames: Vec<_> = frames.into_values().collect();
        assert_eq!(
            frame(&frames, Some("child")).unwrap().parent_id.as_deref(),
            Some("root")
        );
        let grandchild = frame(&frames, Some("grandchild")).unwrap();
        assert_eq!(grandchild.parent_id.as_deref(), Some("child"));
        assert_eq!(grandchild.session_id.as_deref(), Some("grandchild-session"));
        assert!(unvisited_sessions(&session.inner.lock().unwrap().sessions, &visited).is_empty());
        visited.extend((0..MAX_FRAME_SESSIONS).map(|id| format!("visited-{id}")));
        attach(Some("grandchild-session"), "deeper-session", "deeper");
        assert!(unvisited_sessions(&session.inner.lock().unwrap().sessions, &visited).is_empty());
    }

    #[test]
    fn frame_tree_preserves_parent_and_rejects_detached_or_non_web_targets() {
        let mut frames = BTreeMap::new();
        visit_tree(
            &json!({"frame":{"id":"main","url":"https://example.test"},"childFrames":[{"frame":{"id":"child","url":"https://frame.test"}}]}),
            None,
            None,
            0,
            &mut frames,
        );
        visit_tree(
            &json!({"frame":{"id":"child","url":"https://frame.test"}}),
            Some("child-session"),
            None,
            0,
            &mut frames,
        );
        let mut frames: Vec<_> = frames.into_values().collect();
        assert_eq!(
            frame(&frames, Some("child")).unwrap().parent_id.as_deref(),
            Some("main")
        );
        assert_eq!(
            frame(&frames, Some("child")).unwrap().session_id.as_deref(),
            Some("child-session")
        );
        assert_eq!(frame(&frames, None).unwrap().id, "main");
        assert!(frame(&frames, Some("missing")).is_err());
        frames[0].available = false;
        assert!(frame(&frames, Some(&frames[0].id)).is_err());
        frames[0].available = true;
        frames[0].url = "file:///private".into();
        assert!(frame(&frames, Some(&frames[0].id)).is_err());
    }

    #[test]
    fn native_input_rejects_invalid_points_keys_and_fails_ambiguous_preparation_without_dispatch() {
        assert!(point(&json!({"x":-1,"y":2})).is_err());
        assert!(point(&json!({"x":1})).is_err());
        assert_eq!(point(&json!({"x":1,"y":2})).unwrap(), (1., 2.));
        assert!(key("Meta").is_err());
        assert_eq!(key("Enter").unwrap().2, 13);
        assert!(terminal_preparation_error("browser_ambiguous_element"));
        assert!(terminal_preparation_error(
            "browser_frame_transform_unsupported"
        ));
        assert!(!terminal_preparation_error("browser_element_unstable"));
        assert!(require_ready(
            &json!({"ready":false,"code":"browser_ambiguous_element","reason":"Refine o alvo"})
        )
        .is_err());
    }
}
