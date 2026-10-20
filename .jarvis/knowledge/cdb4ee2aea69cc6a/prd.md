# Jarvis — Documento de requisitos do produto

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Pessoas que trabalham em projetos de software e utilizam agentes de IA para investigar, planejar, implementar e verificar mudanças.

Seus trabalhos principais são:

- Conectar seus próprios provedores de IA e escolher modelos, agentes e fluxos de execução.
- Enviar instruções, fornecer anexos e acompanhar o trabalho dos agentes.
- Revisar resultados, alterações em arquivos e operações apresentadas para aprovação.
- Manter vários projetos e conversas persistentes, retomando o trabalho com continuidade de contexto.

A interface é em português do Brasil. Não há outros públicos ou perfis comerciais definidos no registro de produto.

## Product Purpose

Jarvis é um aplicativo desktop para desenvolver software de forma assistida por inteligência artificial. Reúne conversas, arquivos, terminais, navegador, planejamento e acompanhamento de tarefas em um mesmo ambiente.

Seu propósito é permitir que o usuário trabalhe com seus próprios provedores, escolha os modelos de cada agente e acompanhe o desenvolvimento em vários projetos sem perder a continuidade do trabalho. Os agentes podem investigar projetos, consultar documentação, editar arquivos, executar comandos, navegar pelo código, usar servidores MCP e realizar verificações.

## Positioning

> Seus projetos, modelos e agentes em um só lugar.

O posicionamento comunicado em `README.md` combina organização persistente por projeto e conversa, escolha de modelos por agente e ferramentas de desenvolvimento integradas a um aplicativo desktop. A conexão dos próprios provedores do usuário faz parte desse mecanismo.

## Operating Context

- **Ambiente de uso:** aplicativo desktop com interface web e comportamento de janelas nativas. A classificação `web` não muda o contexto desktop do produto.
- **Organização do trabalho:** espaços de trabalho (*workspaces*), projetos e conversas persistentes.
- **Provedores:** o usuário conecta seus próprios provedores e escolhe os modelos utilizados pelos agentes.
- **Sistemas anunciados:** macOS em Apple Silicon, Windows x64 e Linux x64, conforme `README.md`.

**Suporte a consolidar:** `PRODUCT.md` registra macOS em Apple Silicon e Windows x64; `README.md` também anuncia Linux x64 como disponível. A matriz oficial de suporte precisa conciliar essa diferença. Linux não deve ser apresentado como uma capacidade apenas futura com base no registro anterior.

### Fluxo principal de trabalho

1. **Configurar o acesso à IA:** conectar provedores e selecionar modelos, agentes e fluxos de execução.
2. **Organizar o contexto:** selecionar um workspace, projeto e conversa; manter rascunhos por chat e organizar a ordem dos itens.
3. **Solicitar trabalho:** enviar instruções e anexos, utilizar agentes e planos e administrar mensagens em fila.
4. **Acompanhar a execução:** observar raciocínio em tempo real, chamadas de ferramentas e indicadores de atividade.
5. **Revisar o trabalho:** consultar arquivos, diferenças, terminais, navegador e painéis HTTP; revisar resultados e, quando apresentadas, operações antes da execução.
6. **Retomar e acompanhar:** voltar às conversas persistentes e acompanhar mensagens não lidas, notificações e informações de uso.

## Capabilities and Constraints

### Capacidades atuais documentadas

As capacidades abaixo são registradas como existentes em `PRODUCT.md` e `README.md`. Elas não constituem um roadmap de novas funcionalidades.

| Área | Capacidade |
| --- | --- |
| Organização | Workspaces, projetos e conversas persistentes; reordenação por arrastar e soltar; rascunhos por chat; indicadores de atividade, mensagens não lidas e terminais abertos. |
| Conversação | Raciocínio em tempo real, chamadas de ferramentas agrupadas, anexos e filas de mensagens. |
| Agentes e planejamento | Seleção de modelos e fluxos de execução, agentes, planos e acompanhamento de tarefas. |
| Trabalho sobre o projeto | Investigação, consulta de documentação, edição de arquivos, execução de comandos, navegação pelo código, uso de MCPs e verificações. |
| Ferramentas integradas | Arquivos, diferenças (*diffs*), terminais, navegador e painéis HTTP. |
| Conexões e extensibilidade | Conexões com provedores, skills, servidores MCP, hooks e plugins. |
| Acompanhamento e manutenção | Notificações, informações de uso, backups e atualizações. |
| Companhia opcional | Jarvito, um companheiro flutuante de desktop. |

Na integração de navegador, o trecho de `browser-extension/browser.ts` implementa a restauração de associações entre abas e conversas a partir do estado de sessão, considerando somente abas que ainda existem. Esse comportamento não estabelece, por si só, persistência das abas além da sessão nem garantias gerais de isolamento.

### Regras de operação e preservação

- **Continuidade dos dados:** preservar dados existentes, preferências e autenticação dos provedores.
- **Controle do usuário:** preservar controles de execução, atalhos de teclado e comportamento nativo das janelas.
- **Controles de interface:** manter o uso do sistema de componentes instalado, conforme a restrição já registrada em `PRODUCT.md`.
- **Revisão prévia:** operações sensíveis, validações e publicações podem ser apresentadas ao usuário para revisão antes da execução. Essa possibilidade não equivale a uma regra de aprovação manual para toda ação.
- **Recuperação:** as ferramentas de recuperação exigidas pela execução devem estar disponíveis a todos os papéis de agentes, conforme a diretriz de comportamento registrada em `AGENTS.md`.
- **Limite da reformulação registrada:** o trabalho de reformulação de interface não adiciona um novo provedor nem altera a execução dos agentes. Esse limite pertence à reformulação descrita em `PRODUCT.md`, não a uma proibição permanente de evolução do produto.

**Decisão em aberto — aprovações:** as condições que tornam uma revisão obrigatória não estão especificadas nas referências de produto disponíveis. Não há base para afirmar que todas as edições, execuções de comandos, validações ou publicações exigem confirmação.

As referências de produto não especificam um conjunto separado de capacidades futuras planejadas.

## Brand Commitments

Os compromissos abaixo preservam decisões do usuário registradas em `PRODUCT.md`; não estabelecem uma nova direção visual.

- Manter a identidade Jarvis.
- Preservar a decisão de substituir a aparência industrial anterior por uma interface escura inspirada em aplicativos nativos do macOS.
- Manter os compromissos de navegação persistente, superfícies neutras em tons de carvão, grupos de configurações rebaixados, elevação sutil, painéis arredondados, separadores finos, tipografia discreta, espaçamento compacto e controles mínimos.
- Preservar a escolha, registrada em 12/10/2026, de um indicador de execução discreto no compositor, em estilo macOS.
- Manter a fonte de interface do sistema no macOS, com as fontes locais instaladas como alternativas, e fontes monoespaçadas para metadados técnicos e código.

**Tema claro — escopo não confirmado:** `DESIGN.md` contém definições para aparência clara e escura, enquanto o compromisso explícito de produto em `PRODUCT.md` é com a interface escura. Essas definições não substituem a decisão registrada nem confirmam, por si só, a disponibilidade de alternância de temas.

## Assets

- Preservar o ícone existente do aplicativo e a identidade Jarvis.
- A primeira captura de tela fornecida pelo usuário é a referência visual principal; a terceira é complementar, conforme registrado em `PRODUCT.md`.
- As capturas são referências de design, não ativos destinados à distribuição. Seus caminhos não estão identificados no registro fornecido.
- `README.md` referencia um banner da marca Jarvis.

## Evidence on Hand

| Referência | Informação sustentada |
| --- | --- |
| `PRODUCT.md` | Classificação da plataforma como `web`, público, capacidades existentes, restrições de preservação, Jarvito e decisões explícitas de identidade e reformulação. |
| `README.md` | Propósito, apresentação pública, conexão dos próprios provedores, trabalho em vários projetos, operações dos agentes, possibilidade de revisão prévia e sistemas anunciados. |
| `AGENTS.md` | Diretrizes de autonomia, recuperação, preservação de resultados confirmados e respeito à intenção atual do usuário. |
| `browser-extension/browser.ts` | Trecho de implementação da associação entre abas e conversas e da restauração de vínculos a partir do estado de sessão. |
| `browser-extension/browser.test.ts` | Casos de teste declarados para validação de solicitações internas de contexto do navegador; não são um registro de execução dos testes. |
| `DESIGN.md` | Registro visual existente, incluindo definições claras e escuras. Não estabelece sozinho a disponibilidade funcional de temas. |

## Product Principles

1. **Preservar a continuidade do trabalho.** Dados, preferências, autenticações e organização persistente não devem ser perdidos em mudanças de interface. Esse compromisso já está registrado em `PRODUCT.md`.
2. **Manter escolha e controle visíveis.** O usuário conecta seus próprios provedores, escolhe modelos e agentes e acompanha raciocínio e ferramentas; os controles de execução existentes devem ser preservados.
3. **Avaliar autonomia pelo comportamento.** Volume de ações ou ausência de escrita não demonstram, isoladamente, estagnação. A execução deve preservar resultados confirmados e a intenção atual do usuário; erros recuperáveis devem orientar a próxima ação sem encerrar a conversa, conforme `AGENTS.md`.

## Accessibility & Inclusion

- Manter a interface em português do Brasil.
- Preservar os atalhos de teclado existentes.