//! Dependency review and explicit remapping without rewriting historical turns.
use super::{journal, workflow, AgentError, TurnOptions};
use crate::{
    library, model_bindings,
    openai_codex::{OpenAiCodexState, ProviderError},
    persistence::{self, AppState},
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use tauri::{Emitter, Manager};
use workflow::settings::ModelChoice;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    WebSearch,
    Vision,
    ImageGeneration,
    BuiltinAgent,
    CustomAgent,
    Conversation,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    pub id: String,
    pub item_key: String,
    pub kind: Kind,
    pub label: String,
    pub details: Vec<String>,
    pub choice: ModelChoice,
    #[serde(skip)]
    source: ModelChoice,
}

#[derive(Serialize)]
pub struct RemovalPlan {
    pub alias: String,
    pub revision: String,
    pub items: Vec<Reference>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replacement {
    pub id: String,
    pub choice: ModelChoice,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalResult {
    pub replaced: usize,
    pub unresolved: Vec<Reference>,
}

fn error(message: &'static str) -> ProviderError {
    ProviderError::new("provider_dependencies", message)
}
fn storage<T>(_: T) -> ProviderError {
    error("Não foi possível ler os vínculos dos modelos. Os dados foram preservados.")
}
fn stale() -> ProviderError {
    ProviderError::new(
        "provider_links_changed",
        "Os vínculos mudaram. Recarregue a lista antes de remover o provedor.",
    )
}

fn reference(
    db: &Connection,
    key: String,
    kind: Kind,
    label: String,
    details: Vec<String>,
    source: ModelChoice,
) -> Result<Reference, ProviderError> {
    let encoded = model_bindings::encode(&source).map_err(storage)?;
    let id = format!("{:x}", Sha256::digest(format!("{key}\0{encoded}")));
    let choice = if matches!(kind, Kind::WebSearch | Kind::Vision | Kind::ImageGeneration) {
        source.clone()
    } else {
        model_bindings::resolve(db, &key, &source).map_err(storage)?
    };
    Ok(Reference {
        id,
        item_key: key,
        kind,
        label,
        details,
        source,
        choice,
    })
}

fn from_options(options: &TurnOptions) -> ModelChoice {
    ModelChoice {
        account: options.account.clone(),
        model: options.model.clone(),
        reasoning: options.reasoning.clone(),
    }
}

/// Read only model metadata, retaining one current choice and a bounded queue.
fn chat_choices(path: &Path) -> Result<Vec<(TurnOptions, bool)>, ProviderError> {
    if !path.exists() {
        return Ok(vec![]);
    }
    let metadata = std::fs::symlink_metadata(path).map_err(storage)?;
    if !metadata.is_file() || metadata.is_symlink() {
        return Err(error("Um histórico não pôde ser verificado com segurança."));
    }
    let mut seen = BTreeSet::new();
    let mut latest: Option<(String, TurnOptions)> = None;
    let mut queue: BTreeMap<String, TurnOptions> = BTreeMap::new();
    journal::scan(path, 0, |_, _, record| {
        match record.r#type.as_str() {
            "turn_checkpoint" => {
                let turn = &record.data["turn"];
                let id = turn["id"]
                    .as_str()
                    .ok_or_else(AgentError::storage)?
                    .to_owned();
                queue.remove(&id);
                if seen.insert(id.clone()) || latest.as_ref().is_some_and(|(last, _)| last == &id) {
                    let options = serde_json::from_value(turn["options"].clone())
                        .map_err(|_| AgentError::storage())?;
                    latest = Some((id, options));
                }
            }
            "queue_checkpoint" => {
                #[derive(Deserialize)]
                struct Entry {
                    id: String,
                    options: TurnOptions,
                }
                let entries: Vec<Entry> =
                    serde_json::from_value(record.data).map_err(|_| AgentError::storage())?;
                if entries.len() > 20 {
                    return Err(AgentError::storage());
                }
                queue = entries
                    .into_iter()
                    .map(|entry| (entry.id, entry.options))
                    .collect();
            }
            _ => {}
        }
        Ok(())
    })
    .map_err(storage)?;
    Ok(latest
        .into_iter()
        .map(|(_, choice)| (choice, false))
        .chain(queue.into_values().map(|choice| (choice, true)))
        .collect())
}

pub(crate) fn inventory(db: &Connection, home: &Path) -> Result<Vec<Reference>, ProviderError> {
    let mut references = Vec::new();
    for (table, kind, label) in [
        ("web_search_config", Kind::WebSearch, "Web Search"),
        ("vision_config", Kind::Vision, "Vision"),
        (
            "image_generation_config",
            Kind::ImageGeneration,
            "Gerar imagens",
        ),
    ] {
        let sql = if kind == Kind::ImageGeneration {
            "SELECT account_alias, 'gemini-3.1-flash-image' FROM image_generation_config WHERE id=1"
                .to_owned()
        } else {
            format!("SELECT account_alias, model FROM {table} WHERE id=1 AND inherit_chat=0")
        };
        let selected = db
            .query_row(&sql, [], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            })
            .optional()
            .map_err(storage)?;
        if let Some((Some(account), model)) = selected {
            references.push(reference(
                db,
                format!("tool:{table}"),
                kind,
                label.into(),
                vec!["Ferramentas".into()],
                ModelChoice {
                    account,
                    model: model.unwrap_or_default(),
                    reasoning: None,
                },
            )?);
        }
    }
    let profiles = workflow::settings::read(home).map_err(storage)?;
    for (key, choice) in &profiles {
        let (flow, role) = key
            .split_once('/')
            .ok_or_else(|| error("Configuração de agentes inválida."))?;
        let flow: workflow::Flow =
            serde_json::from_value(serde_json::Value::String(flow.into())).map_err(storage)?;
        let role: workflow::Role =
            serde_json::from_value(serde_json::Value::String(role.into())).map_err(storage)?;
        let flow_label = match flow {
            workflow::Flow::Standard => "Padrão",
            workflow::Flow::Designer => "Designer",
            workflow::Flow::Planned => "Planejado",
            workflow::Flow::Complete => "Completo",
            workflow::Flow::Custom => "Customizado",
            workflow::Flow::Publication => "Publicação",
        };
        references.push(reference(
            db,
            format!("builtin:{key}"),
            Kind::BuiltinAgent,
            role.label().into(),
            vec![flow_label.into(), "Agente Jarvis".into()],
            choice.clone(),
        )?);
    }
    let catalog = workflow::catalog::read(home).map_err(storage)?;
    for agent in &catalog.agents {
        if let Some(choice) = &agent.model {
            let mut details = vec!["Agente customizado".into()];
            details.extend(
                catalog
                    .flows
                    .iter()
                    .filter(|flow| flow.steps.iter().any(|step| step.agent_id == agent.id))
                    .map(|flow| format!("Fluxo: {}", flow.name)),
            );
            references.push(reference(
                db,
                format!("custom:{}", agent.id),
                Kind::CustomAgent,
                agent.name.clone(),
                details,
                choice.clone(),
            )?);
        }
    }
    let mut statement = db.prepare("SELECT c.id,c.project_id,COALESCE(c.display_title,c.title),p.name FROM conversations c JOIN projects p ON p.id=c.project_id ORDER BY c.id").map_err(storage)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(storage)?;
    for row in rows {
        let (id, project, title, project_name) = row.map_err(storage)?;
        // Deleted local histories have no reusable model choice to migrate.
        if !crate::data_dir::root(home)
            .join("sessions")
            .join(&project)
            .exists()
        {
            continue;
        }
        let path = library::session_path(home, &project, &id, false).map_err(storage)?;
        for (options, queued) in chat_choices(&path)? {
            let flow = options.workflow.unwrap_or_default();
            // A built-in assignment owns this selection and is already listed above.
            if flow != workflow::Flow::Custom
                && profiles.contains_key(&workflow::settings::key(flow, flow.root()))
            {
                continue;
            }
            let item = reference(
                db,
                format!("chat:{id}"),
                Kind::Conversation,
                title.clone(),
                vec![
                    format!("Projeto: {project_name}"),
                    if queued {
                        "Mensagens na fila".into()
                    } else {
                        "Modelo do chat".into()
                    },
                ],
                from_options(&options),
            )?;
            if let Some(existing) = references.iter_mut().find(|entry| entry.id == item.id) {
                for detail in item.details {
                    if !existing.details.contains(&detail) {
                        existing.details.push(detail);
                    }
                }
            } else {
                references.push(item);
            }
        }
    }
    Ok(references)
}

pub(crate) fn plan(
    db: &Connection,
    home: &Path,
    alias: &str,
) -> Result<RemovalPlan, ProviderError> {
    let account = persistence::list_provider_accounts(db)
        .map_err(storage)?
        .into_iter()
        .find(|record| record.alias == alias)
        .ok_or_else(|| error("Este provedor já foi removido."))?;
    let items: Vec<_> = inventory(db, home)?
        .into_iter()
        .filter(|item| item.choice.account == alias)
        .collect();
    let fingerprint = serde_json::to_vec(&(
        alias,
        account.account_id,
        model_bindings::revision(db).map_err(storage)?,
        &items,
    ))
    .map_err(storage)?;
    Ok(RemovalPlan {
        alias: alias.into(),
        revision: format!("{:x}", Sha256::digest(fingerprint)),
        items,
    })
}

fn apply(
    db: &Connection,
    home: &Path,
    alias: &str,
    revision: &str,
    replacements: &[Replacement],
) -> Result<RemovalResult, ProviderError> {
    let current = plan(db, home, alias)?;
    if current.revision != revision {
        return Err(stale());
    }
    let mut seen = BTreeSet::new();
    for replacement in replacements {
        if !seen.insert(&replacement.id) {
            return Err(error("Um vínculo foi informado mais de uma vez."));
        }
        let item = current
            .items
            .iter()
            .find(|item| item.id == replacement.id)
            .ok_or_else(stale)?;
        if replacement.choice.account == alias {
            return Err(error(
                "Escolha um provedor diferente daquele que será removido.",
            ));
        }
        let active: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM provider_accounts WHERE alias=?1 AND enabled=1)",
                [&replacement.choice.account],
                |row| row.get(0),
            )
            .map_err(storage)?;
        if !active {
            return Err(error(
                "Um provedor de destino não está mais disponível. Revise as substituições.",
            ));
        }
        match item.kind {
            Kind::WebSearch | Kind::Vision => {
                let table = if item.kind == Kind::WebSearch {
                    "web_search_config"
                } else {
                    "vision_config"
                };
                db.execute(
                    &format!("UPDATE {table} SET account_alias=?1,model=?2 WHERE id=1"),
                    params![replacement.choice.account, replacement.choice.model],
                )
                .map_err(storage)?;
            }
            Kind::ImageGeneration => {
                db.execute(
                    "UPDATE image_generation_config SET account_alias=?1 WHERE id=1",
                    [&replacement.choice.account],
                )
                .map_err(storage)?;
            }
            _ => model_bindings::replace(db, &item.item_key, &item.source, &replacement.choice)
                .map_err(storage)?,
        }
    }
    db.execute(
        "UPDATE provider_bindings_revision SET revision=revision+1 WHERE id=1",
        [],
    )
    .map_err(storage)?;
    Ok(RemovalResult {
        replaced: replacements.len(),
        unresolved: current
            .items
            .into_iter()
            .filter(|item| !seen.contains(&item.id))
            .collect(),
    })
}

pub(crate) fn resolve_chat(
    db: &Connection,
    id: &str,
    options: &mut TurnOptions,
) -> Result<(), AgentError> {
    let choice = model_bindings::resolve(db, &format!("chat:{id}"), &from_options(options))?;
    options.account = choice.account;
    options.model = choice.model;
    options.reasoning = choice.reasoning;
    Ok(())
}

fn target_stamp(db: &Connection, alias: &str) -> Result<String, ProviderError> {
    let record = persistence::list_provider_accounts(db)
        .map_err(storage)?
        .into_iter()
        .find(|record| record.alias == alias && record.enabled)
        .ok_or_else(|| error("Um provedor de destino não está mais disponível."))?;
    let config: Option<String> = db
        .query_row(
            "SELECT config FROM custom_provider_configs WHERE alias=?1",
            [alias],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage)?;
    let bytes = serde_json::to_vec(&(
        record.provider_kind,
        record.account_id,
        record.created_at,
        config,
    ))
    .map_err(storage)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[tauri::command]
pub async fn get_provider_removal_plan(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    alias: String,
) -> Result<RemovalPlan, ProviderError> {
    let home = app.path().home_dir().map_err(storage)?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.with_connection(&home, |db| plan(db, &home, &alias))
    })
    .await
    .map_err(storage)?
}

#[derive(Serialize)]
pub struct Inventory {
    pub references: Vec<Reference>,
    pub bindings: Vec<model_bindings::Binding>,
}

#[tauri::command]
pub async fn clear_chat_model_binding(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    oauth: tauri::State<'_, OpenAiCodexState>,
    conversation_id: String,
    choice: ModelChoice,
) -> Result<(), ProviderError> {
    let home = app.path().home_dir().map_err(storage)?;
    let state = state.inner().clone();
    let oauth = oauth.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let stamp = state.with_connection(&home, |db| target_stamp(db, &choice.account))?;
        oauth.inference_model(
            &state,
            &home,
            &choice.account,
            &choice.model,
            choice.reasoning.as_deref(),
        )?;
        state.with_connection(&home, |db| {
            let exists: bool = db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?1)",
                    [&conversation_id],
                    |row| row.get(0),
                )
                .map_err(storage)?;
            if !exists {
                return Err(error("Esta conversa não está mais disponível."));
            }
            if target_stamp(db, &choice.account)? != stamp {
                return Err(stale());
            }
            model_bindings::forget_choice(db, &format!("chat:{conversation_id}"), &choice)
                .map_err(storage)
        })
    })
    .await
    .map_err(storage)??;
    let _ = app.emit("provider-model-bindings:changed", ());
    Ok(())
}

#[tauri::command]
pub async fn get_provider_model_references(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Inventory, ProviderError> {
    let home = app.path().home_dir().map_err(storage)?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.with_connection(&home, |db| {
            Ok(Inventory {
                references: inventory(db, &home)?,
                bindings: model_bindings::list(db).map_err(storage)?,
            })
        })
    })
    .await
    .map_err(storage)?
}

pub async fn remove(
    app: tauri::AppHandle,
    state: AppState,
    oauth: OpenAiCodexState,
    alias: String,
    revision: String,
    replacements: Vec<Replacement>,
) -> Result<RemovalResult, ProviderError> {
    let home = app.path().home_dir().map_err(storage)?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        let preview = state.with_connection(&home, |db| plan(db, &home, &alias))?;
        if preview.revision != revision {
            return Err(stale());
        }
        let mut targets = BTreeMap::new();
        for replacement in &replacements {
            let item = preview
                .items
                .iter()
                .find(|item| item.id == replacement.id)
                .ok_or_else(stale)?;
            if replacement.choice.account == alias {
                return Err(error("Escolha outro provedor para a substituição."));
            }
            let stamp =
                state.with_connection(&home, |db| target_stamp(db, &replacement.choice.account))?;
            if targets
                .insert(replacement.choice.account.clone(), stamp.clone())
                .is_some_and(|previous| previous != stamp)
            {
                return Err(stale());
            }
            if matches!(item.kind, Kind::WebSearch | Kind::Vision)
                && replacement.choice.reasoning.is_some()
            {
                return Err(error(
                    "Esta ferramenta usa apenas a seleção de provedor e modelo.",
                ));
            }
            if item.kind == Kind::ImageGeneration {
                let valid = replacement.choice.model == super::image_generation::MODEL
                    && replacement.choice.reasoning.is_none()
                    && state
                        .list_provider_accounts(&home)
                        .map_err(storage)?
                        .iter()
                        .any(|record| {
                            record.alias == replacement.choice.account
                                && record.enabled
                                && record.provider_kind == "antigravity"
                        });
                if !valid {
                    return Err(error(
                        "Gerar imagens requer uma conta Antigravity e Gemini 3.1 Flash Image.",
                    ));
                }
            } else {
                let (credential, _) = oauth.inference_model(
                    &state,
                    &home,
                    &replacement.choice.account,
                    &replacement.choice.model,
                    replacement.choice.reasoning.as_deref(),
                )?;
                let provider_kind = if credential.custom.is_some() {
                    "custom"
                } else if credential.project_id.is_some() {
                    "antigravity"
                } else {
                    "openai-codex"
                };
                if item.kind == Kind::WebSearch
                    && !super::web_search::supports(provider_kind, &replacement.choice.model)
                {
                    return Err(error("O modelo de destino não oferece Web Search."));
                }
                if item.kind == Kind::Vision {
                    let valid = credential.custom.as_ref().map_or_else(
                        || {
                            ["gpt-", "gemini-", "claude", "o3", "o4"]
                                .iter()
                                .any(|prefix| replacement.choice.model.starts_with(prefix))
                        },
                        |config| {
                            config.models.iter().any(|model| {
                                model.id == replacement.choice.model && model.supports_images
                            })
                        },
                    );
                    if !valid {
                        return Err(error("O modelo de destino não aceita imagens para Vision."));
                    }
                }
            }
        }
        oauth.remove_with_updates(&state, &home, &alias, |db| {
            for (alias, stamp) in &targets {
                if target_stamp(db, alias)? != *stamp {
                    return Err(stale());
                }
            }
            apply(db, &home, &alias, &revision, &replacements)
        })
    })
    .await
    .map_err(storage)??;
    for event in [
        "provider-model-bindings:changed",
        "agent-models:changed",
        "workflow-catalog:changed",
    ] {
        let _ = app.emit(event, ());
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
