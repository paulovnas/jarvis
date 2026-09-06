//! Correlated parent decisions. A leaf never uses the child-completion wait loop.
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Request {
    pub id: String, pub from: String, pub to: String, pub run_id: String,
    pub question: String, pub answer: Option<String>,
}
pub(super) async fn request(exec: &Execution, question: &str, mut signal: watch::Receiver<bool>) -> Result<String, AgentError> {
    if exec.id == "main" { return Err(invalid("O agente principal usa ask_user para esclarecer decisões.")); }
    let id = library::new_id()?;
    let mut changed = exec.hub.changed.subscribe();
    exec.hub.mutate(|state| {
        if state.guidance.len() >= 128 || state.messages.len() >= 128 { return Err(invalid("Limite de orientações pendentes atingido.")); }
        let job = state.jobs.get_mut(&exec.id).ok_or_else(AgentError::internal)?;
        if !job.status.active() { return Err(AgentError::cancelled()); }
        job.status = Status::Waiting;
        let request = Request { id: id.clone(), from: exec.id.clone(), to: job.parent_id.clone(), run_id: state.run_id.clone(), question: question.into(), answer: None };
        state.messages.push(Message { from: exec.id.clone(), to: request.to.clone(), text: format!("Guidance required. Resolve from available context or ask the user, then hub_respond_guidance with this requestId. {}", json!({"requestId":id,"question":question})) });
        state.guidance.insert(id.clone(), request); Ok(())
    })?;
    let result = async {
        loop {
            if *signal.borrow() { return Err(AgentError::cancelled()); }
            {
                let state = exec.hub.manifest.lock().map_err(|_| AgentError::internal())?;
                let request = state.guidance.get(&id).ok_or_else(|| invalid("Pedido de orientação interrompido. Consulte o responsável antes de retomar."))?;
                if let Some(answer) = &request.answer { return Ok(json!({"requestId":id,"answer":answer}).to_string()); }
                let parent_active = if request.to == "main" { state.root_status.active() } else { state.jobs.get(&request.to).is_some_and(|job| job.status.active()) };
                if !parent_active { return Err(invalid("O responsável não está mais executando. Retome a orientação em uma nova rodada.")); }
            }
            tokio::select! { _ = cancelled(&mut signal) => return Err(AgentError::cancelled()), result = changed.changed() => { if result.is_err() { return Err(AgentError::cancelled()); } } }
        }
    }.await;
    exec.hub.mutate(|state| {
        if let Some(job) = state.jobs.get_mut(&exec.id) { if job.status == Status::Waiting { job.status = Status::Running; } }
        if result.is_err() { state.guidance.remove(&id); }
        Ok(())
    })?;
    result
}
pub(super) fn respond(exec: &Execution, id: &str, answer: &str) -> Result<String, AgentError> {
    exec.hub.mutate(|state| {
        let request = state.guidance.get_mut(id).ok_or_else(|| invalid("Pedido de orientação não encontrado."))?;
        if request.to != exec.id || request.run_id != state.run_id || request.answer.is_some()
            || !state.jobs.get(&request.from).is_some_and(|job| job.status.active() && job.run_id == state.run_id) {
            return Err(invalid("Responda apenas ao pedido atual de um agente filho ativo."));
        }
        request.answer = Some(answer.into()); Ok(())
    })?;
    Ok(json!({"requestId":id,"delivered":true}).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::tests::{hub, job};
    fn execution(hub: &Arc<Hub>, id: &str, role: Role) -> Execution {
        Execution { hub: hub.clone(), id: id.into(), role, flow: Flow::Complete, scope: vec![".".into()] }
    }
    #[tokio::test]
    async fn guidance_is_durable_correlated_parent_only_and_resumes_the_designer() {
        let (_fixture, hub) = hub(); let mut child = job(&hub, Role::Designer, "."); child.status = Status::Running;
        hub.mutate(|s| { s.jobs.insert(child.id.clone(), child.clone()); Ok(()) }).unwrap();
        let exec = execution(&hub, &child.id, Role::Designer); let signal = hub.root_signal.clone();
        let waiting = tokio::spawn(async move { request(&exec, "Which brand? Recommended: existing tokens.", signal).await });
        let messages = tokio::time::timeout(Duration::from_secs(2), hub.wait("main", hub.root_signal.clone())).await.unwrap().unwrap();
        assert!(messages[0].text.contains("Which brand?")); assert!(!waiting.is_finished());
        let state = storage::load(&hub.directory, &hub.root.id).unwrap().unwrap();
        let id = state.guidance.keys().next().unwrap();
        assert_eq!(state.jobs[&child.id].status, Status::Interrupted);
        assert!(respond(&execution(&hub, "unrelated", Role::Planner), id, "Wrong answer").is_err());
        assert!(respond(&execution(&hub, "main", Role::Planner), "wrong-id", "Wrong answer").is_err());
        // An ordinary message must not accidentally answer a correlated question.
        hub.mutate(|s| { s.messages.push(Message { from:"main".into(), to:child.id.clone(), text:"Other context".into() }); Ok(()) }).unwrap();
        assert!(!waiting.is_finished());
        respond(&execution(&hub, "main", Role::Planner), id, "Use the existing blue tokens").unwrap();
        let answer = tokio::time::timeout(Duration::from_secs(2), waiting).await.unwrap().unwrap().unwrap();
        assert!(answer.contains("existing blue")); assert_eq!(hub.job(&child.id).unwrap().status, Status::Running);
        assert!(respond(&execution(&hub, "main", Role::Planner), id, "Stale duplicate").is_err());
    }
    #[tokio::test]
    async fn cancelling_a_guidance_request_does_not_leave_an_active_wait() {
        let (_fixture, hub) = hub(); let child = job(&hub, Role::Designer, ".");
        hub.mutate(|s| { s.jobs.insert(child.id.clone(), child.clone()); Ok(()) }).unwrap();
        let exec = execution(&hub, &child.id, Role::Designer); let (cancel, signal) = watch::channel(false);
        let waiting = tokio::spawn(async move { request(&exec, "Missing decision", signal).await });
        tokio::time::timeout(Duration::from_secs(2), hub.wait("main", hub.root_signal.clone())).await.unwrap().unwrap();
        cancel.send_replace(true);
        assert_eq!(tokio::time::timeout(Duration::from_secs(2), waiting).await.unwrap().unwrap().unwrap_err().code, "cancelled");
        assert!(hub.manifest.lock().unwrap().guidance.is_empty());
    }
}
