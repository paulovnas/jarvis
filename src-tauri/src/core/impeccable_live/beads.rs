//! Host-owned task projection for human Live requests, independent of variant count.
use super::{error, CoreError};
use crate::core::beads::Beads;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
use tauri::{Emitter, Manager};
use tokio::sync::watch;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Status {
    Pending,
    Paused,
    Running,
    Completed,
    Cancelled,
    Blocked,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pendente",
            Self::Paused => "Pausada",
            Self::Running => "Em andamento",
            Self::Completed => "Concluída",
            Self::Cancelled => "Cancelada",
            Self::Blocked => "Bloqueada",
        }
    }
}

fn request_call(request_key: &str) -> Result<String, CoreError> {
    if request_key.is_empty()
        || request_key.len() > 256
        || !request_key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
    {
        return Err(error("Identidade da solicitação Live inválida."));
    }
    Ok(format!("impeccable-live:{request_key}"))
}

fn identity(
    project: &str,
    conversation: &str,
    request_key: &str,
) -> Result<(String, String), CoreError> {
    let call = request_call(request_key)?;
    let operation = format!("{:x}", Sha256::digest(format!("{conversation}:{call}")));
    Ok((format!("j{project}-{}", &operation[..16]), operation))
}

fn issue(output: &str) -> Result<Value, CoreError> {
    let value: Value = serde_json::from_str(output)
        .map_err(|_| error("O Beads retornou uma tarefa Live inválida."))?;
    let task = value
        .as_array()
        .and_then(|rows| rows.first())
        .unwrap_or(&value);
    if !task.is_object() {
        return Err(error("A tarefa Live não está disponível no Beads."));
    }
    Ok(task.clone())
}

fn owned(
    task: &Value,
    expected_id: &str,
    conversation: &str,
    operation: &str,
) -> Result<(), CoreError> {
    if task["id"] != expected_id
        || task["metadata"]["jarvis_conversation"] != conversation
        || task["metadata"]["jarvis_operation"] != operation
    {
        return Err(error(
            "A tarefa do Beads não pertence a esta solicitação Live.",
        ));
    }
    Ok(())
}

fn original_conversation(
    task: &Value,
    project: &str,
    request_key: &str,
    bead_id: &str,
) -> Result<String, CoreError> {
    let conversation = task["metadata"]["jarvis_conversation"]
        .as_str()
        .filter(|conversation| {
            conversation.len() == 32 && conversation.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        .ok_or_else(|| error("A origem da tarefa Live é inválida. Os dados foram preservados."))?;
    let (expected_id, operation) = identity(project, conversation, request_key)?;
    if bead_id != expected_id {
        return Err(error(
            "A tarefa não corresponde à solicitação Live deste projeto.",
        ));
    }
    owned(task, &expected_id, conversation, &operation)?;
    Ok(conversation.into())
}

fn bounded(value: &str, maximum: usize) -> String {
    let value = value.trim();
    if value.len() <= maximum {
        return value.to_owned();
    }
    let mut end = maximum;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", value[..end].trim_end())
}

fn request_fields(event: &Value) -> Result<(String, String), CoreError> {
    let prompt = ["freeformPrompt", "message", "prompt", "text"]
        .iter()
        .find_map(|key| {
            event[*key]
                .as_str()
                .filter(|value| !value.trim().is_empty())
        })
        .ok_or_else(|| error("A solicitação Live não informou o pedido do usuário."))?;
    if prompt.contains('\0') {
        return Err(error("O texto da solicitação Live é inválido."));
    }
    let title = bounded(
        prompt
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or(prompt),
        220,
    );
    // Queue/journal retains the complete request. This bounded task synopsis must
    // never copy helper tokens, screenshot blobs, arbitrary HTML or page query strings.
    let mut description = format!(
        "Solicitação enviada pelo usuário no Impeccable Live.\n\n{}",
        bounded(prompt, 12_000)
    );
    if prompt.len() > 12_000 {
        description.push_str("\n\nO pedido completo permanece na fila Live do Jarvis.");
    }
    for (key, label) in [("selector", "Elemento"), ("sourceFile", "Arquivo")] {
        if let Some(value) = event[key].as_str().filter(|value| !value.is_empty()) {
            if value.contains('\0') {
                return Err(error("A referência da solicitação Live é inválida."));
            }
            description.push_str(&format!("\n\n{label}: {}", bounded(value, 1_000)));
        }
    }
    Ok((title, description))
}

fn notify(app: Option<&tauri::AppHandle>, project: &str) {
    if let Some(app) = app {
        let _ = app.emit("beads:changed", project);
    }
}

/// `request_key` must identify one human request, not an Impeccable session or variant.
/// The caller's `check_live` also rechecks the request revision before each mutation.
#[allow(
    clippy::too_many_arguments,
    reason = "Keep project, conversation and request ownership explicit at the host boundary"
)]
pub(super) async fn ensure<F>(
    app: Option<&tauri::AppHandle>,
    home: &Path,
    project_id: &str,
    conversation_id: &str,
    request_key: &str,
    event: &Value,
    signal: watch::Receiver<bool>,
    check_live: F,
) -> Result<String, CoreError>
where
    F: Fn() -> Result<(), CoreError> + Send + Sync,
{
    let call = request_call(request_key)?;
    let (expected_id, operation) = identity(project_id, conversation_id, request_key)?;
    let (title, description) = request_fields(event)?;
    let beads = Beads::new(home, project_id, conversation_id, false)?;
    let task = issue(
        &beads
            .execute(
                "beads_create",
                &json!({"title":title,"description":description,"type":"task"}),
                &call,
                signal,
                check_live,
            )
            .await?,
    )?;
    owned(&task, &expected_id, conversation_id, &operation)?;
    notify(app, project_id);
    Ok(expected_id)
}

fn progress_notes(
    current: &str,
    request_key: &str,
    status: Status,
    reason: &str,
) -> Result<String, CoreError> {
    if reason.contains('\0') || reason.len() > 4_000 {
        return Err(error(
            "O resumo da tarefa Live precisa ter até 4.000 bytes.",
        ));
    }
    let start = format!("[Jarvis Live {request_key}]");
    let end = format!("[/Jarvis Live {request_key}]");
    if reason.contains(&start)
        || reason.contains(&end)
        || current.matches(&start).count() > 1
        || current.matches(&end).count() > 1
    {
        return Err(error("As notas da tarefa Live contêm um marcador conflitante. O conteúdo existente foi preservado."));
    }
    let summary = format!(
        "{start}\nEstado: {}{}\n{end}",
        status.label(),
        if reason.trim().is_empty() {
            String::new()
        } else {
            format!("\n{}", reason.trim())
        }
    );
    let notes = match current.find(&start).and_then(|first| {
        current[first..]
            .find(&end)
            .map(|last| (first, first + last + end.len()))
    }) {
        Some((first, last)) => format!("{}{summary}{}", &current[..first], &current[last..]),
        None if current.trim().is_empty() => summary,
        None => format!("{current}\n\n{summary}"),
    };
    if notes.len() > 16_000 {
        return Err(error(
            "As notas da tarefa Live excederam o limite. O conteúdo existente foi preservado.",
        ));
    }
    Ok(notes)
}

fn update_args(
    task: &Value,
    request_key: &str,
    status: Status,
    reason: &str,
    event: Option<&Value>,
) -> Result<Value, CoreError> {
    if task["status"] == "closed" {
        if status == Status::Completed && event.is_none() {
            return Ok(json!({}));
        }
        return Err(error(
            "Esta tarefa Live já foi encerrada. O resultado confirmado foi preservado.",
        ));
    }
    let mut args = json!({"id": task["id"]});
    if status == Status::Completed {
        // Only a successful close exposes completion. A failed close must not
        // leave a misleading completed note on an open/blocked task.
        return Ok(args);
    }
    let target = match status {
        Status::Pending => Some("open"),
        Status::Paused | Status::Cancelled => Some("deferred"),
        Status::Blocked => Some("blocked"),
        Status::Running | Status::Completed => None,
    };
    if let Some(target) = target.filter(|target| task["status"] != *target) {
        args["status"] = json!(target);
    }
    let current_notes = task["notes"].as_str().unwrap_or_default();
    let notes = progress_notes(current_notes, request_key, status, reason)?;
    if notes != current_notes {
        args["notes"] = json!(notes);
    }
    if let Some(event) = event {
        let (title, description) = request_fields(event)?;
        if task["title"] != title {
            args["title"] = json!(title);
        }
        if task["description"] != description {
            args["description"] = json!(description);
        }
    }
    Ok(args)
}

/// Projects queue state into its owned task. Editing is allowed only before admission.
/// Cancelled requests are deferred with an audit note/comment; no task is deleted.
#[allow(
    clippy::too_many_arguments,
    reason = "Keep task ownership and the exact observed queue transition explicit"
)]
pub(super) async fn sync<F>(
    app: Option<&tauri::AppHandle>,
    home: &Path,
    project_id: &str,
    conversation_id: &str,
    request_key: &str,
    bead_id: &str,
    status: Status,
    reason: &str,
    event: Option<&Value>,
    signal: watch::Receiver<bool>,
    check_live: F,
) -> Result<bool, CoreError>
where
    F: Fn() -> Result<(), CoreError> + Send + Sync,
{
    let call = request_call(request_key)?;
    if event.is_some() && !matches!(status, Status::Pending | Status::Paused) {
        return Err(error(
            "Edite apenas solicitações Live que ainda não começaram.",
        ));
    }
    if reason.contains('\0') || reason.len() > 4_000 {
        return Err(error(
            "O resumo da tarefa Live precisa ter até 4.000 bytes.",
        ));
    }
    if status == Status::Completed && reason.trim().is_empty() {
        return Err(error(
            "Informe a confirmação do resultado antes de concluir a tarefa Live.",
        ));
    }
    let beads = Beads::new(home, project_id, conversation_id, false)?;
    let task = issue(
        &beads
            .execute(
                "beads_show",
                &json!({"id":bead_id}),
                &call,
                signal.clone(),
                &check_live,
            )
            .await?,
    )?;
    // Queues belong to a project and may be explicitly resumed from a later
    // conversation. Resolve the original actor only from the task in this
    // project's private database, then prove its exact request operation.
    let original = original_conversation(&task, project_id, request_key, bead_id)?;
    let beads = Beads::new(home, project_id, &original, false)?;
    let actor = format!("jarvis-{original}");
    if task["assignee"]
        .as_str()
        .is_some_and(|assignee| !assignee.is_empty() && assignee != actor)
    {
        return Err(error("Esta tarefa Live está atribuída a outra conversa. A atribuição existente foi preservada."));
    }
    let args = update_args(&task, request_key, status, reason, event)?;
    if args.as_object().is_some_and(|args| args.is_empty()) {
        return Ok(false);
    }
    let mut changed = false;
    if status == Status::Running && (task["status"] != "in_progress" || task["assignee"] != actor) {
        if matches!(task["status"].as_str(), Some("deferred" | "blocked")) {
            // Beads only claims open tasks; Live pauses while awaiting a variant choice.
            beads
                .execute(
                    "beads_update",
                    &json!({"id":bead_id,"status":"open"}),
                    &format!("{call}:reopen"),
                    signal.clone(),
                    &check_live,
                )
                .await?;
        }
        beads
            .execute(
                "beads_claim",
                &json!({"id":bead_id}),
                &format!("{call}:claim"),
                signal.clone(),
                &check_live,
            )
            .await?;
        changed = true;
    }
    if args.as_object().is_some_and(|args| args.len() > 1) {
        beads
            .execute(
                "beads_update",
                &args,
                &format!("{call}:sync"),
                signal.clone(),
                &check_live,
            )
            .await?;
        changed = true;
    }
    if status == Status::Completed {
        beads
            .execute(
                "beads_close",
                &json!({"id":bead_id,"reason":reason}),
                &format!("{call}:complete"),
                signal,
                &check_live,
            )
            .await?;
        changed = true;
    }
    if status == Status::Cancelled {
        let comment = format!(
            "Solicitação Impeccable Live {request_key} cancelada pelo usuário. {}",
            reason.trim()
        );
        let recorded = task["comments"]
            .as_array()
            .is_some_and(|comments| comments.iter().any(|existing| existing["text"] == comment));
        if let Some(app) = app.filter(|_| !recorded) {
            check_live()?;
            crate::core::beads::dashboard::add_bead_comment(
                app.clone(),
                app.state::<crate::persistence::AppState>(),
                project_id.into(),
                bead_id.into(),
                comment,
            )
            .await?;
            changed = true;
        }
    }
    if changed {
        notify(app, project_id);
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    const PROJECT: &str = "0123456789abcdef0123456789abcdef";
    const CONVERSATION: &str = "fedcba9876543210fedcba9876543210";

    fn task() -> Value {
        let (id, operation) = identity(PROJECT, CONVERSATION, "aabbcc01:1").unwrap();
        json!({"id":id,"status":"open","notes":"Análise confirmada pelo usuário.","metadata":{"jarvis_conversation":CONVERSATION,"jarvis_operation":operation}})
    }

    #[tokio::test]
    #[ignore = "Uses the installed private Beads binary in an isolated temporary home"]
    async fn installed_live_choice_resumes_the_same_paused_or_blocked_bead() {
        use crate::core::{self, ComponentId, Installation, Manifest};
        use std::{fs, path::PathBuf};

        let host = PathBuf::from(
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .unwrap(),
        );
        let installed = core::installed(&host, ComponentId::Beads).unwrap();
        let package = installed.path(&host).unwrap();
        let home = tempfile::tempdir().unwrap();
        let binary = core::install::executable("bd");
        let target = core::root(home.path()).join("test-beads");
        fs::create_dir_all(&target).unwrap();
        fs::copy(package.join(&binary), target.join(&binary)).unwrap();
        let mut manifest = Manifest::default();
        manifest.installations.insert(
            ComponentId::Beads,
            Installation {
                version: installed.version,
                directory: "test-beads".into(),
                files: vec![binary],
            },
        );
        core::save_manifest(home.path(), &manifest).unwrap();
        let (_sender, signal) = watch::channel(false);
        let bead = ensure(
            None,
            home.path(),
            PROJECT,
            CONVERSATION,
            "aabbcc01:1",
            &json!({"freeformPrompt":"Change only the selected card spacing"}),
            signal.clone(),
            || Ok(()),
        )
        .await
        .unwrap();
        let beads = Beads::new(home.path(), PROJECT, CONVERSATION, false).unwrap();
        for status in [
            Status::Running,
            Status::Paused,
            Status::Running,
            Status::Blocked,
            Status::Running,
            Status::Completed,
        ] {
            sync(
                None,
                home.path(),
                PROJECT,
                CONVERSATION,
                "aabbcc01:1",
                &bead,
                status,
                "Manual choice confirmed by the Live journal.",
                None,
                signal.clone(),
                || Ok(()),
            )
            .await
            .unwrap();
            let current = issue(
                &beads
                    .execute(
                        "beads_show",
                        &json!({"id":bead}),
                        "verify",
                        signal.clone(),
                        || Ok(()),
                    )
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(current["id"], bead);
            assert_eq!(
                current["status"],
                match status {
                    Status::Running => "in_progress",
                    Status::Paused => "deferred",
                    Status::Blocked => "blocked",
                    Status::Completed => "closed",
                    _ => unreachable!(),
                }
            );
        }
    }

    #[test]
    fn one_human_request_has_a_retry_stable_task_independent_of_variants() {
        let first = identity(PROJECT, CONVERSATION, "aabbcc01:1").unwrap();
        assert_eq!(
            first,
            identity(PROJECT, CONVERSATION, "aabbcc01:1").unwrap()
        );
        assert_ne!(
            first,
            identity(PROJECT, CONVERSATION, "aabbcc01:2").unwrap()
        );
        assert_ne!(first, identity(PROJECT, PROJECT, "aabbcc01:1").unwrap());
        assert_eq!(
            request_fields(&json!({"freeformPrompt":"Mover a badge","count":1})).unwrap(),
            request_fields(&json!({"freeformPrompt":"Mover a badge","count":3})).unwrap()
        );
    }

    #[test]
    fn rejects_other_task_ownership_and_does_not_copy_page_secrets() {
        let (id, operation) = identity(PROJECT, CONVERSATION, "aabbcc01:1").unwrap();
        owned(&task(), &id, CONVERSATION, &operation).unwrap();
        assert!(owned(&task(), &id, PROJECT, &operation).is_err());
        let (_, description) = request_fields(&json!({"freeformPrompt":"Mover a badge","token":"SECRET_TOKEN","screenshot":"SECRET_BLOB","pageUrl":"http://localhost:3001/?token=SECRET_URL","outerHTML":"SECRET_HTML"})).unwrap();
        assert!(!description.contains("SECRET"));
    }

    #[test]
    fn project_queue_can_resume_from_another_chat_without_changing_task_origin() {
        let task = task();
        let bead_id = task["id"].as_str().unwrap();
        assert_eq!(
            original_conversation(&task, PROJECT, "aabbcc01:1", bead_id).unwrap(),
            CONVERSATION
        );
        assert!(original_conversation(&task, PROJECT, "aabbcc01:2", bead_id).is_err());
        assert!(original_conversation(&task, CONVERSATION, "aabbcc01:1", bead_id).is_err());
        let mut changed = task.clone();
        changed["metadata"]["jarvis_conversation"] = json!(PROJECT);
        assert!(original_conversation(&changed, PROJECT, "aabbcc01:1", bead_id).is_err());
        changed["metadata"]["jarvis_conversation"] = json!("../../other-project");
        assert!(original_conversation(&changed, PROJECT, "aabbcc01:1", bead_id).is_err());
        changed["metadata"]["jarvis_conversation"] = json!(CONVERSATION);
        changed["metadata"]["jarvis_operation"] = json!("different-operation");
        assert!(original_conversation(&changed, PROJECT, "aabbcc01:1", bead_id).is_err());
    }

    #[test]
    fn cancellation_is_deferred_and_preserves_confirmed_notes() {
        let task = task();
        let update = update_args(
            &task,
            "aabbcc01:1",
            Status::Cancelled,
            "Retirada da fila.",
            None,
        )
        .unwrap();
        assert_eq!(update["status"], "deferred");
        assert!(update["notes"]
            .as_str()
            .unwrap()
            .contains("Análise confirmada pelo usuário."));
        assert!(update["notes"].as_str().unwrap().contains("Cancelada"));
        assert!(update["notes"]
            .as_str()
            .unwrap()
            .contains("Retirada da fila."));
        let mut cancelled = task;
        cancelled["status"] = update["status"].clone();
        cancelled["notes"] = update["notes"].clone();
        assert_eq!(
            update_args(
                &cancelled,
                "aabbcc01:1",
                Status::Cancelled,
                "Retirada da fila.",
                None
            )
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
            1
        );
    }

    #[test]
    fn phase_updates_do_not_accumulate_notes_or_erase_other_requests() {
        let current = "Dados confirmados.";
        let pending = progress_notes(current, "first", Status::Pending, "").unwrap();
        let both =
            progress_notes(&pending, "second", Status::Paused, "Modelo indisponível.").unwrap();
        let running = progress_notes(&both, "first", Status::Running, "").unwrap();
        assert!(running.contains(current));
        assert!(running.contains("Modelo indisponível."));
        assert_eq!(running.matches("[Jarvis Live first]").count(), 1);
        assert!(!running.contains("Estado: Pendente"));
        assert_eq!(
            running,
            progress_notes(&running, "first", Status::Running, "").unwrap()
        );
        assert!(progress_notes(&"x".repeat(16_000), "first", Status::Blocked, "Erro").is_err());
    }

    #[test]
    fn completed_data_cannot_be_cancelled_or_edited() {
        let mut completed = task();
        completed["status"] = json!("closed");
        assert!(update_args(
            &completed,
            "aabbcc01:1",
            Status::Cancelled,
            "Cancelar",
            None
        )
        .is_err());
        assert!(update_args(
            &completed,
            "aabbcc01:1",
            Status::Pending,
            "",
            Some(&json!({"message":"Alteração nova"}))
        )
        .is_err());
        assert!(update_args(
            &completed,
            "aabbcc01:1",
            Status::Completed,
            "Já confirmado",
            None
        )
        .unwrap()
        .as_object()
        .unwrap()
        .is_empty());
    }

    #[test]
    fn edited_unicode_text_is_bounded_without_splitting_characters() {
        let (title, description) =
            request_fields(&json!({"message":"á".repeat(10_000),"sourceFile":"src/badge.tsx"}))
                .unwrap();
        assert!(title.len() <= 223);
        assert!(description.len() < 16_000);
        assert!(description.contains("pedido completo permanece na fila"));
        assert!(description.contains("src/badge.tsx"));
        assert!(request_call("bad\nkey").is_err());
    }
}
