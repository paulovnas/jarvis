# Plano de implementação — onboarding e App Shell

## Objective

Entregar o primeiro fluxo nativo persistente do Jarvis e o shell visual Home. No primeiro uso nativo, a aplicação deve criar/abrir a base Rust-owned em `~/.jarvis/jarvis.db`, ler o estado de onboarding sem flash e gravar a conclusão de forma durável. Após a conclusão, Home exibe um shell desktop de três colunas, construído com os primitivos shadcn já instalados, com dados explicitamente mockados e sem comportamento de produto.

## Current / Desired

| Área | Atual (evidência) | Desejado |
|---|---|---|
| Bootstrap/App | `src/App.tsx:48-56` só altera estado efêmero/toast; CTA antigo em `src/App.tsx:155-163` | `App` possui estado discriminado de bootstrap (`loading`, `error`, `onboarding`, `home`), consulta o estado persistido e só renderiza Home após `true` persistido ou conclusão bem-sucedida. |
| Onboarding | Tela existente, CTA/feedback atuais | Preservar a tela; CTA passa a `Finalizar`, fica pendente/desabilitado durante o save, e falha permanece no onboarding com erro recuperável. |
| Título | `src/components/layout/TitleBar.tsx:64-72` fixa `Onboarding` | `TitleBar` recebe label de contexto (`Iniciando`, `Onboarding`, `Início`). |
| Backend | `src-tauri/src/lib.rs:1-13` contém apenas opener/greet | Estado gerenciado Rust, inicialização lazy, migração SQLite e dois comandos Tauri tipados. |
| Banco/dependências | `package.json`/`src-tauri/Cargo.toml` não têm Drizzle/SQLite de domínio | Drizzle schema + migração gerada como fonte de verdade; `rusqlite` bundled para runtime Rust. |
| Tema | `src/index.css:103-135` já define tokens One Dark | Reutilizar tokens, Roboto e pt-BR, sem novo tema. |
| Home | Não existe o shell solicitado | Shell full-height abaixo do TitleBar, com left/center/right redimensionáveis e mock visual. |
| Referência | `docs/metis/desktop/src/App.tsx:699-793`, `.../sidebar/Sidebar.tsx:46-136` e `.../inspector/Inspector.tsx:66-179` mostram geometria e agrupamento | Usar somente geometria/agrupamento como referência; não portar localStorage, código nem integração live. |

## Outcomes e rastreabilidade

- **OUT-001** — No primeiro uso nativo, Jarvis cria/abre `~/.jarvis/jarvis.db`; schema/migração gerada pelo Drizzle cria exatamente uma linha de configuração com estado de onboarding persistido. Requisitos: REQ-001–REQ-005. Implementação: T-001.
- **OUT-002** — Startup lê estado persistido sem flash de onboarding; estado incompleto mostra o onboarding existente com CTA `Finalizar`; escrita durável bem-sucedida vai para Home; lançamentos posteriores ignoram onboarding; falhas nunca simulam conclusão. Requisitos: REQ-006–REQ-009. Implementação: T-001/T-002.
- **OUT-003** — Sidebar esquerda de Home fornece seletor mock de Workspace (`Pessoal` padrão, `Trabalho`), tabs `Projetos`/`Conversas` que comunicam a hierarquia Workspace → Projeto ativo → Conversas, entradas mock e botão inferior `Configurações` cujo único efeito é Sonner `Em breve`. Requisitos: REQ-010–REQ-011. Implementação: T-003.
- **OUT-004** — Centro flexível contém Card shadcn polido com heading exato `Em construção` e nenhum comportamento de produto. Requisito: REQ-012. Implementação: T-003.
- **OUT-005** — Inspector direito mostra visualmente `Arquivos alterados`, `Plano`, `Subagentes` e footer `Contexto`, no agrupamento Metis, sem integrações reais. Requisito: REQ-013. Implementação: T-003.

## Scope

1. Persistência exclusiva do estado `onboarding_completed` em banco SQLite pertencente ao runtime Rust.
2. Contrato IPC mínimo para leitura e conclusão.
3. Gate de bootstrap e estados de loading/erro/pending no App.
4. Ajuste do CTA e do contexto do TitleBar.
5. Shell Home de três painéis, com mocks constantes, acessibilidade básica e redimensionamento desktop.
6. Testes observáveis, documentação operacional mínima e prova nativa descritas em T-004.

## Non-goals

- Não criar contas, projetos, conversas, arquivos, agentes, integrações, queries genéricas ou comportamento de produto.
- Não portar `localStorage` de Metis para Jarvis.
- Não usar router, store global, SQL plugin/proxy/router, JS-native SQLite driver ou nova camada de abstração.
- Não tornar o inspector falsamente interativo: as seções são grupos estáticos, não affordances colapsáveis.
- Não persistir seleção de Workspace, tabs ou mocks.
- Não fazer redesign mobile/responsivo nem collapse da sidebar nesta fatia; preservar o layout desktop.
- Não editar `docs/metis` nem expandir o escopo para além deste plano.

## Decisions / trade-offs

- **D-001 — Caminho nativo exato.** O runtime resolve `~/.jarvis/jarvis.db` em Rust usando o diretório `home_dir` do Tauri. Cria somente `.jarvis`; não há branching por plataforma agora. O frontend nunca recebe o caminho.
- **D-002 — Drizzle como fonte de migração.** Usar `drizzle-orm` e `drizzle-kit` somente em desenvolvimento para `src/db/schema.ts`, `drizzle.config.ts`, SQL/snapshots gerados em `drizzle/` e script Bun `db:generate`. Não adicionar driver SQLite JS, `tauri-plugin-sql` ou `sqlite-proxy`: dois comandos Rust de domínio são menores, mantêm ownership Rust e evitam IPC SQL genérico, mapeamento de valores e complexidade transacional. Adicionar proxy somente quando a amplitude real de queries tipadas do frontend o justificar.
- **D-003 — Runtime Rust direto.** Usar uma única crate SQLite compatível (`rusqlite`, SQLite bundled), atrás de estado gerenciado e serializado. Inicialização lazy no primeiro comando de configuração permite estado visual de retry e é idempotente sob React StrictMode. Comandos Tauri assíncronos devem mover o trabalho SQLite bloqueante para fora da UI.
- **D-004 — Migração ordenada e linha única.** A migração inicial do Drizzle cria `app_config`; Rust aplica SQL gerado atomicamente uma vez usando `PRAGMA user_version` e, de modo idempotente, garante a linha `id=1` sem sobrescrever conclusão. SQL histórico de migração é imutável.
- **D-005 — Gate explícito, sem router/store.** `App` é dono do estado discriminado `loading | error | onboarding | home`; Home só nasce de `true` persistido ou sucesso de conclusão. `Finalizar` fica disabled/pending durante save; falha fica no onboarding e emite Sonner de erro. Erro de bootstrap bloqueia a tela e oferece `Tentar novamente`.
- **D-006 — Composição visual existente.** Compor `Sidebar`, `Select`, `Tabs`, `Resizable`, `ScrollArea`, `Card`, `Separator`, `Badge`, `Button`, `Skeleton` e `Sonner` já instalados; não adicionar componente de registry sem prova de insuficiência. Preservar One Dark/Roboto e pt-BR. Usar geometria/agrupamento Metis, nunca código copiado ou comportamento backend live. `TitleBar` recebe label de contexto.
- **D-007 — Mocks honestos.** Seções do inspector são grupos estáticos, não controles colapsáveis falsos. Workspace/tabs podem ter seleção visual local; todos os mocks são constantes e nunca persistidos.

## Contracts

- **C-001 — Schema.** Tabela `app_config`; `id INTEGER PRIMARY KEY` com check de banco `id = 1`; `onboarding_completed INTEGER NOT NULL DEFAULT 0`, representado no Drizzle com SQLite boolean mode. Não há timestamps nem outras configurações. `id=1` é a única linha.
- **C-002 — IPC.** `get_app_config() -> Result<{ onboardingCompleted: boolean }, serializable error>` e `complete_onboarding() -> Result<{ onboardingCompleted: true }, serializable error>`. Ambos inicializam/acessam a mesma conexão Rust-owned; update parametrizado, idempotente e limitado a `id=1`. Nenhum SQL bruto ou caminho é exposto ao frontend.
- **C-003 — Transições.** Carregamento não resolvido → `loading`; falso → `onboarding`; verdadeiro → `home`; erro de load → retry; sucesso de conclusão → `home`; erro de conclusão → `onboarding` com retry habilitado. Nunca interpretar ausente/erro como falso.
- **C-004 — Shell visual.** Full-height abaixo do TitleBar atual; painéis desktop left/center/right redimensionáveis em proporções derivadas de Metis; asides/controles/foco nomeados e acessíveis; sem collapse da sidebar e sem redesign mobile/responsivo nesta fatia.

## Requirements e mapeamento

| ID | Requisito executável | OUT | Task |
|---|---|---|---|
| REQ-001 | Resolver em Rust o caminho exato `~/.jarvis/jarvis.db`, criar apenas `.jarvis` quando necessário e preservar banco pré-existente. | OUT-001 | T-001 |
| REQ-002 | Adicionar `drizzle-orm`, `drizzle-kit` dev-only, config raiz, `src/db/schema.ts`, diretório de SQL/snapshots gerados e script Bun `db:generate`; Drizzle é a fonte de schema/migração. | OUT-001 | T-001 |
| REQ-003 | Gerar a migração inicial e aplicá-la em ordem, atomicamente e uma única vez por `PRAGMA user_version`; SQL histórico não é editado. | OUT-001 | T-001 |
| REQ-004 | Definir `app_config` segundo C-001, incluindo check de singleton, default `0`, e garantir idempotentemente a linha `id=1`. | OUT-001 | T-001 |
| REQ-005 | Manter conexão e operações no Rust com `rusqlite` bundled, estado serializado e comandos parametrizados tipados conforme C-002; trabalho bloqueante não roda na UI. | OUT-001 | T-001 |
| REQ-006 | Implementar bootstrap explícito com loading visível, sem renderizar onboarding antes da leitura persistida, e retry bloqueante em erro. | OUT-002 | T-002 (com T-001) |
| REQ-007 | Fazer `Finalizar` chamar conclusão durável; desabilitar/pending durante save; sucesso só transita para Home após retorno confirmado; erro mantém onboarding e mostra Sonner de erro. | OUT-002 | T-002 |
| REQ-008 | Em lançamentos posteriores, estado persistido verdadeiro bypassa onboarding; ausente/erro nunca é tratado como falso. | OUT-002 | T-002 (com T-001) |
| REQ-009 | Cobrir no frontend o contrato de estados e chamadas IPC tipadas, sem expor caminho, SQL ou driver ao browser. | OUT-002 | T-002 |
| REQ-010 | Construir sidebar esquerda com Workspace mock (`Pessoal` default, `Trabalho`), tabs `Projetos`/`Conversas`, hierarquia comunicada e entradas constantes mockadas. | OUT-003 | T-003 |
| REQ-011 | Botão inferior `Configurações` não executa configuração: emite somente Sonner com texto exato `Em breve`. Seleção local não é persistida. | OUT-003 | T-003 |
| REQ-012 | Construir centro flexível com Card shadcn, heading exato `Em construção` e nenhum produto/comportamento além do visual. | OUT-004 | T-003 |
| REQ-013 | Construir inspector direito com grupos estáticos `Arquivos alterados`, `Plano`, `Subagentes` e footer `Contexto`, com mocks e sem live integrations. | OUT-005 | T-003 |
| REQ-014 | Atualizar documentação mínima (README sobre path, geração Drizzle e migrações imutáveis), reescrever/remover expectativas obsoletas e executar a validação e smoke nativos definidos em T-004. | OUT-001–OUT-005 | T-004 |

## Tasks, dependências e arquivos prováveis

### T-001 — Fundação de persistência

- Adicionar dependências/locks Bun e Cargo necessárias.
- Criar `src/db/schema.ts`, `drizzle.config.ts`, script `db:generate` e migração/snapshots em `drizzle/`; aceitar o nome exato gerado pela ferramenta.
- Implementar módulo Rust de path, conexão, estado gerenciado, migração e comandos em `src-tauri/src/`; registrar estado/comandos em `src-tauri/src/lib.rs`.
- Garantir aplicação atômica/versionada, singleton e update idempotente sem sobrescrever conclusão.
- Escrever testes Rust para schema/linha/completion conforme T-004.
- **Dependência:** nenhuma outra task para iniciar. **Entrega:** contratos C-001/C-002 prontos para T-002.
- **Arquivos prováveis:** `package.json`, `bun.lock`, `drizzle.config.ts`, `src/db/schema.ts`, `drizzle/*`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/src/lib.rs` e novo módulo Rust de persistência/testes.

### T-002 — Gate frontend e cutover

- Fazer invoke tipado de `get_app_config`/`complete_onboarding`.
- Implementar em `src/App.tsx` a máquina de estados C-003, loading/error/retry, pending e transições sem flash.
- Preservar onboarding existente, alterando CTA para `Finalizar` e seu comportamento; manter erro Sonner e retry.
- Alterar `src/components/layout/TitleBar.tsx` para aceitar contexto e usar `Iniciando`, `Onboarding`, `Início` nos pontos corretos.
- Reescrever/remover asserts antigos de `Começar Configuração`/toast `Configurando`.
- **Dependências:** T-001 (IPC) e contrato Home de T-003 para integração final.
- **Arquivos prováveis:** `src/App.tsx`, `src/components/layout/TitleBar.tsx`, utilitário/tipo de invoke existente se houver, `src/App.test.tsx` e testes diretamente afetados.

### T-003 — Shell Home mockado

- Criar um componente Home coeso, preferencialmente junto à organização já existente, usando os primitivos shadcn disponíveis.
- Montar painéis left/center/right e Resizable; usar constantes mockadas para Workspace, projetos, conversas e seções do inspector.
- Implementar seleção visual local de Workspace/tabs, Card central e Sonner `Em breve` nas configurações.
- Dar nomes acessíveis a asides/controles, preservar One Dark/Roboto/pt-BR e respeitar dimensões desktop.
- Adicionar um teste observável focado nos rótulos do shell e no toast de Configurações.
- **Dependência:** nenhuma para começar; T-001 e T-003 podem avançar independentemente. **Integração:** T-002 consome o Home após bootstrap.
- **Arquivos prováveis:** novo/ajuste de componente em `src/components/`, `src/App.tsx` apenas para composição, `src/index.css` somente se tokens existentes forem insuficientes, e teste focado correspondente.

### T-004 — Documentação e prova final

- Documentar no `README.md` o caminho `~/.jarvis/jarvis.db`, o comando de geração Drizzle e a imutabilidade das migrações; não criar changelog se não existir um.
- Executar testes observáveis, gates e smoke nativo listados abaixo, sem apagar ou sobrescrever dados do usuário.
- Registrar limitações de inspeção visual caso a inspeção nativa não possa ser capturada.
- **Dependências:** T-001, T-002 e T-003 concluídas.
- **Arquivos prováveis:** `README.md` e somente os testes já previstos; não tocar `docs/metis`.

## Risks / mitigations

| Risco | Mitigação obrigatória |
|---|---|
| Duas fontes de verdade (Drizzle versus SQL/runtime) divergirem | Drizzle gera schema/migração; Rust apenas aplica o SQL gerado; manter SQL histórico imutável e revisar `user_version`. |
| SQLite bloqueante congelar UI | Comandos Tauri assíncronos deslocam trabalho bloqueante; estado de loading permanece explícito. |
| React StrictMode disparar carga/conclusão duplicada | Inicialização lazy compartilhada e operações idempotentes; conclusão usa update parametrizado restrito a `id=1`. |
| Erro/missing ser convertido em falso e produzir falsa conclusão | C-003 distingue erro de `false`; só sucesso confirmado permite Home; falha mantém onboarding. |
| Path incorreto ou branching prematuro | Resolver exclusivamente `home_dir/.jarvis/jarvis.db`; testar path exato no ambiente nativo; não expor path ao frontend. |
| Banco existente ser perdido ou conclusão ser sobrescrita | Criar diretório sem apagar; migração versionada/atômica; `INSERT`/garantia idempotente que preserva `true`; smoke não pode remover nem sobrescrever dados pré-existentes. |
| Mocks parecerem integrações reais | Rotular/organizar como visual estático; não adicionar handlers live, persistência, SQL, router ou proxy; inspector não oferece colapso falso. |
| Shell inacessível ou quebrar dimensão desktop | Usar labels/roles/foco acessíveis, componentes existentes, proporções Metis e verificar janela 1360x768 e mínimo configurado. |
| Mudança de contrato deixar testes antigos enganadores | Reescrever/remover asserts de `Começar Configuração` e `Configurando`; testar estados e efeitos observáveis, não implementação interna. |

## Test / Validation

### Testes observáveis duráveis

- **Rust:** em schema novo, migração semeia exatamente uma linha singleton `id=1` com `onboarding_completed=false`; inserir linha extra viola o check; `complete_onboarding` é idempotente e leituras subsequentes retornam `true`; uma conclusão já verdadeira não é revertida.
- **React/App:** bootstrap não resolvido mostra loading, nunca onboarding prematuro; ramos `false` e `true` levam respectivamente a onboarding e Home; `Finalizar` invoca o comando exato e só então exibe Home; rejeição do comando mantém onboarding, reabilita o botão e emite erro; erro inicial exibe retry bloqueante; TitleBar mostra o contexto correto.
- **Home:** labels `Pessoal`, `Trabalho`, `Projetos`, `Conversas`, hierarquia mock, `Em construção`, `Arquivos alterados`, `Plano`, `Subagentes`, `Contexto` são observáveis; `Configurações` emite Sonner `Em breve` e não executa integração.
- Remover ou atualizar asserções obsoletas de `Começar Configuração` e toast `Configurando`; não fixar detalhes internos, cópia de campos, defaults incidentais ou ecos de mocks.

### Gates finais obrigatórios

Executar nesta ordem, somente após todas as tasks:

1. `bun run check`
2. em `src-tauri`, `cargo clippy -- -D warnings`
3. em `src-tauri`, `cargo test`

### Smoke nativo manual

Usar a aplicação Tauri real, sem deletar/sobrescrever banco ou dados já existentes:

1. Quando houver ambiente seguro de primeiro uso, iniciar com path novo e confirmar criação de `~/.jarvis/jarvis.db`, loading sem flash e onboarding incompleto.
2. Clicar `Finalizar`; confirmar escrita durável, transição para Home somente após sucesso e shell left/center/right.
3. Relançar; confirmar bypass do onboarding e entrada direta em Home.
4. Acionar `Configurações`; confirmar Sonner `Em breve` e ausência de efeito de produto.
5. Em janela 1360x768, confirmar três painéis e redimensionamento; confirmar comportamento de janela mínima configurada.
6. Exercitar falha/retry quando disponível; confirmar que nenhum erro mostra falsa conclusão.

Se não for possível capturar inspeção nativa/visual, registrar explicitamente essa limitação; isso não autoriza substituir o smoke por uma alegação não observada.

## Evidence / references

- Estado atual: `src/App.tsx:48-56` (estado efêmero/toast) e `src/App.tsx:155-163` (CTA antigo); `src/components/layout/TitleBar.tsx:64-72` (título hardcoded); `src-tauri/src/lib.rs:1-13` (opener/greet); `package.json` e `src-tauri/Cargo.toml` (sem DB/Drizzle); `src/index.css:103-135` (tokens One Dark).
- Referência geométrica: `docs/metis/desktop/src/App.tsx:699-793` (shell left/center/right); `docs/metis/desktop/src/components/sidebar/Sidebar.tsx:46-136` (search/list/footer settings); `docs/metis/desktop/src/components/inspector/Inspector.tsx:66-179` (Changed Files/Plan/Subagents e footer Context). O `localStorage` do Metis é deliberadamente excluído.
- Contratos de execução: C-001–C-004; decisões aprovadas: D-001–D-007; decomposição: T-001–T-004. Todos os outcomes OUT-001–OUT-005 e requisitos REQ-001–REQ-014 estão mapeados acima.
- Escopo de edição: este plano autoriza implementação posterior nos arquivos prováveis indicados pelas tasks; nesta etapa de redação, somente este arquivo de plano deve ser criado/atualizado.
