use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use serde_json::{json, Value};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, Command},
    sync::mpsc,
    task::JoinHandle,
};

const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const AGENT_NAME: &str = "jarvis-runtime";

fn diagnostic_label(value: Option<&str>) -> String {
    value.map_or_else(
        || "ausente".into(),
        |value| {
            value
                .chars()
                .filter(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
                })
                .take(80)
                .collect()
        },
    )
}

pub(crate) struct RunOptions {
    pub cwd: PathBuf,
    /// Jarvis-owned durable workspace; native continuation must survive process restart.
    pub workspace_dir: PathBuf,
    pub session_id: Option<String>,
    pub model: String,
    pub effort: Option<String>,
    /// The generated custom agent's system instructions, not an initial user message.
    pub prompt: String,
    /// Both empty for tool-free generation; otherwise a local per-run MCP bridge.
    pub mcp_url: String,
    pub mcp_token: String,
}

pub(crate) struct AgyProcess {
    stdin: Option<ChildStdin>,
    events: mpsc::Receiver<Result<Value, String>>,
    child: Option<Box<dyn ChildWrapper>>,
    reader: JoinHandle<()>,
    stderr_reader: JoinHandle<()>,
    agent_file: PathBuf,
}

pub(crate) fn private_directory(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            return Err("A pasta de configuração AGY não pode ser um link ou arquivo.".into())
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => std::fs::create_dir_all(path)
            .map_err(|_| "Não foi possível preparar a configuração privada AGY.".to_owned())?,
        Err(_) => return Err("Não foi possível verificar a configuração privada AGY.".into()),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "Não foi possível proteger a configuração AGY.".to_owned())?;
    }
    Ok(())
}

fn agent_document(options: &RunOptions) -> Result<String, String> {
    let tool_free = options.mcp_url.is_empty() && options.mcp_token.is_empty();
    let servers = if tool_free {
        json!([])
    } else {
        json!([{"name":"jarvis","serverUrl":options.mcp_url,"headers":{"Authorization":format!("Bearer {}",options.mcp_token)}}])
    };
    let mut header = json!({
        "name":AGENT_NAME,"description":"Jarvis-managed agent; only Jarvis executes tools",
        "excludeDefaultComponents":true,"inheritMcp":false,"subagent":false,"mainAgent":true,
        "mcpServers":servers,
    });
    if tool_free {
        header["tools"] = json!([]);
    }
    let header = serde_yaml_ng::to_string(&header)
        .map_err(|_| "Configuração do agente AGY inválida.".to_owned())?;
    Ok(format!("---\n{header}---\n{}\n", options.prompt))
}

fn canonical_destination(path: &Path) -> Result<PathBuf, String> {
    let mut ancestor = path.to_path_buf();
    let mut missing = Vec::new();
    loop {
        match std::fs::canonicalize(&ancestor) {
            Ok(mut canonical) => {
                for component in missing.into_iter().rev() {
                    canonical.push(component);
                }
                return Ok(canonical);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    ancestor
                        .file_name()
                        .ok_or("Caminho AGY inválido.")?
                        .to_owned(),
                );
                if !ancestor.pop() {
                    return Err("Caminho AGY inválido.".into());
                }
            }
            Err(_) => return Err("Não foi possível verificar a pasta de execução AGY.".into()),
        }
    }
}

pub(crate) fn prepare_executor_workspace(project: &Path, workspace: &Path) -> Result<(), String> {
    if !project.is_absolute()
        || !project.is_dir()
        || !workspace.is_absolute()
        || workspace.starts_with(project)
        || workspace
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("A execução AGY exige uma pasta de projeto absoluta e uma configuração privada fora do projeto.".into());
    }
    let project = std::fs::canonicalize(project)
        .map_err(|_| "Não foi possível verificar a pasta de projeto AGY.".to_owned())?;
    if canonical_destination(workspace)?.starts_with(&project) {
        return Err("A configuração privada AGY não pode apontar para dentro do projeto.".into());
    }
    private_directory(workspace)
}

pub(super) fn command_for(
    executable: &Path,
    options: &RunOptions,
) -> Result<(Command, PathBuf), String> {
    super::validate_selection(&options.model, options.effort.as_deref())?;
    prepare_executor_workspace(&options.cwd, &options.workspace_dir)?;
    if options.session_id.as_deref().is_some_and(|id| {
        id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    }) {
        return Err("Identificador de conversa AGY inválido.".into());
    }
    if options.mcp_url.is_empty() != options.mcp_token.is_empty() {
        return Err("A ponte AGY exige endereço e token juntos.".into());
    }
    if !options.mcp_url.is_empty() {
        let url = url::Url::parse(&options.mcp_url)
            .map_err(|_| "Endereço da ponte AGY inválido.".to_owned())?;
        if url.scheme() != "http"
            || !matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || options.mcp_token.len() > 4096
            || !options
                .mcp_token
                .bytes()
                .all(|byte| byte.is_ascii_graphic())
        {
            return Err(
                "A ponte AGY precisa ser local e usar um token válido desta execução.".into(),
            );
        }
    }
    let mut directory = options.workspace_dir.clone();
    for component in [".agents", "agents", AGENT_NAME] {
        directory.push(component);
        private_directory(&directory)?;
    }
    let agent_file = directory.join("agent.md");
    let mut file = tempfile::NamedTempFile::new_in(&directory)
        .map_err(|_| "Não foi possível criar o agente AGY.".to_owned())?;
    file.write_all(agent_document(options)?.as_bytes())
        .map_err(|_| "Não foi possível salvar o agente AGY.".to_owned())?;
    file.flush()
        .map_err(|_| "Não foi possível confirmar a configuração AGY.".to_owned())?;
    file.persist(&agent_file)
        .map_err(|_| "Não foi possível atualizar o agente AGY.".to_owned())?;
    let (model, effort) = super::model_selection(&options.model, options.effort.as_deref());
    let mut command = crate::background::tokio_command(executable);
    command
        .current_dir(&options.workspace_dir)
        .args([
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--agent",
            AGENT_NAME,
            "--disable-slash-commands",
            "--dangerously-skip-permissions",
        ])
        .arg(format!("--model={model}"))
        .arg("--add-dir")
        .arg(&options.cwd);
    if let Some(id) = &options.session_id {
        command.arg(format!("--conversation={id}"));
    }
    if let Some(effort) = effort {
        command.arg(format!("--effort={effort}"));
    }
    // Keep HOME and the vendor keyring untouched. Only the project/tool configuration is isolated.
    Ok((command, agent_file))
}

impl AgyProcess {
    pub(crate) fn spawn(options: RunOptions) -> Result<Self, String> {
        let executable = super::metadata::executable().ok_or("Antigravity CLI não encontrado. Instale o CLI oficial e execute agy para entrar na sua conta.")?;
        let (command, agent_file) = command_for(&executable, &options)?;
        Self::spawn_command(command, agent_file)
    }

    pub(super) fn spawn_command(mut command: Command, agent_file: PathBuf) -> Result<Self, String> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut wrapped = CommandWrap::from(command);
        #[cfg(unix)]
        wrapped.wrap(process_wrap::tokio::ProcessGroup::leader());
        #[cfg(windows)]
        crate::background::windows_job(&mut wrapped);
        wrapped.wrap(KillOnDrop);
        let mut child = wrapped.spawn().map_err(|_| {
            let _ = std::fs::remove_file(&agent_file);
            "Não foi possível iniciar o Antigravity CLI.".to_owned()
        })?;
        let stdin = child.stdin().take().ok_or("Entrada AGY indisponível.")?;
        let stdout = child.stdout().take().ok_or("Saída AGY indisponível.")?;
        let stderr = child
            .stderr()
            .take()
            .ok_or("Diagnóstico AGY indisponível.")?;
        let (send, events) = mpsc::channel(64);
        let reader_events = send.clone();
        let reader = tokio::spawn(async move {
            let error = read_events(&mut BufReader::new(stdout), &reader_events)
                .await
                .err()
                .unwrap_or_else(|| "AGY encerrou a saída sem confirmar a continuidade da sessão. O histórico foi preservado.".into());
            // A descendant may retain stderr after stdout closes. Do not wait
            // for that pipe before notifying the executor of native termination.
            let _ = reader_events.send(Err(error)).await;
        });
        let stderr_reader = tokio::spawn(async move {
            // Drain native diagnostics without persisting secrets or user content.
            drain_diagnostics(&mut BufReader::new(stderr), &send).await;
        });
        Ok(Self {
            stdin: Some(stdin),
            events,
            child: Some(child),
            reader,
            stderr_reader,
            agent_file,
        })
    }

    pub(crate) async fn send_user(&mut self, prompt: &str) -> Result<(), String> {
        let mut encoded =
            serde_json::to_vec(&json!({"event":"user","message":{"role":"user","content":prompt}}))
                .map_err(|_| "Mensagem AGY inválida.")?;
        if encoded.len() > MAX_FRAME_BYTES {
            return Err("Mensagem para AGY excede o limite do transporte.".into());
        }
        encoded.push(b'\n');
        let stdin = self.stdin.as_mut().ok_or("A sessão AGY foi encerrada.")?;
        tokio::time::timeout(Duration::from_secs(10), async {
            stdin
                .write_all(&encoded)
                .await
                .map_err(|_| "Não foi possível enviar a mensagem AGY.".to_owned())?;
            stdin
                .flush()
                .await
                .map_err(|_| "Não foi possível confirmar a mensagem AGY.".to_owned())
        })
        .await
        .map_err(|_| "AGY não recebeu a mensagem; retome a sessão antes de continuar.".to_owned())?
    }

    pub(crate) async fn next_event(&mut self) -> Result<Option<Value>, String> {
        self.events.recv().await.transpose()
    }

    pub(crate) async fn cancel(&mut self) -> Result<(), String> {
        if let Some(child) = self.child.as_mut() {
            // A native failure may already have exited its process group. Reaping
            // the child below confirms shutdown without turning that race into an error.
            let _ = child.start_kill();
        }
        self.stdin.take();
        self.reader.abort();
        self.stderr_reader.abort();
        let _ = std::fs::remove_file(&self.agent_file);
        if let Some(mut child) = self.child.take() {
            tokio::time::timeout(Duration::from_secs(3), child.wait())
                .await
                .map_err(|_| "O processo AGY não confirmou o encerramento.".to_owned())?
                .map_err(|_| "Não foi possível confirmar o encerramento AGY.".to_owned())?;
        }
        Ok(())
    }
}

async fn drain_diagnostics<R: AsyncRead + Unpin>(
    reader: &mut R,
    events: &mpsc::Sender<Result<Value, String>>,
) {
    let marker = b"not found, falling back to default";
    let mut buffer = [0; 4096];
    let mut tail = Vec::with_capacity(buffer.len() + marker.len());
    let mut reported = false;
    while let Ok(count) = reader.read(&mut buffer).await {
        if count == 0 {
            break;
        }
        if reported {
            continue;
        }
        tail.extend_from_slice(&buffer[..count]);
        if tail.windows(marker.len()).any(|window| window == marker) {
            let _ = events.send(Err("AGY não carregou o agente do Jarvis. Atualize o CLI antes de tentar novamente.".into())).await;
            reported = true;
            tail.clear();
        } else if tail.len() >= marker.len() {
            tail.drain(..tail.len() - (marker.len() - 1));
        }
    }
}

impl Drop for AgyProcess {
    fn drop(&mut self) {
        self.reader.abort();
        self.stderr_reader.abort();
        let _ = std::fs::remove_file(&self.agent_file);
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
                });
            }
        }
    }
}

async fn read_events<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    events: &mpsc::Sender<Result<Value, String>>,
) -> Result<(), String> {
    let mut approved_steps = std::collections::HashSet::new();
    let mut bookkeeping_steps = std::collections::HashSet::new();
    while let Some(line) = read_frame(reader).await? {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let event: Value = serde_json::from_slice(&line)
            .map_err(|_| "AGY retornou uma mensagem estruturada inválida.".to_owned())?;
        let kind = event["event"]
            .as_str()
            .ok_or("AGY retornou um evento sem tipo.")?;
        if kind == "init" && event["init"]["agent"].as_str() != Some(AGENT_NAME) {
            return Err(
                "AGY não carregou o agente do Jarvis. Atualize o CLI antes de tentar novamente."
                    .into(),
            );
        }
        if kind == "step_update" && event["step_update"]["step_type"] == "tool" {
            let step = &event["step_update"];
            let name_field = step
                .get("tool_name")
                .or_else(|| step["tool_info"].get("name"));
            let name = name_field.and_then(Value::as_str);
            let server_field = step["tool_info"]["parameters"].get("ServerName");
            let server = server_field.and_then(Value::as_str);
            let index = step["step_index"].as_u64();
            let approved = index.is_some_and(|index| approved_steps.contains(&index));
            let bookkeeping_observed =
                index.is_some_and(|index| bookkeeping_steps.contains(&index));
            let action_field = step["tool_info"]["parameters"].get("Action");
            // AGY injects its task-list helper even when default components are
            // excluded. Listing native tasks has no project/tool effect. Other
            // manage_task actions can kill processes or steer subagents and
            // still belong to the Jarvis boundary.
            let bookkeeping = server_field.is_none()
                && (name == Some("manage_task") || (name_field.is_none() && bookkeeping_observed))
                && (action_field.is_some_and(|action| action == "list")
                    || (action_field.is_none() && bookkeeping_observed));
            // ACTIVE may identify the MCP dispatcher before its parameters arrive.
            // This is a preview, never a confirmed dispatch or approved sparse step.
            let provisional = (name == Some("call_mcp_tool")
                || (name == Some("manage_task") && action_field.is_none()))
                && server_field.is_none()
                && step["state"] == "ACTIVE";
            if !bookkeeping
                && !provisional
                && (name_field.is_some_and(|name| name != "call_mcp_tool")
                    || server_field.is_some_and(|server| server != "jarvis")
                    || (!approved && (name != Some("call_mcp_tool") || server != Some("jarvis"))))
            {
                return Err(format!("AGY tentou usar uma ferramenta fora do Jarvis (tipo: {}; servidor: {}; ferramenta MCP: {}; etapa: {}; estado: {}). A sessão foi interrompida com o histórico preservado.", diagnostic_label(name), diagnostic_label(server), diagnostic_label(step["tool_info"]["parameters"]["ToolName"].as_str()), index.map_or_else(|| "ausente".into(), |index| index.to_string()), diagnostic_label(step["state"].as_str())));
            }
            if name == Some("call_mcp_tool") && server == Some("jarvis") {
                if let Some(index) = index {
                    approved_steps.insert(index);
                }
            }
            if bookkeeping {
                if let Some(index) = index {
                    bookkeeping_steps.insert(index);
                }
            }
        }
        events
            .send(Ok(event))
            .await
            .map_err(|_| "A sessão AGY foi encerrada.")?;
    }
    Ok(())
}

async fn read_frame<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Option<Vec<u8>>, String> {
    let mut frame = Vec::new();
    loop {
        let available = reader
            .fill_buf()
            .await
            .map_err(|_| "Não foi possível ler a resposta AGY.".to_owned())?;
        if available.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Err("AGY encerrou uma mensagem antes de terminá-la.".into())
            };
        }
        let count = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        if frame.len().saturating_add(count) > MAX_FRAME_BYTES {
            return Err("AGY retornou uma mensagem excessiva.".into());
        }
        let complete = available[count - 1] == b'\n';
        frame.extend_from_slice(&available[..count]);
        reader.consume(count);
        if complete {
            return Ok(Some(frame));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stream_rejects_partial_oversized_and_wrong_agent_frames() {
        assert!(
            read_frame(&mut std::io::Cursor::new(b"{\"event\":\"init\"}"))
                .await
                .is_err()
        );
        assert!(
            read_frame(&mut std::io::Cursor::new(vec![b'x'; MAX_FRAME_BYTES + 1]))
                .await
                .is_err()
        );
        let (events, _) = mpsc::channel(4);
        let mut reader =
            std::io::Cursor::new(b"{\"event\":\"init\",\"init\":{\"agent\":\"default\"}}\n");
        assert!(read_events(&mut reader, &events).await.is_err());
        let mut reader = std::io::Cursor::new(b"{\"event\":\"init\",\"init\":{}}\n");
        assert!(read_events(&mut reader, &events).await.is_err());
    }

    #[tokio::test]
    async fn native_tools_and_foreign_mcp_servers_are_rejected_without_losing_previous_events() {
        for step in [
            json!({"step_index":1,"step_type":"tool","tool_name":"run_command","tool_info":{"parameters":{"command":"private secret"}}}),
            json!({"step_index":1,"step_type":"tool","tool_name":"call_mcp_tool","tool_info":{"parameters":{"ServerName":"other"}}}),
            json!({"step_index":1,"step_type":"tool","tool_name":"call_mcp_tool","tool_info":{"parameters":{"ServerName":null}}}),
            json!({"step_index":1,"step_type":"tool","tool_name":"call_mcp_tool","state":"DONE"}),
            json!({"step_index":1,"step_type":"tool"}),
        ] {
            let previous = json!({"event":"step_update","step_update":{"step_index":0,"step_type":"agent_response","text_delta":"Working"}});
            let rejected = json!({"event":"step_update","step_update":step});
            let input = format!("{previous}\n{rejected}\n");
            let (events, mut received) = mpsc::channel(4);
            let error = read_events(&mut std::io::Cursor::new(input.as_bytes()), &events)
                .await
                .unwrap_err();
            assert!(!error.contains("private secret"));
            assert_eq!(received.try_recv().unwrap().unwrap(), previous);
            assert!(received.try_recv().is_err());
        }
    }

    #[tokio::test]
    async fn jarvis_mcp_steps_accept_sparse_completion_updates_after_verified_dispatch() {
        let preview = json!({"event":"step_update","step_update":{"step_index":1,"step_type":"tool","tool_name":"call_mcp_tool","state":"ACTIVE"}});
        let active = json!({"event":"step_update","step_update":{"step_index":1,"step_type":"tool","tool_name":"call_mcp_tool","tool_info":{"parameters":{"ServerName":"jarvis"}}}});
        let done = json!({"event":"step_update","step_update":{"step_index":1,"step_type":"tool","state":"DONE"}});
        let input = format!("{preview}\n{active}\n{done}\n");
        let (events, mut received) = mpsc::channel(4);
        read_events(&mut std::io::Cursor::new(input.as_bytes()), &events)
            .await
            .unwrap();
        assert_eq!(received.try_recv().unwrap().unwrap(), preview);
        assert_eq!(received.try_recv().unwrap().unwrap(), active);
        assert_eq!(received.try_recv().unwrap().unwrap(), done);
    }

    #[tokio::test]
    async fn provisional_mcp_preview_does_not_approve_a_parameterless_dispatch() {
        let preview = json!({"event":"step_update","step_update":{"step_index":1,"step_type":"tool","tool_name":"call_mcp_tool","state":"ACTIVE"}});
        let missing = json!({"event":"step_update","step_update":{"step_index":1,"step_type":"tool","state":"DONE"}});
        let input = format!("{preview}\n{missing}\n");
        let (events, mut received) = mpsc::channel(4);
        assert!(
            read_events(&mut std::io::Cursor::new(input.as_bytes()), &events)
                .await
                .is_err()
        );
        assert_eq!(received.try_recv().unwrap().unwrap(), preview);
        assert!(received.try_recv().is_err());
    }

    #[tokio::test]
    async fn native_task_listing_keeps_the_jarvis_mcp_session_running() {
        let preview = json!({"event":"step_update","step_update":{"step_index":15,"step_type":"tool","tool_name":"manage_task","state":"ACTIVE"}});
        let listing = json!({"event":"step_update","step_update":{"step_index":15,"step_type":"tool","tool_name":"manage_task","state":"ACTIVE","tool_info":{"parameters":{"Action":"list","toolAction":"Listing background tasks","toolSummary":"Task listing"}}}});
        let done = json!({"event":"step_update","step_update":{"step_index":15,"step_type":"tool","state":"DONE"}});
        let mcp = json!({"event":"step_update","step_update":{"step_index":17,"step_type":"tool","tool_name":"call_mcp_tool","tool_info":{"parameters":{"ServerName":"jarvis","ToolName":"bash_wait"}}}});
        let input = format!("{preview}\n{listing}\n{done}\n{mcp}\n");
        let (events, mut received) = mpsc::channel(4);
        read_events(&mut std::io::Cursor::new(input.as_bytes()), &events)
            .await
            .unwrap();
        for expected in [preview, listing, done, mcp] {
            assert_eq!(received.try_recv().unwrap().unwrap(), expected);
        }
        assert!(received.try_recv().is_err());
    }

    #[tokio::test]
    async fn native_task_listing_never_authorizes_process_or_subagent_mutations() {
        let listing = json!({"event":"step_update","step_update":{"step_index":15,"step_type":"tool","tool_name":"manage_task","state":"ACTIVE","tool_info":{"parameters":{"Action":"list"}}}});
        for params in [
            json!({"Action":"kill"}),
            json!({"Action":"send_input"}),
            json!({"Action":"create"}),
            json!({"Action":7}),
            json!({"Action":"list","ServerName":"foreign"}),
        ] {
            let mutation = json!({"event":"step_update","step_update":{"step_index":15,"step_type":"tool","tool_name":"manage_task","state":"ACTIVE","tool_info":{"parameters":params}}});
            let input = format!("{listing}\n{mutation}\n");
            let (events, mut received) = mpsc::channel(3);
            assert!(
                read_events(&mut std::io::Cursor::new(input.as_bytes()), &events)
                    .await
                    .is_err()
            );
            assert_eq!(received.try_recv().unwrap().unwrap(), listing);
            assert!(received.try_recv().is_err());
        }
        let unverified = json!({"event":"step_update","step_update":{"step_index":15,"step_type":"tool","tool_name":"manage_task","state":"DONE"}});
        let (events, _) = mpsc::channel(1);
        assert!(read_events(
            &mut std::io::Cursor::new(format!("{unverified}\n").as_bytes()),
            &events
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn diagnostics_keep_draining_long_lines_and_detect_split_fallback_once() {
        let mut diagnostic = vec![b'x'; MAX_FRAME_BYTES + 4090];
        diagnostic.extend_from_slice(b"not found, falling back to default");
        diagnostic.extend_from_slice(&vec![b'x'; 9000]);
        diagnostic.extend_from_slice(b"not found, falling back to default");
        let mut reader = std::io::Cursor::new(diagnostic);
        let (events, mut received) = mpsc::channel(4);
        drain_diagnostics(&mut reader, &events).await;
        assert_eq!(reader.position() as usize, reader.get_ref().len());
        assert!(received.try_recv().unwrap().is_err());
        assert!(received.try_recv().is_err());
    }

    #[tokio::test]
    async fn failed_launch_removes_the_private_bridge_configuration() {
        let root = tempfile::tempdir().unwrap();
        let agent_file = root.path().join("agent.md");
        std::fs::write(&agent_file, "private token").unwrap();
        let command = crate::background::tokio_command(root.path().join("missing-agy"));
        assert!(AgyProcess::spawn_command(command, agent_file.clone()).is_err());
        assert!(!agent_file.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn process_preserves_user_text_and_cancels_its_private_bridge_configuration() {
        let root = tempfile::tempdir().unwrap();
        let agent_file = root.path().join("agent.md");
        std::fs::write(&agent_file, "private token").unwrap();
        let mut command = crate::background::tokio_command("/bin/sh");
        command.args([
            "-c",
            "IFS= read -r line; printf '%s\\n' \"$line\"; cat >/dev/null",
        ]);
        let mut process = AgyProcess::spawn_command(command, agent_file.clone()).unwrap();
        process
            .send_user("literal \"quote\"\nnext line")
            .await
            .unwrap();
        let event = tokio::time::timeout(Duration::from_secs(2), process.next_event())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(event["event"], "user");
        assert_eq!(event["message"]["content"], "literal \"quote\"\nnext line");
        process.cancel().await.unwrap();
        assert!(!agent_file.exists());
        assert!(process.send_user("late message").await.is_err());
        process.cancel().await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stdout_termination_is_reported_even_while_stderr_remains_open() {
        let root = tempfile::tempdir().unwrap();
        let agent_file = root.path().join("agent.md");
        std::fs::write(&agent_file, "private token").unwrap();
        let mut command = crate::background::tokio_command("/bin/sh");
        command.args(["-c", "exec 1>&-; sleep 10"]);
        let mut process = AgyProcess::spawn_command(command, agent_file.clone()).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(2), process.next_event())
            .await
            .expect("EOF must not depend on the native stderr pipe closing");
        assert!(result.unwrap_err().contains("encerrou a saída"));
        process.cancel().await.unwrap();
        assert!(!agent_file.exists());
    }
}
