use super::{
    compaction::Checkpoint, diffs::FileRevision, queue::QueuedMessage, AgentError, StoredTurn,
    TurnStatus,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

const MAX_JOURNAL: u64 = 64 * 1024 * 1024;
const MAX_RECORD: usize = 10 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    r#type: String,
    version: u8,
    data: serde_json::Value,
}

#[derive(Default)]
pub(super) struct Extras {
    pub queue: Vec<QueuedMessage>,
    pub context: Option<Checkpoint>,
    pub files: std::collections::BTreeMap<String, FileRevision>,
}

fn open(path: &Path, write: bool) -> Result<File, AgentError> {
    let mut options = OpenOptions::new();
    options.read(true).write(write).append(write);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path).map_err(|_| AgentError::storage())?;
    let meta = file.metadata().map_err(|_| AgentError::storage())?;
    if !meta.is_file() || meta.len() > MAX_JOURNAL {
        return Err(AgentError::storage());
    }
    Ok(file)
}

pub(super) fn append(path: &Path, turn: &StoredTurn) -> Result<(), AgentError> {
    append_event(path, "turn_checkpoint", turn)
}

pub(super) fn append_event(
    path: &Path,
    kind: &str,
    value: &impl Serialize,
) -> Result<(), AgentError> {
    let mut bytes = serde_json::to_vec(&Record {
        r#type: kind.into(),
        version: 1,
        data: serde_json::to_value(value).map_err(|_| AgentError::storage())?,
    })
    .map_err(|_| AgentError::storage())?;
    bytes.push(b'\n');
    if bytes.len() > MAX_RECORD {
        return Err(AgentError::new(
            "history_limit",
            "Esta interação atingiu o limite de histórico local.",
        ));
    }
    let mut file = open(path, true)?;
    if file.metadata().map_err(|_| AgentError::storage())?.len() + bytes.len() as u64 > MAX_JOURNAL
    {
        return Err(AgentError::storage());
    }
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| AgentError::storage())
}

pub(super) fn load_all(path: &Path) -> Result<(Vec<StoredTurn>, Extras), AgentError> {
    let mut bytes = vec![];
    open(path, false)?
        .take(MAX_JOURNAL + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AgentError::storage())?;
    if bytes.len() as u64 > MAX_JOURNAL {
        return Err(AgentError::storage());
    }
    let mut turns: Vec<StoredTurn> = vec![];
    let mut extras = Extras::default();
    let mut valid_end = 0;
    for (index, line) in bytes.split_inclusive(|byte| *byte == b'\n').enumerate() {
        if !line.ends_with(b"\n") {
            break;
        }
        valid_end += line.len();
        if index == 0 {
            continue;
        } // The immutable header is verified by library::agent_location.
        if line.len() > MAX_RECORD {
            return Err(AgentError::storage());
        }
        let record: Record = serde_json::from_slice(line).map_err(|_| {
            AgentError::new(
                "invalid_history",
                "O histórico contém um registro inválido. O arquivo original foi preservado.",
            )
        })?;
        if record.version != 1 {
            return Err(AgentError::storage());
        }
        match record.r#type.as_str() {
            "queue_checkpoint" => {
                extras.queue =
                    serde_json::from_value(record.data).map_err(|_| AgentError::storage())?;
                continue;
            }
            "context_checkpoint" => {
                extras.context =
                    Some(serde_json::from_value(record.data).map_err(|_| AgentError::storage())?);
                continue;
            }
            "file_checkpoint" => {
                let file: FileRevision =
                    serde_json::from_value(record.data).map_err(|_| AgentError::storage())?;
                extras.files.insert(file.path.clone(), file);
                continue;
            }
            "turn_checkpoint" => {}
            _ => return Err(AgentError::storage()),
        }
        let turn: StoredTurn =
            serde_json::from_value(record.data).map_err(|_| AgentError::storage())?;
        if turns
            .last()
            .is_some_and(|last| last.turn.id == turn.turn.id)
        {
            *turns.last_mut().unwrap() = turn;
        } else {
            if turns
                .iter()
                .any(|previous| previous.turn.id == turn.turn.id)
            {
                return Err(AgentError::storage());
            }
            turns.push(turn);
        }
    }
    if valid_end < bytes.len() {
        // Preserve crash debris before repairing only an incomplete final line.
        let backup = path.with_extension(format!("recovery-{}.jsonl", crate::library::new_id()?));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut copy = options.open(&backup).map_err(|_| AgentError::storage())?;
        copy.write_all(&bytes)
            .and_then(|()| copy.sync_all())
            .map_err(|_| AgentError::storage())?;
        #[cfg(unix)]
        File::open(path.parent().ok_or_else(AgentError::storage)?)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| AgentError::storage())?;
        let file = open(path, true)?;
        file.set_len(valid_end as u64)
            .and_then(|()| file.sync_all())
            .map_err(|_| AgentError::storage())?;
    }
    for turn in &mut turns {
        if turn.turn.status == TurnStatus::Running {
            interrupt_tools(turn);
            turn.turn.status = TurnStatus::Interrupted;
            turn.turn.error = Some(AgentError::new("interrupted", "O Jarvis foi encerrado durante esta execução. Revise os arquivos antes de continuar; ferramentas não foram repetidas."));
            append(path, turn)?;
        }
    }
    // A queued message and its turn share an ID. A crash between the durable
    // turn reservation and the queue update must never replay the same message.
    extras
        .queue
        .retain(|message| !turns.iter().any(|turn| turn.turn.id == message.id));
    if let Some(context) = &extras.context {
        context.validate(&turns)?;
    }
    Ok((turns, extras))
}

#[cfg(test)]
fn load(path: &Path) -> Result<Vec<StoredTurn>, AgentError> {
    load_all(path).map(|(turns, _)| turns)
}

pub(super) fn interrupt_tools(turn: &mut StoredTurn) {
    for step in &mut turn.turn.steps {
        for tool in &mut step.tools {
            if !turn
                .wire
                .iter()
                .any(|item| item["type"] == "function_call_output" && item["call_id"] == tool.id)
            {
                tool.status = "error".into();
                tool.output = "Execução interrompida; resultado desconhecido. Verifique o estado atual antes de repetir a operação.".into();
                if tool.name == "ask_user" {
                    tool.output = super::questions::cancelled_output();
                }
                turn.wire.push(
                    json!({"type":"function_call_output", "call_id":tool.id, "output":tool.output}),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{tests::Fixture, ApprovalMode, Mode, Step, ToolCall, Turn, TurnOptions};
    use std::fs;
    fn turn() -> StoredTurn {
        StoredTurn {
            turn: Turn {
                id: "turn-1".into(),
                created_at: 1,
                duration_ms: 0,
                user: "Read".into(),
                parts: vec![],
                context_window: Some(128_000),
                options: TurnOptions {
                    account: "test".into(),
                    model: "model".into(),
                    reasoning: None,
                    mode: Mode::Build,
                    approval_mode: ApprovalMode::Manual,
                },
                status: TurnStatus::Running,
                steps: vec![],
                error: None,
            },
            wire: vec![json!({"role":"user", "content":"Read"})],
        }
    }
    #[test]
    fn restart_preserves_messages_and_repairs_missing_tool_result_without_execution() {
        let fixture = Fixture::new();
        let path = fixture.root.join("session.jsonl");
        fs::write(&path, "{\"type\":\"session\"}\n").unwrap();
        let mut item = turn();
        item.turn.steps.push(Step {
            text: "Checking".into(),
            tools: vec![ToolCall {
                id: "call-1".into(),
                name: "write".into(),
                args: json!({}),
                status: "running".into(),
                output: String::new(),
                duration_ms: 0,
            }],
            ..Step::default()
        });
        item.wire.push(
            json!({"type":"function_call", "call_id":"call-1", "name":"write", "arguments":"{}"}),
        );
        append(&path, &item).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].turn.status, TurnStatus::Interrupted);
        assert_eq!(loaded[0].turn.steps[0].text, "Checking");
        assert_eq!(loaded[0].turn.context_window, Some(128_000));
        assert_eq!(loaded[0].wire[2]["call_id"], "call-1");
        let twice = load(&path).unwrap();
        assert_eq!(twice[0].wire.len(), 3);
    }
    #[test]
    fn legacy_turn_without_context_window_remains_readable() {
        let mut value = serde_json::to_value(turn()).unwrap();
        value["turn"]
            .as_object_mut()
            .unwrap()
            .remove("contextWindow");
        let stored: StoredTurn = serde_json::from_value(value).unwrap();
        assert_eq!(stored.turn.context_window, None);
    }

    #[test]
    fn incomplete_tail_is_backed_up_but_corrupt_complete_record_is_not_rewritten() {
        let fixture = Fixture::new();
        let path = fixture.root.join("session.jsonl");
        fs::write(&path, "{}\n").unwrap();
        let mut item = turn();
        item.turn.status = TurnStatus::Completed;
        append(&path, &item).unwrap();
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{broken")
            .unwrap();
        let original = fs::read(&path).unwrap();
        assert_eq!(load(&path).unwrap().len(), 1);
        let backup = fs::read_dir(&fixture.root)
            .unwrap()
            .map(|item| item.unwrap().path())
            .find(|item| item.to_string_lossy().contains("recovery-"))
            .unwrap();
        assert_eq!(fs::read(backup).unwrap(), original);
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{broken}\n")
            .unwrap();
        let corrupt = fs::read(&path).unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), corrupt);
    }
}
