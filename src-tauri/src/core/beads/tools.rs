use super::*;
use serde::Deserialize;

pub fn needs_approval(name: &str) -> bool {
    matches!(
        name,
        "beads_create" | "beads_update" | "beads_claim" | "beads_close" | "beads_dependency"
    )
}
pub fn definitions(plan: bool) -> Vec<Value> {
    let text = json!({"type":"string","minLength":1,"maxLength":16000});
    let id = json!({"type":"string","minLength":1,"maxLength":120});
    let limit = json!({"type":"integer","minimum":1,"maximum":50});
    let mut tools = vec![
        definition("beads_list", "List this project's durable tasks. Optionally filter by status, parent or title substring. Use beads_show for full requirements and progress notes.", json!({"status":{"type":"string","enum":["active","all","open","in_progress","blocked","deferred","closed"]},"parent":id,"query":text,"limit":limit}), &[]),
        definition("beads_ready", "Find unblocked, claimable tasks in this project's Beads tracker.", json!({"parent":id,"limit":limit}), &[]),
        definition("beads_show", "Read full task requirements, status, progress notes, dependencies and current comments by exact ID.", json!({"id":id}), &["id"]),
    ];
    if !plan {
        tools.extend([
            definition("beads_create", "Create a durable task or epic in this project. Consult existing tasks first to avoid duplicates. Use parent for subtasks. Jarvis records the source conversation automatically.", json!({"title":{"type":"string","minLength":1,"maxLength":240},"description":text,"type":{"type":"string","enum":["task","epic","bug","feature","chore","decision"]},"priority":{"type":"integer","minimum":0,"maximum":4},"parent":id}), &["title","description"]),
            definition("beads_update", "Update task fields. notes replaces the progress/handoff summary: retain relevant existing facts. To start work use beads_claim; to complete work use beads_close.", json!({"id":id,"title":{"type":"string","minLength":1,"maxLength":240},"description":text,"notes":text,"status":{"type":"string","enum":["open","blocked","deferred"]},"priority":{"type":"integer","minimum":0,"maximum":4}}), &["id"]),
            definition("beads_claim", "Atomically claim a task for this conversation and mark it in progress. An existing claim by another conversation must be respected.", json!({"id":id}), &["id"]),
            definition("beads_close", "Close completed, validated work. Include concrete evidence in reason. Does not force unresolved gates or dependencies.", json!({"id":id,"reason":text}), &["id","reason"]),
            definition("beads_dependency", "Add or remove a blocking dependency: id cannot proceed until depends_on completes. Cycles are rejected by Beads.", json!({"id":id,"depends_on":id,"action":{"type":"string","enum":["add","remove"]}}), &["id","depends_on","action"]),
        ]);
    }
    tools
}
fn definition(name: &str, description: &str, mut properties: Value, required: &[&str]) -> Value {
    if let Some(fields) = properties.as_object_mut() {
        for (key, schema) in fields {
            if !required.contains(&key.as_str()) {
                *schema = json!({"anyOf":[schema.take(), {"type":"null"}], "description":"Optional. Omit or use null when unused; never invent a placeholder."});
            }
        }
    }
    // Responses can normalize an unspecified strict mode by requiring every
    // property. Explicitly preserve optional fields, including nullable values
    // used by providers that still supply every property.
    json!({"type":"function","name":name,"description":description,"strict":false,"parameters":{"type":"object","properties":properties,"required":required,"additionalProperties":false}})
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    id: Option<String>,
    title: Option<String>,
    description: Option<String>,
    notes: Option<String>,
    status: Option<String>,
    parent: Option<String>,
    query: Option<String>,
    limit: Option<u32>,
    #[serde(rename = "type")]
    kind: Option<String>,
    priority: Option<u8>,
    reason: Option<String>,
    depends_on: Option<String>,
    action: Option<String>,
}
pub(super) struct Call {
    pub args: Vec<String>,
    pub write: bool,
    pub limit: usize,
    pub created_id: Option<String>,
    pub operation: String,
}
fn argument_error() -> CoreError {
    failure("Argumentos inválidos para a ferramenta do Beads.")
}
fn nonempty(value: &str, max: usize) -> Result<(), CoreError> {
    if value.trim().is_empty() || value.len() > max || value.contains('\0') {
        return Err(argument_error());
    }
    Ok(())
}
pub(super) fn issue_id(value: &str, prefix: &str) -> Result<(), CoreError> {
    if !value.starts_with(&format!("{prefix}-"))
        || value.len() > 120
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'.'))
    {
        return Err(failure("A tarefa precisa pertencer ao projeto atual."));
    }
    Ok(())
}
fn flag(args: &mut Vec<String>, name: &str, value: &str) {
    args.push(format!("--{name}={value}"));
}

pub(super) fn parse(
    name: &str,
    args: &Value,
    plan: bool,
    prefix: &str,
    session: &str,
    call_id: &str,
) -> Result<Call, CoreError> {
    let definitions = definitions(plan);
    let definition = definitions
        .iter()
        .find(|d| d["name"] == name)
        .ok_or_else(|| failure("Ferramenta do Beads indisponível neste modo."))?;
    // Validate against the same strict public schema, including fields which
    // belong to other Beads actions, before constructing any CLI arguments.
    let validator =
        jsonschema::validator_for(&definition["parameters"]).map_err(|_| argument_error())?;
    if !validator.is_valid(args) {
        return Err(argument_error());
    }
    let input: Args = serde_json::from_value(args.clone()).map_err(|_| argument_error())?;
    for value in [&input.id, &input.parent, &input.depends_on]
        .into_iter()
        .flatten()
    {
        issue_id(value, prefix)?;
    }
    for value in [
        &input.description,
        &input.notes,
        &input.query,
        &input.reason,
    ]
    .into_iter()
    .flatten()
    {
        nonempty(value, 16_000)?;
    }
    if let Some(title) = &input.title {
        nonempty(title, 960)?;
    }
    let mut command = Vec::new();
    let mut created_id = None;
    let operation = format!("{:x}", Sha256::digest(format!("{session}:{call_id}")));
    let limit = input.limit.unwrap_or(20) as usize;
    let required_id = || input.id.as_deref().ok_or_else(argument_error);
    match name {
        "beads_list" | "beads_ready" => {
            command.push(
                if name == "beads_list" {
                    "list"
                } else {
                    "ready"
                }
                .into(),
            );
            flag(&mut command, "limit", &limit.to_string());
            if let Some(parent) = &input.parent {
                flag(&mut command, "parent", parent);
            }
            if name == "beads_list" {
                command.push("--flat".into());
                flag(&mut command, "sort", "updated");
                command.push("--reverse".into());
                match input.status.as_deref().unwrap_or("active") {
                    "all" => command.push("--all".into()),
                    "active" => flag(&mut command, "status", "open,in_progress,blocked,deferred"),
                    status => flag(&mut command, "status", status),
                }
                if let Some(query) = &input.query {
                    flag(&mut command, "title-contains", query);
                }
            }
        }
        "beads_show" => {
            command.extend(["show".into(), required_id()?.into()]);
        }
        "beads_create" => {
            let id = format!("{prefix}-{}", &operation[..16]);
            command.push("create".into());
            flag(&mut command, "id", &id);
            flag(
                &mut command,
                "title",
                input.title.as_deref().ok_or_else(argument_error)?,
            );
            flag(
                &mut command,
                "description",
                input.description.as_deref().ok_or_else(argument_error)?,
            );
            flag(
                &mut command,
                "type",
                input.kind.as_deref().unwrap_or("task"),
            );
            flag(
                &mut command,
                "priority",
                &input.priority.unwrap_or(2).to_string(),
            );
            if let Some(parent) = input.parent {
                // --parent allocates its own ID and cannot coexist with --id.
                // A parent-child edge keeps creation atomic and retry-stable.
                flag(&mut command, "deps", &format!("parent-child:{parent}"));
            }
            flag(
                &mut command,
                "metadata",
                &json!({"jarvis_conversation":session,"jarvis_operation":operation}).to_string(),
            );
            created_id = Some(id);
        }
        "beads_update" => {
            command.extend(["update".into(), required_id()?.into()]);
            for (key, value) in [
                ("title", &input.title),
                ("description", &input.description),
                ("notes", &input.notes),
                ("status", &input.status),
            ] {
                if let Some(value) = value {
                    flag(&mut command, key, value);
                }
            }
            if let Some(priority) = input.priority {
                flag(&mut command, "priority", &priority.to_string());
            }
            if command.len() == 2 {
                return Err(argument_error());
            }
        }
        "beads_claim" => {
            command.extend(["update".into(), required_id()?.into(), "--claim".into()]);
        }
        "beads_close" => {
            command.extend(["close".into(), required_id()?.into()]);
            flag(
                &mut command,
                "reason",
                input.reason.as_deref().ok_or_else(argument_error)?,
            );
        }
        "beads_dependency" => {
            let target = input.depends_on.as_deref().ok_or_else(argument_error)?;
            if required_id()? == target {
                return Err(argument_error());
            }
            command.extend([
                "dep".into(),
                if input.action.as_deref() == Some("add") {
                    "add"
                } else {
                    "remove"
                }
                .into(),
                required_id()?.into(),
                target.into(),
            ]);
        }
        _ => return Err(argument_error()),
    }
    Ok(Call {
        args: command,
        write: needs_approval(name),
        limit,
        created_id,
        operation,
    })
}

pub(super) fn output(name: &str, value: Value, limit: usize) -> Result<String, CoreError> {
    if matches!(name, "beads_list" | "beads_ready") {
        let rows = value
            .as_array()
            .ok_or_else(|| failure("Listagem inválida do Beads."))?;
        let tasks: Vec<_> = rows
            .iter()
            .take(limit)
            .map(|row| {
                let mut task = serde_json::Map::new();
                for key in [
                    "id",
                    "title",
                    "status",
                    "priority",
                    "issue_type",
                    "assignee",
                    "parent",
                    "updated_at",
                    "dependency_count",
                    "dependent_count",
                    "metadata",
                ] {
                    if let Some(value) = row.get(key) {
                        task.insert(key.into(), value.clone());
                    }
                }
                Value::Object(task)
            })
            .collect();
        return Ok(
            json!({"tasks":tasks,"limit":limit,"may_have_more":rows.len() >= limit}).to_string(),
        );
    }
    Ok(value.to_string())
}
