use super::*;

fn definition(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({"type":"function","name":name,"description":description,"strict":false,"parameters":{"type":"object","properties":properties,"required":required,"additionalProperties":false}})
}
pub(super) fn definitions(flow: Flow, role: Role) -> Vec<Value> {
    let string = json!({"type":"string","minLength":1,"maxLength":16000});
    let strings = json!({"type":"array","maxItems":16,"items":string});
    let mut tools = vec![
        definition("hub_list", "Read compact execution checkpoints, including previous interrupted agents. Beads remains the source of task state. Do not poll; use hub_wait.", json!({}), &[]),
        definition("hub_wait", "Suspend until a child delivers a message or finishes. No polling or timeout loop is needed; cancellation stops waiting. An empty result means there are no active children.", json!({}), &[]),
        definition("hub_send", "Deliver focused evidence or instructions to your parent or an active child. Does not change permissions or wake a completed agent; use hub_retry for a follow-up round.", json!({"to":string,"message":string}), &["to","message"]),
        definition("hub_complete", "Deliver your final structured handoff to the parent and end this agent. Do not use until child work has settled. Reviewer uses approved/rework/blocked; other roles use completed/blocked. Cite actual evidence and validation, and list limitations honestly. taskIds contains exact Beads IDs actually addressed or reviewed (including the epic when reviewed); only approved IDs can be closed in Complete. Use [] for research without a task.", json!({"verdict":{"type":"string","enum":["completed","approved","rework","blocked"]},"summary":string,"outcomes":strings,"evidence":strings,"validation":strings,"limitations":strings,"taskIds":strings}), &["verdict","summary","outcomes","evidence","validation","limitations","taskIds"]),
    ];
    if role.coordinator() {
        let spawn_roles: Vec<_> = [
            Role::Planner,
            Role::Investigator,
            Role::Writer,
            Role::Orchestrator,
            Role::Designer,
            Role::Builder,
            Role::Reviewer,
        ]
        .into_iter()
        .filter(|target| role.spawns(flow, *target))
        .map(|target| serde_json::to_value(target).expect("built-in role serializes"))
        .collect();
        tools.push(definition("hub_respond_guidance", "Answer a pending child's guidance request using its exact requestId. Resolve from known context or ask_user first; do not invent a user decision. Only its parent can respond.", json!({"requestId":string,"answer":string}), &["requestId","answer"]));
        tools.extend([
            definition("hub_cancel", "Cancel a direct child and its descendants. Wait for completion before replacing its work; cancellation is not successful completion.", json!({"id":string}), &["id"]),
            definition("hub_spawn", "Start an isolated permitted agent and return its ID immediately. Supply focused context and acceptance criteria. Dependencies are earlier agent IDs and form a DAG. Implementation roles require a real Beads ID. Designer owns and implements assigned frontend/design work; use Investigator for read-only discovery. Scope is project-relative paths; overlapping writers queue. Use '.' for whole-project shell/MCP access; narrow writers can only read/write their assigned paths and run workflow_check. Up to four independent leaf agents run in parallel.", json!({"role":{"type":"string","enum":spawn_roles},"phase":{"type":"string","enum":["implementation"]},"title":{"type":"string","minLength":1,"maxLength":120},"prompt":string,"acceptance":strings,"scope":strings,"beadId":{"type":["string","null"]},"dependencies":strings}), &["role","title","prompt","acceptance","scope","dependencies"]),
            definition("hub_retry", "Continue an existing direct child from its durable context, after inspecting the task/files and uncertain side effects. Use for recovery or focused rework/follow-up. No automatic replay; at most two additional rounds. Role, scope, dependencies and permissions stay fixed.", json!({"id":string,"prompt":string}), &["id","prompt"]),
        ]);
    }
    if role == Role::Designer || role.coordinator() {
        tools.push(definition("hub_request_guidance", "Ask your parent for a material missing decision and wait for its correlated response. Include context, impact and recommendation. Delegated Designer must use this instead of questioning the user. Coordinators may escalate to their parent. Cancellation interrupts the wait; the main agent uses ask_user.", json!({"question":string}), &["question"]));
    }
    if role == Role::Designer {
        tools.push(definition("design_brief", "Read or replace your durable design brief. Record accepted answers, direction, assumptions, constraints, selected resource IDs and pending decisions. This survives compaction/restart and is isolated per agent. Omit text to read.", json!({"text":{"type":"string","maxLength":4000}}), &[]));
    }
    if matches!(role, Role::Builder | Role::Designer | Role::Reviewer) {
        tools.push(definition("workflow_check", "Run one supported project validation command, serialized with other checks. The command must exist in this project. Reports actual output, never implies user acceptance. Use path for a nested package.", json!({"check":{"type":"string","enum":["bun_lint","bun_typecheck","bun_test","bun_build","bun_check","cargo_check","cargo_test","cargo_clippy"]},"path":{"type":"string"}}), &["check","path"]));
    }
    tools
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Dispatch {
    role: Role,
    #[serde(default)]
    phase: Phase,
    title: String,
    prompt: String,
    acceptance: Vec<String>,
    scope: Vec<String>,
    bead_id: Option<String>,
    dependencies: Vec<String>,
}

fn relative(value: &str) -> bool {
    !value.is_empty()
        && value.len() < 4096
        && !Path::new(value).is_absolute()
        && Path::new(value).components().all(|c| {
            matches!(
                c,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
}
pub(super) fn overlap(a: &[String], b: &[String]) -> bool {
    a.iter().any(|left| {
        b.iter().any(|right| {
            left == "."
                || right == "."
                || Path::new(left).starts_with(right)
                || Path::new(right).starts_with(left)
        })
    })
}
pub(super) fn path_allowed(root: &Path, args: &Value, scope: &[String], role: Role) -> bool {
    let Some(path) = args["path"].as_str() else {
        return false;
    };
    let path = Path::new(path)
        .strip_prefix(root)
        .unwrap_or_else(|_| Path::new(path));
    if !relative(&path.to_string_lossy()) {
        return false;
    }
    if role == Role::Writer
        && !(path.parent() == Some(Path::new("docs"))
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("PLAN-") && name.ends_with(".md")))
    {
        return false;
    }
    scope
        .iter()
        .any(|scope| scope == "." || path.starts_with(scope))
}
fn bounded(text: &str) -> bool {
    !text.trim().is_empty() && text.len() <= 16_000
}
fn strings(items: &[String], required: bool) -> bool {
    (!required || !items.is_empty()) && items.len() <= 16 && items.iter().all(|text| bounded(text))
}

fn workflow_command(root: &Path, check: &str) -> Result<&'static str, AgentError> {
    let (command, script) = match check {
        "bun_lint" => ("bun run lint", Some("lint")),
        "bun_typecheck" => ("bun run typecheck", Some("typecheck")),
        "bun_test" => ("bun run test", Some("test")),
        "bun_build" => ("bun run build", Some("build")),
        "bun_check" => ("bun run check", Some("check")),
        "cargo_check" => ("cargo check", None),
        "cargo_test" => ("cargo test", None),
        "cargo_clippy" => ("cargo clippy --all-targets -- -D warnings", None),
        _ => return Err(invalid("Validação indisponível.")),
    };
    if let Some(script) = script {
        let manifest = std::fs::read(root.join("package.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        let scripts = manifest
            .as_ref()
            .and_then(|manifest| manifest["scripts"].as_object());
        if !scripts.is_some_and(|scripts| scripts.contains_key(script)) {
            let available = scripts
                .map(|scripts| scripts.keys().cloned().collect::<Vec<_>>().join(", "))
                .filter(|scripts| !scripts.is_empty())
                .unwrap_or_else(|| "nenhum".into());
            return Err(invalid(&format!(
                "O script '{script}' não existe neste package.json. Scripts disponíveis: {available}. Escolha uma verificação existente ou use bash com o comando real do projeto."
            )));
        }
    }
    Ok(command)
}

pub(super) async fn execute(
    exec: &Execution,
    tool: &ToolCall,
    mut signal: watch::Receiver<bool>,
) -> Result<String, AgentError> {
    if *signal.borrow() {
        return Err(AgentError::cancelled());
    }
    let schema = definitions(exec.flow, exec.role)
        .into_iter()
        .find(|d| d["name"] == tool.name)
        .ok_or_else(|| invalid("Ferramenta de coordenação indisponível."))?;
    if !jsonschema::validator_for(&schema["parameters"])
        .map_err(|_| AgentError::internal())?
        .is_valid(&tool.args)
    {
        return Err(invalid("Argumentos inválidos para a coordenação."));
    }
    match tool.name.as_str() {
        "hub_request_guidance" => {
            guidance::request(exec, tool.args["question"].as_str().unwrap(), signal).await
        }
        "hub_respond_guidance" => guidance::respond(
            exec,
            tool.args["requestId"].as_str().unwrap(),
            tool.args["answer"].as_str().unwrap(),
        ),
        "design_brief" => exec.hub.mutate(|state| {
            if let Some(text) = tool.args["text"].as_str() {
                if text.len() > 16000 {
                    return Err(invalid("Resuma o briefing em até 4 mil caracteres."));
                }
                state.design_briefs.insert(exec.id.clone(), text.into());
            }
            Ok(json!({"brief":state.design_briefs.get(&exec.id)}).to_string())
        }),
        "hub_spawn" => spawn(
            exec,
            serde_json::from_value(tool.args.clone()).map_err(|_| invalid("Despacho inválido."))?,
        ),
        "hub_list" => {
            let state = exec
                .hub
                .manifest
                .lock()
                .map_err(|_| AgentError::internal())?;
            Ok(json!({"agents":state.jobs.values().map(|job| json!({"id":job.id,"parent":job.parent_id,"role":job.role,"status":job.status,"beadId":job.bead_id,"summary":job.handoff.as_ref().map(|h|h.summary.chars().take(300).collect::<String>()),"error":job.error})).collect::<Vec<_>>()}).to_string())
        }
        "hub_wait" => Ok(json!({"messages": exec.wait_for_children(signal).await?}).to_string()),
        "hub_send" => {
            let to = tool.args["to"].as_str().unwrap();
            let message = tool.args["message"].as_str().unwrap();
            exec.hub.mutate(|state| {
                let parent = state.jobs.get(&exec.id).map(|job| job.parent_id.as_str());
                let child = state
                    .jobs
                    .get(to)
                    .is_some_and(|job| job.parent_id == exec.id && job.status.active());
                if parent != Some(to) && !child {
                    return Err(invalid(
                        "Envie mensagens apenas ao responsável ou a um filho ativo.",
                    ));
                }
                if state.messages.len() >= 128 {
                    return Err(invalid("Aguarde a entrega das mensagens pendentes."));
                }
                state.messages.push(Message {
                    from: exec.id.clone(),
                    to: to.into(),
                    text: message.into(),
                });
                Ok(())
            })?;
            Ok(json!({"delivered":true,"to":to}).to_string())
        }
        "hub_retry" => retry(
            exec,
            tool.args["id"].as_str().unwrap(),
            tool.args["prompt"].as_str().unwrap(),
        ),
        "hub_cancel" => {
            let id = tool.args["id"].as_str().unwrap();
            if exec.hub.job(id)?.parent_id != exec.id {
                return Err(invalid("Cancele apenas seus agentes filhos."));
            }
            cancel_tree(&exec.hub, id)?;
            Ok(json!({"id":id,"cancellationRequested":true}).to_string())
        }
        "hub_complete" => complete(
            exec,
            serde_json::from_value(tool.args.clone()).map_err(|_| invalid("Handoff inválido."))?,
        ),
        "workflow_check" => {
            let _lock = tokio::select! { _ = cancelled(&mut signal) => return Err(AgentError::cancelled()), lock = exec.hub.check_lock.write() => lock };
            let path = tool.args["path"].as_str().unwrap();
            if !relative(path) {
                return Err(invalid("Pasta de validação inválida."));
            }
            let root = exec
                .hub
                .root
                .root
                .join(path)
                .canonicalize()
                .map_err(|_| invalid("Pasta de validação não encontrada."))?;
            if !root.starts_with(&exec.hub.root.root) {
                return Err(invalid("A validação precisa ocorrer dentro do projeto."));
            }
            let command = workflow_command(&root, tool.args["check"].as_str().unwrap())?;
            let call = ToolCall {
                name: "bash".into(),
                args: json!({"command":command,"timeoutSeconds":120}),
                ..tool.clone()
            };
            tools::execute(&root, &call, Mode::Build, signal).await
        }
        _ => Err(invalid("Ferramenta de coordenação desconhecida.")),
    }
}

fn spawn(exec: &Execution, input: Dispatch) -> Result<String, AgentError> {
    if !exec.role.spawns(exec.flow, input.role) {
        return Err(invalid(
            "O papel solicitado não pertence às delegações deste agente.",
        ));
    }
    validate_phase(exec.flow, exec.role, input.role, input.phase)?;
    if input.title.trim().is_empty()
        || input.title.len() > 480
        || !bounded(&input.prompt)
        || !strings(&input.acceptance, true)
        || !strings(&input.scope, true)
        || input.scope.iter().any(|p| !relative(p))
        || input.dependencies.len() > 16
    {
        return Err(invalid(
            "Informe objetivo, aceite e escopo válidos para o agente.",
        ));
    }
    if input.prompt.len()
        + input.acceptance.iter().map(String::len).sum::<usize>()
        + input.scope.iter().map(String::len).sum::<usize>()
        > 32_000
    {
        return Err(invalid("O despacho precisa ser mais compacto (até 32 KB)."));
    }
    if matches!(
        input.role,
        Role::Builder | Role::Designer | Role::Reviewer | Role::Orchestrator
    ) && input
        .bead_id
        .as_deref()
        .is_none_or(|id| id.trim().is_empty())
    {
        return Err(invalid(
            "Vincule o trabalho a uma tarefa ou épico real do Beads.",
        ));
    }
    let id = library::new_id()?;
    let mut options = exec
        .hub
        .root
        .data
        .lock()
        .map_err(|_| AgentError::internal())?
        .turns
        .last()
        .ok_or_else(AgentError::internal)?
        .turn
        .options
        .clone();
    let job = exec.hub.mutate(|state| {
        if state.jobs.len() >= MAX_JOBS {
            return Err(invalid(
                "Limite de agentes deste fluxo atingido. Conclua o trabalho em andamento.",
            ));
        }
        let mut parent = exec.id.as_str();
        let mut depth = 0;
        while let Some(job) = state.jobs.get(parent) {
            parent = &job.parent_id;
            depth += 1;
            if depth >= 4 {
                return Err(invalid("Limite de profundidade de delegação atingido."));
            }
        }
        if input.dependencies.iter().any(|id| {
            !state
                .jobs
                .get(id)
                .is_some_and(|job| job.parent_id == exec.id)
        }) {
            return Err(invalid(
                "Dependências devem ser agentes já despachados pelo mesmo responsável.",
            ));
        }
        settings::apply(&mut options, &state.profiles, exec.flow, input.role);
        let job = Job {
            custom_agent: None,
            phase: input.phase,
            id: id.clone(),
            parent_id: exec.id.clone(),
            run_id: state.run_id.clone(),
            role: input.role,
            title: input.title,
            prompt: input.prompt,
            acceptance: input.acceptance,
            scope: input.scope,
            bead_id: input.bead_id,
            dependencies: input.dependencies,
            status: Status::Queued,
            created_at: now(),
            updated_at: now(),
            duration_ms: 0,
            attempts: 1,
            handoff: None,
            error: None,
            options,
        };
        state.jobs.insert(id.clone(), job.clone());
        Ok(job)
    })?;
    launch(exec.hub.clone(), job, None)?;
    Ok(json!({"id":id,"status":"queued"}).to_string())
}
fn validate_phase(flow: Flow, parent: Role, role: Role, phase: Phase) -> Result<(), AgentError> {
    if phase == Phase::Discovery {
        return Err(invalid(
            "Descobertas sem alteração pertencem ao Investigador. O Designer executa o escopo visual atribuído.",
        ));
    }
    if !parent.spawns(flow, role) {
        return Err(invalid(
            "O papel solicitado não pertence às delegações deste agente.",
        ));
    }
    Ok(())
}
fn retry(exec: &Execution, id: &str, prompt: &str) -> Result<String, AgentError> {
    if !bounded(prompt) {
        return Err(invalid("Informe o contexto da retomada."));
    }
    let job = exec.hub.mutate(|state| {
        let job = state.jobs.get_mut(id).ok_or_else(|| invalid("Agente não encontrado."))?;
        if job.parent_id != exec.id || !exec.role.spawns(exec.flow, job.role) || job.status.active() || job.attempts >= 3 { return Err(invalid("Retomada indisponível: confira o responsável, o estado e o limite de duas revisões.")); }
        validate_phase(exec.flow, exec.role, job.role, job.phase)?;
        job.attempts += 1; job.status = Status::Queued; job.handoff = None; job.error = None; job.updated_at = now(); job.duration_ms = 0; job.run_id = state.run_id.clone();
        job.options = state.options.clone();
        settings::apply(&mut job.options, &state.profiles, exec.flow, job.role);
        Ok(job.clone())
    })?;
    launch(exec.hub.clone(), job, Some(format!("Resume from the durable checkpoint. Inspect current Beads and files before repeating any uncertain tool action. Current instruction from your coordinator:\n{prompt}")))?;
    Ok(json!({"id":id,"status":"queued"}).to_string())
}
fn complete(exec: &Execution, handoff: Handoff) -> Result<String, AgentError> {
    if exec.id == "main" {
        return Err(invalid(
            "O agente principal entrega a resposta diretamente ao usuário.",
        ));
    }
    if !bounded(&handoff.summary)
        || !strings(&handoff.outcomes, true)
        || !strings(&handoff.evidence, false)
        || !strings(&handoff.validation, false)
        || !strings(&handoff.limitations, false)
    {
        return Err(invalid(
            "Handoff incompleto: informe resultados, evidências e limitações.",
        ));
    }
    if serde_json::to_vec(&handoff)
        .map_err(|_| AgentError::internal())?
        .len()
        > 32_000
    {
        return Err(invalid("O handoff precisa ser mais compacto (até 32 KB)."));
    }
    if !strings(&handoff.task_ids, false)
        || (handoff.verdict == Verdict::Approved
            && ((exec.flow != Flow::Custom && handoff.task_ids.is_empty())
                || handoff.evidence.is_empty()))
    {
        return Err(invalid(
            "A aprovação deve citar tarefas verificadas e evidências.",
        ));
    }
    if exec.flow != Flow::Custom
        && ((exec.role == Role::Reviewer && handoff.verdict == Verdict::Completed)
            || (exec.role != Role::Reviewer
                && matches!(handoff.verdict, Verdict::Approved | Verdict::Rework)))
    {
        return Err(invalid("Veredito incompatível com o papel do agente."));
    }
    exec.hub.mutate(|state| {
        if state
            .jobs
            .values()
            .any(|job| job.parent_id == exec.id && job.status.active())
        {
            return Err(invalid(
                "Aguarde os agentes filhos antes de entregar o handoff.",
            ));
        }
        state
            .jobs
            .get_mut(&exec.id)
            .ok_or_else(AgentError::internal)?
            .handoff = Some(handoff);
        Ok(())
    })?;
    Ok("Handoff registrado; a conclusão será entregue ao responsável pelo runtime.".into())
}

fn admitted(state: &Manifest, job: &Job) -> Result<bool, AgentError> {
    for id in &job.dependencies {
        let dependency = state
            .jobs
            .get(id)
            .ok_or_else(|| invalid("Checkpoint de dependência indisponível."))?;
        if dependency.status.active() {
            return Ok(false);
        }
        if dependency.status != Status::Completed {
            return Err(invalid("Uma dependência não foi concluída com sucesso."));
        }
    }
    let active: Vec<_> = state
        .jobs
        .values()
        .filter(|other| {
            other.id != job.id
                && matches!(other.status, Status::Running | Status::Waiting)
                && !other.role.coordinator()
        })
        .collect();
    if !job.role.coordinator() && active.len() >= MAX_ACTIVE {
        return Ok(false);
    }
    if active.iter().any(|other| {
        overlap(&job.scope, &other.scope)
            && ((job.writes() && (other.writes() || other.role == Role::Reviewer))
                || (job.role == Role::Reviewer && other.writes()))
    }) {
        return Ok(false);
    }
    Ok(true)
}
async fn await_admission(
    hub: &Hub,
    job: &Job,
    mut signal: watch::Receiver<bool>,
) -> Result<(), AgentError> {
    let mut changed = hub.changed.subscribe();
    loop {
        changed.borrow_and_update();
        if *signal.borrow() || *hub.root_signal.borrow() {
            return Err(AgentError::cancelled());
        }
        let available = {
            let state = hub.manifest.lock().map_err(|_| AgentError::internal())?;
            admitted(&state, job)?
        };
        if available {
            let ready = hub.mutate(|state| {
                if !admitted(state, job)? {
                    return Ok(false);
                }
                state
                    .jobs
                    .get_mut(&job.id)
                    .ok_or_else(AgentError::internal)?
                    .status = Status::Running;
                Ok(true)
            })?;
            if ready {
                return Ok(());
            }
            continue;
        }
        tokio::select! { _ = cancelled(&mut signal) => return Err(AgentError::cancelled()), _ = changed.changed() => {} }
    }
}
async fn check_bead(hub: &Hub, job: &Job, signal: watch::Receiver<bool>) -> Result<(), AgentError> {
    let Some(id) = &job.bead_id else {
        return Ok(());
    };
    let beads =
        crate::core::beads::Beads::new(&hub.env.home, hub.root.project_id()?, &hub.root.id, true)?;
    let output = beads
        .execute(
            "beads_show",
            &json!({"id":id}),
            "workflow-dispatch",
            signal,
            || {
                library::agent_location(&hub.env.state, &hub.env.home, &hub.root.id)
                    .map(|_| ())
                    .map_err(|_| crate::core::error("Projeto indisponível."))
            },
        )
        .await?;
    let value: Value =
        serde_json::from_str(&output).map_err(|_| invalid("Resposta inválida do Beads."))?;
    let task = value
        .as_array()
        .and_then(|rows| rows.first())
        .unwrap_or(&value);
    if task["assignee"].as_str().is_some_and(|assignee| {
        !assignee.is_empty() && assignee != format!("jarvis-{}", hub.root.id)
    }) {
        return Err(invalid(
            "A tarefa pertence a outra conversa. Respeite a atribuição existente.",
        ));
    }
    let state = hub.manifest.lock().map_err(|_| AgentError::internal())?;
    let review_ready = review_dependencies(&state, job);
    validate_bead(&value, &review_ready)
}
fn review_dependencies(state: &Manifest, job: &Job) -> Vec<String> {
    if job.role != Role::Reviewer {
        return vec![];
    }
    job.dependencies
        .iter()
        .filter_map(|id| state.jobs.get(id))
        .filter(|worker| {
            worker.run_id == state.run_id
                && worker.status == Status::Completed
                && worker.phase == Phase::Implementation
                && matches!(worker.role, Role::Builder | Role::Designer)
        })
        .filter_map(|worker| {
            worker
                .bead_id
                .as_ref()
                .filter(|id| {
                    worker.handoff.as_ref().is_some_and(|handoff| {
                        handoff.verdict == Verdict::Completed && handoff.task_ids.contains(id)
                    })
                })
                .cloned()
        })
        .collect()
}
fn validate_bead(value: &Value, review_ready: &[String]) -> Result<(), AgentError> {
    let task = value
        .as_array()
        .and_then(|rows| rows.first())
        .unwrap_or(value);
    if !matches!(task["status"].as_str(), Some("open" | "in_progress")) {
        return Err(invalid(
            "A tarefa do Beads não está disponível para execução.",
        ));
    }
    // A review may inspect an explicitly completed implementation before its
    // Bead closes. Keep the durable edge intact; all other blockers still apply.
    if task["dependencies"].as_array().is_some_and(|deps| {
        deps.iter().any(|dep| {
            dep["dependency_type"] == "blocks"
                && dep["status"] != "closed"
                && !(matches!(dep["status"].as_str(), Some("open" | "in_progress"))
                    && dep["id"]
                        .as_str()
                        .is_some_and(|id| review_ready.iter().any(|ready| ready == id)))
        })
    }) {
        return Err(invalid("A tarefa ainda possui dependências em aberto no Beads. Revisores precisam declarar em dependencies os agentes de implementação com handoff concluído; preserve as dependências do Beads."));
    }
    Ok(())
}

pub(super) fn launch(hub: Arc<Hub>, job: Job, resume: Option<String>) -> Result<(), AgentError> {
    let prepared = storage::worker(&hub, &job, resume);
    let (session, signal) = match prepared {
        Ok(value) => value,
        Err(error) => {
            settle(&hub, &job, &Err(error.clone()), None)?;
            return Err(error);
        }
    };
    hub.live
        .lock()
        .map_err(|_| AgentError::internal())?
        .insert(job.id.clone(), session.clone());
    tauri::async_runtime::spawn(async move {
        let mut root_signal = hub.root_signal.clone();
        let cancel = session
            .data
            .lock()
            .ok()
            .and_then(|data| data.active.as_ref().map(|active| active.cancel.clone()));
        let bridge = tauri::async_runtime::spawn(async move {
            cancelled(&mut root_signal).await;
            if let Some(cancel) = cancel {
                cancel.send_replace(true);
            }
        });
        let task_hub = hub.clone();
        let task_job = job.clone();
        let task_session = session.clone();
        // Supervise panics too: a failed worker must never leave a coordinator
        // waiting forever or retain an active runtime after its task has gone.
        let result = tauri::async_runtime::spawn(async move {
            await_admission(&task_hub, &task_job, signal.clone()).await?;
            check_bead(&task_hub, &task_job, signal.clone()).await?;
            let flow = task_hub
                .manifest
                .lock()
                .map_err(|_| AgentError::internal())?
                .flow;
            let exec = Execution {
                hub: task_hub.clone(),
                id: task_job.id.clone(),
                role: task_job.role,
                flow,
                scope: task_job.scope.clone(),
            };
            super::super::run_turn(
                &task_session,
                &task_hub.env.state,
                &task_hub.env.oauth,
                &task_hub.env.mcp,
                &task_hub.env.home,
                signal,
                Some(exec),
            )
            .await
        })
        .await
        .unwrap_or_else(|_| Err(AgentError::internal()));
        if result.is_err() {
            let _ = cancel_tree(&hub, &job.id);
            await_children_settled(&hub, &job.id).await;
        }
        bridge.abort();
        finish(&session, result.clone());
        let duration_ms = session
            .data
            .lock()
            .ok()
            .and_then(|data| data.turns.last().map(|turn| turn.turn.duration_ms));
        if let Ok(mut live) = hub.live.lock() {
            live.remove(&job.id);
        }
        if let Err(error) = settle(&hub, &job, &result, duration_ms) {
            if let Ok(data) = hub.root.data.lock() {
                if let Some(active) = &data.active {
                    active.cancel.send_replace(true);
                }
            }
            let _ = hub.root.update(false, |data| {
                data.storage_failed = error.code == "session_storage";
            });
        }
        hub.changed.send_modify(|revision| *revision += 1);
        (hub.emit)(&hub.root.id);
    });
    Ok(())
}
async fn await_children_settled(hub: &Hub, id: &str) {
    let mut changed = hub.changed.subscribe();
    while hub.children_active(id).unwrap_or(false) {
        if changed.changed().await.is_err() {
            break;
        }
    }
}
fn cancel_tree(hub: &Hub, id: &str) -> Result<(), AgentError> {
    let mut ids = vec![id.to_owned()];
    {
        let state = hub.manifest.lock().map_err(|_| AgentError::internal())?;
        let mut cursor = 0;
        while cursor < ids.len() {
            let children: Vec<_> = state
                .jobs
                .values()
                .filter(|job| job.parent_id == ids[cursor])
                .map(|job| job.id.clone())
                .collect();
            ids.extend(children);
            cursor += 1;
        }
    }
    let live = hub.live.lock().map_err(|_| AgentError::internal())?;
    for id in ids {
        if let Some(session) = live.get(&id) {
            if let Some(active) = &session
                .data
                .lock()
                .map_err(|_| AgentError::internal())?
                .active
            {
                active.cancel.send_replace(true);
            }
        }
    }
    Ok(())
}
fn settle(
    hub: &Hub,
    original: &Job,
    result: &Result<(), AgentError>,
    duration_ms: Option<u64>,
) -> Result<(), AgentError> {
    hub.mutate(|state| {
        let job = state.jobs.get_mut(&original.id).ok_or_else(AgentError::internal)?;
        job.status = match result {
            Ok(()) if job.handoff.as_ref().is_some_and(|h| matches!(h.verdict, Verdict::Completed | Verdict::Approved)) => Status::Completed,
            Ok(()) => Status::Blocked,
            Err(error) if error.code == "cancelled" => Status::Cancelled,
            Err(_) => Status::Failed,
        };
        job.updated_at = now(); job.duration_ms = duration_ms.unwrap_or_else(|| job.updated_at.saturating_sub(original.updated_at)); job.error = result.as_ref().err().map(|error| error.message.clone());
        let text = json!({"agent":job.id,"role":job.role,"status":job.status,"beadId":job.bead_id,"handoff":job.handoff,"error":job.error}).to_string();
        state.messages.push(Message { from: job.id.clone(), to: job.parent_id.clone(), text });
        Ok(())
    })
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
