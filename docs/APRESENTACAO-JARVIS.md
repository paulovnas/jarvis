# Apresentação do Jarvis

Fala, pessoal. Boa tarde!

Vira e mexe eu brincava com a Amanda quando falávamos sobre inteligência artificial. Eu chamava aquele meu “aglomerado” de skills, prompts e automações de **Jarvis**: “vou pedir para o Jarvis”, “vou fazer isso com o Jarvis” e por aí vai.

Hoje eu posso dizer que o Jarvis evoluiu. Ele saiu de uma pequena skill, passou por um fluxo SDD e virou um aplicativo desktop completo, construído em **Rust com Tauri v2**, React e TypeScript.

Eu me inspirei bastante no CLI do Codex, que também é feito em Rust e hoje é minha principal referência para o núcleo da aplicação. Boa parte da eficiência, da execução de ferramentas, da recuperação de falhas e do gerenciamento de contexto veio de conceitos que estudei no Codex. O restante veio de muitos dias, semanas e meses usando diferentes CLIs, provedores e modelos de IA em situações reais.

Meu intuito com o Jarvis não é apenas criar algo que atenda ao meu jeito de trabalhar. Quero também ajudar mais pessoas a entrarem nesse universo sem que o primeiro contato precise ser tão avançado ou dependa de dezenas de ferramentas configuradas separadamente.

A ideia é que ele sirva como um **pontapé inicial** para entender agentes, fluxos, prompts, skills, MCPs e ferramentas e, a partir disso, permitir que cada pessoa crie sua própria forma de trabalhar.

Com uma interface amigável e uma instalação guiada, o Jarvis reúne em um único lugar recursos que normalmente precisariam ser instalados, configurados e mantidos manualmente. Entre eles estão Context-mode, Ponytail, Beads, Open Design, servidores LSP e, opcionalmente, Context7.

De forma resumida, estes são os principais recursos disponíveis hoje:

## Provedores e modelos

- Conexão com a conta do **ChatGPT/OpenAI Codex** diretamente pelo navegador.
- Conexão com a conta Google para uso dos modelos **Gemini por meio do Antigravity**.
- Suporte a provedores customizados usando os três protocolos mais comuns do mercado: **OpenAI Chat Completions, OpenAI Responses e Anthropic Messages**.
- Possibilidade de manter várias contas, definir aliases e escolher provedor, modelo e nível de raciocínio por conversa ou por agente.
- Catálogo automático dos modelos disponíveis nas contas Codex e Antigravity.
- Configuração independente de Web Search, Vision e geração de imagens.
- Visualização dos limites de uso, consumo de tokens, cache e tempo até a renovação das janelas do provedor.
- Alertas configuráveis para avisar quando uma conta atingir determinada porcentagem do limite.

## Agentes e fluxos

- Agentes nativos para construção geral, design/frontend e operações com GitHub.
- Modos diretos para tarefas menores e fluxos Planejado ou Completo para trabalhos que envolvem várias etapas e especialidades.
- Criação de agentes personalizados com nome, instruções, modelo, nível de raciocínio e permissões individuais de ferramentas.
- Agentes podem ser usados sozinhos, somente dentro de fluxos ou das duas formas.
- Editor visual em canvas para montar fluxos com agentes, conexões, dependências e caminhos de correção.
- O próprio Jarvis pode criar ou editar um agente ou fluxo quando o usuário descreve exatamente o que precisa. A IA prepara a configuração e apresenta a proposta para aprovação antes de salvar.
- Os fluxos nativos também podem ser visualizados no canvas, facilitando entender como foram construídos e usar a mesma estrutura como referência.

## Trabalho dentro do projeto

- Organização por workspaces, projetos e conversas persistentes.
- Rascunhos, modelo selecionado, tarefas, terminais e histórico preservados por conversa.
- Explorer de arquivos, visualização de código, diff das alterações e abertura no editor padrão do sistema.
- Busca de arquivos e conteúdo, navegação semântica com LSP, localização de definições, referências, símbolos e diagnósticos.
- Edição de arquivos e patch transacional para mudanças em vários arquivos sem deixar uma alteração aplicada pela metade.
- Terminal integrado e configurável, com suporte a processos persistentes e indicação de qual chat possui terminais abertos.
- Navegador integrado para abrir aplicações locais, navegar, testar interfaces e realizar verificações visuais.
- Anexos, leitura de documentos, Vision e geração de imagens dentro da conversa.
- Perguntas estruturadas para o usuário, fila de mensagens durante a execução e opção para tentar novamente após erros recuperáveis.
- Respostas podem ser copiadas por inteiro ou salvas em Markdown.

## Planejamento, contexto e eficiência

- Tasks visíveis mesmo nos agentes diretos, mostrando o que foi concluído, o que está em andamento e o que ainda falta.
- Planejamento persistente com épicos, tarefas, dependências, bloqueios, comentários e validações por meio do Beads.
- Subagentes especializados com estado, tempo de execução e atividade atual visíveis na interface.
- Context-mode para pesquisar e processar grandes volumes de conteúdo sem consumir a janela do modelo com dados desnecessários.
- Carregamento do histórico sob demanda e compactação automática de conversas extensas.
- Leitura de `AGENTS.md` hierárquicos conforme o agente entra em diferentes áreas de um projeto ou monorepo.
- Reutilização segura de leituras enquanto os arquivos não mudam.
- Catálogo de ferramentas reduzido conforme a intenção, as permissões e o agente selecionado, economizando tokens e diminuindo escolhas erradas.
- Proteções contra repetição de ferramentas, retries controlados e retomada de execuções sem repetir ações cujo resultado já foi confirmado ou ficou incerto.
- Métricas de tempo, ferramentas, tokens e cache para acompanhar o comportamento e o custo das execuções.

## Skills e MCPs

- Marketplace de skills dentro do próprio Jarvis.
- Instalação, atualização, ativação, desativação, visualização e remoção de skills.
- Suporte a skills globais e skills específicas do projeto.
- Conexão com servidores MCP locais ou remotos.
- Teste de conexão, descoberta das ferramentas oferecidas pelo MCP e ativação individual de cada uma delas.
- Permissões por agente para definir exatamente quais skills, MCPs e ferramentas cada especialista poderá utilizar.
- Quando o usuário pede um MCP específico, essa intenção é preservada durante continuações, compactações e passagens para subagentes.

## Git e GitHub

- Suporte tanto a projetos com um único repositório quanto a pastas que agrupam vários repositórios, como frontend, backend e microsserviços.
- Visualização da branch atual, alterações locais e estado de sincronização de cada repositório.
- Agente especializado em GitHub que pode preparar e executar commit, push, pull request e merge.
- Configuração por projeto das instruções de publicação, convenção de commits e template de pull request.
- Revisão da proposta antes da publicação quando o usuário quiser manter aprovação manual.
- Execução autônoma quando o usuário der uma ordem explícita e autorizar o agente a concluir todo o processo.
- Uso do GitHub CLI para criação e merge de pull requests diretamente pelo Jarvis.

## Experiência desktop e gerenciamento

- Onboarding guiado para instalar o Core, conectar provedores e adicionar o primeiro projeto.
- Diagnóstico e reparo dos componentes instalados pelo Jarvis.
- Atualização automática do aplicativo, do Core e das skills, com acompanhamento de progresso.
- Notificações nativas para conclusões, erros, perguntas, validações e limites dos provedores.
- Backup e restauração das preferências, agentes, fluxos, skills e configurações de MCP.
- Credenciais protegidas pelo armazenamento seguro do sistema operacional.
- Controle de permissões e aprovações para ações sensíveis, sem bloquear desnecessariamente aquilo que o usuário já autorizou.
- Personalização de projetos com nome, ícone e cor.
- Tela de detalhes com métricas, atividade, Kanban e opções específicas de cada projeto.

Hoje o Jarvis está disponível para **macOS com Apple Silicon** e **Windows x64**. O projeto é independente e sem fins lucrativos, criado com a intenção de facilitar o desenvolvimento assistido por IA e compartilhar tudo aquilo que fui aprendendo durante esse processo.

Ele continua evoluindo conforme o uso real revela problemas, necessidades e novas ideias. Se alguém quiser testar, sugerir alguma coisa ou contribuir, vou gostar bastante de ouvir o feedback.

- **Repositório:** <https://github.com/paulovnas/jarvis>
- **Downloads:** <https://github.com/paulovnas/jarvis/releases>
- **Sugestões e problemas:** <https://github.com/paulovnas/jarvis/issues>
