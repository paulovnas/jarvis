use super::{AgentError, Session};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

const MAX_TASKS: usize = 20;
const MAX_ID_CHARS: usize = 64;
const MAX_TITLE_CHARS: usize = 160;

pub(super) const INSTRUCTIONS: &str = r#"
Direct task tracking: Standard and direct Designer flows use update_tasks instead of Beads. Do not call beads_* tools in these flows. For implementation work or any request with more than one material action, inspect enough to understand the scope and then call update_tasks before the first mutating tool, even when the list has only two items. Keep the ordered list concise and outcome-oriented; replace the complete list whenever scope or status changes. Keep exactly one item in_progress while working, mark items completed only after their outcome is verified, and use blocked only when progress actually depends on missing input or an external condition. Before the final response, leave no pending or in_progress items. Purely informational or trivial responses may finish without creating a task list.
"#;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Status {
    Pending,
    InProgress,
    Completed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Task {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) status: Status,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    tasks: Vec<Task>,
}

pub(super) fn definition() -> Value {
    json!({
        "type": "function",
        "name": "update_tasks",
        "description": "Replace the direct agent's complete ordered task list. Use it to expose planned, active, completed and blocked work in the Jarvis inspector.",
        "parameters": {
            "type": "object",
            "properties": {
                "tasks": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_TASKS,
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {
                                "type": "string",
                                "minLength": 1,
                                "maxLength": MAX_ID_CHARS,
                                "pattern": "^[A-Za-z0-9][A-Za-z0-9._-]*$"
                            },
                            "title": {
                                "type": "string",
                                "minLength": 1,
                                "maxLength": MAX_TITLE_CHARS
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed", "blocked"]
                            }
                        },
                        "required": ["id", "title", "status"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["tasks"],
            "additionalProperties": false
        }
    })
}

pub(super) fn execute(session: &Session, args: &Value) -> Result<String, AgentError> {
    let parsed: Arguments = serde_json::from_value(args.clone()).map_err(|_| invalid())?;
    let tasks = validate(parsed.tasks)?;
    let completed = tasks
        .iter()
        .filter(|task| task.status == Status::Completed)
        .count();
    let total = tasks.len();
    session.replace_tasks(tasks)?;
    Ok(json!({ "updated": total, "completed": completed }).to_string())
}

fn validate(mut tasks: Vec<Task>) -> Result<Vec<Task>, AgentError> {
    if tasks.is_empty() || tasks.len() > MAX_TASKS {
        return Err(invalid());
    }
    let mut ids = HashSet::with_capacity(tasks.len());
    let mut active = 0;
    for task in &mut tasks {
        if task.id.chars().count() > MAX_ID_CHARS
            || !task
                .id
                .starts_with(|character: char| character.is_ascii_alphanumeric())
            || !task
                .id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
            || !ids.insert(task.id.clone())
        {
            return Err(invalid());
        }
        let title = task.title.trim();
        if title.is_empty()
            || title.chars().count() > MAX_TITLE_CHARS
            || title.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        task.title = title.to_owned();
        if task.status == Status::InProgress {
            active += 1;
        }
    }
    if active > 1 {
        return Err(AgentError::new(
            "invalid_tasks",
            "Mantenha no máximo uma tarefa em andamento.",
        ));
    }
    Ok(tasks)
}

fn invalid() -> AgentError {
    AgentError::new(
        "invalid_tasks",
        "A lista de tarefas é inválida. Use de 1 a 20 itens com IDs únicos, títulos curtos e estados válidos.",
    )
}

pub(super) fn has_active(tasks: &[Task]) -> bool {
    tasks.iter().any(|task| task.status == Status::InProgress)
}

pub(super) fn has_unfinished(tasks: &[Task]) -> bool {
    tasks
        .iter()
        .any(|task| matches!(task.status, Status::Pending | Status::InProgress))
}

pub(super) fn requires_active_task(name: &str) -> bool {
    matches!(
        name,
        "write"
            | "edit"
            | "apply_patch"
            | "bash"
            | "generate_image"
            | "process_start"
            | "process_stop"
            | "process_remove"
            | "terminal_start"
            | "terminal_write"
            | "terminal_close"
    ) || name.starts_with("mcp_")
        || super::browser::mutating(name)
        || crate::core::context::needs_approval(name)
}

pub(super) fn context(tasks: &[Task]) -> String {
    if tasks.is_empty() {
        String::new()
    } else {
        format!(
            "Native direct-task checkpoint (resume this exact ordered state with update_tasks): {}",
            json!(tasks)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, title: &str, status: Status) -> Task {
        Task {
            id: id.into(),
            title: title.into(),
            status,
        }
    }

    #[test]
    fn validates_ids_titles_limits_and_single_active_item() {
        assert!(validate(vec![task(
            "inspect",
            "  Inspect project  ",
            Status::InProgress
        )])
        .is_ok_and(|tasks| tasks[0].title == "Inspect project"));
        assert!(validate(vec![
            task("same", "One", Status::Pending),
            task("same", "Two", Status::Completed),
        ])
        .is_err());
        assert!(validate(vec![task("bad id", "One", Status::Pending)]).is_err());
        assert!(validate(vec![task("empty", "  ", Status::Pending)]).is_err());
        assert!(validate(vec![
            task("one", "One", Status::InProgress),
            task("two", "Two", Status::InProgress),
        ])
        .is_err());
        assert!(validate(
            (0..=MAX_TASKS)
                .map(|index| task(&format!("task-{index}"), "Work", Status::Pending))
                .collect()
        )
        .is_err());
    }

    #[test]
    fn mutable_tools_require_an_active_task_but_reads_and_updates_do_not() {
        for name in [
            "write",
            "apply_patch",
            "bash",
            "process_start",
            "terminal_write",
            "browser_click",
            "ctx_execute",
            "mcp_external_action",
        ] {
            assert!(requires_active_task(name), "{name} should require a task");
        }
        for name in [
            "read",
            "search",
            "lsp_definition",
            "web_search",
            "update_tasks",
        ] {
            assert!(
                !requires_active_task(name),
                "{name} should remain available"
            );
        }
    }

    #[test]
    fn compaction_context_preserves_the_current_order_and_status() {
        let tasks = vec![
            task("inspect", "Inspect", Status::Completed),
            task("build", "Build", Status::InProgress),
        ];
        let checkpoint = context(&tasks);
        assert!(checkpoint.find("inspect").unwrap() < checkpoint.find("build").unwrap());
        assert!(checkpoint.contains("in_progress"));
    }
}
