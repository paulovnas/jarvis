//! Tool visibility and dispatch share the same capability intersection.
use super::*;
use crate::agent::{attachments, browser, image_generation, processes, terminals, tools, vision, web_search};

pub(crate) fn required(name: &str) -> bool {
    matches!(name, "ctx_search" | "ctx_index" | "ctx_stats" | "hub_complete")
}

pub(super) fn validate(denied: &[String]) -> Result<(), AgentError> {
    if denied.len() > 256 || denied.iter().collect::<BTreeSet<_>>().len() != denied.len() || denied.iter().any(|name| required(name) || name.is_empty() || name.len() > 128 || !(name == "mcp_*" || name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'))) {
        return Err(invalid("Permissões inválidas. Os recursos obrigatórios do Core não podem ser desativados."));
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Permission {
    id: String,
    name: String,
    group: String,
    description: String,
    required: bool,
    capabilities: Vec<Capability>,
}

fn description(name: &str) -> &'static str {
    match name {
        "read" => "Ler arquivos do projeto.", "list" => "Listar pastas e arquivos.", "search" => "Pesquisar texto no projeto.",
        "write" => "Criar ou substituir arquivos.", "edit" => "Editar trechos de arquivos.", "bash" => "Executar comandos no shell.",
        "ask_user" => "Solicitar respostas e escolhas visuais ao usuário.", "web_search" => "Pesquisar na web com a conta configurada.",
        "read_attachment" => "Ler documentos anexados à conversa.", "vision" => "Analisar imagens usando Vision.", "generate_image" => "Gerar imagens com a conta configurada.",
        "read_skill" => "Ler instruções de uma skill ativa.", "find_skills" => "Encontrar skills ativas para a tarefa.",
        "ctx_search" => "Recuperar trechos da memória e do conteúdo indexado.", "ctx_index" => "Indexar conteúdo sem carregar tudo no contexto.", "ctx_stats" => "Consultar a redução de conteúdo do Context-mode.",
        "ctx_execute" => "Processar dados e comandos fora do contexto.", "ctx_execute_file" => "Processar um arquivo e retornar somente o resultado.", "ctx_batch_execute" => "Executar e indexar consultas em lote.", "ctx_fetch_and_index" => "Buscar e indexar uma página web.",
        "process_start" => "Iniciar um serviço persistente no projeto.", "process_list" => "Listar serviços da conversa.", "process_output" => "Consultar a saída de um serviço.", "process_check_port" => "Verificar se uma porta está disponível.",
        "terminal_start" => "Abrir um terminal interativo.", "terminal_list" => "Listar os terminais da conversa.", "terminal_output" => "Ler a saída de um terminal.",
        "browser_list" => "Listar abas do navegador.", "browser_open" => "Abrir uma aba no navegador.", "browser_navigate" => "Navegar para uma URL.", "browser_snapshot" => "Consultar os elementos da página.", "browser_screenshot" => "Capturar uma imagem da página.", "browser_click" => "Clicar em um elemento da página.", "browser_fill" => "Preencher um campo da página.", "browser_press" => "Enviar uma tecla para a página.", "browser_scroll" => "Rolar a página.",
        "beads_list" => "Listar tarefas do projeto.", "beads_ready" => "Encontrar tarefas disponíveis.", "beads_show" => "Consultar os detalhes de uma tarefa.", "beads_create" => "Criar tarefas e épicos.", "beads_update" => "Atualizar tarefas e notas.", "beads_claim" => "Assumir uma tarefa.", "beads_close" => "Concluir uma tarefa validada.", "beads_dependency" => "Gerenciar dependências entre tarefas.",
        "design_search" => "Pesquisar recursos do Open Design.", "design_read" => "Consultar templates, sistemas e skills de design.",
        "context7_resolve_library_id" => "Encontrar uma biblioteca no Context7.", "context7_query_docs" => "Consultar documentação e exemplos de bibliotecas.",
        "browser_console" => "Consultar erros e mensagens do console.", "browser_close" => "Fechar uma aba do navegador.",
        "hub_complete" => "Entregar o resultado da etapa ao fluxo.", "workflow_check" => "Executar as verificações suportadas do projeto.",
        "hub_list" => "Consultar agentes e seus estados.", "hub_wait" => "Aguardar o resultado de subagentes.", "hub_send" => "Enviar uma orientação a outro agente.",
        "hub_spawn" => "Iniciar um subagente nos fluxos nativos.", "hub_retry" => "Retomar o trabalho de um subagente.", "hub_cancel" => "Cancelar um subagente e seus descendentes.",
        "hub_request_guidance" => "Solicitar uma decisão ao coordenador.", "hub_respond_guidance" => "Responder à dúvida de um subagente.",
        "validation_publish" => "Publicar itens para a validação do usuário.", "design_brief" => "Registrar as decisões de design da etapa.",
        "mcp_*" => "Permitir ferramentas dos MCPs configurados.",
        _ => "Recurso disponível conforme a configuração e o fluxo da conversa.",
    }
}

fn permission(name: &str, group: &str) -> Permission {
    Permission { id: name.into(), name: name.into(), group: group.into(), description: description(name).into(), required: required(name), capabilities: [Capability::ReadOnly, Capability::WriteFiles, Capability::Commands].into_iter().filter(|c| custom::capability_allows(*c, name)).collect() }
}

fn builtin_permissions() -> Vec<Permission> {
    let mut result = BTreeMap::new();
    let groups = [
        ("Projeto", tools::definitions(Mode::Build)),
        ("Pesquisa e mídia", vec![web_search::definition(), attachments::definition(), vision::definition(), image_generation::definition()]),
        ("Skills", vec![crate::skills::definition(), crate::skills::search_definition()]),
        ("Processos e terminal", [processes::definitions(Mode::Build), terminals::definitions(Mode::Build)].concat()),
        ("Navegador", browser::definitions(Mode::Build)),
        ("Beads · Core", crate::core::beads::definitions(false)),
        ("Open Design · Core", crate::core::design::definitions()),
        ("Context7 · Core", crate::core::context7::definitions()),
        ("Fluxo", [dispatch::definitions(Role::Builder), dispatch::definitions(Role::Planner), dispatch::definitions(Role::Designer), vec![validation::definition()]].concat()),
    ];
    for (group, tools) in groups { for tool in tools { if let Some(name) = tool["name"].as_str() { result.insert(name.to_owned(), permission(name, group)); } } }
    for name in crate::core::context::TOOLS { result.insert(name.into(), permission(name, "Context-mode · Core")); }
    result.insert("mcp_*".into(), permission("mcp_*", "MCPs"));
    result.into_values().collect()
}

#[tauri::command]
pub async fn get_agent_tool_permissions(app: tauri::AppHandle, state: tauri::State<'_, AppState>, mcp: tauri::State<'_, crate::mcp::McpState>) -> Result<Vec<Permission>, AgentError> {
    use tauri::Manager;
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let state = state.inner().clone();
    let mcp = mcp.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut result = builtin_permissions();
        for server in mcp.list(&state, &home).map_err(|cause| invalid(&cause.message))? {
            if let Some(check) = &server.last_check { for name in &check.tools {
                let mut tool = permission(&crate::mcp::runtime::wire_name(&server, name), &format!("MCP · {}", server.name));
                tool.name.clone_from(name);
                tool.description = if server.enabled { "Ferramenta descoberta neste MCP." } else { "O MCP está desativado nas configurações." }.into();
                result.push(tool);
            } }
        }
        Ok(result)
    }).await.map_err(|_| AgentError::internal())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_catalog_tool_has_enforced_capabilities_and_mandatory_core_is_not_configurable() {
        let tools = builtin_permissions();
        for id in ["ask_user", "web_search", "ctx_search", "terminal_start", "browser_screenshot", "beads_show", "hub_complete"] { assert!(tools.iter().any(|p| p.id == id), "{id}"); }
        let mut agent = super::super::tests::example().agents[0].clone();
        agent.capability = Capability::Commands;
        for tool in tools {
            agent.denied_tools = vec![tool.id.clone()];
            assert_eq!(custom::allowed(&agent, &tool.id), tool.required, "{}", tool.id);
            assert_eq!(validate(&agent.denied_tools).is_err(), tool.required);
        }
        agent.denied_tools = vec!["mcp_*".into()];
        assert!(!custom::allowed(&agent, "mcp_future_tool"));
        assert!(validate(&["../tool".into()]).is_err());
    }
}
