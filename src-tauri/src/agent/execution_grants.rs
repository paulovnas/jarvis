//! Scoped execution grants.
//!
//! Grants can only turn a policy `Ask` into `Allow`; a policy `Deny` is final.
//! Command-prefix matching uses argv element boundaries, never raw text.

use super::execution_policy::{CommandPlan, ExecutionDecision, ExecutionEffects, PolicyOutcome};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const STORE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) enum GrantMatch {
    Exact,
    CommandPrefix,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(super) enum GrantScope {
    Conversation { conversation_id: String },
    Project { project_id: String },
    Repository { project_id: String, root: PathBuf },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(super) enum GrantDuration {
    Once,
    Session,
    Until { expires_at: u64 },
    Persistent,
}

impl GrantDuration {
    fn persisted(self) -> bool {
        matches!(self, Self::Until { .. } | Self::Persistent)
    }

    fn expires_at(self) -> Option<u64> {
        match self {
            Self::Until { expires_at } => Some(expires_at),
            Self::Once | Self::Session | Self::Persistent => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(super) enum GrantSubject {
    CommandExact {
        plan: CommandPlan,
    },
    NativeCommandExact {
        plan: CommandPlan,
        working_directory: PathBuf,
        read_paths: Vec<PathBuf>,
        write_paths: Vec<PathBuf>,
    },
    CommandPrefix {
        argv: Vec<String>,
    },
    ToolExact {
        name: String,
        arguments: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ExecutionGrant {
    pub id: String,
    pub scope: GrantScope,
    pub subject: GrantSubject,
    pub effects: ExecutionEffects,
    pub duration: GrantDuration,
    pub created_at: u64,
    pub last_used_at: Option<u64>,
    pub uses: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) enum GrantScopeKind {
    Conversation,
    Project,
    Repository,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) enum GrantDurationKind {
    Once,
    Session,
    Until,
    Persistent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExecutionGrantSummary {
    pub id: String,
    pub scope: GrantScopeKind,
    pub scope_root: Option<String>,
    pub match_kind: GrantMatch,
    pub duration: GrantDurationKind,
    #[cfg_attr(test, ts(type = "number | null"))]
    pub expires_at: Option<u64>,
    pub subject: String,
    pub effects: ExecutionEffects,
    #[cfg_attr(test, ts(type = "number"))]
    pub created_at: u64,
    #[cfg_attr(test, ts(type = "number | null"))]
    pub last_used_at: Option<u64>,
    #[cfg_attr(test, ts(type = "number"))]
    pub uses: u64,
}

impl ExecutionGrant {
    fn expired(&self, now: u64) -> bool {
        self.duration
            .expires_at()
            .is_some_and(|expires_at| expires_at <= now)
            || (self.duration == GrantDuration::Once && self.uses > 0)
    }
}

#[derive(Debug, Clone)]
pub(super) struct GrantContext<'a> {
    pub conversation_id: &'a str,
    pub project_id: &'a str,
    pub working_directory: &'a Path,
}

#[derive(Debug, Clone)]
pub(super) struct CreateGrant<'a> {
    pub scope: GrantScope,
    pub match_kind: GrantMatch,
    pub duration: GrantDuration,
    pub outcome: &'a PolicyOutcome,
    pub tool_name: &'a str,
    pub tool_arguments: &'a Value,
    pub now: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GrantFile {
    version: u32,
    grants: Vec<ExecutionGrant>,
}

impl Default for GrantFile {
    fn default() -> Self {
        Self {
            version: STORE_VERSION,
            grants: Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
struct GrantState {
    path: Option<PathBuf>,
    grants: Vec<ExecutionGrant>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct GrantStore {
    inner: Arc<Mutex<GrantState>>,
}

impl GrantStore {
    pub(super) fn setup(&self, path: PathBuf, now: u64) -> Result<(), String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| "Não foi possível abrir as autorizações de execução.".to_owned())?;
        let mut stored = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<GrantFile>(&bytes)
                .map_err(|_| "As autorizações de execução armazenadas são inválidas.".to_owned())?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => GrantFile::default(),
            Err(_) => return Err("Não foi possível ler as autorizações de execução.".into()),
        };
        if stored.version != STORE_VERSION {
            return Err("A versão das autorizações de execução não é compatível.".into());
        }
        stored
            .grants
            .retain(|grant| grant.duration.persisted() && !grant.expired(now));
        state.path = Some(path);
        state.grants = stored.grants;
        persist(&state)
    }

    pub(super) fn create(&self, request: CreateGrant<'_>) -> Result<ExecutionGrant, String> {
        if request.outcome.decision != ExecutionDecision::Ask {
            return Err("Somente decisões que pedem autorização podem gerar um grant.".into());
        }
        validate_scope(&request.scope, request.now, request.duration)?;
        let subject = subject(&request)?;
        let grant = ExecutionGrant {
            id: crate::library::new_id().map_err(|_| {
                "Não foi possível identificar a autorização de execução.".to_owned()
            })?,
            scope: request.scope,
            subject,
            effects: request.outcome.effects.clone(),
            duration: request.duration,
            created_at: request.now,
            last_used_at: None,
            uses: 0,
        };
        let mut state = self
            .inner
            .lock()
            .map_err(|_| "Não foi possível salvar a autorização de execução.".to_owned())?;
        state
            .grants
            .retain(|existing| !existing.expired(request.now));
        state.grants.push(grant.clone());
        if let Err(error) = persist(&state) {
            state.grants.retain(|existing| existing.id != grant.id);
            return Err(error);
        }
        Ok(grant)
    }

    pub(super) fn authorize(
        &self,
        outcome: &PolicyOutcome,
        tool_name: &str,
        tool_arguments: &Value,
        context: GrantContext<'_>,
        now: u64,
    ) -> Result<Option<String>, String> {
        if outcome.decision != ExecutionDecision::Ask {
            return Ok(None);
        }
        let mut state = self
            .inner
            .lock()
            .map_err(|_| "Não foi possível consultar as autorizações de execução.".to_owned())?;
        let before = state.grants.len();
        state.grants.retain(|grant| !grant.expired(now));
        let matched = state.grants.iter_mut().find(|grant| {
            scope_matches(&grant.scope, &context)
                && effects_cover(&grant.effects, &outcome.effects)
                && subject_matches(&grant.subject, outcome, tool_name, tool_arguments)
        });
        let result = matched.map(|grant| {
            grant.uses = grant.uses.saturating_add(1);
            grant.last_used_at = Some(now);
            grant.id.clone()
        });
        if before != state.grants.len() || matched_persisted(&state.grants, result.as_deref()) {
            persist(&state)?;
        }
        Ok(result)
    }

    pub(super) fn list(&self, now: u64) -> Result<Vec<ExecutionGrant>, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| "Não foi possível consultar as autorizações de execução.".to_owned())?;
        let before = state.grants.len();
        state.grants.retain(|grant| !grant.expired(now));
        if before != state.grants.len() {
            persist(&state)?;
        }
        Ok(state.grants.clone())
    }

    pub(super) fn list_for_project(
        &self,
        project_id: &str,
        now: u64,
    ) -> Result<Vec<ExecutionGrantSummary>, String> {
        Ok(self
            .list(now)?
            .into_iter()
            .filter(|grant| grant.belongs_to_project(project_id))
            .map(ExecutionGrantSummary::from)
            .collect())
    }

    pub(super) fn revoke_for_project(&self, project_id: &str, id: &str) -> Result<bool, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| "Não foi possível revogar a autorização de execução.".to_owned())?;
        let before = state.grants.len();
        state
            .grants
            .retain(|grant| grant.id != id || !grant.belongs_to_project(project_id));
        let removed = before != state.grants.len();
        if removed {
            persist(&state)?;
        }
        Ok(removed)
    }
}

impl ExecutionGrant {
    fn belongs_to_project(&self, project_id: &str) -> bool {
        match &self.scope {
            GrantScope::Project { project_id: stored }
            | GrantScope::Repository {
                project_id: stored, ..
            } => stored == project_id,
            GrantScope::Conversation { .. } => false,
        }
    }
}

impl From<ExecutionGrant> for ExecutionGrantSummary {
    fn from(grant: ExecutionGrant) -> Self {
        let (scope, scope_root) = match &grant.scope {
            GrantScope::Conversation { .. } => (GrantScopeKind::Conversation, None),
            GrantScope::Project { .. } => (GrantScopeKind::Project, None),
            GrantScope::Repository { root, .. } => (
                GrantScopeKind::Repository,
                Some(root.to_string_lossy().into_owned()),
            ),
        };
        let (match_kind, subject) = match &grant.subject {
            GrantSubject::CommandExact { plan } => {
                (GrantMatch::Exact, display_command(plan, false))
            }
            GrantSubject::NativeCommandExact {
                plan,
                working_directory,
                ..
            } => (
                GrantMatch::Exact,
                format!(
                    "{} · acesso nativo em {}",
                    display_command(plan, false),
                    working_directory.display()
                ),
            ),
            GrantSubject::CommandPrefix { argv } => (
                GrantMatch::CommandPrefix,
                format!("{} …", display_argv(argv)),
            ),
            GrantSubject::ToolExact { name, .. } => (GrantMatch::Exact, name.clone()),
        };
        let duration = match grant.duration {
            GrantDuration::Once => GrantDurationKind::Once,
            GrantDuration::Session => GrantDurationKind::Session,
            GrantDuration::Until { .. } => GrantDurationKind::Until,
            GrantDuration::Persistent => GrantDurationKind::Persistent,
        };
        Self {
            id: grant.id,
            scope,
            scope_root,
            match_kind,
            duration,
            expires_at: grant.duration.expires_at(),
            subject,
            effects: grant.effects,
            created_at: grant.created_at,
            last_used_at: grant.last_used_at,
            uses: grant.uses,
        }
    }
}

fn display_command(plan: &CommandPlan, prefix: bool) -> String {
    let separator = if prefix { " " } else { " | " };
    plan.invocations
        .iter()
        .map(|invocation| display_argv(&invocation.argv))
        .collect::<Vec<_>>()
        .join(separator)
}

fn display_argv(argv: &[String]) -> String {
    argv.iter()
        .map(|argument| {
            if argument.chars().any(char::is_whitespace) {
                serde_json::to_string(argument).unwrap_or_else(|_| argument.clone())
            } else {
                argument.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn subject(request: &CreateGrant<'_>) -> Result<GrantSubject, String> {
    if let Some(directory) = &request.outcome.native_working_directory {
        if request.match_kind != GrantMatch::Exact {
            return Err(
                "A execução fora do isolamento exige autorização exata do comando e diretório."
                    .into(),
            );
        }
        return Ok(GrantSubject::NativeCommandExact {
            plan: request
                .outcome
                .command
                .clone()
                .ok_or("A autorização nativa exige um comando.")?,
            working_directory: directory.clone(),
            read_paths: request.outcome.read_paths.clone(),
            write_paths: request.outcome.write_paths.clone(),
        });
    }
    match (request.outcome.command.as_ref(), request.match_kind) {
        (Some(plan), GrantMatch::Exact) => Ok(GrantSubject::CommandExact { plan: plan.clone() }),
        (Some(plan), GrantMatch::CommandPrefix) => Ok(GrantSubject::CommandPrefix {
            argv: safe_prefix(plan)?,
        }),
        (None, GrantMatch::Exact) => Ok(GrantSubject::ToolExact {
            name: request.tool_name.to_owned(),
            arguments: canonical_json(request.tool_arguments),
        }),
        (None, GrantMatch::CommandPrefix) => {
            Err("A autorização por prefixo exige um comando estruturado.".into())
        }
    }
}

fn safe_prefix(plan: &CommandPlan) -> Result<Vec<String>, String> {
    if plan.dynamic || !plan.redirections.is_empty() || plan.invocations.len() != 1 {
        return Err(
            "Comandos compostos, dinâmicos ou redirecionados só aceitam autorização exata.".into(),
        );
    }
    let argv = &plan.invocations[0].argv;
    if argv.len() < 2 {
        return Err("O prefixo precisa incluir o executável e uma operação específica.".into());
    }
    if argv[1].starts_with('-') {
        return Err("Opções antes da operação exigem autorização exata para preservar o diretório e a configuração.".into());
    }
    let mut prefix = vec![argv[0].clone()];
    let operation = argv[1..]
        .iter()
        .find(|argument| !argument.starts_with('-'))
        .ok_or_else(|| "O prefixo precisa conter uma operação estável.".to_owned())?;
    prefix.push(operation.clone());
    Ok(prefix)
}

pub(super) fn can_prefix(outcome: &PolicyOutcome) -> bool {
    outcome.native_working_directory.is_none()
        && outcome
            .command
            .as_ref()
            .is_some_and(|plan| safe_prefix(plan).is_ok())
}

fn scope_matches(scope: &GrantScope, context: &GrantContext<'_>) -> bool {
    match scope {
        GrantScope::Conversation { conversation_id } => conversation_id == context.conversation_id,
        GrantScope::Project { project_id } => project_id == context.project_id,
        GrantScope::Repository { project_id, root } => {
            project_id == context.project_id && context.working_directory.starts_with(root)
        }
    }
}

fn subject_matches(
    subject: &GrantSubject,
    outcome: &PolicyOutcome,
    tool_name: &str,
    tool_arguments: &Value,
) -> bool {
    match subject {
        GrantSubject::CommandExact { plan } => {
            outcome.native_working_directory.is_none() && outcome.command.as_ref() == Some(plan)
        }
        GrantSubject::NativeCommandExact {
            plan,
            working_directory,
            read_paths,
            write_paths,
        } => {
            outcome.command.as_ref() == Some(plan)
                && outcome.native_working_directory.as_ref() == Some(working_directory)
                && outcome.read_paths == *read_paths
                && outcome.write_paths == *write_paths
        }
        GrantSubject::CommandPrefix { argv } => {
            outcome.native_working_directory.is_none()
                && outcome.command.as_ref().is_some_and(|plan| {
                    !plan.dynamic
                        && plan.redirections.is_empty()
                        && plan.invocations.len() == 1
                        && plan.invocations[0].argv.starts_with(argv)
                })
        }
        GrantSubject::ToolExact { name, arguments } => {
            outcome.command.is_none()
                && name == tool_name
                && arguments == &canonical_json(tool_arguments)
        }
    }
}

fn effects_cover(granted: &ExecutionEffects, requested: &ExecutionEffects) -> bool {
    (!requested.reads_filesystem || granted.reads_filesystem)
        && (!requested.writes_filesystem || granted.writes_filesystem)
        && (!requested.uses_network || granted.uses_network)
        && (!requested.controls_processes || granted.controls_processes)
        && (!requested.destructive || granted.destructive)
        && (!requested.dynamic || granted.dynamic)
        && (!requested.unknown || granted.unknown)
}

fn validate_scope(scope: &GrantScope, now: u64, duration: GrantDuration) -> Result<(), String> {
    let valid_id = |id: &str| !id.trim().is_empty() && id.len() <= 128 && !id.contains('\0');
    match scope {
        GrantScope::Conversation { conversation_id } if valid_id(conversation_id) => {}
        GrantScope::Project { project_id } if valid_id(project_id) => {}
        GrantScope::Repository { project_id, root }
            if valid_id(project_id) && root.is_absolute() => {}
        _ => return Err("O escopo da autorização de execução é inválido.".into()),
    }
    if let GrantDuration::Until { expires_at } = duration {
        if expires_at <= now {
            return Err("A expiração da autorização precisa estar no futuro.".into());
        }
    }
    Ok(())
}

fn matched_persisted(grants: &[ExecutionGrant], id: Option<&str>) -> bool {
    id.and_then(|id| grants.iter().find(|grant| grant.id == id))
        .is_some_and(|grant| grant.duration.persisted())
}

fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), canonical_json(value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(canonical_json).collect()),
        _ => value.clone(),
    }
}

fn persist(state: &GrantState) -> Result<(), String> {
    let Some(path) = &state.path else {
        return Ok(());
    };
    let parent = path
        .parent()
        .ok_or_else(|| "O caminho das autorizações de execução é inválido.".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|_| "Não foi possível preparar as autorizações de execução.".to_owned())?;
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("O arquivo das autorizações de execução não é seguro.".into());
        }
    }
    let file = GrantFile {
        version: STORE_VERSION,
        grants: state
            .grants
            .iter()
            .filter(|grant| grant.duration.persisted())
            .cloned()
            .collect(),
    };
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| "Não foi possível salvar as autorizações de execução.".to_owned())?;
    temporary
        .write_all(
            &serde_json::to_vec_pretty(&file).map_err(|_| {
                "Não foi possível serializar as autorizações de execução.".to_owned()
            })?,
        )
        .map_err(|_| "Não foi possível salvar as autorizações de execução.".to_owned())?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| "Não foi possível sincronizar as autorizações de execução.".to_owned())?;
    temporary
        .persist(path)
        .map_err(|_| "Não foi possível substituir as autorizações de execução.".to_owned())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{
        execution_policy::{
            evaluate, parse_command, ExecutionOperation, ExecutionScope, NetworkPolicy,
            PolicyRequest,
        },
        tool_contract::{ApprovalPolicy, Capabilities, Effect},
    };

    fn outcome(command: &str) -> PolicyOutcome {
        let root = Path::new("/project");
        let mut scope = ExecutionScope::project(root, root);
        scope.network = NetworkPolicy::Ask;
        evaluate(PolicyRequest {
            tool_name: "bash",
            capabilities: Capabilities {
                effect: Effect::Mutating,
                approval: ApprovalPolicy::AccordingToTurn,
                parallel_safe: false,
            },
            scope: &scope,
            operation: ExecutionOperation::Command(parse_command(command).unwrap()),
        })
    }

    fn context<'a>() -> GrantContext<'a> {
        GrantContext {
            conversation_id: "conversation",
            project_id: "project",
            working_directory: Path::new("/project/backend"),
        }
    }

    #[test]
    fn native_grants_cannot_be_inherited_from_sandboxed_commands_or_other_directories() {
        let store = GrantStore::default();
        let original = outcome("npm test");
        let create = |outcome: &PolicyOutcome| {
            store
                .create(CreateGrant {
                    scope: GrantScope::Conversation {
                        conversation_id: "conversation".into(),
                    },
                    match_kind: GrantMatch::Exact,
                    duration: GrantDuration::Session,
                    outcome,
                    tool_name: "bash",
                    tool_arguments: &Value::Null,
                    now: 10,
                })
                .unwrap()
        };
        create(&original);
        let mut native = original.clone();
        native.native_working_directory = Some(PathBuf::from("/project/backend"));
        assert!(store
            .authorize(&native, "bash", &Value::Null, context(), 11)
            .unwrap()
            .is_none());
        assert!(!can_prefix(&native));
        let grant = create(&native);
        assert_eq!(
            store
                .authorize(&native, "bash", &Value::Null, context(), 12)
                .unwrap(),
            Some(grant.id)
        );
        native.native_working_directory = Some(PathBuf::from("/project/frontend"));
        assert!(store
            .authorize(&native, "bash", &Value::Null, context(), 13)
            .unwrap()
            .is_none());
        native.native_working_directory = Some(PathBuf::from("/project/backend"));
        native.write_paths.push(PathBuf::from("/other"));
        assert!(store
            .authorize(&native, "bash", &Value::Null, context(), 14)
            .unwrap()
            .is_none());
    }

    #[test]
    fn exact_once_grant_is_consumed_without_broadening() {
        let store = GrantStore::default();
        let requested = outcome("git push origin main");
        store
            .create(CreateGrant {
                scope: GrantScope::Conversation {
                    conversation_id: "conversation".into(),
                },
                match_kind: GrantMatch::Exact,
                duration: GrantDuration::Once,
                outcome: &requested,
                tool_name: "bash",
                tool_arguments: &serde_json::json!({"command":"git push origin main"}),
                now: 10,
            })
            .unwrap();
        assert!(store
            .authorize(&requested, "bash", &Value::Null, context(), 11)
            .unwrap()
            .is_some());
        assert!(store
            .authorize(&requested, "bash", &Value::Null, context(), 12)
            .unwrap()
            .is_none());
        let different = outcome("git push origin other");
        assert!(store
            .authorize(&different, "bash", &Value::Null, context(), 13)
            .unwrap()
            .is_none());
    }

    #[test]
    fn prefix_matches_argv_boundaries_only_in_the_same_repository() {
        let directory = tempfile::tempdir().unwrap();
        let backend = directory.path().join("backend");
        let frontend = directory.path().join("frontend");
        let backend_context = || GrantContext {
            conversation_id: "conversation",
            project_id: "project",
            working_directory: &backend,
        };
        let store = GrantStore::default();
        let requested = outcome("git push origin main");
        store
            .create(CreateGrant {
                scope: GrantScope::Repository {
                    project_id: "project".into(),
                    root: backend.clone(),
                },
                match_kind: GrantMatch::CommandPrefix,
                duration: GrantDuration::Session,
                outcome: &requested,
                tool_name: "bash",
                tool_arguments: &Value::Null,
                now: 10,
            })
            .unwrap();
        let another_branch = outcome("git push origin release");
        assert!(store
            .authorize(&another_branch, "bash", &Value::Null, backend_context(), 11,)
            .unwrap()
            .is_some());
        let wrong_repo = GrantContext {
            conversation_id: "conversation",
            project_id: "project",
            working_directory: &frontend,
        };
        assert!(store
            .authorize(&another_branch, "bash", &Value::Null, wrong_repo, 12)
            .unwrap()
            .is_none());
        let lookalike = outcome("git push-force origin main");
        assert!(store
            .authorize(&lookalike, "bash", &Value::Null, backend_context(), 13)
            .unwrap()
            .is_none());
    }

    #[test]
    fn denied_policy_cannot_be_granted() {
        let store = GrantStore::default();
        let denied = outcome("sudo git status");
        assert_eq!(denied.decision, ExecutionDecision::Deny);
        assert!(store
            .create(CreateGrant {
                scope: GrantScope::Project {
                    project_id: "project".into(),
                },
                match_kind: GrantMatch::Exact,
                duration: GrantDuration::Persistent,
                outcome: &denied,
                tool_name: "bash",
                tool_arguments: &Value::Null,
                now: 10,
            })
            .is_err());
    }

    #[test]
    fn offline_grant_cannot_authorize_a_script_with_network_effects() {
        let store = GrantStore::default();
        let requested = outcome("npm test");
        assert!(requested.effects.uses_network);
        let mut previous = requested.clone();
        previous.effects.uses_network = false;
        store
            .create(CreateGrant {
                scope: GrantScope::Project {
                    project_id: "project".into(),
                },
                match_kind: GrantMatch::Exact,
                duration: GrantDuration::Persistent,
                outcome: &previous,
                tool_name: "bash",
                tool_arguments: &Value::Null,
                now: 10,
            })
            .unwrap();
        assert!(store
            .authorize(&requested, "bash", &Value::Null, context(), 11)
            .unwrap()
            .is_none());
        store
            .create(CreateGrant {
                scope: GrantScope::Project {
                    project_id: "project".into(),
                },
                match_kind: GrantMatch::Exact,
                duration: GrantDuration::Session,
                outcome: &requested,
                tool_name: "bash",
                tool_arguments: &Value::Null,
                now: 12,
            })
            .unwrap();
        assert!(store
            .authorize(&requested, "bash", &Value::Null, context(), 13)
            .unwrap()
            .is_some());
    }

    #[test]
    fn expired_grants_are_not_restored_and_revocation_is_persisted() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("grants.json");
        let store = GrantStore::default();
        store.setup(path.clone(), 10).unwrap();
        let requested = outcome("git push origin main");
        let grant = store
            .create(CreateGrant {
                scope: GrantScope::Project {
                    project_id: "project".into(),
                },
                match_kind: GrantMatch::Exact,
                duration: GrantDuration::Until { expires_at: 20 },
                outcome: &requested,
                tool_name: "bash",
                tool_arguments: &Value::Null,
                now: 10,
            })
            .unwrap();
        assert_eq!(store.list(19).unwrap().len(), 1);
        assert!(store.revoke_for_project("project", &grant.id).unwrap());
        let restored = GrantStore::default();
        restored.setup(path, 21).unwrap();
        assert!(restored.list(21).unwrap().is_empty());
    }

    #[test]
    fn unsafe_prefix_shapes_are_rejected() {
        let store = GrantStore::default();
        let requested = outcome("git push origin main && git status");
        assert!(store
            .create(CreateGrant {
                scope: GrantScope::Project {
                    project_id: "project".into(),
                },
                match_kind: GrantMatch::CommandPrefix,
                duration: GrantDuration::Session,
                outcome: &requested,
                tool_name: "bash",
                tool_arguments: &Value::Null,
                now: 10,
            })
            .unwrap_err()
            .contains("compostos"));
    }
}
