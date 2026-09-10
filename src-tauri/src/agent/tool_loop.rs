use super::{AgentError, ToolCall};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};

const STEER_AFTER: usize = 5;
const READ_CACHE_CAPACITY: usize = 64;
pub(super) const READ_REUSE_MESSAGE: &str = "Resultado reutilizado pelo Jarvis: o arquivo e o intervalo continuam byte a byte iguais à leitura anterior. Use o conteúdo já presente no histórico; uma nova cópia foi omitida para reduzir contexto.";

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReadEntry {
    fingerprint: [u8; 32],
    output_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ReadReuse {
    pub(super) original_bytes: u64,
}

#[derive(Default)]
pub(super) struct ReadReuseCache {
    entries: HashMap<super::tools::ReadIdentity, ReadEntry>,
    order: VecDeque<super::tools::ReadIdentity>,
}

impl ReadReuseCache {
    pub(super) fn resolve(&self, observation: &super::tools::ReadObservation) -> Option<ReadReuse> {
        self.entries
            .get(&observation.identity)
            .filter(|entry| {
                entry.fingerprint == observation.fingerprint
                    && entry.output_bytes > READ_REUSE_MESSAGE.len() as u64
            })
            .map(|entry| ReadReuse {
                original_bytes: entry.output_bytes,
            })
    }

    pub(super) fn remember(
        &mut self,
        observation: super::tools::ReadObservation,
        available_in_history: bool,
    ) {
        self.remove(&observation.identity);
        if !available_in_history {
            return;
        }
        while self.entries.len() >= READ_CACHE_CAPACITY {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
        self.order.push_back(observation.identity.clone());
        self.entries.insert(
            observation.identity,
            ReadEntry {
                fingerprint: observation.fingerprint,
                output_bytes: observation.output_bytes,
            },
        );
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    pub(super) fn failed_read(&mut self) {
        self.clear();
    }

    fn remove(&mut self, identity: &super::tools::ReadIdentity) {
        if self.entries.remove(identity).is_some() {
            self.order.retain(|candidate| candidate != identity);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CallKey {
    name: String,
    arguments: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Observation {
    call: CallKey,
    result: ResultClass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResultClass {
    Error,
    Success(u64),
}

#[derive(Default)]
pub(super) struct Guard {
    last: Option<Observation>,
    consecutive: usize,
    steered: Option<CallKey>,
    stale_mutation_paths: HashSet<String>,
}

impl Guard {
    pub(super) fn before_call(&self, tool: &ToolCall) -> Result<(), AgentError> {
        if mutating(&tool.name) {
            let stale: Vec<_> = paths(tool)
                .into_iter()
                .filter(|path| self.stale_mutation_paths.contains(path))
                .collect();
            if !stale.is_empty() {
                return Err(AgentError::new(
                    "stale_edit_context",
                    &format!(
                        "A alteração anterior falhou e o conteúdo pode ter mudado. Leia novamente {} antes de tentar outra escrita ou patch nesses arquivos.",
                        stale.join(", ")
                    ),
                ));
            }
        }
        if exempt(&tool.name) {
            return Ok(());
        }
        let call = call_key(tool);
        if self.steered.as_ref() == Some(&call) {
            return Err(AgentError::new(
                "repeated_tool_loop",
                "O agente repetiu a mesma chamada após receber uma orientação para mudar de estratégia. A execução foi interrompida antes de executar a ação novamente.",
            ));
        }
        Ok(())
    }

    pub(super) fn observe(
        &mut self,
        tool: &ToolCall,
        failed: bool,
        output: &str,
    ) -> Option<String> {
        if tool.name == "read" && !failed {
            for path in paths(tool) {
                self.stale_mutation_paths.remove(&path);
            }
        } else if failed && matches!(tool.name.as_str(), "edit" | "apply_patch") {
            self.stale_mutation_paths.extend(paths(tool));
        }
        if exempt(&tool.name) {
            self.reset();
            return None;
        }
        let observation = Observation {
            call: call_key(tool),
            result: if failed {
                ResultClass::Error
            } else {
                ResultClass::Success(stable_hash(output.as_bytes()))
            },
        };
        if self.last.as_ref() == Some(&observation) {
            self.consecutive += 1;
        } else {
            self.last = Some(observation.clone());
            self.consecutive = 1;
            self.steered = None;
        }
        if self.consecutive != STEER_AFTER {
            return None;
        }
        self.steered = Some(observation.call);
        Some(format!(
            "Jarvis detected {STEER_AFTER} consecutive identical calls to `{}` with the same result class. Do not call it again with the same arguments. Explain what is blocking progress and choose a different source, query, file, tool, or approach. If user input is required, use ask_user.",
            tool.name
        ))
    }

    fn reset(&mut self) {
        self.last = None;
        self.consecutive = 0;
        self.steered = None;
    }
}

fn mutating(name: &str) -> bool {
    matches!(name, "write" | "edit" | "apply_patch")
}

fn paths(tool: &ToolCall) -> Vec<String> {
    let raw = if tool.name == "apply_patch" {
        super::patch::target_paths(&tool.args).unwrap_or_default()
    } else {
        tool.args["path"]
            .as_str()
            .map(|path| vec![path.to_owned()])
            .unwrap_or_default()
    };
    raw.into_iter()
        .map(|path| path.replace('\\', "/").trim_start_matches("./").to_owned())
        .collect()
}

fn call_key(tool: &ToolCall) -> CallKey {
    CallKey {
        name: tool.name.clone(),
        arguments: canonical_json(&tool.args),
    }
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(object) => {
            let mut entries: Vec<_> = object.iter().collect();
            entries.sort_by_key(|(left, _)| *left);
            let fields = entries
                .into_iter()
                .map(|(key, value)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap_or_default(),
                        canonical_json(value)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{fields}}}")
        }
        Value::Array(array) => format!(
            "[{}]",
            array
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn exempt(name: &str) -> bool {
    matches!(
        name,
        "ask_user"
            | "process_list"
            | "process_output"
            | "process_check_port"
            | "terminal_output"
            | "browser_snapshot"
            | "browser_console"
            | "hub_wait"
            | "hub_list"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{fs, path::Path};
    use tokio::sync::watch;

    fn tool(name: &str, args: Value) -> ToolCall {
        ToolCall {
            id: "call".into(),
            name: name.into(),
            args,
            status: "completed".into(),
            output: String::new(),
            duration_ms: 0,
        }
    }

    async fn read(root: &Path, args: Value) -> super::super::tools::ReadObservation {
        let (_send, signal) = watch::channel(false);
        super::super::tools::execute_with_revision(
            root,
            &tool("read", args),
            super::super::Mode::Plan,
            signal,
        )
        .await
        .unwrap()
        .read
        .unwrap()
    }

    #[test]
    fn fifth_identical_result_steers_and_next_identical_call_is_stopped() {
        let mut guard = Guard::default();
        let call = tool("read", json!({"path":"src/app.ts"}));
        for _ in 0..4 {
            assert!(guard.observe(&call, false, "same").is_none());
            assert!(guard.before_call(&call).is_ok());
        }
        assert!(guard
            .observe(&call, false, "same")
            .is_some_and(|message| message.contains("5 consecutive identical calls")));
        assert_eq!(
            guard.before_call(&call).unwrap_err().code,
            "repeated_tool_loop"
        );
    }

    #[test]
    fn canonical_arguments_ignore_object_key_order_and_changed_results_reset_sequence() {
        let mut guard = Guard::default();
        let first = tool("search", json!({"query":"needle","path":"src"}));
        let reordered = tool("search", json!({"path":"src","query":"needle"}));
        assert!(guard.observe(&first, false, "one").is_none());
        assert!(guard.observe(&reordered, false, "one").is_none());
        assert_eq!(guard.consecutive, 2);
        assert!(guard.observe(&reordered, false, "two").is_none());
        assert_eq!(guard.consecutive, 1);
    }

    #[test]
    fn polling_tools_never_accumulate_repetition() {
        let mut guard = Guard::default();
        let call = tool("process_output", json!({"id":"server"}));
        for _ in 0..20 {
            assert!(guard.observe(&call, false, "unchanged").is_none());
            assert!(guard.before_call(&call).is_ok());
        }
        assert_eq!(guard.consecutive, 0);
    }

    #[test]
    fn repeated_errors_share_a_result_class_even_when_messages_change() {
        let mut guard = Guard::default();
        let call = tool("web_search", json!({"query":"status"}));
        for index in 0..4 {
            assert!(guard
                .observe(&call, true, &format!("request {index} failed"))
                .is_none());
        }
        assert!(guard.observe(&call, true, "request 5 failed").is_some());
    }

    #[test]
    fn failed_edit_requires_a_fresh_read_before_another_mutation() {
        let mut guard = Guard::default();
        let edit = tool(
            "edit",
            json!({"path":"src/app.ts","oldText":"old","newText":"new"}),
        );
        assert!(guard.observe(&edit, true, "stale excerpt").is_none());
        assert_eq!(
            guard.before_call(&edit).unwrap_err().code,
            "stale_edit_context"
        );
        let read = tool("read", json!({"path":"./src/app.ts"}));
        guard.observe(&read, false, "fresh source");
        assert!(guard.before_call(&edit).is_ok());
    }

    #[test]
    fn failed_patch_protects_every_target_until_each_file_is_read() {
        let mut guard = Guard::default();
        let patch = tool(
            "apply_patch",
            json!({"patchText":"*** Begin Patch\n*** Update File: src/a.ts\n@@\n-old\n+new\n*** Update File: src/b.ts\n@@\n-old\n+new\n*** End Patch"}),
        );
        guard.observe(&patch, true, "hunk mismatch");
        assert_eq!(
            guard.before_call(&patch).unwrap_err().code,
            "stale_edit_context"
        );
        guard.observe(&tool("read", json!({"path":"src/a.ts"})), false, "fresh a");
        assert!(guard.before_call(&patch).is_err());
        guard.observe(&tool("read", json!({"path":"src/b.ts"})), false, "fresh b");
        assert!(guard.before_call(&patch).is_ok());
    }

    #[tokio::test]
    async fn unchanged_equivalent_read_is_reused_but_full_file_changes_are_not() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let path = root.join("source.txt");
        let original = (1..=80)
            .map(|line| format!("line {line:03}: aaaa\n"))
            .collect::<String>();
        fs::write(&path, &original).unwrap();

        let mut cache = ReadReuseCache::default();
        let first = read(&root, json!({"path":"source.txt"})).await;
        assert!(cache.resolve(&first).is_none());
        cache.remember(first.clone(), true);

        let repeated = read(&root, json!({"path":"./source.txt","offset":1,"limit":200})).await;
        assert_eq!(first.identity, repeated.identity);
        assert_eq!(
            cache.resolve(&repeated),
            Some(ReadReuse {
                original_bytes: first.output_bytes
            })
        );

        let ranged_before = read(&root, json!({"path":"source.txt","offset":1,"limit":10})).await;
        cache.remember(ranged_before.clone(), true);

        let modified_at = fs::metadata(&path).unwrap().modified().unwrap();
        let changed = original.replacen("line 080: aaaa", "line 080: bbbb", 1);
        assert_eq!(changed.len(), original.len());
        fs::write(&path, changed).unwrap();
        fs::File::open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified_at))
            .unwrap();
        let changed_outside_range =
            read(&root, json!({"path":"source.txt","offset":1,"limit":10})).await;
        assert_eq!(ranged_before.identity, changed_outside_range.identity);
        assert_ne!(ranged_before.fingerprint, changed_outside_range.fingerprint);
        assert!(cache.resolve(&changed_outside_range).is_none());
        cache.remember(changed_outside_range, true);

        fs::write(&path, original).unwrap();
        let changed_again = read(&root, json!({"path":"source.txt","offset":1,"limit":10})).await;
        assert!(cache.resolve(&changed_again).is_none());
    }

    #[tokio::test]
    async fn failed_removed_or_symlinked_reads_discard_prior_reuse_references() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let path = root.join("source.txt");
        let content = "safe source line\n".repeat(40);
        fs::write(&path, &content).unwrap();
        let mut cache = ReadReuseCache::default();
        let previous = read(&root, json!({"path":"source.txt"})).await;
        cache.remember(previous.clone(), true);

        fs::remove_file(&path).unwrap();
        let (_send, signal) = watch::channel(false);
        assert!(super::super::tools::execute_with_revision(
            &root,
            &tool("read", json!({"path":"source.txt"})),
            super::super::Mode::Plan,
            signal.clone(),
        )
        .await
        .is_err());
        cache.failed_read();
        fs::write(&path, &content).unwrap();
        assert!(cache
            .resolve(&read(&root, json!({"path":"source.txt"})).await)
            .is_none());

        #[cfg(unix)]
        {
            cache.remember(read(&root, json!({"path":"source.txt"})).await, true);
            fs::remove_file(&path).unwrap();
            fs::write(root.join("target.txt"), &content).unwrap();
            std::os::unix::fs::symlink(root.join("target.txt"), &path).unwrap();
            assert!(super::super::tools::execute_with_revision(
                &root,
                &tool("read", json!({"path":"source.txt"})),
                super::super::Mode::Plan,
                signal,
            )
            .await
            .is_err());
            cache.failed_read();
            assert!(cache.resolve(&previous).is_none());
        }
    }

    #[tokio::test]
    async fn write_edit_shell_and_external_changes_never_return_a_stale_read() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let path = root.join("cache.txt");
        let mut cache = ReadReuseCache::default();
        let values = [
            "A".repeat(300),
            "B".repeat(300),
            "C".repeat(300),
            "D".repeat(300),
        ];
        fs::write(&path, &values[0]).unwrap();
        cache.remember(read(&root, json!({"path":"cache.txt"})).await, true);
        let (_send, signal) = watch::channel(false);

        super::super::tools::execute_with_revision(
            &root,
            &tool("write", json!({"path":"cache.txt","content":values[1]})),
            super::super::Mode::Build,
            signal.clone(),
        )
        .await
        .unwrap();
        let after_write = read(&root, json!({"path":"cache.txt"})).await;
        assert!(cache.resolve(&after_write).is_none());
        cache.remember(after_write, true);

        super::super::tools::execute_with_revision(
            &root,
            &tool(
                "edit",
                json!({"path":"cache.txt","oldText":values[1],"newText":values[2]}),
            ),
            super::super::Mode::Build,
            signal.clone(),
        )
        .await
        .unwrap();
        let after_edit = read(&root, json!({"path":"cache.txt"})).await;
        assert!(cache.resolve(&after_edit).is_none());
        cache.remember(after_edit, true);

        let command = if cfg!(windows) {
            "Set-Content -NoNewline -Path cache.txt -Value ('D' * 300)"
        } else {
            "printf '%*s' 300 '' | tr ' ' D > cache.txt"
        };
        super::super::tools::execute(
            &root,
            &tool("bash", json!({"command":command})),
            super::super::Mode::Build,
            signal,
        )
        .await
        .unwrap();
        let after_shell = read(&root, json!({"path":"cache.txt"})).await;
        assert!(cache.resolve(&after_shell).is_none());
        cache.remember(after_shell, true);

        fs::write(&path, &values[0]).unwrap();
        assert!(cache
            .resolve(&read(&root, json!({"path":"cache.txt"})).await)
            .is_none());
    }

    #[test]
    fn read_cache_is_bounded_and_drops_unavailable_or_compacted_history() {
        let mut cache = ReadReuseCache::default();
        let observation = |index: usize| super::super::tools::ReadObservation {
            identity: super::super::tools::ReadIdentity {
                path: format!("/project/{index}.txt").into(),
                offset: 1,
                limit: 200,
            },
            fingerprint: [index as u8; 32],
            output_bytes: 1_000,
        };
        for index in 0..=READ_CACHE_CAPACITY {
            cache.remember(observation(index), true);
        }
        assert_eq!(cache.entries.len(), READ_CACHE_CAPACITY);
        assert!(cache.resolve(&observation(0)).is_none());
        assert!(cache.resolve(&observation(READ_CACHE_CAPACITY)).is_some());

        cache.remember(observation(READ_CACHE_CAPACITY), false);
        assert!(cache.resolve(&observation(READ_CACHE_CAPACITY)).is_none());
        cache.remember(observation(1), true);
        cache.clear();
        assert!(cache.resolve(&observation(1)).is_none());

        cache.remember(observation(2), true);
        cache.failed_read();
        assert!(cache.resolve(&observation(2)).is_none());
    }
}
