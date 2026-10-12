//! Opt-in integration fixture: real Live helper/browser and native session,
//! provider, tool contract and file operations. Only the LLM HTTP peer is fake.
//! Run this test alone because native Core setup uses a process-wide OnceLock.
use super::*;
use crate::agent::{context_manager::StepContext, provider, tool_contract::Orchestrator};
use crate::openai_codex::{custom, CodexCredential, ProviderModel};
use axum::{
    extract::State, http::StatusCode, response::IntoResponse, routing::get, routing::post, Json,
};
use futures_util::{SinkExt, StreamExt};
use std::{collections::VecDeque, process::Stdio};
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};

const PICKED: &str = "<section id=\"badge-parent\" class=\"product-card\">\n  <h1>Live fixture</h1><span class=\"version\">v0.1</span>\n</section>";

#[derive(Clone)]
struct Script {
    replies: Arc<Mutex<VecDeque<Value>>>,
    requests: Arc<Mutex<Vec<Value>>>,
    refuse: bool,
}

async fn completion(
    State(script): State<Script>,
    Json(request): Json<Value>,
) -> axum::response::Response {
    script.requests.lock().unwrap().push(request);
    if script.refuse {
        return (StatusCode::BAD_REQUEST, Json(json!({"error":{"message":"Isolated fixture provider rejected this request","type":"invalid_request_error"}}))).into_response();
    }
    let delta = script
        .replies
        .lock()
        .unwrap()
        .pop_front()
        .expect("unexpected provider request");
    let reason = if delta["tool_calls"].is_array() {
        "tool_calls"
    } else {
        "stop"
    };
    let sse = format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{"index":0,"delta":delta,"finish_reason":null}]}),
        json!({"choices":[{"index":0,"delta":{},"finish_reason":reason}]})
    );
    (
        [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
        sse,
    )
        .into_response()
}

fn call(id: &str, name: &str, args: Value) -> Value {
    json!({"tool_calls":[{"index":0,"id":id,"type":"function","function":{"name":name,"arguments":args.to_string()}}]})
}

fn engine_call(id: &str, command: &str, args: Vec<String>) -> Value {
    call(id, "impeccable", json!({"command":command,"args":args}))
}

struct HttpPeer {
    endpoint: String,
    script: Script,
    task: tokio::task::JoinHandle<()>,
}

impl HttpPeer {
    async fn start(replies: Vec<Value>, refuse: bool) -> Self {
        let script = Script {
            replies: Arc::new(Mutex::new(replies.into())),
            requests: Arc::new(Mutex::new(vec![])),
            refuse,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let app = axum::Router::new()
            .route("/chat/completions", post(completion))
            .with_state(script.clone());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            endpoint,
            script,
            task,
        }
    }
}

impl Drop for HttpPeer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct LiveFixture {
    fixture: Fixture,
    helper: tokio::process::Child,
    page: tokio::task::JoinHandle<()>,
    page_url: String,
}

impl LiveFixture {
    async fn start(resources: &Path) -> Self {
        let fixture = Fixture::new();
        crate::core::design::setup(resources, &fixture.root).unwrap();
        fs::create_dir_all(fixture.root.join(".impeccable/live")).unwrap();
        fs::write(
            fixture.root.join(".impeccable/live/config.json"),
            json!({"files":["index.html"],"insertBefore":"</body>","commentSyntax":"html"})
                .to_string(),
        )
        .unwrap();
        fs::write(fixture.root.join("index.html"), format!("<!doctype html>\n<html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><title>Isolated Live fixture</title>\n<style>body{{margin:40px;font:16px sans-serif;background:#f8fafc}}.product-card{{display:flex;align-items:center;gap:24px;box-sizing:border-box;width:640px;height:180px;padding:28px;background:#fff;border:1px solid #d0d6de}}.version{{padding:8px;border:1px solid #d0d6de;border-radius:8px}}</style>\n</head><body>\n{PICKED}\n<p id=\"proof\">Waiting for native tools</p>\n</body></html>\n")).unwrap();
        // A foreground child, a private cwd and port zero cannot adopt or stop
        // any existing user helper. Logs intentionally stay off the test output.
        let mut command = crate::core::design::command(&fixture.root, &fixture.root).unwrap();
        let helper = command
            .args(["live-server", "--port=0"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let info_path = fixture.root.join(".impeccable/live/server.json");
        let info = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if let Ok(text) = fs::read_to_string(&info_path) {
                    if let Ok(value) = serde_json::from_str::<Value>(&text) {
                        break value;
                    }
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("private Live helper did not start");
        let port = info["port"].as_u64().unwrap().to_string();
        let injected = cli(&fixture.root, "live-inject", &["--port".into(), port]).await;
        assert_eq!(injected["ok"], true);
        let root = fixture.root.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let page_url = format!("http://{}/", listener.local_addr().unwrap());
        let app = axum::Router::new().route(
            "/",
            get(move || {
                let root = root.clone();
                async move {
                    (
                        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
                        fs::read_to_string(root.join("index.html")).unwrap(),
                    )
                }
            }),
        );
        let page = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            fixture,
            helper,
            page,
            page_url,
        }
    }
}

impl Drop for LiveFixture {
    fn drop(&mut self) {
        let _ = self.helper.start_kill();
        self.page.abort();
    }
}

async fn cli(home: &Path, command: &str, args: &[String]) -> Value {
    let mut process = crate::core::design::command(home, home).unwrap();
    let output = tokio::time::timeout(
        Duration::from_secs(25),
        process.arg(command).args(args).output(),
    )
    .await
    .expect("isolated CLI timed out")
    .unwrap();
    assert!(
        output.status.success(),
        "isolated {command} failed (output withheld)"
    );
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|_| panic!("isolated {command} returned non-JSON output"))
}

struct Browser {
    child: tokio::process::Child,
    socket: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    sequence: u64,
}

impl Browser {
    async fn start(binary: &Path, profile: &Path, page_url: &str) -> Self {
        fs::create_dir_all(profile).unwrap();
        let child = tokio::process::Command::new(binary)
            .args([
                "--headless",
                "--no-sandbox",
                "--disable-gpu",
                "--disable-background-networking",
                "--no-first-run",
                "--no-default-browser-check",
                "--remote-debugging-address=127.0.0.1",
                "--remote-debugging-port=0",
            ])
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg("about:blank")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let port = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if let Ok(text) = fs::read_to_string(profile.join("DevToolsActivePort")) {
                    if let Some(port) = text.lines().next() {
                        break port.to_owned();
                    }
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("isolated browser did not start");
        let tab: Value = reqwest::Client::new()
            .put(format!("http://127.0.0.1:{port}/json/new?about:blank"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let (socket, _) =
            tokio_tungstenite::connect_async(tab["webSocketDebuggerUrl"].as_str().unwrap())
                .await
                .unwrap();
        let mut browser = Self {
            child,
            socket,
            sequence: 0,
        };
        browser.command("Page.enable", json!({})).await;
        browser
            .command("Page.navigate", json!({"url":page_url}))
            .await;
        browser
            .wait("!!document.getElementById('impeccable-live-global-bar')")
            .await;
        browser
    }

    async fn command(&mut self, method: &str, params: Value) -> Value {
        self.sequence += 1;
        let id = self.sequence;
        self.socket
            .send(Message::Text(
                json!({"id":id,"method":method,"params":params})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let message = self.socket.next().await.unwrap().unwrap();
                if let Message::Text(text) = message {
                    let response: Value = serde_json::from_str(text.as_str()).unwrap();
                    if response["id"] == id {
                        assert!(
                            response.get("error").is_none(),
                            "isolated browser command failed: {method}"
                        );
                        break response["result"].clone();
                    }
                }
            }
        })
        .await
        .expect("isolated browser command timed out")
    }

    async fn evaluate(&mut self, expression: &str) -> Value {
        let result = self
            .command(
                "Runtime.evaluate",
                json!({"expression":expression,"returnByValue":true,"awaitPromise":true}),
            )
            .await;
        assert!(
            result.get("exceptionDetails").is_none(),
            "isolated browser evaluation failed"
        );
        result["result"]["value"].clone()
    }

    async fn wait(&mut self, expression: &str) {
        tokio::time::timeout(Duration::from_secs(25), async {
            loop {
                if self.evaluate(expression).await == true {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("isolated browser condition did not become true: {expression}"));
    }

    async fn pick_and_send(&mut self) {
        self.evaluate("(()=>{const pick=document.getElementById('impeccable-live-pick-toggle');if(pick.dataset.active!=='true')pick.click();return true})()").await;
        let point = self.evaluate("(()=>{const r=document.getElementById('badge-parent').getBoundingClientRect();return {x:r.left+24,y:r.bottom-24}})()").await;
        for (kind, button, count) in [
            ("mouseMoved", "none", 0),
            ("mousePressed", "left", 1),
            ("mouseReleased", "left", 1),
        ] {
            self.command("Input.dispatchMouseEvent", json!({"type":kind,"x":point["x"],"y":point["y"],"button":button,"clickCount":count})).await;
        }
        self.wait("!!document.getElementById('impeccable-live-input')")
            .await;
        self.evaluate("(()=>{const input=document.getElementById('impeccable-live-input');input.value='Mova a badge de versão para a direita';input.dispatchEvent(new Event('input',{bubbles:true}));input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));return true})()").await;
    }

    async fn steer(&mut self) {
        self.evaluate("(()=>{const input=document.getElementById('impeccable-live-page-chat-input');input.value='Atualize a confirmação de execução';input.dispatchEvent(new Event('input',{bubbles:true}));document.getElementById('impeccable-live-page-chat-send').click();return true})()").await;
        self.wait(
            "document.getElementById('impeccable-live-page-chat').dataset.processing==='true'",
        )
        .await;
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

fn model_choice(account: &str, model: &str) -> TurnOptions {
    let mut selected = options(ApprovalMode::Yolo);
    selected.account = account.into();
    selected.model = model.into();
    selected.workflow = Some(workflow::Flow::Designer);
    selected.model_selection = Some(workflow::settings::ModelChoice {
        executor: selected.executor,
        account: account.into(),
        model: model.into(),
        reasoning: None,
        service_tier: None,
        fallback: None,
    });
    selected
}

async fn run_provider(
    session: &Session,
    home: &Path,
    peer: &HttpPeer,
    selected: &TurnOptions,
    signal: watch::Receiver<bool>,
) -> Result<(), AgentError> {
    let mut credential = CodexCredential::new(
        format!("isolated-{}", selected.account),
        "",
        0,
        &selected.account,
        None,
        None,
    );
    credential.custom = Some(serde_json::from_value::<custom::Config>(json!({"baseUrl":peer.endpoint,"protocol":"openai-completions","authMode":"bearer","tokenField":"max_tokens","models":[{"id":selected.model,"name":"Local fixture","contextWindow":128000,"maxOutputTokens":4000,"supportsImages":false,"supportsTools":true,"reasoning":"none","reasoningLevels":[],"defaultReasoningLevel":null,"thinkingBudget":null}]})).unwrap());
    let model: ProviderModel = serde_json::from_value(json!({"id":selected.model,"name":"Local fixture","reasoningLevels":[],"defaultReasoningLevel":null,"contextWindow":128000})).unwrap();
    let provider = provider::TurnSession::new(
        credential,
        &model,
        session.id.clone(),
        telemetry::TraceContext::new(&session.id, "live-e2e"),
        &session.transports,
    )
    .unwrap();
    let mut definitions = tools::definitions(Mode::Build)
        .into_iter()
        .filter(|definition| matches!(definition["name"].as_str(), Some("read" | "edit")))
        .collect::<Vec<_>>();
    definitions.push(impeccable::definition());
    let instructions = format!(
        "{}\n{}",
        tools::instructions(&session.root, Mode::Build, ApprovalMode::Yolo),
        crate::core::design::HOST_ADAPTATION
    );
    for _ in 0..8 {
        let step = StepContext::capture(
            session,
            selected,
            &instructions,
            &definitions,
            provider.capabilities(),
        )?;
        let response = provider.stream(&step, signal.clone(), |_| Ok(())).await?;
        let calls = response.tool_calls().to_vec();
        session.update(true, |data| {
            data.turns.last_mut().unwrap().wire.extend(response.output)
        })?;
        if calls.is_empty() {
            return Ok(());
        }
        let contract = Orchestrator::new(&definitions);
        for call in calls {
            let prepared = contract.preflight(&call)?;
            session.update(true, |data| {
                data.turns.last_mut().unwrap().turn.steps.push(Step {
                    context_id: Some(step.id().to_owned()),
                    tools: vec![call.clone()],
                    ..Step::default()
                });
            })?;
            let result = match prepared.handler {
                tool_contract::Handler::Impeccable => {
                    impeccable::execute(
                        None,
                        home,
                        session,
                        &session.id,
                        &call.args,
                        selected,
                        false,
                        signal.clone(),
                    )
                    .await
                }
                tool_contract::Handler::Native => {
                    tools::execute(&session.root, &call, Mode::Build, signal.clone()).await
                }
                _ => panic!("unexpected fixture tool handler"),
            };
            // Use the actual harness settlement contract: scope/schema/runtime
            // refusals become tool feedback; cancellation/storage stay terminal.
            let (output, status, structured) = crate::agent::settle_tool_result(result)?;
            core_runtime::checkpoint_tool(
                session,
                &call,
                &output,
                status,
                0,
                structured.as_deref(),
            )
            .await?;
        }
    }
    panic!("isolated Live turn exceeded eight provider steps");
}

fn generate_replies(event: &Value) -> Vec<Value> {
    let id = event["id"].as_str().unwrap();
    let scaffold = &event["scaffold"];
    assert_eq!(
        scaffold["sourceWritten"], false,
        "fixture must exercise deferred, atomic source publication"
    );
    let wrapper = scaffold["wrapperBlock"].as_str().unwrap();
    let marker = "<!-- Variants: insert below this line -->";
    assert_eq!(wrapper.matches(marker).count(), 1);
    let mut variants = format!("\n  <style data-impeccable-css=\"{id}\">\n    @scope ([data-impeccable-variant=\"1\"]) {{ .version {{ margin-left: auto; }} }}\n    @scope ([data-impeccable-variant=\"2\"]) {{ .version {{ margin-left: auto; border-width: 2px; }} }}\n    @scope ([data-impeccable-variant=\"3\"]) {{ .version {{ margin-left: auto; border-radius: 16px; }} }}\n  </style>");
    for number in 1..=3 {
        let hidden = if number == 1 {
            ""
        } else {
            " style=\"display: none\""
        };
        variants.push_str(&format!(
            "\n  <div data-impeccable-variant=\"{number}\"{hidden}>{PICKED}</div>"
        ));
    }
    let published = wrapper.replace(marker, &format!("{marker}{variants}"));
    vec![
        engine_call(
            "reject-historical-action",
            "live-resume",
            vec!["--id".into(), "ca170dc6".into()],
        ),
        engine_call("resume", "live-resume", vec!["--id".into(), id.into()]),
        call("read-picked-source", "read", json!({"path":"index.html"})),
        call(
            "publish-variants",
            "edit",
            json!({"path":"index.html","oldText":PICKED,"newText":published}),
        ),
        engine_call(
            "generation-ack",
            "live-poll",
            vec![
                "--reply".into(),
                id.into(),
                "done".into(),
                "--file".into(),
                "index.html".into(),
            ],
        ),
        json!({"content":"Três posições para a badge estão prontas para comparar."}),
    ]
}

fn assert_idle(session: &Session) {
    let snapshot = session.snapshot().unwrap();
    assert!(snapshot.active_turn_id.is_none());
    assert!(snapshot.queued_messages.is_empty());
    assert_eq!(snapshot.turns.last().unwrap().status, TurnStatus::Completed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Opt-in real helper/browser E2E; set JARVIS_TEST_IMPECCABLE_RESOURCES and JARVIS_TEST_LIVE_BROWSER and run this test alone"]
async fn live_pick_send_native_provider_tools_ack_and_retry_unlock_the_browser() {
    let resources = PathBuf::from(
        std::env::var_os("JARVIS_TEST_IMPECCABLE_RESOURCES")
            .expect("set JARVIS_TEST_IMPECCABLE_RESOURCES"),
    );
    let browser_binary = PathBuf::from(
        std::env::var_os("JARVIS_TEST_LIVE_BROWSER").expect("set JARVIS_TEST_LIVE_BROWSER"),
    );
    let mut live = LiveFixture::start(&resources).await;
    let session = session(&live.fixture);
    let mut browser = Browser::start(
        &browser_binary,
        &live.fixture.root.join("browser-profile"),
        &live.page_url,
    )
    .await;
    assert_eq!(
        browser
            .evaluate(
                "document.getElementById('impeccable-live-pick-toggle').getAttribute('aria-label')"
            )
            .await,
        "Selecionar elemento",
        "the real installed Live browser controls must use pt-BR"
    );
    let original_x = browser
        .evaluate("document.querySelector('.version').getBoundingClientRect().left")
        .await
        .as_f64()
        .unwrap();
    browser.pick_and_send().await;
    let event = cli(&live.fixture.root, "live-poll", &["--timeout=10000".into()]).await;
    assert_eq!(event["type"], "generate");
    assert_eq!(event["element"]["id"], "badge-parent");
    assert_eq!(event["count"], 3);
    assert!(event["generationReadyAt"].is_number());
    let id = event["id"].as_str().unwrap();
    let pending = cli(
        &live.fixture.root,
        "live-resume",
        &["--id".into(), id.into()],
    )
    .await;
    assert_eq!(
        pending["pendingEvent"]["generationReadyAt"],
        event["generationReadyAt"]
    );
    let info: Value = serde_json::from_slice(
        &fs::read(live.fixture.root.join(".impeccable/live/server.json")).unwrap(),
    )
    .unwrap();
    let endpoint = format!("http://127.0.0.1:{}", info["port"].as_u64().unwrap());
    let token = info["token"].as_str().unwrap();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let health: Value = client
        .get(format!("{endpoint}/health?token={token}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health["jarvisQueueProtocol"], 1);
    let journal = live
        .fixture
        .root
        .join(format!(".impeccable/live/sessions/{id}.jsonl"));
    let before = fs::read(&journal).unwrap();
    let stale = client
        .get(format!(
            "{endpoint}/poll?token={token}&id={id}&jarvisRequestSeq={}",
            u64::MAX
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), reqwest::StatusCode::CONFLICT);
    let stale_ack = client.post(format!("{endpoint}/poll?token={token}"))
        .json(&json!({"token":token,"id":id,"type":"agent_done","sourceEventType":"generate","jarvisRequestSeq":u64::MAX}))
        .send().await.unwrap();
    assert_eq!(stale_ack.status(), reqwest::StatusCode::CONFLICT);
    assert_eq!(fs::read(&journal).unwrap(), before);
    let selected = model_choice("fixture-provider-a", "fixture-model-a");
    let content = format!("Impeccable Live · ação {id}\nMova a badge de versão para a direita");
    // The current chat turn may use another provider when the page submits.
    // Live owns its queue; a busy chat never stores another generic copy.
    let previous = model_choice("prior-chat-provider", "prior-chat-model");
    let _previous_signal = session
        .reserve("Previous chat action".into(), previous)
        .unwrap();
    assert!(session
        .submit_live_message(content.clone(), selected.clone(), vec![])
        .is_err());
    let queued = session.snapshot().unwrap();
    assert!(queued.queued_messages.is_empty());
    finish(&session, Ok(()));
    let signal = session
        .submit_live_message(content.clone(), selected.clone(), vec![])
        .unwrap();
    let peer = HttpPeer::start(generate_replies(&event), false).await;
    let outcome = run_provider(&session, &live.fixture.root, &peer, &selected, signal).await;
    assert!(
        outcome.is_ok(),
        "native fixture turn failed: {:?}",
        outcome.as_ref().err().map(|error| &error.code)
    );
    finish(&session, outcome);
    assert_idle(&session);
    assert!(session
        .snapshot()
        .unwrap()
        .turns
        .last()
        .unwrap()
        .steps
        .iter()
        .flat_map(|step| &step.tools)
        .any(|tool| tool.id == "reject-historical-action"
            && tool.status == "error"
            && tool.output.contains("Esta execução pertence à ação")));
    assert!(
        !live
            .fixture
            .root
            .join(".impeccable/live/sessions/ca170dc6.jsonl")
            .exists(),
        "a rejected historical ID must not create or alter a Live session"
    );
    assert_eq!(
        session
            .snapshot()
            .unwrap()
            .turns
            .last()
            .unwrap()
            .options
            .account,
        selected.account
    );
    assert!(peer
        .script
        .requests
        .lock()
        .unwrap()
        .iter()
        .all(|request| request["model"] == selected.model));
    let acknowledged = cli(
        &live.fixture.root,
        "live-resume",
        &["--id".into(), id.into()],
    )
    .await;
    assert!(
        acknowledged["pendingEvent"].is_null(),
        "a completed model response alone is not a Live ACK"
    );
    browser.wait("document.querySelectorAll('[data-impeccable-variant=\"1\"]').length===1 && document.querySelectorAll('[data-impeccable-variant=\"2\"]').length===1 && document.querySelectorAll('[data-impeccable-variant=\"3\"]').length===1").await;
    browser.wait(&format!("(()=>{{const badge=document.querySelector('[data-impeccable-variant=\"1\"] .version');return badge&&badge.getBoundingClientRect().left>{}}})()", original_x + 150.0)).await;
    browser.wait("(()=>{const bar=document.getElementById('impeccable-live-bar');return !!bar && [...bar.querySelectorAll('button')].some(button=>button.textContent.includes('Aceitar')&&!button.disabled) && !bar.textContent.includes('Gerando')})()").await;
    assert!(fs::read_to_string(live.fixture.root.join("index.html"))
        .unwrap()
        .contains("margin-left: auto"));

    // A real page-bar action reaches the durable journal, and an actual failed
    // provider request is followed by the exact error ACK that clears its lock.
    browser.steer().await;
    let steer = cli(&live.fixture.root, "live-poll", &["--timeout=10000".into()]).await;
    assert_eq!(steer["type"], "steer");
    assert_eq!(steer["message"], "Atualize a confirmação de execução");
    let steer_id = steer["id"].as_str().unwrap();
    let steer_content = format!(
        "Impeccable Live · ação {steer_id}\n{}",
        steer["message"].as_str().unwrap()
    );
    let failed_signal = session
        .submit_live_message(steer_content.clone(), selected.clone(), vec![])
        .unwrap();
    let refusing = HttpPeer::start(vec![], true).await;
    let failure = run_provider(
        &session,
        &live.fixture.root,
        &refusing,
        &selected,
        failed_signal.clone(),
    )
    .await
    .unwrap_err();
    impeccable::execute(None, &live.fixture.root, &session, &session.id, &json!({"command":"live-poll","args":["--reply",steer_id,"error","O provedor da fixture recusou a solicitação. Troque o modelo e retome."]}), &selected, false, failed_signal).await.unwrap();
    finish(&session, Err(failure));
    browser.wait("document.getElementById('impeccable-live-page-chat').dataset.processing!=='true' && !document.getElementById('impeccable-live-page-chat-input').disabled").await;
    let failed = session.snapshot().unwrap();
    assert_eq!(failed.turns.last().unwrap().status, TurnStatus::Error);
    let turn_id = failed.turns.last().unwrap().id.clone();
    let changed = model_choice("fixture-provider-b", "fixture-model-b");
    let (retry_signal, _) = session
        .retry_failed_turn(&turn_id, changed.model_selection.as_ref())
        .unwrap();
    let retry_options = session
        .snapshot()
        .unwrap()
        .turns
        .last()
        .unwrap()
        .options
        .clone();
    assert_eq!(retry_options.account, changed.account);
    assert_eq!(retry_options.model, changed.model);
    let retry_peer = HttpPeer::start(vec![
        engine_call("retry-resume", "live-resume", vec!["--id".into(), steer_id.into()]),
        call("read-current-source", "read", json!({"path":"index.html"})),
        call("confirm-native-tools", "edit", json!({"path":"index.html","oldText":"Waiting for native tools","newText":"Native provider retry completed"})),
        engine_call("retry-ack", "live-poll", vec!["--reply".into(), steer_id.into(), "steer_done".into(), "Confirmação atualizada.".into()]),
        json!({"content":"A confirmação foi atualizada usando o modelo selecionado."}),
    ], false).await;
    let outcome = run_provider(
        &session,
        &live.fixture.root,
        &retry_peer,
        &retry_options,
        retry_signal,
    )
    .await;
    assert!(
        outcome.is_ok(),
        "native retry failed: {:?}",
        outcome.as_ref().err().map(|error| &error.code)
    );
    finish(&session, outcome);
    assert_idle(&session);
    assert_eq!(
        session.snapshot().unwrap().turns.last().unwrap().id,
        turn_id
    );
    assert_eq!(
        session.snapshot().unwrap().turns.last().unwrap().user,
        steer_content
    );
    assert!(retry_peer
        .script
        .requests
        .lock()
        .unwrap()
        .iter()
        .all(|request| request["model"] == changed.model));
    let final_state = cli(
        &live.fixture.root,
        "live-resume",
        &["--id".into(), steer_id.into()],
    )
    .await;
    assert!(final_state["pendingEvent"].is_null());
    assert_eq!(final_state["snapshot"]["phase"], "steer_done");
    // The fixture's simple HTTP server has no HMR. A browser navigation proves
    // the actual saved source is served, separately from the Live preview DOM.
    browser
        .command("Page.navigate", json!({"url":live.page_url}))
        .await;
    browser
        .wait("document.getElementById('proof')?.textContent==='Native provider retry completed'")
        .await;
    browser.wait("!!document.getElementById('impeccable-live-page-chat-input') && !document.getElementById('impeccable-live-page-chat-input').disabled").await;
    browser.child.kill().await.unwrap();
    browser.child.wait().await.unwrap();
    live.helper.kill().await.unwrap();
    live.helper.wait().await.unwrap();
}
