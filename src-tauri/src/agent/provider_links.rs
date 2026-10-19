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
    ChatTitle,
    PromptRefinement,
    MessagePrediction,
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
    if let Some(choice) = &options.model_selection {
        return choice.clone();
    }
    ModelChoice {
        executor: options.executor,
        account: options.account.clone(),
        model: options.model.clone(),
        reasoning: options.reasoning.clone(),
        service_tier: options.service_tier,
        fallback: None,
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
    #[derive(Deserialize)]
    struct Entry {
        id: String,
        options: TurnOptions,
    }
    #[derive(Deserialize)]
    struct TurnEntry {
        id: String,
        #[serde(default)]
        options: serde_json::Value,
    }
    #[derive(Deserialize)]
    struct TurnData {
        turn: TurnEntry,
    }
    #[derive(Deserialize)]
    struct Metadata<T> {
        data: T,
    }
    journal::scan_selected(
        path,
        &["turn_checkpoint", "queue_checkpoint"],
        |kind, bytes| {
            match kind {
                "turn_checkpoint" => {
                    let Metadata {
                        data:
                            TurnData {
                                turn: TurnEntry { id, options },
                            },
                    } = serde_json::from_slice(bytes).map_err(|_| AgentError::storage())?;
                    queue.remove(&id);
                    if seen.insert(id.clone())
                        || latest.as_ref().is_some_and(|(last, _)| last == &id)
                    {
                        let options =
                            serde_json::from_value(options).map_err(|_| AgentError::storage())?;
                        latest = Some((id, options));
                    }
                }
                "queue_checkpoint" => {
                    let Metadata { data: entries }: Metadata<Vec<Entry>> =
                        serde_json::from_slice(bytes).map_err(|_| AgentError::storage())?;
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
        },
    )
    .map_err(storage)?;
    Ok(latest
        .into_iter()
        .map(|(_, choice)| (choice, false))
        .chain(queue.into_values().map(|choice| (choice, true)))
        .collect())
}

struct Conversation {
    id: String,
    project: String,
    title: String,
    project_name: String,
    choices: Vec<(TurnOptions, bool)>,
}

fn conversations(db: &Connection) -> Result<Vec<Conversation>, ProviderError> {
    let mut statement = db.prepare("SELECT c.id,COALESCE(c.journal_project_id,c.project_id),COALESCE(c.display_title,c.title),p.name FROM conversations c JOIN projects p ON p.id=c.project_id ORDER BY c.id").map_err(storage)?;
    let rows = statement
        .query_map([], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                project: row.get(1)?,
                title: row.get(2)?,
                project_name: row.get(3)?,
                choices: Vec::new(),
            })
        })
        .map_err(storage)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage)?;
    Ok(rows)
}

struct InventorySources {
    title: Option<ModelChoice>,
    refinement: Option<ModelChoice>,
    prediction: Option<ModelChoice>,
    profiles: workflow::settings::ModelSettings,
    catalog: workflow::catalog::Catalog,
    conversations: Vec<Conversation>,
}

impl InventorySources {
    fn read(
        home: &Path,
        mut conversations: Vec<Conversation>,
        mut read_choices: impl FnMut(&Path) -> Result<Vec<(TurnOptions, bool)>, ProviderError>,
    ) -> Result<Self, ProviderError> {
        let preferences = crate::system::backup_preferences(home).map_err(storage)?;
        let title = preferences.chat_title_model;
        let refinement = preferences.prompt_refinement_model;
        let prediction = preferences.message_prediction_model;
        let profiles = workflow::settings::read(home).map_err(storage)?;
        let catalog = workflow::catalog::read(home).map_err(storage)?;
        for conversation in &mut conversations {
            // Deleted local histories have no reusable model choice to migrate.
            if crate::data_dir::root(home)
                .join("sessions")
                .join(&conversation.project)
                .exists()
            {
                let path =
                    library::session_path(home, &conversation.project, &conversation.id, false)
                        .map_err(storage)?;
                conversation.choices = read_choices(&path)?;
            }
        }
        Ok(Self {
            title,
            refinement,
            prediction,
            profiles,
            catalog,
            conversations,
        })
    }
}

pub(crate) fn inventory(db: &Connection, home: &Path) -> Result<Vec<Reference>, ProviderError> {
    inventory_from_sources(
        db,
        InventorySources::read(home, conversations(db)?, chat_choices)?,
    )
}

fn inventory_from_sources(
    db: &Connection,
    sources: InventorySources,
) -> Result<Vec<Reference>, ProviderError> {
    let InventorySources {
        title,
        refinement,
        prediction,
        profiles,
        catalog,
        conversations,
    } = sources;
    let mut references = Vec::new();
    if let Some(choice) = title {
        references.push(reference(
            db,
            crate::system::CHAT_TITLE_MODEL_KEY.into(),
            Kind::ChatTitle,
            "Títulos das conversas".into(),
            vec!["Configurações gerais".into()],
            choice,
        )?);
    }
    if let Some(choice) = refinement {
        references.push(reference(
            db,
            crate::system::PROMPT_REFINEMENT_MODEL_KEY.into(),
            Kind::PromptRefinement,
            "Refinar prompt".into(),
            vec!["Configurações gerais".into()],
            choice,
        )?);
    }
    if let Some(choice) = prediction {
        references.push(reference(
            db,
            crate::system::MESSAGE_PREDICTION_MODEL_KEY.into(),
            Kind::MessagePrediction,
            "Previsão de mensagens".into(),
            vec!["Configurações gerais".into()],
            choice,
        )?);
    }
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
            "SELECT account_alias, COALESCE(model, 'gemini-3.1-flash-image') FROM image_generation_config WHERE id=1"
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
                    executor: crate::claude::Executor::Jarvis,
                    account,
                    model: model.unwrap_or_default(),
                    reasoning: None,
                    service_tier: None,
                    fallback: None,
                },
            )?);
        }
    }
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
            workflow::Flow::Video => "Vídeo",
            workflow::Flow::ImageGenerator => "Imagem",
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
    // Unsent choices belong to the conversation even when the agent has a global default.
    let mut overrides = db.prepare("SELECT m.conversation_id,m.agent_key,m.choice,COALESCE(c.display_title,c.title),p.name FROM conversation_agent_models m JOIN conversations c ON c.id=m.conversation_id JOIN projects p ON p.id=c.project_id ORDER BY m.conversation_id,m.agent_key").map_err(storage)?;
    let choices = overrides
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(storage)?;
    for row in choices {
        let (conversation, selected, encoded, title, project) = row.map_err(storage)?;
        references.push(reference(
            db,
            workflow::settings::chat::binding_key(&conversation, &selected),
            Kind::Conversation,
            title,
            vec![
                format!("Projeto: {project}"),
                format!("Modelo do chat: {selected}"),
            ],
            serde_json::from_str(&encoded).map_err(storage)?,
        )?);
    }
    for Conversation {
        id,
        title,
        project_name,
        choices,
        ..
    } in conversations
    {
        for (options, queued) in choices {
            if workflow::settings::chat::read(db, &id)
                .map_err(storage)?
                .contains_key(&workflow::settings::chat::selection_key(&options).map_err(storage)?)
                && !queued
            {
                continue;
            }
            let mut flow = options.workflow.unwrap_or_default();
            if flow == workflow::Flow::Custom && options.custom_workflow_id.is_none() {
                // The composer applies GitHub's profile on the next send; queued turns
                // retain their submitted choice. Custom agents override both at runtime.
                if !queued && options.custom_agent_id.as_deref() == Some("builtin:github") {
                    flow = workflow::Flow::Publication;
                } else if !queued
                    && catalog.agents.iter().any(|agent| {
                        Some(agent.id.as_str()) == options.custom_agent_id.as_deref()
                            && agent.usage != workflow::catalog::AgentUsage::FlowOnly
                            && agent.model.is_some()
                    })
                {
                    continue;
                }
            }
            // Configured assignments are already listed above.
            if !queued
                && flow != workflow::Flow::Custom
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
    let secondary = references
        .iter()
        .filter_map(|item| {
            item.source
                .fallback
                .as_deref()
                .map(|fallback| (item, fallback))
        })
        .map(|(item, fallback)| {
            let mut details = item.details.clone();
            details.push("Modelo secundário".into());
            reference(
                db,
                format!("{}:fallback", item.item_key),
                item.kind,
                item.label.clone(),
                details,
                fallback.clone(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    references.extend(secondary);
    references.retain(|item| item.choice.executor == crate::claude::Executor::Jarvis);
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
        replacement
            .choice
            .validate_shape()
            .map_err(|cause| ProviderError {
                code: "provider_dependencies".into(),
                message: cause.message,
            })?;
        if replacement.choice.fallback.is_some() {
            return Err(error(
                "Substitua o modelo principal e o secundário separadamente.",
            ));
        }
        if replacement.choice.account == alias {
            return Err(error(
                "Escolha um provedor diferente daquele que será removido.",
            ));
        }
        let active: bool = replacement.choice.executor != crate::claude::Executor::Jarvis
            || db
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
        if replacement.choice.executor != crate::claude::Executor::Jarvis
            && matches!(
                item.kind,
                Kind::WebSearch
                    | Kind::Vision
                    | Kind::ImageGeneration
                    | Kind::ChatTitle
                    | Kind::PromptRefinement
                    | Kind::MessagePrediction
            )
        {
            return Err(error(
                "Esta ferramenta requer um provedor Jarvis, não um executor externo.",
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
                    "UPDATE image_generation_config SET account_alias=?1,model=?2 WHERE id=1",
                    params![replacement.choice.account, replacement.choice.model],
                )
                .map_err(storage)?;
            }
            _ => model_bindings::replace(db, &item.item_key, &item.source, &replacement.choice)
                .map_err(storage)?,
        }
    }
    // Both replacements must still form a valid primary/secondary pair before commit.
    inventory(db, home)?;
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
    if workflow::settings::chat::resolve_options(db, id, options)? {
        return Ok(());
    }
    let choice = model_bindings::resolve(db, &format!("chat:{id}"), &from_options(options))?;
    choice.apply(options);
    if options.model_selection.is_some() {
        options.model_selection = Some(choice);
    }
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
        let stamp = if choice.executor == crate::claude::Executor::Jarvis {
            Some(state.with_connection(&home, |db| target_stamp(db, &choice.account))?)
        } else {
            None
        };
        workflow::settings::validate_choice(&state, &oauth, &home, &choice).map_err(|cause| {
            ProviderError {
                code: "provider_dependencies".into(),
                message: cause.message,
            }
        })?;
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
            if let Some(stamp) = &stamp {
                if target_stamp(db, &choice.account)? != *stamp {
                    return Err(stale());
                }
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
    tauri::async_runtime::spawn_blocking(move || model_references(&state, &home, chat_choices))
        .await
        .map_err(storage)?
}

fn model_references(
    state: &AppState,
    home: &Path,
    read_choices: impl FnMut(&Path) -> Result<Vec<(TurnOptions, bool)>, ProviderError>,
) -> Result<Inventory, ProviderError> {
    let rows = state.with_connection(home, |db| conversations(db))?;
    // Journals can contain hundreds of megabytes. Never retain the shared
    // database guard while reading them, preferences or agent catalogs.
    let mut sources = InventorySources::read(home, rows, read_choices)?;
    state.with_connection(home, |db| {
        let mut choices: BTreeMap<_, _> = sources
            .conversations
            .into_iter()
            .map(|row| ((row.id, row.project), row.choices))
            .collect();
        sources.conversations = conversations(db)?
            .into_iter()
            .map(|mut row| {
                row.choices = choices
                    .remove(&(row.id.clone(), row.project.clone()))
                    .unwrap_or_default();
                row
            })
            .collect();
        Ok(Inventory {
            references: inventory_from_sources(db, sources)?,
            bindings: model_bindings::list(db).map_err(storage)?,
        })
    })
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
            if replacement.choice.executor != crate::claude::Executor::Jarvis {
                if matches!(
                    item.kind,
                    Kind::WebSearch
                        | Kind::Vision
                        | Kind::ImageGeneration
                        | Kind::ChatTitle
                        | Kind::PromptRefinement
                        | Kind::MessagePrediction
                ) {
                    return Err(error(
                        "Esta ferramenta requer um provedor Jarvis, não um executor externo.",
                    ));
                }
                workflow::settings::validate_choice(&state, &oauth, &home, &replacement.choice)
                    .map_err(|cause| ProviderError {
                        code: "provider_dependencies".into(),
                        message: cause.message,
                    })?;
                continue;
            }
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
                && (replacement.choice.reasoning.is_some()
                    || replacement.choice.service_tier.is_some())
            {
                return Err(error(
                    "Esta ferramenta usa apenas a seleção de provedor e modelo.",
                ));
            }
            if item.kind == Kind::ImageGeneration {
                let valid = replacement.choice.reasoning.is_none()
                    && replacement.choice.service_tier.is_none()
                    && super::image_generation::compatible(
                        &state,
                        &home,
                        &replacement.choice.account,
                        &replacement.choice.model,
                    );
                if !valid {
                    return Err(error(
                        "Gerar imagens requer uma conta ativa e um modelo de imagens compatível.",
                    ));
                }
            } else {
                let (credential, model) = oauth.inference_model(
                    &state,
                    &home,
                    &replacement.choice.account,
                    &replacement.choice.model,
                    replacement.choice.reasoning.as_deref(),
                )?;
                crate::openai_codex::validate_service_tier(
                    &credential,
                    &model,
                    replacement.choice.service_tier,
                )
                .map_err(|cause| ProviderError {
                    code: "provider_dependencies".into(),
                    message: cause.message,
                })?;
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
    app.state::<crate::system::SystemState>().changed(&app);
    Ok(result)
}

#[cfg(test)]
mod tests;
