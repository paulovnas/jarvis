---
name: criar-agentes-e-fluxos
description: Configura agentes, fluxos personalizados, servidores MCP e instruções AGENTS.md do projeto com revisão e aprovação antes de salvar.
---

# Autoria assistida do Jarvis

Use esta skill quando o usuário pedir um novo agente, um novo fluxo ou a melhoria de uma definição personalizada existente.

1. Entenda o resultado esperado, os limites, as ferramentas necessárias e como o usuário reconhecerá uma boa execução. Use `ask_user` apenas para decisões materiais que não possam ser inferidas do pedido e do projeto.
2. Consulte `jarvis_catalog` com `view: overview`. Para editar, leia também o item exato com `view: agent` ou `view: flow`.
3. Preserve IDs ao editar. Para novos agentes, fluxos e etapas, gere IDs distintos com 32 caracteres hexadecimais.
4. Escreva instruções específicas, verificáveis e proporcionais ao papel. Conceda somente a capacidade e as ferramentas necessárias. Defina o uso como `solo` para o agente principal de um chat, `flow_only` para uso exclusivo em fluxos ou `mixed` para ambos. Um modelo nulo herda o modelo atual do chat.
5. Em fluxos, use somente agentes `mixed` ou `flow_only`, conecte todas as etapas a partir da entrada, mantenha a saída normal sem ciclos e use `onRework` somente para retornos intencionais de correção.
6. Envie uma única proposta coerente com `jarvis_propose_agent` ou `jarvis_propose_flow`, usando a revisão exata recém-lida. O Jarvis abrirá a revisão para o usuário e só salvará após aprovação explícita.
7. Se a proposta for recusada, incorpore a observação do usuário. Não reenvie a mesma proposta sem uma mudança solicitada.

Agentes e fluxos nativos do Jarvis são imutáveis. Nunca tente alterá-los por arquivos, terminal ou outras ferramentas.

## Servidores MCP

Quando o usuário pedir um MCP, consulte `mcpServers` no `jarvis_catalog` para evitar duplicatas e confirme o comando ou endpoint na documentação oficial do servidor. Use `jarvis_propose_mcp` para um único cadastro global. Inclua somente nomes de variáveis e cabeçalhos em `envKeys` e `headerKeys`: o usuário preencherá os valores privadamente no painel. Nunca envie credenciais no comando, argumentos, URL ou resumo. Não edite configurações por arquivos ou shell.

O cadastro sempre requer aprovação explícita, mesmo no modo YOLO. Só após aprovação use a descoberta e `mcp_activate` para conectar e obter as ferramentas nesta conversa. Diferencie cadastro, conexão e autenticação: um cadastro salvo não prova que o servidor está conectado ou autenticado, nem autoriza todas as suas operações. A ferramenta não altera servidores existentes nem oferece OAuth; nesses casos, indique o requisito preciso nas Configurações sem inventar suporte.

## Instruções do projeto: AGENTS.md

Quando o usuário pedir a criação ou atualização das instruções do projeto, consulte `jarvis_catalog` com `view: project_instructions`. Ele retorna o arquivo atual, a revisão exata e a seção editável do Jarvis. Leia os arquivos relevantes para confirmar a arquitetura, as convenções e os comandos de validação; não invente scripts, ferramentas ou política de publicação. Preserve regras do usuário, seções gerenciadas por ferramentas e instruções de subdiretórios. Pergunte somente por uma decisão material que a evidência disponível não resolve.

Prepare uma seção curta, específica e verificável: limites reais do projeto, implementação mínima que respeite validação e segurança, mudanças restritas ao pedido e checks que comprovem o comportamento solicitado. Um teste deve falhar quando o resultado esperado estiver errado; repetir detalhes da implementação não comprova o critério. Distinga código, testes executados, integração, publicação e resultado observado. Não copie diretrizes de terceiros integralmente nem instale um plugin global de comportamento para esse pedido.

Use `jarvis_propose_project_instructions` com `revision`, um resumo em pt-BR e `content` contendo somente a seção proposta, sem marcadores gerenciados. O painel mostra o arquivo antes/depois e exige aprovação explícita mesmo em YOLO. O Jarvis cria AGENTS.md quando ele está ausente, acrescenta sua seção quando já há regras ou atualiza somente sua própria seção. Ele recusa revisões antigas, marcadores ambíguos e alterações que ultrapassem o limite carregado no prompt. Não use arquivos ou shell para contornar essa revisão. Após recusa, respeite a observação; após alteração concorrente ou resultado incerto, releia o estado antes de propor novamente. As instruções salvas valem para os próximos turnos.

Se AGENTS.md estiver ausente e instruções persistentes forem úteis, ofereça a criação uma única vez em um momento natural de configuração ou conclusão. Continue o pedido atual; não abra uma proposta sem solicitação nem repita a oferta em turnos sem relação com essa configuração.
