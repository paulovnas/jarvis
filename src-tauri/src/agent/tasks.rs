use super::{AgentError, Session};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

const MAX_TASKS: usize = 20;
const MAX_ID_CHARS: usize = 64;
const MAX_TITLE_CHARS: usize = 160;

pub(super) const INSTRUCTIONS: &str = r#"
Direct task tracking: Standard and direct Designer flows use update_tasks instead of the private Jarvis workflow Beads. Do not call internal beads_* tools in these flows. Read-only project_beads_* tools, when available, refer only to the checkout's independent project history. For implementation work or any request with more than one material action, inspect enough to understand the scope and then call update_tasks before the first mutating tool, even when the list has only two items. Keep the ordered list concise and outcome-oriented; replace the complete list whenever scope or status changes. Keep exactly one item in_progress while working, mark items completed only after their outcome is verified, and use blocked only when progress actually depends on missing input or an external condition. Before the final response, leave no pending or in_progress items. Purely informational or trivial responses may finish without creating a task list.
"#;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "TaskStatus"))]
#[serde(rename_all = "snake_case")]
pub(super) enum Status {
    Pending,
    InProgress,
    Completed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "DirectTask"))]
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
    ) || super::browser::mutating(name)
        || crate::core::context::needs_approval(name)
}

// This exemption affects task bookkeeping only. It never grants execution permission
// or replaces shell/publication policy. Unknown syntax remains conservative.
pub(super) fn requires_active_task_for(tool: &super::ToolCall) -> bool {
    let read_only = match tool.name.as_str() {
        "bash" => tool.args["command"]
            .as_str()
            .is_some_and(read_only_inspection),
        "ctx_batch_execute" => tool.args["commands"].as_array().is_some_and(|commands| {
            !commands.is_empty()
                && commands.iter().all(|command| {
                    command["command"]
                        .as_str()
                        .is_some_and(read_only_inspection)
                })
        }),
        _ => false,
    };
    !read_only && requires_active_task(&tool.name)
}

fn read_only_inspection(command: &str) -> bool {
    // The shared parser does not yet preserve newline command boundaries.
    if command.contains(['\n', '\r', '$', '`', '(', ')']) {
        return false;
    }
    let Ok(plan) = super::execution_policy::parse_command(command) else {
        return false;
    };
    !plan.dynamic
        && plan
            .redirections
            .iter()
            .all(|redirect| redirect.target == "/dev/null")
        && plan
            .invocations
            .iter()
            .all(|invocation| read_only_invocation(&invocation.argv))
}

fn read_only_invocation(argv: &[String]) -> bool {
    let words: Vec<_> = argv.iter().map(String::as_str).collect();
    if words.contains(&"&") {
        return false;
    }
    match words.as_slice() {
        ["pwd" | "Get-Location" | "true" | "false"] | ["cd", _] => true,
        ["git", rest @ ..] => {
            let rest = match rest {
                ["-C", _, rest @ ..] => rest,
                rest => rest,
            };
            if rest.iter().any(|word| {
                word.starts_with("--output") || matches!(*word, "--ext-diff" | "--textconv")
            }) {
                return false;
            }
            match rest {
                ["status" | "diff" | "log" | "show" | "rev-parse" | "ls-files" | "merge-base", ..] => {
                    true
                }
                ["branch", args @ ..] => args.iter().all(|arg| {
                    matches!(
                        *arg,
                        "--list" | "--all" | "--remotes" | "--verbose" | "--show-current"
                    ) || arg.strip_prefix('-').is_some_and(|flags| {
                        !flags.is_empty() && flags.chars().all(|flag| "arv".contains(flag))
                    }) || (args.contains(&"--list") && !arg.starts_with('-'))
                }),
                _ => false,
            }
        }
        ["gh", "pr", "view" | "list" | "status", ..] => true,
        _ => false,
    }
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

    #[test]
    fn inspection_does_not_require_tasks_but_unknown_or_effectful_commands_do() {
        for command in [
            "git status --short",
            "git -C backend diff --stat",
            "git -C 'frontend app' diff --stat && git log -3",
            "cd movart-express-front && git branch -avv && git rev-parse HEAD origin/hml && git merge-base HEAD origin/hml",
            "git branch --list hml && git rev-parse --verify refs/heads/hml 2>/dev/null || true; git rev-parse --verify refs/remotes/origin/hml",
            "git log -3",
            "gh pr view --json state",
            "pwd",
            "Get-Location",
        ] {
            assert!(read_only_inspection(command), "{command}");
        }
        for command in [
            "npm test",
            "git commit -m fix",
            "git diff --output=stolen.txt",
            "git status; rm file",
            "git status & rm file",
            "git status\nrm file",
            "git status && git commit -m fix",
            "git branch feature",
            "git branch --list --delete feature",
            "git branch -l feature",
            "gh api -X POST repos/a/b",
            "git show $(touch file)",
            "git diff > file",
        ] {
            assert!(!read_only_inspection(command), "{command}");
        }
    }

    #[test]
    fn inspection_batches_only_skip_task_bookkeeping_when_every_command_is_read_only() {
        let mut tool = super::super::ToolCall {
            id: "inspection".into(),
            name: "ctx_batch_execute".into(),
            args: json!({"commands":[
                {"label":"PR", "command":"gh pr list --head paulovnas --base hml --state open"},
                {"label":"Refs", "command":"git branch -avv && git merge-base HEAD origin/hml"},
                {"label":"Diff", "command":"git log --oneline --left-right --cherry-pick -n 20 origin/hml...HEAD && git diff --stat origin/hml...HEAD"}
            ]}),
            status: "running".into(),
            output: String::new(),
            duration_ms: 0,
        };
        assert!(!requires_active_task_for(&tool));
        for command in ["git push origin HEAD", "npm test", "git diff > result.txt"] {
            tool.args["commands"][1]["command"] = json!(command);
            assert!(requires_active_task_for(&tool), "{command}");
        }
        tool.args = json!({"commands":[]});
        assert!(requires_active_task_for(&tool));
        tool.args = json!({"commands":[{"label":"Missing command"}]});
        assert!(requires_active_task_for(&tool));
    }

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
        ] {
            assert!(requires_active_task(name), "{name} should require a task");
        }
        for name in [
            "read",
            "search",
            "lsp_definition",
            "web_search",
            "update_tasks",
            "mcp_external_lookup",
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
