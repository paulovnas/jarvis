//! Subscription quotas use the dashboard session; API/device tokens cannot read it.
use super::{error, CodexCredential, OpenAiCodexState, ProviderError};
use crate::{
    agent::browser::extension,
    persistence::{AppState, ProviderAccountRecord},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};

const ORIGIN: &str = "https://hyper.charm.land";
const SCRIPT: &str = include_str!("panel.js");

fn changed() -> ProviderError {
    ProviderError::new(
        "hyper_panel_changed",
        "A conta ou a chave Hyper mudou durante a conexão. Abra o painel novamente.",
    )
}

fn identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Connection {
    user_id: String,
    team_id: String,
    team_name: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Team {
    id: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    connected: bool,
    team_name: Option<String>,
    team_id: Option<String>,
    teams: Vec<Team>,
    flow_id: Option<String>,
}

impl Status {
    fn saved(connection: Option<&Connection>) -> Self {
        Self {
            connected: connection.is_some(),
            team_name: connection.map(|c| c.team_name.clone()),
            team_id: connection.map(|c| c.team_id.clone()),
            teams: vec![],
            flow_id: None,
        }
    }
}

#[derive(Clone)]
struct Flow {
    id: String,
    account_id: String,
    created_at: i64,
    key_hash: Vec<u8>,
    started: Instant,
}

type CachedUsage = (Instant, Result<Value, ProviderError>);

#[derive(Default)]
pub(in crate::openai_codex) struct State {
    flows: Mutex<HashMap<String, Flow>>,
    usage: tokio::sync::Mutex<HashMap<String, CachedUsage>>,
}

pub(crate) fn scope(account_id: &str) -> String {
    format!("hyper-panel:{account_id}")
}

fn snapshot(
    state: &AppState,
    home: &Path,
    oauth: &OpenAiCodexState,
    alias: &str,
) -> Result<(ProviderAccountRecord, CodexCredential), ProviderError> {
    let credential = super::credential(state, home, oauth.manager.secret_store.as_ref(), alias)?;
    let record = state
        .list_provider_accounts(home)
        .map_err(|_| ProviderError::database())?
        .into_iter()
        .find(|record| {
            record.alias == alias && record.provider_kind == super::KIND && record.enabled
        })
        .ok_or_else(|| error("Ative a conta Charm Hyper para conectar o painel."))?;
    if record.account_id != credential.account_id {
        return Err(changed());
    }
    Ok((record, credential))
}

fn matches(flow: &Flow, record: &ProviderAccountRecord, credential: &CodexCredential) -> bool {
    flow.started.elapsed() < Duration::from_secs(600)
        && flow.account_id == record.account_id
        && flow.created_at == record.created_at
        && flow.key_hash == Sha256::digest(credential.access.as_bytes()).to_vec()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Probe {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    teams: Vec<Team>,
    #[serde(default)]
    team_id: Option<String>,
    #[serde(default)]
    dashboard: Option<Value>,
}

fn probe(
    value: Value,
    expected_team: Option<&str>,
    expected_user: Option<&str>,
) -> Result<Probe, ProviderError> {
    let probe: Probe = serde_json::from_value(value)
        .map_err(|_| error("O painel Hyper retornou dados inválidos."))?;
    if let Some(cause) = probe.error.as_deref() {
        return Err(error(match cause {
            "different_user" => "A conta aberta no Hyper mudou. Reconecte o painel para confirmar a conta das cotas.",
            "different_team" => "A equipe conectada não está disponível nesta sessão Hyper. Reconecte o painel.",
            "wrong_origin" => "A aba das cotas saiu do Hyper. Reconecte o painel nas configurações do provedor.",
            _ => "Entre no Hyper na aba das cotas e verifique a conexão novamente.",
        }));
    }
    if probe.user_id.as_ref().is_none_or(|id| !identifier(id))
        || probe.teams.is_empty()
        || probe.teams.len() > 100
        || probe.teams.iter().any(|team| {
            !identifier(&team.id) || team.name.trim().is_empty() || team.name.len() > 256
        })
        || probe
            .team_id
            .as_ref()
            .is_some_and(|id| !probe.teams.iter().any(|team| &team.id == id))
    {
        return Err(error(
            "O painel Hyper não confirmou o usuário e as equipes da conexão.",
        ));
    }
    if expected_team.is_some_and(|id| probe.team_id.as_deref() != Some(id))
        || expected_user.is_some_and(|id| probe.user_id.as_deref() != Some(id))
    {
        return Err(error(
            "A sessão Hyper não corresponde à conta e à equipe conectadas. Reconecte o painel.",
        ));
    }
    Ok(probe)
}

async fn request(
    app: &tauri::AppHandle,
    owner: &str,
    request: Value,
) -> Result<Value, ProviderError> {
    tokio::time::timeout(Duration::from_secs(15), extension::request(app, owner, request))
        .await.map_err(|_| error("O navegador não confirmou a consulta Hyper a tempo. Verifique a aba e tente novamente."))?
        .map_err(|_| error("Conecte a extensão do Jarvis e mantenha a aba do painel aberta para consultar as cotas Hyper."))
}

async fn tab(app: &tauri::AppHandle, owner: &str) -> Result<Option<String>, ProviderError> {
    let catalog = request(app, owner, json!({"action":"list"})).await?;
    Ok(catalog["tabs"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|tab| {
            let url = url::Url::parse(tab["url"].as_str()?).ok()?;
            (url.origin().ascii_serialization() == ORIGIN)
                .then(|| tab["id"].as_str().map(str::to_owned))
                .flatten()
        }))
}

async fn read(
    app: &tauri::AppHandle,
    owner: &str,
    team_id: Option<&str>,
    user_id: Option<&str>,
) -> Result<Probe, ProviderError> {
    if team_id.is_some_and(|id| !identifier(id)) {
        return Err(error("Selecione uma equipe Hyper válida."));
    }
    let id = tab(app, owner).await?.ok_or_else(|| {
        error("Abra a conexão do painel Hyper nas configurações e mantenha essa aba aberta.")
    })?;
    let expression = SCRIPT.replace(
        "__JARVIS_HYPER_SELECTION__",
        &json!({"teamId":team_id,"userId":user_id}).to_string(),
    );
    let value = request(
        app,
        owner,
        json!({"action":"evaluate","id":id,"expression":expression}),
    )
    .await?;
    probe(value, team_id, user_id)
}

#[tauri::command]
pub async fn get_charm_hyper_panel_status(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, AppState>,
    oauth_state: tauri::State<'_, OpenAiCodexState>,
    alias: String,
) -> Result<Status, ProviderError> {
    let home = super::super::home_dir(&app)?;
    let state = persistence_state.inner().clone();
    let oauth = oauth_state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (_, credential) = snapshot(&state, &home, &oauth, &alias)?;
        Ok(Status::saved(credential.hyper_panel.as_ref()))
    })
    .await
    .map_err(|_| ProviderError::internal())?
}

#[tauri::command]
pub async fn start_charm_hyper_panel_connection(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, AppState>,
    oauth_state: tauri::State<'_, OpenAiCodexState>,
    alias: String,
) -> Result<Status, ProviderError> {
    let home = super::super::home_dir(&app)?;
    let state = persistence_state.inner().clone();
    let oauth = oauth_state.inner().clone();
    let read_oauth = oauth.clone();
    let read_alias = alias.clone();
    let (record, credential) = tauri::async_runtime::spawn_blocking(move || {
        snapshot(&state, &home, &read_oauth, &read_alias)
    })
    .await
    .map_err(|_| ProviderError::internal())??;
    let owner = scope(&record.account_id);
    if let Some(id) = tab(&app, &owner).await? {
        request(&app, &owner, json!({"action":"attach","id":id})).await?;
        request(&app, &owner, json!({"action":"select","id":id})).await
    } else {
        request(
            &app,
            &owner,
            json!({"action":"open","url":format!("{ORIGIN}/app/overview"),"newWindow":false}),
        )
        .await
    }?;
    let flow = Flow {
        id: super::super::random_token(24)?,
        account_id: record.account_id,
        created_at: record.created_at,
        key_hash: Sha256::digest(credential.access.as_bytes()).to_vec(),
        started: Instant::now(),
    };
    let mut status = Status::saved(credential.hyper_panel.as_ref());
    status.flow_id = Some(flow.id.clone());
    let mut flows = oauth
        .manager
        .hyper_panel
        .flows
        .lock()
        .map_err(|_| ProviderError::internal())?;
    flows.retain(|_, flow| flow.started.elapsed() < Duration::from_secs(600));
    flows.insert(alias, flow);
    Ok(status)
}

#[tauri::command]
pub async fn finish_charm_hyper_panel_connection(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, AppState>,
    oauth_state: tauri::State<'_, OpenAiCodexState>,
    alias: String,
    flow_id: String,
    team_id: Option<String>,
) -> Result<Status, ProviderError> {
    let home = super::super::home_dir(&app)?;
    let state = persistence_state.inner().clone();
    let oauth = oauth_state.inner().clone();
    let flow = oauth
        .manager
        .hyper_panel
        .flows
        .lock()
        .map_err(|_| ProviderError::internal())?
        .get(&alias)
        .filter(|flow| flow.id == flow_id && flow.started.elapsed() < Duration::from_secs(600))
        .cloned()
        .ok_or_else(|| error("A conexão expirou. Abra o painel Hyper novamente."))?;
    let data = read(&app, &scope(&flow.account_id), team_id.as_deref(), None).await?;
    let Some(team_id) = data.team_id else {
        return Ok(Status {
            connected: false,
            team_name: None,
            team_id: None,
            teams: data.teams,
            flow_id: Some(flow_id),
        });
    };
    let dashboard = data
        .dashboard
        .as_ref()
        .ok_or_else(|| error("O painel Hyper não retornou as cotas da equipe."))?;
    let mut validation = super::super::usage::AccountUsage {
        alias: alias.clone(),
        fetched_at: None,
        email: None,
        plan: None,
        windows: vec![],
        reset_credits: None,
        credit_balance: None,
        error: None,
    };
    super::quotas::apply(dashboard, &mut validation)?;
    let connection = Connection {
        user_id: data.user_id.ok_or_else(ProviderError::internal)?,
        team_name: data
            .teams
            .iter()
            .find(|team| team.id == team_id)
            .ok_or_else(ProviderError::internal)?
            .name
            .clone(),
        team_id,
    };
    let manager = oauth.manager.clone();
    let status = tauri::async_runtime::spawn_blocking(move || {
        let _guard = oauth
            .manager
            .credentials_guard
            .lock()
            .map_err(|_| ProviderError::internal())?;
        let (record, mut credential) = snapshot(&state, &home, &oauth, &alias)?;
        let mut flows = oauth
            .manager
            .hyper_panel
            .flows
            .lock()
            .map_err(|_| ProviderError::internal())?;
        if !matches(&flow, &record, &credential)
            || flows.get(&alias).is_none_or(|active| active.id != flow_id)
        {
            return Err(changed());
        }
        credential.hyper_panel = Some(connection.clone());
        oauth
            .manager
            .secret_store
            .store(&alias, &credential)
            .map_err(|_| ProviderError::internal())?;
        flows.remove(&alias);
        oauth.manager.usage_cache.invalidate(&alias);
        Ok(Status::saved(Some(&connection)))
    })
    .await
    .map_err(|_| ProviderError::internal())??;
    manager.hyper_panel.usage.lock().await.clear();
    Ok(status)
}

#[tauri::command]
pub async fn disconnect_charm_hyper_panel(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, AppState>,
    oauth_state: tauri::State<'_, OpenAiCodexState>,
    alias: String,
) -> Result<(), ProviderError> {
    let home = super::super::home_dir(&app)?;
    let state = persistence_state.inner().clone();
    let oauth = oauth_state.inner().clone();
    let manager = oauth.manager.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = oauth
            .manager
            .credentials_guard
            .lock()
            .map_err(|_| ProviderError::internal())?;
        let (_, mut credential) = snapshot(&state, &home, &oauth, &alias)?;
        credential.hyper_panel = None;
        oauth
            .manager
            .secret_store
            .store(&alias, &credential)
            .map_err(|_| ProviderError::internal())?;
        oauth
            .manager
            .hyper_panel
            .flows
            .lock()
            .map_err(|_| ProviderError::internal())?
            .remove(&alias);
        oauth.manager.usage_cache.invalidate(&alias);
        Ok::<(), ProviderError>(())
    })
    .await
    .map_err(|_| ProviderError::internal())??;
    manager.hyper_panel.usage.lock().await.clear();
    Ok(())
}

pub(crate) async fn usage(
    app: &tauri::AppHandle,
    state: &AppState,
    home: &Path,
    oauth: &OpenAiCodexState,
    alias: &str,
) -> Result<Option<Value>, ProviderError> {
    let read_state = state.clone();
    let read_home = home.to_path_buf();
    let read_oauth = oauth.clone();
    let read_alias = alias.to_owned();
    let (record, credential) = tauri::async_runtime::spawn_blocking(move || {
        snapshot(&read_state, &read_home, &read_oauth, &read_alias)
    })
    .await
    .map_err(|_| ProviderError::internal())??;
    let Some(connection) = credential.hyper_panel.clone() else {
        return Ok(None);
    };
    let key = format!(
        "{}/{}/{:x}/{}/{}",
        record.account_id,
        record.created_at,
        Sha256::digest(credential.access.as_bytes()),
        connection.user_id,
        connection.team_id
    );
    let mut cache = oauth.manager.hyper_panel.usage.lock().await;
    cache.retain(|_, (at, _)| at.elapsed() < Duration::from_secs(60));
    if let Some((_, result)) = cache.get(&key) {
        return result.clone().map(Some);
    }
    let result = read(
        app,
        &scope(&record.account_id),
        Some(&connection.team_id),
        Some(&connection.user_id),
    )
    .await
    .and_then(|probe| {
        probe
            .dashboard
            .ok_or_else(|| error("O painel Hyper não informou as cotas da equipe conectada."))
    });
    let read_state = state.clone();
    let read_home = home.to_path_buf();
    let read_oauth = oauth.clone();
    let read_alias = alias.to_owned();
    let (current, saved) = tauri::async_runtime::spawn_blocking(move || {
        snapshot(&read_state, &read_home, &read_oauth, &read_alias)
    })
    .await
    .map_err(|_| ProviderError::internal())??;
    if current.account_id != record.account_id
        || current.created_at != record.created_at
        || saved.access != credential.access
        || saved.hyper_panel != credential.hyper_panel
    {
        return Err(changed());
    }
    cache.insert(key, (Instant::now(), result.clone()));
    result.map(Some)
}

#[cfg(test)]
mod tests;
