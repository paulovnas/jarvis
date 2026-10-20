//! Model choices select declared graph exits; only the runtime schedules steps.
use super::*;

pub(super) fn next_step(
    step: &catalog::Step,
    verdict: &Verdict,
    route: Option<&str>,
) -> Result<Option<String>, AgentError> {
    if matches!(verdict, Verdict::Completed | Verdict::Approved) {
        if step.routes.is_empty() {
            return if route.is_none() {
                Ok(step.next.clone())
            } else {
                Err(invalid(
                    "Esta etapa usa uma saída fixa e não aceita uma rota.",
                ))
            };
        }
        return step
            .routes
            .iter()
            .find(|exit| Some(exit.id.as_str()) == route)
            .map(|exit| exit.target.clone())
            .ok_or_else(|| {
                invalid(&format!(
                    "Escolha exatamente uma rota declarada em hub_complete.route: {}.",
                    step.routes
                        .iter()
                        .map(|exit| exit.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            });
    }
    if route.is_some() {
        return Err(invalid(
            "Uma rota só pode ser escolhida ao concluir ou aprovar uma etapa.",
        ));
    }
    match verdict {
        Verdict::Rework | Verdict::Blocked => step.on_rework.clone().map(Some).ok_or_else(|| {
            invalid("Correção solicitada, mas a etapa não possui uma saída de correção.")
        }),
        _ => Err(invalid("Etapa bloqueada.")),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TraceRequest {
    id: String,
    routes: Vec<String>,
}

pub(in crate::agent) fn trace(
    catalog: &catalog::Catalog,
    args: &Value,
) -> Result<String, AgentError> {
    let request: TraceRequest = serde_json::from_value(args.clone())
        .map_err(|_| invalid("Informe o ID do fluxo e as rotas do teste de percurso."))?;
    if request.routes.len() > 48 {
        return Err(invalid("Use até 48 escolhas de rota por teste."));
    }
    let definition = catalog.resolve(&request.id)?;
    let mut choices = request.routes.iter();
    let mut cursor = Some(definition.flow.entry.clone());
    let mut steps = vec![];
    while let Some(id) = cursor {
        if steps.len() >= usize::from(definition.flow.max_steps) {
            return Err(invalid(
                "O teste de percurso atingiu o limite de execuções do fluxo.",
            ));
        }
        let step = definition
            .flow
            .steps
            .iter()
            .find(|step| step.id == id)
            .ok_or_else(|| invalid("Etapa indisponível."))?;
        let route = if step.routes.is_empty() {
            None
        } else {
            choices.next().map(String::as_str)
        };
        cursor = next_step(step, &Verdict::Completed, route)?;
        let agent = definition
            .agents
            .iter()
            .find(|agent| agent.id == step.agent_id)
            .ok_or_else(|| invalid("Agente indisponível."))?;
        steps.push(
            json!({"stepId":step.id,"agentId":agent.id,"agentName":agent.name,"route":route}),
        );
    }
    if choices.next().is_some() {
        return Err(invalid(
            "Há escolhas de rota após a finalização do percurso.",
        ));
    }
    Ok(json!({"flowId":definition.flow.id,"catalogRevision":catalog.revision,"mode":"routing_only","steps":steps,"terminal":true,"modelsExecuted":false,"toolsExecuted":false,"limitations":["Structural routing only: choices were supplied by the caller, not inferred by a model. Does not validate semantic classification, model availability or specialist tools."]}).to_string())
}
