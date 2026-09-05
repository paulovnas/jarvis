//! Antigravity's Google OAuth, Cloud Code Assist project and account catalog.
//! Protocol reference: OMP's OAuth registry and Antigravity discovery adapter.
use super::{
    build_codex_client, current_time_millis, CodexCredential, ProviderError, ProviderModel,
};
use base64::Engine;
use serde_json::{json, Value};
use std::{
    io::Read,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

mod catalog;

pub(crate) const ENDPOINTS: [&str; 2] = [
    "https://daily-cloudcode-pa.googleapis.com",
    "https://daily-cloudcode-pa.sandbox.googleapis.com",
];
const AUTHORIZE: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN: &str = "https://oauth2.googleapis.com/token";
const USERINFO: &str = "https://www.googleapis.com/oauth2/v1/userinfo?alt=json";
// Public installed-app OAuth client shipped by the Antigravity reference client.
const CLIENT_ID: &str = "MTA3MTAwNjA2MDU5MS10bWhzc2luMmgyMWxjcmUyMzV2dG9sb2poNGc0MDNlcC5hcHBzLmdvb2dsZXVzZXJjb250ZW50LmNvbQ==";
const CLIENT_SECRET: &str = "R09DU1BYLUs1OEZXUjQ4NkxkTEoxbUxCOHNYQzR6NnFEQWY=";
const SCOPES: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile https://www.googleapis.com/auth/cclog https://www.googleapis.com/auth/experimentsandconfigs";
static CLIENT_VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn decode(value: &str) -> Result<String, ProviderError> {
    String::from_utf8(
        base64::engine::general_purpose::STANDARD
            .decode(value)
            .map_err(|_| invalid())?,
    )
    .map_err(|_| invalid())
}
fn invalid() -> ProviderError {
    ProviderError::new(
        "antigravity_response",
        "O Antigravity retornou uma resposta inválida. Tente conectar novamente.",
    )
}
fn check_cancel(cancel: &AtomicBool) -> Result<(), ProviderError> {
    if cancel.load(Ordering::Acquire) {
        Err(ProviderError::new("cancelled", "A conexão foi cancelada."))
    } else {
        Ok(())
    }
}

pub(super) fn authorization_url(
    redirect: &str,
    challenge: &str,
    state: &str,
) -> Result<String, ProviderError> {
    let mut url = url::Url::parse(AUTHORIZE).map_err(|_| invalid())?;
    url.query_pairs_mut()
        .append_pair("client_id", &decode(CLIENT_ID)?)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", redirect)
        .append_pair("scope", SCOPES)
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");
    Ok(url.into())
}

pub(crate) fn user_agent() -> String {
    let version = CLIENT_VERSION.get().map(String::as_str).unwrap_or("2.8.0");
    format!("antigravity/hub/{version} (aidev_client; os_type=darwin; arch=arm64; cl=963137146)")
}

fn discover_client_version(client: &reqwest::blocking::Client) {
    if CLIENT_VERSION.get().is_some() {
        return;
    }
    let Ok(response) = client.get("https://antigravity-hub-auto-updater-974169037036.us-central1.run.app/manifest/latest-arm64-mac.yml").timeout(Duration::from_secs(5)).send() else { return; };
    if !response.status().is_success() {
        return;
    }
    let mut text = String::new();
    if response.take(16384).read_to_string(&mut text).is_err() {
        return;
    }
    for line in text.lines() {
        if let Some(version) = line.trim().strip_prefix("version:") {
            let version = version.trim().trim_matches(['\'', '"']);
            if version.len() <= 24
                && version.split('.').count() == 3
                && version
                    .split('.')
                    .all(|part| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit()))
            {
                let _ = CLIENT_VERSION.set(version.into());
            }
            break;
        }
    }
}

fn read_json(response: reqwest::blocking::Response) -> Result<Value, ProviderError> {
    if !response.status().is_success() {
        return Err(match response.status().as_u16() {
            401 => ProviderError::new("antigravity_auth", "A autorização Google expirou. Reconecte a conta."),
            403 => ProviderError::new("antigravity_access", "A conta Google não tem acesso ao Antigravity. Verifique o acesso no aplicativo oficial e tente novamente."),
            429 => ProviderError::new("antigravity_limit", "O limite do Antigravity foi atingido. Tente novamente mais tarde."),
            _ => ProviderError::new("antigravity_request", "Não foi possível concluir a solicitação ao Antigravity. Tente novamente."),
        });
    }
    let mut bytes = Vec::new();
    response
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(invalid());
    }
    serde_json::from_slice(&bytes).map_err(|_| invalid())
}
fn send(request: reqwest::blocking::RequestBuilder) -> Result<Value, ProviderError> {
    read_json(request.send().map_err(|_| {
        ProviderError::new(
            "antigravity_network",
            "Não foi possível conectar ao Google. Verifique sua conexão.",
        )
    })?)
}

fn token_request(
    client: &reqwest::blocking::Client,
    endpoint: &str,
    fields: &[(&str, &str)],
) -> Result<Value, ProviderError> {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    form.append_pair("client_id", &decode(CLIENT_ID)?)
        .append_pair("client_secret", &decode(CLIENT_SECRET)?);
    for (key, value) in fields {
        form.append_pair(key, value);
    }
    send(
        client
            .post(endpoint)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(form.finish()),
    )
}
fn apply_token(
    value: &Value,
    old: Option<&CodexCredential>,
) -> Result<CodexCredential, ProviderError> {
    let access = value["access_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(invalid)?;
    let refresh = value["refresh_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| old.map(|c| c.refresh.as_str()))
        .ok_or_else(|| {
            ProviderError::new(
                "missing_refresh",
                "O Google não retornou uma autorização permanente. Tente conectar novamente.",
            )
        })?;
    let seconds = value["expires_in"]
        .as_i64()
        .filter(|v| *v > 0 && *v <= 31_536_000)
        .ok_or_else(invalid)?;
    let mut credential = old
        .cloned()
        .unwrap_or_else(|| CodexCredential::new("", "", 0, "", None, None));
    credential.access = access.into();
    credential.refresh = refresh.into();
    credential.expires = current_time_millis()? + seconds * 1000 - 300_000.min(seconds * 500);
    Ok(credential)
}

fn control(
    client: &reqwest::blocking::Client,
    endpoint: &str,
    access: &str,
    action: &str,
    body: &Value,
) -> Result<Value, ProviderError> {
    send(
        client
            .post(format!("{endpoint}/v1internal:{action}"))
            .bearer_auth(access)
            .header("user-agent", user_agent())
            .json(body),
    )
}
fn project(value: &Value) -> Option<&str> {
    value["cloudaicompanionProject"]
        .as_str()
        .or_else(|| value["cloudaicompanionProject"]["id"].as_str())
        .filter(|s| !s.is_empty() && s.len() <= 512)
}
fn load_project(
    client: &reqwest::blocking::Client,
    endpoint: &str,
    access: &str,
) -> Result<Value, ProviderError> {
    let value = control(
        client,
        endpoint,
        access,
        "loadCodeAssist",
        &json!({"metadata":{"ideType":"ANTIGRAVITY"}}),
    )?;
    if value["paidTier"].is_null() {
        if let Some(id) = project(&value) {
            return control(
                client,
                endpoint,
                access,
                "loadCodeAssist",
                &json!({"cloudaicompanionProject":id,"metadata":{"ideType":"ANTIGRAVITY"}}),
            );
        }
    }
    Ok(value)
}
fn discover_project(
    client: &reqwest::blocking::Client,
    endpoint: &str,
    access: &str,
    cancel: &AtomicBool,
) -> Result<String, ProviderError> {
    check_cancel(cancel)?;
    let initial = load_project(client, endpoint, access)?;
    let allowed = initial["allowedTiers"]
        .as_array()
        .is_some_and(|tiers| tiers.iter().any(|t| t["id"] == "free-tier"));
    if !allowed
        && initial["ineligibleTiers"]
            .as_array()
            .is_some_and(|tiers| tiers.iter().any(|t| t["tierId"] == "free-tier"))
    {
        return Err(ProviderError::new("antigravity_ineligible", "Esta conta precisa concluir a ativação ou verificação no Antigravity oficial antes de conectar."));
    }
    if initial["currentTier"].is_null() {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut operation = control(
            client,
            endpoint,
            access,
            "onboardUser",
            &json!({"tierId":"free-tier","metadata":{"ideType":"ANTIGRAVITY"}}),
        )?;
        while operation["done"] != true {
            check_cancel(cancel)?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ProviderError::new(
                    "antigravity_timeout",
                    "A ativação no Antigravity demorou demais. Tente novamente.",
                ));
            }
            let name = operation["name"]
                .as_str()
                .filter(|s| {
                    !s.is_empty()
                        && s.len() < 1024
                        && s.split('/').all(|p| {
                            !p.is_empty()
                                && p != ".."
                                && p.bytes()
                                    .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
                        })
                })
                .ok_or_else(invalid)?;
            std::thread::sleep(Duration::from_millis(500).min(remaining));
            check_cancel(cancel)?;
            operation = send(
                client
                    .get(format!("{endpoint}/v1internal/{name}"))
                    .bearer_auth(access)
                    .header("user-agent", user_agent())
                    .timeout(remaining),
            )?;
        }
        if !operation["error"].is_null() {
            return Err(ProviderError::new(
                "antigravity_activation",
                "Não foi possível ativar esta conta no Antigravity.",
            ));
        }
    }
    check_cancel(cancel)?;
    project(&load_project(client, endpoint, access)?).map(str::to_owned).ok_or_else(|| ProviderError::new("antigravity_project", "O Google não disponibilizou um projeto para esta conta. Abra o Antigravity oficial e tente conectar novamente."))
}

pub(super) fn exchange(
    code: &str,
    verifier: &str,
    redirect: &str,
    cancel: &AtomicBool,
) -> Result<CodexCredential, ProviderError> {
    let client = build_codex_client().map_err(|_| invalid())?;
    discover_client_version(&client);
    exchange_with(
        &client,
        TOKEN,
        USERINFO,
        ENDPOINTS[0],
        code,
        verifier,
        redirect,
        cancel,
    )
}
#[allow(clippy::too_many_arguments)]
fn exchange_with(
    client: &reqwest::blocking::Client,
    token_url: &str,
    userinfo: &str,
    endpoint: &str,
    code: &str,
    verifier: &str,
    redirect: &str,
    cancel: &AtomicBool,
) -> Result<CodexCredential, ProviderError> {
    check_cancel(cancel)?;
    let value = token_request(
        client,
        token_url,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", redirect),
        ],
    )?;
    let mut credential = apply_token(&value, None)?;
    check_cancel(cancel)?;
    let profile = send(client.get(userinfo).bearer_auth(&credential.access))?;
    let id = profile["id"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256)
        .ok_or_else(invalid)?;
    credential.account_id = format!("google:{id}");
    credential.email = profile["email"]
        .as_str()
        .filter(|s| s.len() <= 320)
        .map(|s| s.trim().to_lowercase());
    credential.project_id = Some(discover_project(
        client,
        endpoint,
        &credential.access,
        cancel,
    )?);
    Ok(credential)
}
pub(super) fn refresh(old: &CodexCredential) -> Result<CodexCredential, ProviderError> {
    if old.project_id.is_none() {
        return Err(invalid());
    }
    let client = build_codex_client().map_err(|_| invalid())?;
    apply_token(
        &token_request(
            &client,
            TOKEN,
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", &old.refresh),
            ],
        )?,
        Some(old),
    )
}

pub(crate) fn reasoning(id: &str, model: &Value) -> Vec<String> {
    if let Some(routes) = model["_routes"].as_object() {
        return ["none", "low", "medium", "high"]
            .into_iter()
            .filter(|effort| routes.contains_key(*effort))
            .map(str::to_owned)
            .collect();
    }
    if model["supportsThinking"] != true {
        return vec![];
    }
    let levels: &[&str] = if id.starts_with("gemini-3.1-pro") || id.starts_with("gemini-3-pro") {
        &["low", "high"]
    } else {
        &["low", "medium", "high"]
    };
    levels.iter().map(|s| (*s).into()).collect()
}
#[cfg(test)]
fn normalize_models(payload: &Value) -> Option<Vec<ProviderModel>> {
    Some(model_list(&catalog::models(payload)?))
}
fn model_list(models: &std::collections::BTreeMap<String, Value>) -> Vec<ProviderModel> {
    let mut result = Vec::new();
    for (id, value) in models {
        if !value.is_object()
            || value["isInternal"] == true
            || ["chat_20706", "chat_23310", "gemini-2.5-pro"].contains(&id.as_str())
            || id.is_empty()
            || id.len() > 200
        {
            continue;
        }
        let levels = reasoning(id, value);
        let default = if levels.iter().any(|s| s == "medium") {
            Some("medium".into())
        } else {
            levels.last().cloned()
        };
        result.push(ProviderModel {
            id: id.clone(),
            name: value["displayName"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or(id)
                .into(),
            reasoning_levels: levels,
            default_reasoning_level: default,
            context_window: Some(
                value["maxTokens"]
                    .as_u64()
                    .filter(|v| *v > 0)
                    .unwrap_or(200_000),
            ),
        });
    }
    result.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    result
}
pub(super) fn fetch_models(
    client: &reqwest::blocking::Client,
    credential: &mut CodexCredential,
) -> Option<Vec<ProviderModel>> {
    discover_client_version(client);
    for endpoint in ENDPOINTS {
        let Ok(payload) = control(
            client,
            endpoint,
            &credential.access,
            "fetchAvailableModels",
            &json!({}),
        ) else {
            continue;
        };
        let Some(catalog) = catalog::models(&payload) else {
            continue;
        };
        let models = model_list(&catalog);
        credential.antigravity_models = catalog;
        credential.antigravity_endpoint = Some(endpoint.into());
        return Some(models);
    }
    None
}

#[cfg(test)]
mod tests;
