use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::{
    net::TcpListener,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

pub(super) struct Call {
    pub id: Value,
    pub params: Value,
    pub reply: oneshot::Sender<Value>,
    pub received: std::time::Instant,
}

#[derive(Clone)]
struct Endpoint {
    authorization: String,
    tools: Arc<[Value]>,
    calls: mpsc::Sender<Call>,
}

pub(super) struct Server {
    url: String,
    authorization: String,
    task: JoinHandle<()>,
    pub calls: mpsc::Receiver<Call>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Server {
    pub(super) async fn start(tools: Vec<Value>) -> Result<Self, String> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|_| "Não foi possível abrir a ponte local Muse.")?;
        let address = listener
            .local_addr()
            .map_err(|_| "Ponte Muse indisponível.")?;
        let mut secret = [0_u8; 32];
        getrandom::fill(&mut secret).map_err(|_| "Não foi possível proteger a ponte Muse.")?;
        let authorization = format!(
            "Bearer {}",
            secret
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let (sender, calls) = mpsc::channel(32);
        let endpoint = Endpoint {
            authorization: authorization.clone(),
            tools: tools.into(),
            calls: sender,
        };
        let router = Router::new()
            .route("/mcp", post(handle))
            .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
            .with_state(endpoint);
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(Self {
            url: format!("http://{address}/mcp"),
            authorization,
            task,
            calls,
        })
    }

    pub(super) fn config(&self) -> Value {
        json!({"transport":"streamableHttp","url":self.url,"mode":"required",
            "headers":{"Authorization":self.authorization}})
    }
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Response {
    Json(json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})).into_response()
}

async fn handle(State(endpoint): State<Endpoint>, headers: HeaderMap, body: Bytes) -> Response {
    if headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        != Some(endpoint.authorization.as_str())
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // Native clients have no Origin. Browser requests do not belong to this private bridge.
    if headers.contains_key("origin") {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Ok(request) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let id = &request["id"];
    if request["jsonrpc"] != "2.0" || !request.is_object() {
        return rpc_error(id, -32600, "Invalid request");
    }
    let Some(method) = request["method"].as_str() else {
        return rpc_error(id, -32600, "Invalid method");
    };
    if request.get("id").is_none() {
        return if method == "notifications/initialized" {
            StatusCode::ACCEPTED.into_response()
        } else {
            StatusCode::BAD_REQUEST.into_response()
        };
    }
    if !id.is_string() && !id.is_i64() && !id.is_u64() {
        return rpc_error(&Value::Null, -32600, "Invalid request id");
    }
    let result = match method {
        "initialize" => json!({"protocolVersion":"2025-03-26","capabilities":{"tools":{}},
            "serverInfo":{"name":"jarvis","version":env!("CARGO_PKG_VERSION")}}),
        "ping" => json!({}),
        "tools/list" => json!({"tools":endpoint.tools}),
        "tools/call" => {
            let params = &request["params"];
            if !params["name"].is_string() || !params["arguments"].is_object() {
                return rpc_error(id, -32602, "Tool name and object arguments are required");
            }
            let (reply, result) = oneshot::channel();
            if endpoint
                .calls
                .try_send(Call {
                    id: id.clone(),
                    params: params.clone(),
                    reply,
                    received: std::time::Instant::now(),
                })
                .is_err()
            {
                return rpc_error(id, -32000, "Jarvis tool queue unavailable");
            }
            match result.await {
                Ok(value) => value,
                Err(_) => {
                    return rpc_error(
                        id,
                        -32000,
                        "Jarvis tool request ended; inspect its receipt before retrying",
                    )
                }
            }
        }
        _ => return rpc_error(id, -32601, "Unsupported Jarvis MCP method"),
    };
    Json(json!({"jsonrpc":"2.0","id":id,"result":result})).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn private_mcp_requires_auth_rejects_browser_origins_and_preserves_request_ids() {
        let server = Server::start(vec![json!({"name":"read_file"})])
            .await
            .unwrap();
        let client = reqwest::Client::new();
        let request = json!({"jsonrpc":"2.0","id":"7","method":"tools/list"});
        assert_eq!(
            client
                .post(&server.url)
                .json(&request)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            client
                .post(&server.url)
                .header("Authorization", &server.authorization)
                .header("Origin", "https://attacker.test")
                .json(&request)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        let value: Value = client
            .post(&server.url)
            .header("Authorization", &server.authorization)
            .json(&request)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(value["id"], "7");
        assert_eq!(value["result"]["tools"][0]["name"], "read_file");
    }

    #[tokio::test]
    async fn calls_wait_for_the_native_handler_and_malformed_arguments_never_enter_the_queue() {
        let mut server = Server::start(vec![]).await.unwrap();
        let client = reqwest::Client::new();
        let request = json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"read_file","arguments":{}}});
        let send = client
            .post(&server.url)
            .header("Authorization", &server.authorization)
            .json(&request)
            .send();
        let answer = async {
            let call = server.calls.recv().await.unwrap();
            assert_eq!(call.id, 7);
            call.reply
                .send(json!({"content":[{"type":"text","text":"confirmed"}]}))
                .unwrap();
        };
        let (response, ()) = tokio::join!(send, answer);
        let value: Value = response.unwrap().json().await.unwrap();
        assert_eq!(value["result"]["content"][0]["text"], "confirmed");
        let mut bad = request;
        bad["params"]["arguments"] = json!([]);
        let value: Value = client
            .post(&server.url)
            .header("Authorization", &server.authorization)
            .json(&bad)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(value["error"]["code"], -32602);
        assert!(server.calls.try_recv().is_err());
    }
}
