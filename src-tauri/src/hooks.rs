//! User-approved command hooks. Native middleware remains immutable.
pub(crate) mod mcp_dispatch;
pub(crate) mod runtime;

use crate::persistence::{AppState, PersistenceError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
};
use tauri::{Emitter, Manager};

const MAX_HOOKS: usize = 64;
const MAX_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Event {
    PreToolUse,
    PermissionRequest,
    PostToolUse,
    SessionStart,
    UserPromptSubmit,
    PreCompact,
    PostCompact,
    Stop,
    SubagentStart,
    SubagentStop,
    Interrupt,
    SessionEnd,
    BeforeAgent,
}

impl Event {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::PreToolUse => "PreToolUse",
            Self::PermissionRequest => "PermissionRequest",
            Self::PostToolUse => "PostToolUse",
            Self::SessionStart => "SessionStart",
            Self::UserPromptSubmit => "UserPromptSubmit",
            Self::PreCompact => "PreCompact",
            Self::PostCompact => "PostCompact",
            Self::Stop => "Stop",
            Self::SubagentStart => "SubagentStart",
            Self::SubagentStop => "SubagentStop",
            Self::Interrupt => "Interrupt",
            Self::SessionEnd => "SessionEnd",
            Self::BeforeAgent => "BeforeAgent",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Hook {
    pub id: String,
    pub name: String,
    pub event: Event,
    pub command: String,
    pub matcher: String,
    pub timeout_seconds: u64,
    pub enabled: bool,
}

impl Hook {
    pub(crate) fn matches(&self, event: Event, tool_name: Option<&str>) -> bool {
        if !self.enabled || self.event != event {
            return false;
        }
        // Codex ignores matchers for user input and stop lifecycle events.
        if matches!(
            event,
            Event::UserPromptSubmit | Event::Stop | Event::Interrupt
        ) {
            return true;
        }
        if self.matcher.is_empty() || self.matcher == "*" {
            true
        } else if exact_matcher(&self.matcher) {
            tool_name.is_some_and(|name| self.matcher.split('|').any(|part| part == name))
        } else {
            tool_name.is_some_and(|name| {
                regex::Regex::new(&self.matcher).is_ok_and(|matcher| matcher.is_match(name))
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeHook {
    pub id: String,
    pub name: String,
    pub event: Event,
    pub description: String,
    pub command: Option<String>,
    pub matcher: Option<String>,
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Catalog {
    pub revision: u64,
    pub hooks: Vec<Hook>,
    pub native_hooks: Vec<NativeHook>,
    pub untrusted_ids: Vec<String>,
}

impl Catalog {
    pub(crate) fn trusted_hooks(&self) -> impl Iterator<Item = &Hook> {
        self.hooks
            .iter()
            .filter(|hook| !self.untrusted_ids.contains(&hook.id))
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Stored {
    revision: u64,
    hooks: Vec<Hook>,
    #[serde(default)]
    trusted_hashes: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct HooksError {
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for HooksError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(formatter)
    }
}

impl std::error::Error for HooksError {}

impl From<PersistenceError> for HooksError {
    fn from(_: PersistenceError) -> Self {
        storage_error()
    }
}

fn error(code: &'static str, message: &str) -> HooksError {
    HooksError {
        code,
        message: message.into(),
    }
}

fn invalid(message: &str) -> HooksError {
    error("invalid_hook", message)
}

fn storage_error() -> HooksError {
    error("hooks_storage", "Não foi possível acessar os hooks salvos.")
}

fn exact_matcher(matcher: &str) -> bool {
    matcher
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '|'))
}

pub(crate) fn validate(hook: &Hook) -> Result<(), HooksError> {
    if hook.id.starts_with("native-") {
        return Err(invalid("Os hooks nativos são somente leitura."));
    }
    if hook.id.len() != 32
        || !hook
            .id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("O identificador do hook é inválido."));
    }
    if hook.event == Event::BeforeAgent {
        return Err(invalid("Este evento está reservado aos hooks nativos."));
    }
    if hook.name.trim().is_empty() || hook.name.len() > 160 || hook.name.contains('\0') {
        return Err(invalid(
            "Informe um nome de até 160 caracteres para o hook.",
        ));
    }
    if hook.command.trim().is_empty() || hook.command.len() > 16_384 || hook.command.contains('\0')
    {
        return Err(invalid("Informe um comando válido de até 16 KiB."));
    }
    if !(1..=600).contains(&hook.timeout_seconds) {
        return Err(invalid("O tempo limite deve ser de 1 a 600 segundos."));
    }
    if hook.matcher.len() > 4096 || hook.matcher.contains('\0') {
        return Err(invalid("O matcher deve ter até 4 KiB."));
    }
    if !hook.matcher.is_empty()
        && hook.matcher != "*"
        && !exact_matcher(&hook.matcher)
        && regex::Regex::new(&hook.matcher).is_err()
    {
        return Err(invalid("O matcher contém uma expressão regular inválida."));
    }
    Ok(())
}

fn native_hooks() -> Vec<NativeHook> {
    [
        (
            "context-session", "Context-mode: contexto inicial", Event::SessionStart,
            "Inicializa o histórico privado e recupera contexto para a conversa.", Some(3),
        ),
        (
            "impeccable-session", "Impeccable: contexto de design", Event::SessionStart,
            "Inicializa a inspeção de design quando o Core Impeccable está instalado.", Some(5),
        ),
        (
            "impeccable-edit", "Impeccable: qualidade da interface", Event::PostToolUse,
            "Inspeciona arquivos de interface alterados e registra achados nos recursos do Core.", Some(5),
        ),
        (
            "impeccable-stop", "Impeccable: revisão de design", Event::Stop,
            "Revisa alterações de interface ao concluir, com no máximo uma correção automática por turno.", Some(30),
        ),
        (
            "context-input", "Context-mode: mensagem do usuário", Event::UserPromptSubmit,
            "Registra a intenção atual do usuário no histórico privado de eventos.", Some(3),
        ),
        (
            "context-tool", "Context-mode: resultado de ferramenta", Event::PostToolUse,
            "Registra os resultados já confirmados das ferramentas, incluindo falhas.", Some(3),
        ),
        (
            "context-precompact", "Context-mode: antes de compactar", Event::PreCompact,
            "Preserva um snapshot do contexto antes da compactação.", Some(3),
        ),
        (
            "context-postcompact", "Context-mode: depois de compactar", Event::PostCompact,
            "Registra o resumo e a contagem de mensagens após a compactação.", Some(3),
        ),
        (
            "context-stop", "Context-mode: fim da execução", Event::Stop,
            "Registra o desfecho do turno para futuras retomadas.", Some(3),
        ),
        (
            "ponytail", "Ponytail: orientação do agente", Event::BeforeAgent,
            "Acrescenta a orientação de implementação ao contexto antes de consultar o modelo.", None,
        ),
        (
            "context-routing", "Context-mode: orientação de ferramentas", Event::PreToolUse,
            "Orienta consultas extensas de rede para as ferramentas de contexto quando disponíveis, preservando as aprovações normais.", None,
        ),
        (
            "beads", "Beads: tarefas do projeto", Event::SessionStart,
            "Recupera um snapshot das tarefas ativas do projeto para orientar a execução.", None,
        ),
    ]
    .into_iter()
    .map(|(id, name, event, description, timeout_seconds)| NativeHook {
        id: format!("native-{id}"),
        name: name.into(),
        event,
        description: description.into(),
        command: None,
        matcher: (event == Event::PreToolUse).then(|| "bash".into()),
        timeout_seconds,
    })
    .collect()
}

fn catalog(stored: Stored) -> Result<Catalog, HooksError> {
    if stored.hooks.len() > MAX_HOOKS {
        return Err(invalid("O limite é de 64 hooks manuais."));
    }
    let mut ids = std::collections::HashSet::new();
    let mut untrusted_ids = Vec::new();
    for hook in &stored.hooks {
        validate(hook)?;
        if !ids.insert(&hook.id) {
            return Err(invalid("Há identificadores de hooks repetidos."));
        }
        if stored.trusted_hashes.get(&hook.id) != Some(&hook_hash(hook)?) {
            untrusted_ids.push(hook.id.clone());
        }
    }
    Ok(Catalog {
        revision: stored.revision,
        hooks: stored.hooks,
        native_hooks: native_hooks(),
        untrusted_ids,
    })
}

// This detects unreviewed configuration edits; it is not a boundary against
// another program running as the same local user and rewriting both fields.
fn hook_hash(hook: &Hook) -> Result<String, HooksError> {
    let mut normalized = hook.clone();
    normalized.name = normalized.name.trim().into();
    let bytes = serde_json::to_vec(&normalized).map_err(|_| storage_error())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn trusted_hashes(catalog: &Catalog) -> Result<BTreeMap<String, String>, HooksError> {
    catalog
        .trusted_hooks()
        .map(|hook| Ok((hook.id.clone(), hook_hash(hook)?)))
        .collect()
}

pub(crate) fn read(home: &Path) -> Result<Catalog, HooksError> {
    let path = crate::data_dir::root(home).join("hooks.json");
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return catalog(Stored::default());
        }
        Err(_) => return Err(storage_error()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| storage_error())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(invalid("O arquivo de hooks excede o limite permitido."));
    }
    let stored = serde_json::from_slice(&bytes).map_err(|_| storage_error())?;
    catalog(stored)
}

pub(crate) fn load(state: &AppState, home: &Path) -> Result<Catalog, HooksError> {
    state.with_connection(home, |_| read(home))
}

pub(crate) fn preview_upsert(current: &Catalog, mut hook: Hook) -> Result<Catalog, HooksError> {
    validate(&hook)?;
    hook.name = hook.name.trim().into();
    let mut trusted_hashes = trusted_hashes(current)?;
    // Only the exact hook approved in Settings or the authoring panel is trusted.
    trusted_hashes.insert(hook.id.clone(), hook_hash(&hook)?);
    let mut hooks = current.hooks.clone();
    if let Some(existing) = hooks.iter_mut().find(|existing| existing.id == hook.id) {
        *existing = hook;
    } else {
        hooks.push(hook);
    }
    catalog(Stored {
        revision: current.revision.checked_add(1).ok_or_else(storage_error)?,
        hooks,
        trusted_hashes,
    })
}

fn check_revision(current: &Catalog, expected_revision: u64) -> Result<(), HooksError> {
    if current.revision != expected_revision {
        return Err(error(
            "hooks_conflict",
            "Os hooks foram alterados. Recarregue a lista e revise a mudança.",
        ));
    }
    Ok(())
}

fn write(home: &Path, catalog: &Catalog) -> Result<(), HooksError> {
    let bytes = serde_json::to_vec(&Stored {
        revision: catalog.revision,
        hooks: catalog.hooks.clone(),
        trusted_hashes: trusted_hashes(catalog)?,
    })
    .map_err(|_| storage_error())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(invalid("A configuração de hooks excede o limite de 1 MiB."));
    }
    let directory = crate::data_dir::root(home);
    fs::create_dir_all(&directory).map_err(|_| storage_error())?;
    let mut file = tempfile::NamedTempFile::new_in(&directory).map_err(|_| storage_error())?;
    file.write_all(&bytes).map_err(|_| storage_error())?;
    file.as_file_mut().sync_all().map_err(|_| storage_error())?;
    file.persist(directory.join("hooks.json"))
        .map_err(|_| storage_error())?;
    #[cfg(unix)]
    fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|_| storage_error())?;
    Ok(())
}

pub(crate) fn upsert(
    state: &AppState,
    home: &Path,
    hook: Hook,
    expected_revision: u64,
) -> Result<Catalog, HooksError> {
    validate(&hook)?;
    state.with_connection(home, |_| {
        let current = read(home)?;
        check_revision(&current, expected_revision)?;
        let changed = preview_upsert(&current, hook)?;
        write(home, &changed)?;
        Ok(changed)
    })
}

pub(crate) fn delete(
    state: &AppState,
    home: &Path,
    id: &str,
    expected_revision: u64,
) -> Result<Catalog, HooksError> {
    state.with_connection(home, |_| {
        let mut current = read(home)?;
        check_revision(&current, expected_revision)?;
        if current.native_hooks.iter().any(|hook| hook.id == id) {
            return Err(invalid("Os hooks nativos são somente leitura."));
        }
        let position = current
            .hooks
            .iter()
            .position(|hook| hook.id == id)
            .ok_or_else(|| invalid("O hook não existe mais. Recarregue a lista."))?;
        current.hooks.remove(position);
        current.untrusted_ids.retain(|untrusted| untrusted != id);
        current.revision = current.revision.checked_add(1).ok_or_else(storage_error)?;
        write(home, &current)?;
        Ok(current)
    })
}

#[tauri::command]
pub(crate) async fn list_hooks(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Catalog, HooksError> {
    let home = app.path().home_dir().map_err(|_| storage_error())?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || load(&state, &home))
        .await
        .map_err(|_| storage_error())?
}

#[tauri::command]
pub(crate) async fn save_hook(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    hook: Hook,
    expected_revision: u64,
) -> Result<Catalog, HooksError> {
    let home = app.path().home_dir().map_err(|_| storage_error())?;
    let state = state.inner().clone();
    let saved = tauri::async_runtime::spawn_blocking(move || {
        upsert(&state, &home, hook, expected_revision)
    })
    .await
    .map_err(|_| storage_error())??;
    let _ = app.emit("hooks:changed", ());
    Ok(saved)
}

#[tauri::command]
pub(crate) async fn delete_hook(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
    expected_revision: u64,
) -> Result<Catalog, HooksError> {
    let home = app.path().home_dir().map_err(|_| storage_error())?;
    let state = state.inner().clone();
    let saved =
        tauri::async_runtime::spawn_blocking(move || delete(&state, &home, &id, expected_revision))
            .await
            .map_err(|_| storage_error())??;
    let _ = app.emit("hooks:changed", ());
    Ok(saved)
}

#[cfg(test)]
mod tests;
