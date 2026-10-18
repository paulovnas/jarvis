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

Quando o usuário pedir configuração ou reparo de um MCP, consulte `mcpServers` no `jarvis_catalog` para evitar duplicatas e confirme o comando ou endpoint na documentação oficial. Para editar, leia `view: mcp` com o ID existente: essa visualização retorna a configuração pública, a revisão exata e o último teste, sem valores privados. Use `jarvis_propose_mcp` para criar ou atualizar um único cadastro global; atualizações exigem `id` e `revision`. Preserve os demais campos e as chaves mantidas em `envKeys`/`headerKeys`: seus valores existentes são reutilizados nativamente. Somente valores novos ou substituídos são preenchidos privadamente no painel. Um endereço HTTP alterado exige novos valores privados. Nunca envie credenciais no comando, argumentos, URL ou resumo. Não edite configurações do Jarvis por arquivos ou shell.

Criação e edição sempre requerem aprovação explícita, mesmo no modo YOLO. O painel compara o antes/depois; uma alteração concorrente exige reler o catálogo e preparar outra proposta. OAuth existente é preservado quando o endpoint não muda; configurar OAuth novo ou migrar credenciais legadas de argumentos/URLs requer Configurações. MCPs pertencentes a plugins continuam sendo gerenciados pelo plugin.

Para reparar o código de um servidor local, identifique o executável, script ou módulo Python real a partir da configuração e da instalação. Leia os arquivos antes de alterá-los e preserve o código alheio ao pedido. Use as ferramentas de arquivo dentro do projeto. Se a instalação estiver fora dele, utilize `bash` com `sandboxPermissions: require_escalated` e uma justificativa precisa pelo fluxo de aprovação existente; não enfraqueça o escopo das ferramentas de arquivo nem contorne uma recusa. Execute os testes pertinentes do servidor. Não corte ferramentas válidas do servidor para contornar um limite do cliente.

Após corrigir o cadastro ou o código, use `jarvis_test_mcp` com o ID e a revisão atuais, quando disponível. Ele inicia uma conexão/processo novo, verifica inicialização e catálogo completo e encerra essa conexão, sem chamar ferramentas anunciadas. Diferencie testes do código, conexão, autenticação e uma operação real: o teste de catálogo não prova que um efeito de áudio funciona. Use a descoberta e `mcp_activate` para operar o servidor nesta conversa; uma configuração editada invalida conexões da revisão anterior. Não repita operações com resultado incerto. No Jarvito, utilize `jarvito_catalog` para inspecionar/editar o cadastro e encaminhe manutenção de código e testes operacionais para um chat com ferramentas de projeto.

## Instruções do projeto: AGENTS.md

Quando o usuário pedir a criação ou atualização das instruções do projeto, consulte `jarvis_catalog` com `view: project_instructions`. Ele retorna o arquivo atual, a revisão exata e a seção editável do Jarvis. Leia os arquivos relevantes para confirmar a arquitetura, as convenções e os comandos de validação; não invente scripts, ferramentas ou política de publicação. Preserve regras do usuário, seções gerenciadas por ferramentas e instruções de subdiretórios. Pergunte somente por uma decisão material que a evidência disponível não resolve.

Prepare uma seção curta, específica e verificável: limites reais do projeto, implementação mínima que respeite validação e segurança, mudanças restritas ao pedido e checks que comprovem o comportamento solicitado. Um teste deve falhar quando o resultado esperado estiver errado; repetir detalhes da implementação não comprova o critério. Distinga código, testes executados, integração, publicação e resultado observado. Não copie diretrizes de terceiros integralmente nem instale um plugin global de comportamento para esse pedido.

Use `jarvis_propose_project_instructions` com `revision`, um resumo em pt-BR e `content` contendo somente a seção proposta, sem marcadores gerenciados. O painel mostra o arquivo antes/depois e exige aprovação explícita mesmo em YOLO. O Jarvis cria AGENTS.md quando ele está ausente, acrescenta sua seção quando já há regras ou atualiza somente sua própria seção. Ele recusa revisões antigas, marcadores ambíguos e alterações que ultrapassem o limite carregado no prompt. Não use arquivos ou shell para contornar essa revisão. Após recusa, respeite a observação; após alteração concorrente ou resultado incerto, releia o estado antes de propor novamente. As instruções salvas valem para os próximos turnos.

Se AGENTS.md estiver ausente e instruções persistentes forem úteis, ofereça a criação uma única vez em um momento natural de configuração ou conclusão. Continue o pedido atual; não abra uma proposta sem solicitação nem repita a oferta em turnos sem relação com essa configuração.
