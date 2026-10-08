//! ChatGPT App connector IDs use one fixed first-party MCP gateway.
use crate::{
    mcp::{self, config::Config, McpError, Server},
    openai_codex::OpenAiCodexState,
    persistence::AppState,
};
use futures_util::stream::BoxStream;
use reqwest::header::{HeaderName, HeaderValue};
use rmcp::{
    model::{ClientJsonRpcMessage, Tool},
    transport::streamable_http_client::{
        SseError, StreamableHttpClient, StreamableHttpError, StreamableHttpPostResponse,
    },
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sse_stream::Sse;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub(crate) const GATEWAY: &str = "https://chatgpt.com/backend-api/ps/mcp";

#[derive(Clone)]
pub(crate) struct Context {
    pub state: AppState,
    pub oauth: OpenAiCodexState,
    pub home: PathBuf,
}

pub(crate) fn server_id(plugin: &str) -> String {
    let digest = Sha256::digest(plugin.as_bytes());
    format!(
        "plugin-app:{}",
        digest[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

pub(crate) fn servers(
    home: &Path,
    project: Option<&Path>,
) -> Result<Vec<(Server, Config)>, McpError> {
    let catalog = super::catalog(home)
        .map_err(|_| mcp::error("Não foi possível acessar os plugins de Apps."))?;
    let overlay = super::load_active_for_project(home, project)
        .map_err(|_| mcp::error("Não foi possível acessar os plugins de Apps."))?;
    let mut groups = BTreeMap::new();
    for app in overlay.apps {
        groups.entry(app.plugin_id.clone()).or_insert(app);
    }
    Ok(groups
        .into_iter()
        .map(|(plugin, app)| {
            let digest = Sha256::digest(
                format!(
                    "{}:{}",
                    app.plugin_hash,
                    catalog.apps_account_id.as_deref().unwrap_or_default()
                )
                .as_bytes(),
            );
            let revision =
                i64::from_be_bytes(digest[..8].try_into().unwrap_or_default()) & i64::MAX;
            let server = Server {
                id: server_id(&plugin),
                name: format!("{plugin} · Apps"),
                kind: "http".into(),
                enabled: true,
                configured: catalog.apps_account_id.is_some(),
                revision,
                last_check: None,
            };
            let config = Config::Remote {
                url: GATEWAY.into(),
                headers: BTreeMap::new(),
                oauth: Some(false),
                enabled: true,
                timeout: 30_000,
                request_timeout: 300_000,
            };
            (server, config)
        })
        .collect())
}

pub(crate) fn connector_ids(
    home: &Path,
    server: &Server,
    project: Option<&Path>,
) -> Result<HashSet<String>, McpError> {
    let overlay = super::load_active_for_project(home, project)
        .map_err(|_| mcp::error("Não foi possível acessar os Apps deste plugin."))?;
    Ok(overlay
        .apps
        .into_iter()
        .filter(|app| server_id(&app.plugin_id) == server.id)
        .map(|app| app.id)
        .collect())
}

pub(crate) fn allows_tool(ids: &HashSet<String>, tool: &Tool) -> bool {
    serde_json::to_value(tool)
        .ok()
        .and_then(|value| value["_meta"]["connector_id"].as_str().map(str::to_owned))
        .is_some_and(|id| ids.contains(&id))
}

pub(crate) fn auth_failure(value: &Value, expected_connector: Option<&str>) -> Option<String> {
    let failure = &value["_meta"]["_codex_apps"]["connector_auth_failure"];
    if value["isError"].as_bool() != Some(true)
        || failure["is_auth_failure"].as_bool() != Some(true)
        || expected_connector.is_none()
        || failure["connector_id"].as_str() != expected_connector
    {
        return None;
    }
    let link = failure["connector_id"]
        .as_str()
        .and_then(|id| connect_url(failure["connector_name"].as_str().unwrap_or(id), id));
    Some(link.map_or_else(
        || "Autorize o App no ChatGPT e atualize a conexão do plugin.".into(),
        |url| format!("Autorize o App no ChatGPT: {url}. Atualize a conexão após a autorização."),
    ))
}

pub(crate) fn connect_url(name: &str, connector: &str) -> Option<String> {
    if connector.is_empty()
        || connector.len() > 200
        || !connector
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return None;
    }
    let slug: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    Some(format!(
        "https://chatgpt.com/apps/{}/{connector}",
        slug.trim_matches('-')
    ))
}

#[derive(Clone)]
pub(crate) struct AppHttpClient {
    context: Context,
    alias: String,
    account_id: String,
    client: reqwest::Client,
}

#[derive(Clone)]
pub(crate) struct AppBinding {
    alias: String,
    account_id: String,
}

pub(crate) fn binding(
    context: &Context,
    server: &Server,
    project: Option<&Path>,
) -> Result<AppBinding, McpError> {
    let current = servers(&context.home, project)?
        .into_iter()
        .find(|(candidate, _)| {
            candidate.id == server.id
                && candidate.revision == server.revision
                && candidate.configured
        });
    if current.is_none() {
        return Err(mcp::coded_error(
            "plugin_apps_changed",
            "A configuração dos Apps mudou. Inicie uma nova execução.",
        ));
    }
    let alias = super::catalog(&context.home)
        .map_err(|_| mcp::error("Não foi possível acessar a conta dos Apps."))?
        .apps_account_id
        .ok_or_else(|| {
            mcp::coded_error(
                "plugin_apps_account_required",
                "Selecione uma conta ChatGPT nas configurações dos plugins.",
            )
        })?;
    let records = context
        .state
        .list_provider_accounts(&context.home)
        .map_err(|_| mcp::error("Não foi possível acessar a conta dos Apps."))?;
    let account = records
        .into_iter()
        .find(|account| {
            account.alias == alias && account.enabled && account.provider_kind == "openai-codex"
        })
        .ok_or_else(|| {
            mcp::coded_error(
                "plugin_apps_account_required",
                "Selecione uma conta ChatGPT ativa nas configurações dos plugins.",
            )
        })?;
    Ok(AppBinding {
        alias,
        account_id: account.account_id,
    })
}

pub(crate) async fn http_client_bound(
    context: Context,
    server: Server,
    prior: Option<AppBinding>,
    project: Option<&Path>,
) -> Result<AppHttpClient, McpError> {
    if !server.id.starts_with("plugin-app:") {
        return Err(mcp::error("O servidor não pertence aos Apps."));
    }
    let binding = prior.map_or_else(|| binding(&context, &server, project), Ok)?;
    let alias = binding.alias;
    let credentials_context = context.clone();
    let credentials_alias = alias.clone();
    let account_id = tauri::async_runtime::spawn_blocking(move || {
        credentials_context
            .oauth
            .app_credential(
                &credentials_context.state,
                &credentials_context.home,
                &credentials_alias,
            )
            .map(|credential| credential.account_id)
    })
    .await
    .map_err(|_| mcp::error("Não foi possível verificar a conta dos Apps."))?
    .map_err(|_| {
        mcp::coded_error(
            "plugin_apps_account_required",
            "Reconecte uma conta ChatGPT ativa para usar os Apps.",
        )
    })?;
    if account_id != binding.account_id {
        return Err(mcp::coded_error(
            "plugin_apps_account_changed",
            "A conta dos Apps mudou. Inicie uma nova execução.",
        ));
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| mcp::error("Não foi possível preparar a conexão dos Apps."))?;
    Ok(AppHttpClient {
        context,
        alias,
        account_id,
        client,
    })
}

impl AppHttpClient {
    pub(crate) fn binding(&self) -> AppBinding {
        AppBinding {
            alias: self.alias.clone(),
            account_id: self.account_id.clone(),
        }
    }
    async fn credentials(
        &self,
        uri: &str,
        mut headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(String, HashMap<HeaderName, HeaderValue>), StreamableHttpError<reqwest::Error>>
    {
        if uri != GATEWAY {
            return Err(StreamableHttpError::UnexpectedServerResponse(
                "O token dos Apps só pode ser enviado ao gateway oficial.".into(),
            ));
        }
        let context = self.context.clone();
        let alias = self.alias.clone();
        let credential = tauri::async_runtime::spawn_blocking(move || {
            context
                .oauth
                .app_credential(&context.state, &context.home, &alias)
        })
        .await
        .map_err(|_| {
            StreamableHttpError::UnexpectedServerResponse(
                "Não foi possível verificar a autenticação dos Apps.".into(),
            )
        })?
        .map_err(|_| {
            StreamableHttpError::UnexpectedServerResponse(
                "A conta ChatGPT exige nova autenticação.".into(),
            )
        })?;
        if credential.account_id != self.account_id {
            return Err(StreamableHttpError::UnexpectedServerResponse(
                "A conta dos Apps mudou. Inicie uma nova execução.".into(),
            ));
        }
        for name in [
            "authorization",
            "chatgpt-account-id",
            "x-openai-product-sku",
            "originator",
        ] {
            headers.remove(name);
        }
        headers.insert(
            HeaderName::from_static("chatgpt-account-id"),
            HeaderValue::from_str(&self.account_id).map_err(|_| {
                StreamableHttpError::UnexpectedServerResponse("Identidade ChatGPT inválida.".into())
            })?,
        );
        headers.insert(
            HeaderName::from_static("x-openai-product-sku"),
            HeaderValue::from_static("codex"),
        );
        Ok((credential.access, headers))
    }
}

impl StreamableHttpClient for AppHttpClient {
    type Error = reqwest::Error;

    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        _auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        self.post_message_with_max_sse_event_size(
            uri,
            message,
            session_id,
            None,
            headers,
            1024 * 1024,
        )
        .await
    }
    async fn post_message_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        _auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
        limit: usize,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        let (token, headers) = self.credentials(&uri, headers).await?;
        // No replay after an uncertain tool result; token refresh precedes the request.
        self.client
            .post_message_with_max_sse_event_size(
                uri,
                message,
                session_id,
                Some(token),
                headers,
                limit,
            )
            .await
    }
    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        _auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), StreamableHttpError<Self::Error>> {
        let (token, headers) = self.credentials(&uri, headers).await?;
        self.client
            .delete_session(uri, session_id, Some(token), headers)
            .await
    }
    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        _auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, StreamableHttpError<Self::Error>> {
        self.get_stream_with_max_sse_event_size(
            uri,
            session_id,
            last_event_id,
            None,
            headers,
            1024 * 1024,
        )
        .await
    }
    async fn get_stream_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        _auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
        limit: usize,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, StreamableHttpError<Self::Error>> {
        let (token, headers) = self.credentials(&uri, headers).await?;
        self.client
            .get_stream_with_max_sse_event_size(
                uri,
                session_id,
                last_event_id,
                Some(token),
                headers,
                limit,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn project_only_apps_keep_their_connector_scope_and_exact_account_binding() {
        let home = tempfile::tempdir().unwrap();
        let project = home.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        let state = AppState::default();
        let secrets = crate::openai_codex::InMemorySecretStore::default();
        let alias = "openai-codex-plugin-apps";
        state
            .with_connection(home.path(), |db| {
                crate::openai_codex::commit_provider_account(
                    db,
                    &secrets,
                    alias,
                    &crate::openai_codex::CodexCredential::new(
                        "fixture-access",
                        "fixture-refresh",
                        i64::MAX,
                        "fixture-account",
                        None,
                        None,
                    ),
                )
                .unwrap();
                Ok::<_, crate::persistence::PersistenceError>(())
            })
            .unwrap();
        let draft = serde_json::from_value(serde_json::json!({"name":"project-apps","description":"Scoped connector","apps":{"mail":{"id":"connector-one"}}})).unwrap();
        let prepared =
            super::super::preview(home.path(), 0, super::super::Operation::Create { draft })
                .await
                .unwrap();
        let mut catalog = super::super::apply(home.path(), &prepared).unwrap();
        let plugin_id = catalog.installed[0].id.clone();
        for operation in [
            super::super::Operation::SetAppsAccount {
                account_id: Some(alias.into()),
            },
            super::super::Operation::SetEnabled {
                plugin_id: plugin_id.clone(),
                enabled: false,
                project_path: None,
            },
            super::super::Operation::SetEnabled {
                plugin_id,
                enabled: true,
                project_path: Some(project.to_string_lossy().into_owned()),
            },
        ] {
            let prepared = super::super::preview(home.path(), catalog.revision, operation)
                .await
                .unwrap();
            catalog = super::super::apply(home.path(), &prepared).unwrap();
        }
        assert!(servers(home.path(), None).unwrap().is_empty());
        let server = servers(home.path(), Some(&project)).unwrap().remove(0).0;
        let context = Context {
            state: state.clone(),
            oauth: OpenAiCodexState::default(),
            home: home.path().to_owned(),
        };
        assert!(binding(&context, &server, None).is_err());
        let binding = binding(&context, &server, Some(&project)).unwrap();
        assert_eq!(binding.alias, alias);
        assert_eq!(binding.account_id, "fixture-account");
        assert_eq!(
            connector_ids(home.path(), &server, Some(&project)).unwrap(),
            HashSet::from(["connector-one".into()])
        );
        assert!(connector_ids(home.path(), &server, None)
            .unwrap()
            .is_empty());
        state.close();
    }

    #[test]
    fn connector_filter_and_auth_links_cannot_escape_the_gateway() {
        let ids = HashSet::from(["connector-one".into()]);
        let tool: Tool = serde_json::from_value(serde_json::json!({"name":"read","inputSchema":{"type":"object"},"_meta":{"connector_id":"connector-one"}})).unwrap();
        assert!(allows_tool(&ids, &tool));
        assert!(!allows_tool(&HashSet::from(["another".into()]), &tool));
        assert_eq!(
            connect_url("Google Drive", "connector-one").unwrap(),
            "https://chatgpt.com/apps/google-drive/connector-one"
        );
        assert!(connect_url("Mail", "../../steal").is_none());
        let mut failure = serde_json::json!({"isError":true,"_meta":{"_codex_apps":{"connector_auth_failure":{"is_auth_failure":true,"connector_id":"connector-one"}}}});
        assert!(auth_failure(&failure, Some("connector-one")).is_some());
        assert!(auth_failure(&failure, Some("another-connector")).is_none());
        assert!(auth_failure(&failure, None).is_none());
        failure["isError"] = serde_json::json!(false);
        assert!(auth_failure(&failure, Some("connector-one")).is_none());
        assert!(auth_failure(&serde_json::json!({"isError":true,"_meta":{"_codex_apps":{"connector_auth_failure":true}}}),Some("connector-one")).is_none());
    }
}
