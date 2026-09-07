# Jarvis no Windows — auditoria e roteiro de implementação

> Documento de preparação para implementar e validar o suporte **em um ambiente Windows real**. A auditoria foi feita no macOS; não representa compilação, instalação ou homologação do Jarvis no Windows.

**Navegação:** [prioridades](#2-resultado-principal-e-prioridades) · [dados e caminhos](#6-pasta-de-configuração-dados-e-caminhos) · [PowerShell](#8-powershell-e-execução-de-comandos) · [Core](#11-auditoria-dos-cinco-componentes-do-core) · [interface Windows](#14-janela-menus-e-acabamento-windows) · [atualizações](#18-build-instalador-assinatura-e-atualização) · [fases](#19-sequência-de-implementação-recomendada) · [validação manual](#21-matriz-de-validação-manual-windows) · [comandos iniciais](#22-comandos-para-começar-na-máquina-windows).

## 1. Escopo e referência da auditoria

| Item | Referência |
| --- | --- |
| Data | 7 de setembro de 2026 |
| Branch de origem | `main` |
| HEAD durante a leitura | `e2db77049a6aa2cfce7902b3d9998529ae3ac398` |
| Versão nos arquivos do projeto | `0.8.3-beta.1` |
| Estado analisado | HEAD **mais as alterações locais pendentes**, incluindo notificações, não lidos, processos e ajustes dos agentes/sidebar |
| Acompanhamento desta auditoria | Bead `jarvis-lpb` |
| Entrega desta etapa | Documentação; nenhuma adaptação de plataforma, publicação, tag, commit ou push |

As referências de arquivo são relativas à raiz do repositório. Linhas citadas são pontos de entrada da fotografia acima e podem se deslocar. Na máquina Windows, registre o SHA que efetivamente recebeu essas alterações antes de começar.

A intenção é manter o mesmo produto: cinco componentes obrigatórios do Core, provedores e modelos existentes, fluxos, histórico, anexos, perguntas, validação manual, uso de skills/MCPs, aparência industrial e preferências persistentes. Não criar uma edição reduzida para Windows nem exigir WSL para executar o Jarvis.

Nesta primeira migração, implementar e testar localmente no Windows. **Adicionar Windows ao CI de release somente depois da adaptação e da validação manual**, preservando o pipeline atual de macOS.

### Como interpretar as conclusões

- **Confirmado no código:** comportamento observado na implementação, configuração ou dependência inspecionada.
- **Risco a validar:** há uma diferença de plataforma relevante, mas não foi reproduzida em Windows nesta auditoria.
- **Proposta:** direção recomendada para a implementação futura, ainda não existente no produto.

Encontrar `cfg(windows)` ou um arquivo `.ico` é preparação parcial, não evidência de suporte funcional. Da mesma forma, uma referência a `/bin` dentro de um bloco `cfg(unix)` não é automaticamente um bug de Windows.

## 2. Resultado principal e prioridades

O frontend e boa parte da persistência podem ser reaproveitados. Os principais trabalhos estão nas integrações nativas e na execução das ferramentas. Hoje não se deve anunciar o Jarvis como funcional no Windows: credenciais, shell, supervisão dos processos, apresentação da janela e distribuição ainda precisam de adaptação ou validação específica.

| Prioridade | Área | Evidência atual | Consequência / trabalho necessário |
| --- | --- | --- | --- |
| P0 | Credenciais dos provedores | [`openai_codex.rs`](../src-tauri/src/openai_codex.rs), `KeychainSecretStore`, aproximadamente linhas 288–350: implementação não macOS retorna `Unavailable` | Implementar cofre seguro Windows antes de testar login e persistência das contas |
| P0 | Segredos de MCP e Context7 | [`mcp/mod.rs`](../src-tauri/src/mcp/mod.rs), `Secrets`/`Keychain`, linhas 59–106; [`core/context7.rs`](../src-tauri/src/core/context7.rs) reutiliza esse cofre | O Context7 obrigatório não pode ser considerado configurável enquanto essa dependência não funcionar |
| P0 | Shell do agente | [`agent/tools.rs`](../src-tauri/src/agent/tools.rs), `shell`, linha 460: `/bin/bash` | Comandos do agente não executam em um Windows nativo sem adaptação |
| P0 | Processos persistentes | [`agent/processes.rs`](../src-tauri/src/agent/processes.rs), `start`: `/bin/bash`; `Group::stop` não encerra grupo em não Unix | Trocar o lançador e garantir encerramento da árvore de processos pertencente ao Jarvis |
| P0 | Validações dos fluxos | [`workflow/dispatch.rs`](../src-tauri/src/agent/workflow/dispatch.rs), `workflow_check`, linhas 97–107: cria chamada `bash` | Lint/testes/build também dependem do shell atual, embora os comandos de Bun/Cargo sejam portáveis |
| P0 | Resolução de executáveis | [`mcp/executable.rs`](../src-tauri/src/mcp/executable.rs): `HOME`, Homebrew, nvm Unix, `node` sem extensão | MCPs como `npx`, `npm` e ferramentas instaladas pelo usuário precisam de resolução nativa Windows |
| P0 | Integridade do Core | [`core/install.rs`](../src-tauri/src/core/install.rs), [`core/health.rs`](../src-tauri/src/core/health.rs) | Existe preparação Windows, mas o onboarding só pode liberar uso após os cinco componentes passarem por instalação e diagnóstico reais |
| P1 | Caminhos e diffs Git | [`agent/diffs/working.rs`](../src-tauri/src/agent/diffs/working.rs), `Repository`; [`agent/diffs.rs`](../src-tauri/src/agent/diffs.rs) | Tratar prefixos canônicos Windows, separadores de caminho de Git, CRLF e arquivos efetivamente pendentes da sessão |
| P1 | Arquivos e transações | `library`, `agent/journal`, `skills/store`, Core | Revalidar ACLs, links/reparse points, arquivos abertos, substituições e exclusões no NTFS |
| P1 | Título/controles | [`TitleBar.tsx`](../src/components/layout/TitleBar.tsx), linhas 81–102 | Os botões continuam como bolinhas à esquerda em todas as plataformas; Windows precisa de título/logo à esquerda e controles à direita |
| P1 | Notificações e não lidos | [`system/notifications.rs`](../src-tauri/src/system/notifications.rs), [`system/unread.rs`](../src-tauri/src/system/unread.rs) | Há envio genérico de notificação; identificação do app precisa de UAT. A badge nativa é explicitamente omitida em Windows |
| P1 | Atualização e reabertura | [`updater/mod.rs`](../src-tauri/src/updater/mod.rs), [`updater/relaunch.rs`](../src-tauri/src/updater/relaunch.rs) | O instalador Windows encerra o processo; adaptar limpeza e reabertura sem depender do código posterior a `install()` |
| P1 | Pacote distribuível | `tauri.conf.json`, scripts de release | Não há receita de distribuição Windows pronta; começar por instalador NSIS local e teste de atualização |
| P2 | Acabamento nativo | DPI, Snap, atalhos, WebView2, ícones e acessibilidade | Homologar o uso diário mantendo a identidade visual |
| P2 | Automação de publicação | `.github/workflows/release-macos.yml`, `scripts/release-*.ts` | Generalizar depois da homologação local, sem substituir ou quebrar o release macOS |

P0 indica bloqueio para uso básico; P1, requisito para uma versão Windows utilizável com as funcionalidades atuais; P2, acabamento ou distribuição posterior. Essa classificação não afirma que a compilação Windows já foi tentada.

### Preparações existentes que devem ser preservadas

- `app.path().home_dir()` já é usado pelos comandos principais para construir `.jarvis`.
- Dependências Objective-C/Keychain estão condicionadas a macOS no Cargo.
- `main.rs` já usa `windows_subsystem = "windows"` em release para não abrir um console do próprio aplicativo.
- O instalador do Core conhece `windows`, ZIP e nomes `.exe`; Node/npm têm caminhos específicos de Windows.
- O Beads prepara `USERPROFILE`, `APPDATA` e `SystemRoot` no ambiente isolado.
- A persistência da janela já considera área útil dos monitores e escala.
- Anexos já removem tanto `/` quanto `\` do nome recebido.
- Skills já seguem diretórios canônicos, limitam recursão e evitam ciclos.
- `keepawake` já possui backend Windows; não é necessário substituí-lo apenas por ser Windows.
- As permissões Tauri para minimizar, maximizar, fechar, arrastar e tela cheia já existem.

**Falso positivo descartado:** os caminhos `bd` e `dolt/bin/dolt` em `core/health.rs:161` pertencem ao reparo de permissões dentro de `#[cfg(unix)]`. A verificação de runtime em `health.rs:183` já escolhe `bd.exe` e `dolt/bin/dolt.exe` no Windows. Não alterar esse trecho como se fosse um defeito confirmado.

## 3. Política de suporte recomendada

| Dimensão | Primeira entrega recomendada | Limite que deve ficar explícito |
| --- | --- | --- |
| Sistema | Windows 11 em versão suportada pela Microsoft | Windows 10 requer uma rodada própria; não prometer compatibilidade só porque Tauri consegue abrir |
| Arquitetura | `x86_64-pc-windows-msvc` | Não usar GNU/MinGW como primeira variante |
| ARM64 | Avaliação posterior | O Dolt consultado não publica ZIP Windows ARM64; o Core completo precisa de todos os artefatos compatíveis |
| Shell do agente | PowerShell 7 (`pwsh.exe`), com fallback explícito para Windows PowerShell 5.1 | Informar versão e sintaxe ao modelo; nunca tratar 5.1 e 7 como equivalentes |
| Instalação do app | Por usuário, NSIS, sem exigir execução diária como administrador | Diretório do executável é separado de dados e credenciais |
| Dados | Perfil do usuário, `.jarvis` | Não usar a pasta do repositório, Downloads ou `Program Files` para estado mutável |
| Projetos | Diretórios locais NTFS na primeira homologação | UNC, unidades mapeadas, pastas de rede e OneDrive devem ser testados separadamente |
| Runtime de interface | WebView2 Evergreen com versão mínima compatível com o frontend | A política offline precisa ser definida no instalador |
| WSL/Git Bash | Opcionais, fora da configuração padrão | Não converter silenciosamente uma sessão nativa em sessão WSL |

No Windows ARM64, a execução da edição x64 por emulação pode ser investigada como modo separado. Nesse caso, **todos os componentes do Core devem seguir a arquitetura x64 da edição instalada**, sem misturar detecção do processador do host com arquitetura do processo. Isso não equivale a suporte ARM64 nativo.

## 4. Ambiente de desenvolvimento Windows

### Dependências do desenvolvedor

1. Git for Windows, com `git.exe` acessível pelo ambiente do aplicativo.
2. Bun compatível com o lockfile. O CI macOS atual fixa **Bun 1.3.14**; usar essa referência inicialmente e avaliar atualizações separadamente.
3. Rust via rustup, toolchain MSVC, Cargo e Clippy.
4. Microsoft C++ Build Tools, workload **Desktop development with C++**, compilador MSVC e Windows SDK.
5. Microsoft Edge WebView2 Runtime.
6. PowerShell 7, recomendável também para trabalhar no repositório.
7. GitHub CLI somente se for necessário acompanhar/disparar o CI ou publicar depois. Não é requisito de execução do Jarvis.

O SQLite do Rust é `rusqlite` com `bundled`; isso evita depender de uma instalação global de SQLite, mas o build ainda precisa da toolchain C/C++ correta. Instalar Visual Studio completo não é obrigatório se Build Tools + SDK atenderem ao projeto.

### Dependências do usuário final

O usuário não deve precisar de Rust, Visual Studio, GitHub CLI ou ferramentas de assinatura. O Jarvis instala seus runtimes privados do Core. Git e as toolchains dos **projetos do usuário** são requisitos distintos: o Node privado do Context-mode não é automaticamente o Node escolhido para compilar qualquer projeto.

O diagnóstico deve diferenciar:

- “Ferramenta interna do Jarvis ausente/corrompida”.
- “Git não encontrado para Marketplace/diff”.
- “Este projeto precisa de uma versão de Node/Bun/Python/etc. não encontrada”.
- “O executável existe, mas não pode ser executado nesta arquitetura”.

Não resolver esses cenários solicitando ao usuário que execute o Jarvis inteiro como administrador.

## 5. Limites arquiteturais para a adaptação

Criar uma camada pequena de plataforma no Rust, em vez de espalhar novos `cfg!` e strings Windows por cada ferramenta. Os nomes abaixo são sugestões, **não módulos já existentes**.

| Responsabilidade proposta | Contrato | Consumidores |
| --- | --- | --- |
| `platform::paths` | Home confiável, raiz `.jarvis`, caminho de exibição, caminho nativo, caminho relativo de Git | Persistência, Core, skills, anexos, diffs |
| `platform::secrets` | `load/store/delete`, namespace, erros tipados e exclusão idempotente | Provedores, MCPs e Context7 |
| `platform::shell` | Executável, família, versão, transporte do script, encoding e política de saída | Shell do agente e processos persistentes |
| `platform::process` | Spawn supervisionado, captura, timeout, cancelamento, árvore pertencente ao app e janelas de console | Shell, Core, MCPs, Git, checks e processos |
| `platform::desktop` | Plataforma/capacidades, integração da taskbar e atalhos nativos necessários | Janela, notificações e interface |

Regras de implementação:

- Lógica pesada continua no Rust. React recebe estado/capacidades e desenha a interface.
- Não introduzir shell para operações que já aceitam executável + argumentos.
- Separar **comando livre do modelo** de **comando fixo do produto**.
- Evitar condições baseadas somente em `navigator.platform`; preferir uma informação nativa centralizada com fallback seguro nos testes do frontend.
- Manter o identificador `com.foxtag.jarvis` e os IDs das contas/conversas; a plataforma não cria outra identidade lógica para os mesmos registros.
- Validar macOS a cada fase que alterar uma abstração compartilhada.

## 6. Pasta de configuração, dados e caminhos

### 6.1 Localização

Manter o contrato pedido para o produto:

```text
macOS:   /Users/<usuario>/.jarvis
Windows: C:\Users\<usuario>\.jarvis
```

`C:\Users` é apenas exemplo. Resolver o diretório do perfil via API de home/known folder. Ele pode estar em outra unidade, conter espaços/acentos ou ser redirecionado. `%USERPROFILE%` e `$env:USERPROFILE` são as notações Windows para o perfil; `.jarvis` deve ser construída com operações de caminho, não concatenação de barras.

O código de produção já usa `app.path().home_dir()` em vários pontos. A principal exceção confirmada é `mcp/executable.rs`, que lê apenas `HOME` e usa diretório vazio como fallback. Há também testes opt-in que usam `HOME`. Corrigir a exceção sem trocar toda a persistência por `%APPDATA%` inadvertidamente.

Se o perfil não puder ser resolvido, retornar diagnóstico claro. Não cair silenciosamente no diretório atual nem criar `.jarvis` junto ao executável.

### 6.2 Mapa de estado existente

```text
<perfil>\.jarvis\
  jarvis.db                          metadados, biblioteca, configuração e não lidos
  desktop.json                       janela, painéis e preferências de layout
  system.json                        notificações e impedimento de repouso
  agents.json                        provedor/modelo/raciocínio por agente
  skills.json                        configuração das skills
  skills\                            skills gerenciadas pelo Jarvis
  cache\skills\                      checkouts temporários do Marketplace
  skill-transactions\                journal/recuperação de instalações de skills
  sessions\<projeto>\<sessao>.jsonl    histórico de conversas
  workflows\<sessao>\                 estado e journals dos agentes
  attachments\<sessao>\<anexo>\        conteúdo, preview e metadados
  context-mode\<hash-da-sessao>\       índices e estado do Context-mode
  beads\projects\<projeto>\           tracker privado do projeto
  core\
    manifest.json                    gerações instaladas
    install.lock                     exclusão mútua do instalador
    context7.json                     referência da credencial do Context7
    <componente>\<geracao>\           pacote, runtime e recibo de recuperação
```

O mapa é orientativo, não uma lista de arquivos que sempre existirão. Bancos, journals, locks e caches podem ter arquivos auxiliares. `context7.json` deve continuar contendo referência, não a chave em texto puro.

O cache/perfil do WebView2 é uma exceção a decidir explicitamente: o runtime pode usar diretórios de dados de aplicativo do Windows. Se o objetivo for centralizar também isso em `.jarvis`, configurar o diretório do WebView2 por API suportada, mantendo instâncias e atualizações compatíveis. Não presumir que `home_dir()` controla o perfil do navegador embutido.

### 6.3 Caminhos nativos, identidade e interface

- Rust: usar `Path`/`PathBuf`/`OsString` para IO e processos.
- Não identificar um projeto pela string exibida ao usuário. `C:\Repo`, `c:\repo` e um caminho canônico com `\\?\` podem apontar para o mesmo local.
- `canonicalize()` no Windows pode devolver caminhos estendidos, como `\\?\C:\...`. Tratar isso em comparações, integração com Git, Node e serialização.
- Não remover `\\?\` indiscriminadamente do caminho usado em IO: isso pode quebrar caminhos longos. Criar representação própria de exibição quando necessário.
- `C:repo` é relativo à unidade, não o mesmo que `C:\repo`. UNC (`\\servidor\share`) e caminhos de dispositivo têm semânticas próprias.
- Não aplicar `to_lowercase()` global aos caminhos: existem diretórios NTFS com sensibilidade a caixa habilitada e arquivos em outros sistemas.
- Um caminho Windows em JSON precisa de escape: `"C:\\Projetos\\Loja"`; isso não implica que o texto passado a `Command::arg` deva conter essas barras duplicadas.
- URI `file://`, caminho nativo e importação JavaScript são contratos diferentes. Para módulos JS, usar conversão suportada de caminho para URL; nunca `import("file://" + path)`.
- Em dados relativos apresentados na UI/Git, adotar `/` como forma canônica documentada, convertendo apenas na fronteira com o filesystem.

### 6.4 Permissões e substituição de arquivos

`0700`, `0600`, `O_NOFOLLOW`, links físicos e `fsync` de diretório são tratados em vários pontos com `cfg(unix)`. Não copiar essas operações para Windows nem considerar que sua ausência mantém a mesma proteção.

Definir e testar o equivalente necessário:

- ACL do usuário para dados privados; manter permissões administrativas/sistema necessárias e evitar leitura ampla por outros usuários.
- Detecção de reparse points, links simbólicos e junctions em diretórios nos quais redirecionamento não é permitido.
- Conferência do destino efetivamente aberto quando houver operações sensíveis; verificar caminho antes de abrir não elimina sozinho uma corrida de substituição.
- Substituição atômica no mesmo volume, inclusive com destino existente.
- Tratamento limitado de `sharing violation`/arquivo em uso, sem transformar erro persistente em retry infinito.
- Fechar handles de SQLite, arquivos e processos antes de remover uma geração do Core ou uma sessão.
- Preservar journal de transação e geração anterior até terminar a operação.

`tempfile::NamedTempFile::persist` já é usado e tem implementação multiplataforma; não há evidência de que precise ser substituído em todos os lugares. Testar seu contrato real com antivírus e arquivos abertos. `fs::rename` de diretórios também merece testes de falha/recuperação no Windows.

### 6.5 Migração de dados de um Mac

Transportar o projeto por Git não transporta o estado em `.jarvis`. Copiar `.jarvis` também não produz uma instalação Windows funcional automaticamente.

| Dado | Tratamento proposto |
| --- | --- |
| Histórico e anexos | Preservar IDs, validar integridade, versão do banco e referências; fazer backup consistente com app fechado |
| Caminhos de projetos | Pedir remapeamento `/Users/...` → pasta escolhida no Windows; não adivinhar |
| Preferências de janela | Preservar painéis; recalcular/restaurar limites alcançáveis nos monitores atuais |
| Contas e credenciais | Reconectar/reconfigurar. Keychain não é exportado ao copiar JSON/SQLite |
| Core | Reinstalar runtimes nativos; nunca executar binários Mach-O no Windows |
| Skills instaladas | Revalidar conteúdo/origem, atalhos, paths e scripts; não herdar suposição de executável Unix |
| Beads/Context-mode | Fazer migração consistente e compatível com a versão dos bancos; não copiar arquivos abertos ou descartar WAL |

Um assistente de importação entre sistemas é uma feature separada. Para a primeira homologação, começar com perfil Windows novo e não usar a cópia bruta do perfil macOS como teste de instalação limpa.

## 7. Armazenamento seguro de credenciais

### Situação real

Existem dois contratos diferentes: `SecretStore` dos provedores e `Secrets` dos MCPs. Ambos têm implementação macOS e fallback não funcional fora do macOS. O Context7 usa o cofre dos MCPs, inclusive para verificar se a configuração obrigatória está acessível.

As contas não armazenam somente uma chave curta. OAuth inclui tokens e metadados serializados, e os MCPs armazenam configuração serializada que pode conter ambiente/headers. Esse tamanho importa.

### Backend Windows recomendado

Avaliar **DPAPI no escopo do usuário**, com blobs criptografados dentro de `.jarvis`, como backend comum para os dois contratos. Isso acomoda payloads variáveis e mantém o estado sob a pasta do Jarvis. O diretório, nomes e formato desse cofre seriam novos; por exemplo, `.jarvis/secrets/` com IDs opacos, schema e namespace.

Windows Credential Manager também é opção, mas a estrutura `CREDENTIALW` documenta limite de `CRED_MAX_CREDENTIAL_BLOB_SIZE`, **5 × 512 bytes**. Não salvar um JSON OAuth/MCP inteiro nele sem avaliar o tamanho real. Se escolhido, usar envelope criptográfico ou armazenamento auxiliar cuidadosamente definido, em vez de truncar dados ou espalhar fragmentos sem transação.

Requisitos do contrato:

1. Reutilizar namespaces estáveis de provedor/MCP/Core; não transformar alias visível em caminho inseguro.
2. Diferenciar segredo ausente, cofre indisponível, dado corrompido, acesso negado e falha de gravação.
3. Exclusão de uma referência que já não existe deve ter sucesso. Isso evita repetir o problema de remoção que já ocorreu com o Context7 como MCP.
4. Trocar credenciais transacionalmente: gravar a nova, atualizar referência, só então remover a anterior; reverter em falha.
5. Não fazer fallback para JSON em texto puro nem “corrigir” erro apagando o cofre inteiro.
6. Não registrar token, header de autorização, conteúdo secreto de MCP ou chave Context7 em diagnósticos.
7. Manter interface do usuário em pt-BR com “armazenamento seguro” quando a mensagem for compartilhada entre plataformas.
8. Testar armazenamento de payload maior que 2.560 bytes, múltiplas contas, atualização de token, reinício, remoção e reinstalação do app.
9. Testar execução por outro usuário do Windows: ele não deve obter as credenciais do primeiro.
10. No caso de DPAPI, explicar que o blob é vinculado ao contexto do usuário/Windows; copiar o arquivo para outro computador não é uma estratégia geral de migração de credenciais.

**Chaves de release são outro assunto.** A chave privada do updater e certificados de assinatura não devem ser incluídos no app nem no perfil de cada usuário. Mantê-los nos secrets de CI e, quando houver backup local do mantenedor, em local não versionado apropriado.

## 8. PowerShell e execução de comandos

### 8.1 Separar três casos

| Caso | Implementação recomendada |
| --- | --- |
| Comando fixo do Jarvis, como `git rev-parse` ou `cargo test` | Executável + vetor de argumentos, sem shell |
| Script livre fornecido pelo agente | Interpretador explicitamente selecionado, versão conhecida, política de encoding/saída |
| Servidor/watcher persistente | Mesmo resolvedor, mas supervisão/lifecycle próprio no registro de processos |

O nome atual da ferramenta é `bash`. Há contratos, checks de autorização, hooks, histórico e cards que o conhecem. Duas estratégias possíveis:

- Preservar o nome interno por compatibilidade, mas expor uma descrição inequívoca de que no Windows ele executa PowerShell e exibir esse nome na UI.
- Introduzir `shell` como nome futuro e manter leitura/roteamento compatível com chamadas históricas `bash`.

Não renomear apenas a definição da ferramenta. Revisar `workflow/contracts.rs`, `workflow.rs`, `workflow/dispatch.rs`, `core/hooks.rs`, prompts, cartões e desserialização do histórico.

### 8.2 Política de shell

1. Resolver `pwsh.exe` e verificar que executa, com versão identificada.
2. Na ausência, usar Windows PowerShell 5.1 do sistema com contrato explícito.
3. Não selecionar automaticamente um `bash.exe` de WSL encontrado no `PATH`.
4. Não carregar perfil do usuário por padrão: profiles podem imprimir texto, pedir input ou trocar diretório.
5. Executar sem UI interativa: flags compatíveis como `-NoLogo`, `-NoProfile`, `-NonInteractive`.
6. Escolher transporte robusto para o script: `-Command`, arquivo temporário ou `-EncodedCommand`, com testes próprios. `-EncodedCommand` usa Base64 de texto UTF-16LE, **não UTF-8**.
7. Definir UTF-8 para a captura; PowerShell 5.1, 7, programas nativos e redirecionamentos têm diferenças reais.
8. Não alterar permanentemente `ExecutionPolicy`, perfil, registro ou `PATH` global do computador para fazer o app funcionar.

O uso de arquivo `.ps1` pode esbarrar em política de execução corporativa. Tratar a escolha do transporte de maneira explícita; uma política bloqueada deve aparecer no diagnóstico. Não ocultar a restrição executando por outro interpretador.

### 8.3 Informar o ambiente ao agente

Adicionar ao contexto técnico de cada execução, inclusive subagentes:

```text
OS: Windows
Architecture: x86_64
Shell: PowerShell 7.x (or Windows PowerShell 5.1, as actually detected)
Working directory: <native project path>
Jarvis data directory: <resolved profile>\.jarvis
Use PowerShell syntax for shell calls. Native tools receive separate arguments.
Available project runtimes: <only successfully detected executables and versions>
```

Esses campos são proposta de contrato, não uma instrução para preencher versões fictícias. Manter instruções dos agentes em inglês como hoje, UI em pt-BR e escolhas de modelo intactas.

No Context-mode, alinhar o shell que `ctx_execute(language="shell")` realmente usa com o shell anunciado. Não permitir que a ferramenta nativa execute PowerShell enquanto o Core executa Git Bash silenciosamente para o mesmo pedido.

### 8.4 Diferenças que não podem ser resolvidas por substituição de texto

| Intenção | POSIX comum | PowerShell |
| --- | --- | --- |
| Variável de ambiente | `$HOME`, `$PATH` | `$env:USERPROFILE`, `$env:PATH`; `$HOME` existe no PowerShell, mas não é garantia de env `HOME` para filhos |
| Definir variável de ambiente | `NAME=value command` | Definir `$env:NAME` com escopo adequado, ou passar `env` diretamente pelo Rust |
| Invocar caminho com espaços | `'/path/tool' arg` | `& 'C:\Program Files\Tool\tool.exe' arg` |
| Encadear somente em sucesso | `a && b` | Disponível no PowerShell 7; em 5.1 verificar resultado explicitamente |
| Ler arquivo literal | `cat file` | `Get-Content -LiteralPath ...`; para tools internas, preferir leitura Rust |
| Buscar texto | `grep` | `Select-String` ou `rg.exe` se instalado |
| Copiar/mover | `cp`, `mv` | `Copy-Item`, `Move-Item`, com parâmetros e semântica próprios |
| Remover | `rm -rf` | `Remove-Item -LiteralPath ... -Recurse -Force`, somente no escopo autorizado |
| Saída descartada | `/dev/null` | `$null` no PowerShell; `Stdio::null()` no Rust |
| Continuar linha | `\` | Backtick, ou preferencialmente arrays/splatting sem continuação frágil |
| Here-document | `<<'EOF'` | Here-string; regras de delimitador e interpolação diferentes |
| Processo em background | `&`, `nohup` | Não usar para fugir do registro nativo; usar `process_start` |

Aliases `cat`, `rm`, `curl` podem existir no PowerShell, mas não aceitam necessariamente os argumentos POSIX esperados. Não orientar o modelo a confiar neles. `curl.exe` é diferente de um alias PowerShell chamado `curl`.

### 8.5 Erros, quoting e códigos de saída

- `$ErrorActionPreference = 'Stop'` ajuda com erros de cmdlets, mas não torna todo exit code nativo não zero uma exceção em todas as versões de PowerShell.
- Preservar `$LASTEXITCODE` quando a operação é nativa. Um `Write-Output` posterior não pode transformar um build com falha em sucesso.
- Capturar stdout e stderr separadamente, com limite de bytes, sem perder conteúdo final quando o processo termina.
- Não usar `JSON.stringify()` como escape de shell. Caminhos, aliases e conteúdo do usuário são argumentos/dados, não trechos concatenados em um comando.
- Testar argumentos com espaços, acentos, aspas, apóstrofo, `&`, `;`, `$`, parênteses e backtick.
- Considerar limite de tamanho da linha de comando do Windows; scripts maiores podem precisar de arquivo/stdin, mantendo isolamento e limpeza.
- Evitar prompt interativo de npm, Git e instaladores em ferramentas finitas. Uma autorização do produto não pode ficar escondida num subprocesso sem entrada.

### 8.6 Executáveis, PATH e shims

`mcp/executable.rs` hoje adiciona caminhos Unix e busca Node sem extensão. Criar resolução Windows que:

- Entenda `Path`/`PATH` como a mesma variável e respeite um ambiente explicitamente configurado pelo usuário.
- Use `split_paths`/`join_paths` e `;` de Windows, sem concatenar com `:`.
- Resolva `.exe`, `.com`, `.cmd` e `.bat` conforme o contrato e `PATHEXT`.
- Trate wrappers `.cmd/.bat` como scripts com semântica própria. `cmd.exe /d /s /c` exige quoting rigoroso; o comportamento de spawn de arquivos batch não deve ser confundido com o de `.exe`.
- Para npm/npx do **runtime privado**, prefira `node.exe` + `npm-cli.js`/`npx-cli.js`, como o instalador do Context7 já faz para npm.
- Verifique executáveis obtidos do ambiente da GUI, que pode diferir do terminal aberto depois de instalar uma ferramenta.
- Não use por acidente aliases da Microsoft Store que abrem a loja em vez de executar Python/Node.
- Não injete caminhos versionados internos do Core no comando de um projeto: uma atualização do Core os invalida.

## 9. Processos, cancelamento e portas

### 9.1 Supervisão de toda a árvore

O app atualmente usa process groups em Unix. No Windows, matar somente `pwsh.exe` pode deixar `npm → node → servidor` vivo. Isso impacta botão Parar, timeout, cancelamento de agente, erro de MCP, atualização e fechamento do app.

Usar **Windows Job Objects** para agrupar apenas processos pertencentes a uma execução. A versão auditada de `process-wrap`, **9.1.0**, já oferece a feature `job-object` e `tokio::JobObject`. O Cargo atual usa `default-features = false` com `tokio1`, `process-group`, `kill-on-drop`, sem habilitar `job-object`.

Antes de adotá-la, validar o ciclo completo da API: spawn suspenso/atribuição ao Job, retomada, handles, `KillOnDrop`, flags para não abrir console e execução sob outro Job de IDE/CI. Evitar a corrida de o processo gerar filhos antes de pertencer ao Job.

Aplicar a política a:

- `agent/tools.rs` — comandos finitos.
- `agent/processes.rs` — processos persistentes.
- `mcp/runtime.rs` — servidores MCP locais.
- `core/install.rs` — npm, verificação de runtime e instaladores.
- `core/beads/process.rs` — chamadas ao Beads.
- Chamadas de Git e processos de hooks que possam manter descendentes/handles.

Não adotar `taskkill /IM node.exe` nem matar o dono de uma porta como solução genérica. Isso pode encerrar serviços e terminais que o usuário abriu fora do Jarvis. Se houver fallback por PID/árvore, comprovar propriedade e considerar reuso de PID.

### 9.2 Console invisível e lifecycle

`windows_subsystem` em `main.rs` resolve o console **do aplicativo release**, não o dos filhos. Aplicar a configuração de spawn apropriada para evitar janelas piscando em npm, Git, MCPs, hooks e diagnósticos. Combinar flags com a implementação do Job Object, em vez de uma sobrescrever a outra.

Comportamentos a preservar:

- Processo continua depois que o turno do modelo acaba.
- Processo termina ao usuário confirmar Parar ou quando o Jarvis encerra.
- Processo com falha/encerrado pode ser removido da lista.
- Nova execução não substitui silenciosamente outra em andamento.
- Cancelamento e timeout não deixam a UI aguardando leitura de um pipe herdado por um neto.
- Após encerrar, drenar/fechar streams e liberar portas; não deixar leitores esperando indefinidamente.
- No relaunch/update, separar o novo Jarvis do Job de execução das ferramentas: o sucessor não deve morrer quando o pai sair.

### 9.3 Portas

`process_check_port` já usa sockets TCP e consulta IPv4/IPv6 sem conectar ou encerrar o dono. É uma boa base multiplataforma, mas requer testes Windows de bind exclusivo/reuso, dual stack e portas excluídas pelo sistema/Hyper-V.

Porta ocupada, porta reservada/proibida e falha de inspeção são estados diferentes. A ferramenta não deve declarar que encontrou o servidor correto apenas porque a porta está ocupada. A consulta também não é reserva de porta: a segunda checagem no `process_start` continua necessária, e o bind real ainda pode perder a corrida.

Preservar a regra dos fluxos: nos diretos, servidor persistente somente quando solicitado; nos Planejado/Completo, quando necessário à validação manual dentro do escopo. A migração não adiciona automação de navegador.

## 10. Instalação, atualização e diagnóstico do Core

### 10.1 Estado e contratos compartilhados

Todos os cinco componentes continuam obrigatórios. Não contornar o onboarding ou a barra de bloqueio para declarar Windows funcional.

O instalador já verifica hashes dos downloads, valida pacotes, extrai em gerações e usa lock. A saúde local não deve ser confundida com acesso momentâneo à internet. Estar offline não torna um runtime íntegro “corrompido”; pacote ausente, executável incompatível ou credencial obrigatória inacessível exigem reparo.

O registro `Installation` atual contém `version`, `directory` e `files`, **sem sistema/arquitetura/schema de runtime**. Proposta: evoluir esse registro/recibo com versão de schema, plataforma, arquitetura e runtimes relevantes. Isso ajuda a detectar perfil copiado de macOS e não reutilizar uma geração incompatível. Tratar registros antigos sem esses campos por migração explícita e verificação, não por suposição.

### 10.2 Extração e arquivos em uso

- Manter validação de traversal e integridade já existente.
- Adicionar casos Windows: drive prefix, UNC, nomes reservados (`CON`, `NUL`, `COM1` etc.), alternate data streams (`arquivo:stream`), pontos/espaços finais e colisão por caixa.
- Validar o caminho final de cada entrada e seus pais; não permitir que junction/reparse point redirecione uma extração ou reparo para fora do Core.
- Tratar formatos e hierarquia interna dos ZIPs por componente. A existência do asset não prova que o executável está no caminho esperado depois de remover a primeira pasta.
- Não tentar atualizar uma pasta contendo executável/DLL em uso. Instalar nova geração, verificar, trocar referência e limpar a anterior após liberar seus usuários.
- Persistir o journal necessário para recuperar interrupção entre essas etapas.
- Distinguir falta de espaço, arquivo bloqueado, quarentena do antivírus, incompatibilidade de CPU e indisponibilidade de rede.
- Não desabilitar Defender nem criar exclusões amplas automaticamente.

### 10.3 Progresso e reparo

Preservar as barras de download e o estado de extração/configuração/verificação. Mostrar progresso indeterminado quando não houver total real; não simular porcentagem exata durante npm/extração.

O diagnóstico deve retornar, sem segredos: componente e versão, OS/arquitetura, caminho resolvido, executável esperado/encontrado, resultado do probe, espaço disponível quando relevante e categoria do erro. Permitir copiar detalhes úteis.

Reparo/desinstalação deve atingir somente a geração do pacote problemático. **Reinstalar Beads ou Context-mode não autoriza apagar seus bancos/históricos**. Reinstalar Context7 não deve apagar sua credencial sem necessidade; se ela estiver inválida/inacessível, oferecer reconfiguração.

As ações de diagnóstico e Solucionar continuam acessíveis quando o Core bloqueia o chat. Não criar um bloqueio do qual o próprio usuário não consegue sair para reparar.

## 11. Auditoria dos cinco componentes do Core

### 11.1 Context-mode

**Pontos de entrada:** [`core/install.rs`](../src-tauri/src/core/install.rs), [`core/context.rs`](../src-tauri/src/core/context.rs), [`core/hooks.rs`](../src-tauri/src/core/hooks.rs), [`core/context-hook.mjs`](../src-tauri/src/core/context-hook.mjs).

O Jarvis instala Node privado **22.23.2** e Bun, baixa `context-mode` do npm, usa `--ignore-scripts` e executa `server.bundle.mjs` por Node. Os hooks são chamados por Node diretamente, com um adaptador próprio; não dependem de instalar hooks abertos para o usuário configurar.

O probe do Node em `install_node` não testa apenas `--version`: cria tabela virtual FTS5 em `node:sqlite`. Preservar esse teste, porque abrir SQLite sem FTS5 não prova que busca/indexação funcionam.

O código-fonte local do Context-mode já tem suporte a shell Windows, `windowsHide`, arquivos `.ps1`/`.cmd`, normalização de nomes de shell e tratamento de limpeza de temporários em uso. Também contém adaptador `node:sqlite`, com fallback para `better-sqlite3`. Isso é evidência favorável, mas o pacote npm e seus bundles efetivamente baixados precisam corresponder a uma versão com esses comportamentos.

Trabalho necessário:

1. Confirmar layout do ZIP Node: `runtime/node.exe` e `runtime/node_modules/npm/bin/npm-cli.js`.
2. Confirmar versão/CPU do Bun instalado. O asset x64 padrão pode ter requisitos de CPU diferentes de `baseline`; escolher conscientemente, não apenas por ser x64.
3. Centralizar o `PATH` do contexto. Hoje `context::environment` ainda adiciona `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin` e `/bin` sem condição de plataforma.
4. Definir o shell efetivo suportado no ambiente do Core e verificar que o upstream o respeita; não presumir que sua escolha coincide com a do Jarvis.
5. Confirmar que os imports de bundles funcionam com caminhos Windows, espaços e acentos.
6. Testar `SessionStart`, pré/pós-ferramenta, registro de eventos, compactação e restauração, mantendo os dados por sessão.
7. Revalidar roteamento de `pre_tool`: hoje só observa chamadas de nome `bash`; os padrões já incluem `Invoke-WebRequest`, mas não cobrem todo PowerShell. Roteamento é orientação, não sandbox de segurança.
8. Executar uma chamada real de indexação + busca FTS5 e uma execução shell. Listar ferramentas MCP não cobre esses recursos.
9. Testar cancelamento com banco aberto e limpeza de temporários `*-wal`/`*-shm` sem perder estado persistente.
10. Não exigir build global de `better-sqlite3` como solução automática. Primeiro confirmar o caminho suportado por `node:sqlite`/FTS5 que o Jarvis já preparou.

Os runtimes opcionais que `ctx_execute` descobre — Python, Go, Rust etc. — devem refletir o que executa no computador. Não anunciar uma linguagem porque um stub da Microsoft Store foi encontrado no `PATH`.

### 11.2 Ponytail

**Pontos de entrada:** [`core/ponytail.rs`](../src-tauri/src/core/ponytail.rs), `install::install` e os testes de Ponytail.

No Jarvis, Ponytail é principalmente um pacote de instruções/skills. A instalação verifica nome/versão do pacote e arquivos como `AGENTS.md` e `skills/ponytail/SKILL.md`. O Jarvis não precisa iniciar a aplicação upstream inteira.

O risco de binário nativo é menor; os pontos relevantes são caminhos, parsing de Markdown e comandos sugeridos ao agente:

- Não passar a usar `%APPDATA%\ponytail` global por causa da convenção do upstream. O estado que o Jarvis gerencia permanece em `.jarvis`.
- Validar metadados com LF, CRLF e BOM onde o contrato permitir. O parser de Ponytail tem separadores textuais específicos com `\n`; não confundir um tarball LF, que pode funcionar, com todos os arquivos editados no Windows.
- Rever orientações que assumem comandos POSIX. A informação de plataforma do agente precisa prevalecer sobre exemplos que não executam em PowerShell.
- Confirmar que níveis/modos e instruções continuam idênticos entre os fluxos.

### 11.3 Beads e Dolt

**Pontos de entrada:** [`core/beads.rs`](../src-tauri/src/core/beads.rs), [`core/beads/process.rs`](../src-tauri/src/core/beads/process.rs), [`core/beads/dashboard.rs`](../src-tauri/src/core/beads/dashboard.rs), instalador e diagnóstico do Core.

O tracker usado pelo Jarvis é privado, por projeto, em `.jarvis/beads/projects`. As chamadas configuram **`BD_DOLT_MODE=embedded`** e **`BEADS_DOLT_AUTO_START=0`**. Não introduzir um daemon Dolt/TCP só para Windows sem necessidade demonstrada; a integração atual é embedded.

O comando já usa `install::executable("bd")`, informa `USERPROFILE`/`APPDATA` privados e preserva `SystemRoot` no Windows. O ambiente é intencionalmente limpo: não herdar configuração Beads/Git/Dolt do usuário ou de outro projeto.

Trabalho e validação:

- Confirmar `bd.exe`, `dolt.exe`, eventuais DLLs e layout final extraído.
- Verificar capability de **embedded Dolt na distribuição exata de `bd`**, não apenas `bd version`. O código upstream distingue métodos de build; um executável que imprime versão não prova que oferece a feature necessária.
- Não construir o Beads com flags simplificadas ou por outro método só para conseguir um `.exe` se isso remover a capacidade embedded usada pelo Jarvis.
- Revisar ambiente mínimo: `SystemRoot`, diretório temporário privado quando necessário, PATH nativo e requisitos efetivamente usados pelo binário. Não reintroduzir todo o ambiente global para resolver um erro pontual.
- Integrar os processos ao Job Object e manter o timeout/limite de saída existentes.
- Testar criação de projeto, criação/consulta de épico e tarefa, dependências, comentários, transições, dashboard/Kanban e planos abertos.
- Testar duas conversas do mesmo projeto concorrendo pelo tracker; outro projeto não pode compartilhar banco por engano.
- Testar reinício após interrupção no meio de uma mutação: consultar estado antes de repetir, sem duplicar comentário/tarefa.
- Confirmar liberação de locks/handles antes de limpeza de projeto e reinstalação de runtime.
- Preservar `.beads` de um checkout do usuário: ele não é o banco privado do Jarvis e não deve ser inicializado, apagado ou sincronizado pela migração.

### 11.4 Open Design

**Pontos de entrada:** [`core/design.rs`](../src-tauri/src/core/design.rs), [`core/design.md`](../src-tauri/src/core/design.md), instalação em `core/install.rs`.

O Jarvis resolve um commit do Open Design, baixa o arquivo-fonte e gera/valida o índice de recursos. Não inicia a aplicação Next.js inteira do Open Design como parte do Core. Preservar essa fronteira evita acrescentar servidores e dependências que o Jarvis não usa.

Riscos a validar:

- Extração de pacote grande com caminhos longos, nomes incompatíveis e colisões por caixa.
- Preservação de licenças, metadados e índice `jarvis-design.json`.
- Links/caminhos do índice: o código já converte `\` para `/` em parte do catálogo; padronizar isso para todos os recursos.
- Templates e scripts não se tornam compatíveis com Windows apenas por terem sido baixados. O Designer deve escolher a forma de execução disponível e informar bloqueios reais.
- Parsing de front matter/Markdown sob CRLF, quando o conteúdo vier de checkout ou edição Windows.
- Download, extração e indexação devem continuar mostrando atividade/progresso; manter tooltip de que a instalação pode demorar.
- Atualização não apaga artefatos de design criados pelo usuário nem força mudança no projeto.

Preservar comportamento dos agentes: Designer direto pode perguntar; Designers subordinados comunicam lacunas ao Planejador/Orquestrador conforme o fluxo. Não mudar isso por plataforma.

### 11.5 Context7

**Pontos de entrada:** [`core/context7.rs`](../src-tauri/src/core/context7.rs), `install::install`, armazenamento MCP.

O pacote `@upstash/context7-mcp` é instalado por npm privado; não voltar para um `npx` global temporário. O instalador já usa Node + `npm-cli.js`, valida integridade e testa descoberta de ferramentas.

Dependências e testes:

1. Cofre Windows funcional antes da configuração da chave.
2. Node/npm locais no caminho Windows correto.
3. Chave passada somente no canal necessário, sem aparecer em logs, mensagens ou relatórios copiados.
4. Configurar, salvar, fechar o app, reabrir, resolver uma biblioteca e consultar documentação.
5. Separar credencial ausente/inacessível, chave rejeitada pelo serviço e indisponibilidade de rede.
6. Reinstalar pacote preservando a referência segura da credencial quando ela continua válida.
7. Não recriar o antigo MCP manual duplicado do Context7.

### 11.6 Disponibilidade de artefatos consultada

Consulta somente de metadados/checksums em 07/09/2026; **os ZIPs não foram instalados nem homologados no Windows**.

| Componente | Versão consultada | Artefatos Windows relevantes observados |
| --- | --- | --- |
| Node privado | `22.23.2` | `node-v22.23.2-win-x64.zip`, `node-v22.23.2-win-arm64.zip` nos checksums oficiais |
| Bun | `bun-v1.4.2` | `bun-windows-x64.zip`, `bun-windows-x64-baseline.zip`, `bun-windows-aarch64.zip` |
| Beads | `v1.2.2` | `beads_1.2.2_windows_amd64.zip`, `beads_1.2.2_windows_arm64.zip` |
| Dolt | `v2.3.2` | `dolt-windows-amd64.zip` e variantes `.7z`/`.msi`; nenhum ZIP Windows ARM64 na release consultada |

Essas versões são uma fotografia de disponibilidade, **não uma atualização recomendada ou já aplicada ao Core**. O instalador deve continuar comparando versões/contratos e verificando integridade. Reconsultar na implementação; o layout interno e as capabilities ainda precisam de validação.

## 12. MCPs e skills

### 12.1 MCPs locais e remotos

`mcp/runtime.rs` executa `command[0]` + argumentos, resolve `cwd`, passa ambiente e faz transporte stdio pelo `rmcp`. Preservar o objeto de configuração adotado pelo Jarvis; não transformar o textarea em um campo de shell.

| Situação | Tratamento Windows |
| --- | --- |
| `node` + arquivo `.mjs` | Resolver Node real, preservar argumentos e caminho com espaços |
| `npx -y pacote` | Resolver shim `.cmd` ou entrada JS de forma explícita; não usar suposição de shebang Unix |
| Executável absoluto | Aceitar caminho Windows válido e exibir mensagem útil se faltar |
| `cwd` relativo/absoluto | Resolver e validar semanticamente; não duplicar drive/prefixo |
| `environment` com `Path` | Não acrescentar um segundo `PATH` conflitante; preservar override intencional |
| Servidor imprime banners | Não poluir stdout JSON-RPC com mensagens do shell/profile |
| Cancelamento/timeout | Encerrar Job e descendentes, liberar streams e atualizar estado de descoberta |
| MCP HTTP | Revalidar proxy, TLS, headers e reconexão; não depende de PowerShell |

O erro atual de spawn é genérico. Melhorar o diagnóstico para diferenciar executável inexistente, arquitetura errada, acesso negado e timeout do protocolo. Nunca incluir variáveis/headers secretos na mensagem. OAuth de MCP remoto é uma limitação própria do contrato atual, não um problema criado pela migração Windows.

Após salvar/alterar configuração, manter descoberta automática e badges das ferramentas. Ativar/desativar deve preservar configuração; excluir pede confirmação e tolera referência de segredo já ausente.

### 12.2 Descoberta de skills e atalhos

Locais atuais:

- `.jarvis/skills`, sempre.
- `.agents/skills` do perfil e do projeto, conforme configuração.

O scanner usa caminhos canônicos, conjunto de diretórios visitados e limites de profundidade/quantidade. Isso já ajuda com links e ciclos. Porém, **um atalho `.lnk` do Explorer não é um link simbólico nem uma junction**.

Política proposta para a primeira entrega:

1. Suportar pastas normais, links simbólicos de diretório e junctions válidas nos locais de skills autorizados.
2. Não exigir modo desenvolvedor/admin só para ler uma junction existente.
3. Testar criação/leitura em um ambiente que não pode criar symlinks, distinguindo limitação de criação de limitação de leitura.
4. Detectar ciclos, destino ausente e duplicatas pelo destino real.
5. Separar leitura de instalação/exclusão: remover uma skill vinculada não pode apagar a pasta de destino compartilhada.
6. Para `.lnk`, decidir explicitamente entre resolver via Shell API com validação do alvo ou explicar que essa variante ainda não é suportada. Não anunciar “todo atalho funciona” sem implementar esse caso.
7. Não varrer pastas do Codex ou outros aplicativos como fallback quando `.agents/skills` estiver vazia.
8. Ao ligar o switch de `.agents/skills`, atualizar a listagem imediatamente, sem exigir reinício.

O parser de skills já lê linhas e tolera BOM UTF-8. Validar arquivos globais em CRLF, nomes com acentos e referências a scripts; a capacidade de ler `SKILL.md` não garante executar um `.sh` incluído nela.

### 12.3 Marketplace e atualização

O Marketplace clona repositórios com `git`. `skills/store.rs` usa explicitamente `core.hooksPath=/dev/null` e `GIT_CONFIG_GLOBAL=/dev/null`. Git for Windows pode interpretar alguns desses caminhos, mas essa equivalência não deve ser uma dependência oculta.

Usar arquivo vazio e diretório de hooks vazio controlados pelo Jarvis, ou mecanismo comprovadamente portável, preservando o isolamento. Manter desabilitados prompts, helpers e protocolos que o fluxo já restringe.

Testar transação com atualização interrompida, diretório aberto no Explorer, antivírus segurando arquivo, arquivo somente leitura, exclusão e recuperação na próxima inicialização. A mudança de permissão executável Unix participa do cálculo de hash em uma plataforma e não na outra; não assumir que copiar metadados do Mac mantém o mesmo fingerprint de skill no Windows.

## 13. Git, arquivos alterados e edição de projetos

### 13.1 Caminhos de Git são um contrato separado

`agent/diffs.rs` e `agent/diffs/working.rs` usam `git rev-parse`, `ls-tree` e `git show <ref>:<path>`. Atualmente há conversão de `Path` para string no argumento de objeto Git. No Windows, um caminho relativo com `\` precisa ser convertido para o formato de caminho do Git antes de formar `HEAD:src/arquivo.ts`.

Outro risco: raiz canônica Rust com prefixo `\\?\` e raiz devolvida por `git rev-parse` sem esse prefixo podem falhar em `strip_prefix` apesar de representar a mesma pasta. Normalizar/comparar por contrato de caminho; não tratar uma diferença textual como projeto externo automaticamente nem relaxar a proteção de escopo.

Ao ler a raiz da saída do Git, considerar `\r\n` e nomes válidos sem remover espaços do nome real. O código de `working::Repository::open` hoje remove apenas `\n` da raiz. Verificar a saída do Git for Windows instalado e tornar o parser robusto.

### 13.2 Preservar o significado da lista de alterações

A lista do inspector deve mostrar somente **linhas/arquivos atribuíveis à sessão e ainda não commitados**. Não substituí-la por um simples `git status` do projeto para simplificar Windows.

Cobrir:

- Alteração anterior à sessão, que deve continuar fora da lista.
- Alteração da sessão parcialmente commitada: mostrar apenas o restante.
- Arquivo novo, excluído, renomeado, sem `HEAD`, binário e acima do limite de preview.
- Repositório como subpasta, worktree com `.git` como arquivo e caminho com espaços/acentos.
- Conversas concorrentes alterando partes diferentes do mesmo arquivo.
- Git ausente/inacessível: estado de indisponibilidade claro, não diff vazio enganoso.

`ChangedFiles.tsx` divide o caminho com `/`. Isso funciona se o backend mantiver caminhos relativos normalizados; documentar/testar essa fronteira.

### 13.3 CRLF, UTF-8 e modos de arquivo

O `HEAD` pode conter LF enquanto o arquivo no Windows contém CRLF por configuração Git. Comparar bytes/linhas sem considerar esse contrato pode fazer a tela indicar todo o arquivo como alterado ou atribuir linhas à sessão errada.

- Preservar o estilo de newline do arquivo editado, salvo mudança solicitada.
- Avaliar normalização apenas na comparação/visualização, sem regravar o projeto inteiro.
- Testar BOM UTF-8 e arquivos que não sejam UTF-8; manter erro/limite explícito para codificação não suportada.
- Não mudar `git config --global core.autocrlf` do usuário.
- Definir `.gitattributes` do **Jarvis** em uma alteração futura própria se necessário; isso não autoriza alterar todos os projetos abertos no app.
- Modos executáveis POSIX, symlink de arquivo e junction precisam de política própria; não converter tudo em arquivo comum silenciosamente.

### 13.4 Arquivos seguros e transações

As ferramentas de arquivo atualmente rejeitam links simbólicos no caminho do projeto e usam verificações adicionais Unix. Implementar os casos Windows equivalentes sem enfraquecer a garantia de ficar dentro do projeto.

Casos de teste necessários: root canônico, caminho absoluto permitido, `..`, caminho de outra unidade, `C:relativo`, UNC, reparse point para fora, link físico, arquivo substituído entre checagem e abertura, arquivo readonly e destino em uso. Para entradas inválidas, retornar motivo adequado; não sugerir contornar a restrição pelo shell.

## 14. Janela, menus e acabamento Windows

### 14.1 Barra de título

O pedido visual deve resultar em dois layouts explícitos:

```text
macOS:   [fechar minimizar tela cheia]                 [logo horizontal Jarvis]
Windows: [logo horizontal Jarvis]                [minimizar maximizar fechar]
```

A logo horizontal já contém “Jarvis”; não repetir o nome nem reintroduzir “Início”. Manter as cores/fontes/superfícies do produto no conteúdo, usando controles de janela reconhecíveis no Windows.

| Interação | macOS, preservar | Windows, implementar/validar |
| --- | --- | --- |
| Fechar | Bolinha vermelha à esquerda | X à direita, hover destrutivo, área clicável adequada |
| Minimizar | Bolinha amarela | Traço à direita |
| Controle de expansão | Bolinha verde entra/sai de tela cheia | Maximiza/restaura; não entrar em fullscreen automaticamente |
| Duplo clique na área de título | Maximiza/restaura | Maximiza/restaura |
| Tela cheia | Header pode sumir no modo macOS | Se oferecida, ação separada com saída alcançável; não copiar a ocultação macOS sem política |
| Arrastar | Região já usa Tauri | Arrastar apenas áreas não interativas, restaurar ao arrastar janela maximizada conforme suporte nativo |
| Menu de sistema | Menu do macOS | Alt+Espaço e comportamento nativo esperado; não confundir com menu de conversa |
| Snap | Sem obrigação de equivalência | Win+Setas, Win+Z e Snap Layouts sobre maximizar precisam de verificação nativa |

Tauri está com `decorations: false`. Um botão HTML chamando `toggleMaximize()` não garante hover nativo de Snap Layouts. Se esse comportamento for necessário, avaliar integração/hit-test de caption suportado pela versão Tauri/Tao, ou composição com decoração nativa. Não declarar Snap completo com base apenas em maximizar funcionar.

Compor controles com os primitivos shadcn existentes. Manter `cursor-pointer`, labels acessíveis, foco por teclado e drag region fora dos botões. Não reestilizar componentes de registry para essa adaptação.

### 14.2 Menus e atalhos

`app_menu.rs` é exclusivo de macOS e instala Sobre o Jarvis/Configurações no menu de aplicativo. Windows não tem esse mesmo menu global do sistema.

- Manter Sobre/versão e Configurações acessíveis na statusbar, como já estão.
- Adicionar menu local somente se necessário, sem duplicar uma barra global de menus macOS no Windows.
- Garantir Ctrl+, para Configurações se mantido como atalho do produto. Hoje o registro está no menu macOS; não basta o texto `CmdOrCtrl` existir no arquivo que não compila em Windows.
- Validar Ctrl+C/V/X/A/Z/Y, Enter, Shift+Enter, Tab e Escape em composer, modais, comentários e seletor de skills.
- Não usar tecla Windows/Meta como substituta de Ctrl no Windows.
- Não interceptar Alt+F4, Ctrl+C de edição ou navegação de foco indevidamente.
- Confirmações devem manter Enter para confirmar, com foco e contexto corretos quando há modais empilhadas.

### 14.3 DPI, múltiplos monitores e persistência

`desktop.rs` salva posição física, tamanho normalizado por escala, estado maximizado/fullscreen e preferências de painéis. O restore já usa área útil do monitor, inclusive barra de tarefas. Reaproveitar e homologar:

- Escalas 100%, 125%, 150%, 175% e 200%.
- Dois monitores com escalas diferentes, inclusive um à esquerda/acima com coordenadas negativas.
- Desconectar monitor, trocar monitor principal e abrir depois com outro layout.
- Maximizar, minimizar, fechar, reabrir; minimizar não deve sobrescrever tamanho normal com dimensões inválidas.
- Área útil com taskbar em outra posição ou ocultação automática.
- Zoom/acessibilidade de texto, janela na largura mínima de 1024 e altura mínima de 480.
- Snap seguido de persistência/restauração, sem classificar um tamanho temporário como fullscreen.

Preferências vindas de macOS não devem abrir o Windows fora da tela nem restaurar um modo de fullscreen que impede sair da interface. Caso seja adicionada plataforma de origem ao arquivo, migrar sem perder largura/collapse dos painéis.

### 14.4 WebView2 e conteúdo

Testar no WebView2 real, não só em um Chrome aberto na URL do Vite:

- Borda neon do composer, agentes em execução e contexto durante compactação.
- Blur de modais sobre modais, contraste da aba ativa e posicionamento de popovers.
- Scroll do histórico sem uma segunda rolagem mover o composer; largura alinhada do histórico/composer ao recolher sidebars.
- Skeletons de carregamento e imagens sem deslocamento inesperado do layout.
- Kanban, drawer, diff e Markdown/código com blocos largos e nomes longos.
- Roboto/fontes mono, caracteres de desenho, emojis e ícones nas escalas Windows.
- Movimento reduzido, contraste do sistema e navegação por teclado.

Não trocar a identidade industrial por componentes visualmente genéricos. A adaptação nativa deve concentrar-se nos controles da janela, entrada, feedback do sistema e comportamento do desktop.

### 14.5 Anexos, imagens e correção de texto

O composer usa eventos de clipboard do WebView e anexos nativos/bytes; revalidar:

- Colar imagem da Ferramenta de Captura, navegador e editor de imagem.
- Colar texto com acentos/emoji e misturar texto + imagem sem duplicar anexo.
- Arquivo copiado no Explorer pode chegar como lista de arquivos/URI em vez de imagem; não prometer esse caso sem tratar o formato efetivo.
- Seleção pelo botão +, arquivo em Downloads/OneDrive, arquivo indisponível e nomes longos.
- Placeholder some imediatamente depois da colagem; skeleton aparece antes do preview.
- Download da imagem gerada por diálogo nativo, cancelamento e substituição de arquivo existente.
- Dimensões/peso e botão de download continuam corretos; filename não sai do contêiner.
- Copiar código pelo Clipboard API e feedback de falha se o WebView negar a operação.
- Autocorreção permanece desativada nos campos técnicos; exceções do composer/comentários devem ser verificadas no WebView2 e nas opções de digitação do Windows.
- IME/composição: Enter durante composição não envia a mensagem ou confirma um diálogo por acidente.

## 15. Notificações, não lidos e repouso

### 15.1 Notificações nativas

O backend não macOS usa `notify-rust`, informa `app_id("com.foxtag.jarvis")` no Windows e considera `authorize()` bem-sucedido sem consultar uma autorização real. Isso não prova que o usuário verá banners.

Definir uma identidade Windows consistente entre executável, atalho do Menu Iniciar, instalador e notificações (**AppUserModelID/AUMID**). Verificar o comportamento da versão concreta da dependência de notificação para aplicativos Win32 instalados. Não depender de uma identidade de outro aplicativo para fazer o toast aparecer.

Requisitos:

- Nome **Jarvis** e ícone corretos no banner/central de notificações.
- Botão Testar em Configurações gera um teste identificável e orienta o diagnóstico se o sistema bloquear.
- Preservar configuração liga/desliga e evitar pedidos repetitivos de permissão.
- Considerar Não incomodar/Assistente de foco, notificações globais desativadas e configuração por aplicativo. Não prometer contornar essas opções.
- Completion de fluxo Planejado/Completo apenas ao término do fluxo; sem notificações individuais de cada subagente.
- Perguntas/solicitações de intervenção podem notificar antes do término.
- Erro que impede continuar notifica depois da política de retry existente; manter as cinco falhas consecutivas previstas e não notificar cada tentativa transitória.
- Texto contém projeto e conversa corretos, sem segredos nem saída inteira da ferramenta.
- Definir ativação ao clicar: restaurar a janela e selecionar a conversa associada; testar app aberto, minimizado e fechado. Se a dependência atual não fornece esse callback, isso é trabalho adicional explícito.
- Verificar dev e app instalado separadamente: identidade/atalhos de uma sessão `tauri dev` podem não ser os mesmos de uma instalação NSIS.

Um retorno `Ok` de `.show()` é evidência de chamada aceita pela biblioteca, **não de banner exibido**. Registrar essa distinção nos testes.

### 15.2 Badge da taskbar e conversas não lidas

`system/unread.rs` já mantém leitura por evento observado, mas a atualização visual nativa faz nada no Windows. Implementar overlay/badge adequado à taskbar — por exemplo, via `ITaskbarList3::SetOverlayIcon` ou API Tauri equivalente disponível — usando a mesma contagem lógica.

- Overlay legível em ícone pequeno: decidir `1…9`, `9+` ou ponto conforme desenho; não prometer a mesma cápsula grande da Dock.
- Limpar quando a conversa relevante foi exibida/lida; não limpar toda a contagem só porque a janela recebeu foco.
- Restaurar contagem ao reabrir e ao Explorer/taskbar recriar seus botões.
- Manter a bolinha de não lido da sidebar e destaque do projeto correspondente.
- Ao abrir por notificação, marcar somente eventos efetivamente apresentados.
- Testar duas conversas/projetos concluindo ao mesmo tempo, com uma ainda não lida.
- Revalidar a correção recente de reatividade: resposta concluída em background deve aparecer na primeira volta à conversa, sem exigir alternar duas vezes.

A preferência de banners e o estado de não lido são conceitos separados. Preservar/definir explicitamente a política de badge quando notificações estiverem desligadas, sem descartar eventos de leitura por esse motivo.

### 15.3 Impedir repouso

O Jarvis usa `keepawake::Builder` com `idle(true)`. Na versão **0.6.1**, o backend Windows usa `SetThreadExecutionState`, incluindo `ES_SYSTEM_REQUIRED` para idle. Já existe uma base nativa.

Homologar os três modos:

| Modo | Resultado esperado |
| --- | --- |
| Desligado | Nenhuma inibição mantida pelo Jarvis |
| Enquanto houver agentes/chats ativos | Inibir durante trabalho conforme a contagem do app; liberar ao terminar, cancelar ou falhar |
| Enquanto Jarvis aberto | Manter inibição até encerrar o app, inclusive minimizado |

Garantir criação/liberação no thread correto e limpeza em fechamento/update. Não prometer impedir desligamento, suspensão explícita, fechamento da tampa ou ações de política corporativa. Bloqueio de tela não é o mesmo que repouso, e `idle(true)` não implica manter a tela acesa.

Para diagnóstico manual, `powercfg /requests` pode ajudar a conferir pedidos ativos; sua disponibilidade/permissão deve ser tratada como diagnóstico, não requisito para o app funcionar.

## 16. Provedores, OAuth e rede

Os protocolos HTTP/SSE dos provedores não precisam ser reescritos por serem usados no Windows. A maior dependência confirmada é o cofre; o restante exige homologação do runtime nativo e da rede.

Cobrir OpenAI Codex, Antigravity e Custom nos três endpoints: OpenAI Completions, OpenAI Responses e Anthropic Messages.

### OAuth

`openai_codex.rs` usa listener em loopback; Codex monta callback com `localhost` e Antigravity com `127.0.0.1`. Preservar os contratos exigidos por cada provedor, PKCE, state, timeout e cancelamento.

Testar navegador padrão, porta ocupada, IPv4/IPv6/localhost, firewall local, callback duplicado, fechamento da aba de login e autorização cancelada. Não abrir o callback em `0.0.0.0` para contornar um problema de loopback e não alterar redirect URI sem verificar suporte do provedor.

### Transporte e reconexão

- Proxy corporativo, variáveis de proxy e confiança de certificados do stack `reqwest`/rustls usado no build.
- Não desativar verificação TLS para conseguir funcionar.
- Rede perdida no meio de streaming, sleep/wake, troca de Wi-Fi e sessão em background.
- Tokens renovados no cofre correto; reabrir app sem novo login desnecessário.
- Headers de identificação Jarvis para Custom/OpenRouter preservados.
- Descoberta de modelos, janela de contexto e raciocínio preservados nos três endpoints Custom.
- Web Search/Vision herdando o chat/agente ou seleção dedicada; geração de imagens só quando habilitada para Antigravity conforme contrato atual.
- Uso/limites na statusbar, timezone de reset, reserva/déficit e tick temporal da barra.
- Streams UTF-8 fragmentados, inclusive caracteres multibyte divididos entre chunks; não interpretar cada pedaço isolado como uma linha PowerShell.

Os testes de conta real usam credenciais temporárias/configuradas pelo usuário em Windows. Fixtures e mocks não comprovam comportamento do serviço; não registrar dados sensíveis nos logs de aceite.

## 17. Sessões, recursos e consistência ao trocar de janela

Preservar o carregamento paginado do histórico e índices, evitando reintroduzir leitura integral ao adaptar caminhos.

- Journals JSONL e offsets devem ter codificação/newline definidos pelo Jarvis. Não passar dados de sessão por ferramentas que convertam LF para CRLF ou UTF-8 para UTF-16 sem migrar o índice.
- Abrir sessão grande, carregar páginas anteriores e navegar pelos marcadores sem bloquear a UI.
- Compactação manual/automática deve continuar registrada no histórico, bloquear interação conforme contrato e atualizar uso de contexto.
- Foco/visibilidade do WebView2 podem variar com minimização, Alt+Tab, desktops virtuais e bloqueio de tela. Reconciliar estado nativo ao retomar.
- Resposta, erro, pergunta e fila recebidos em background não podem ficar presos em “Pensando e executando…”.
- Não reenviar uma mensagem em fila duas vezes por ressincronizar a janela.
- Limpeza de conversas antigas continua pedindo confirmação, preservando a última sessão de cada projeto e excluindo somente estado Jarvis relacionado.
- Arquivos em uso e locks podem impedir limpeza parcial: registrar/reconciliar em vez de anunciar liberação de espaço que não ocorreu.
- Evitar executar duas instâncias mutadoras sobre o mesmo perfil sem uma política de coordenação. O handoff do atualizador pode exigir exceção controlada; avaliar instance lock/single instance em conjunto com a reabertura.

## 18. Build, instalador, assinatura e atualização

### 18.1 Estado do build

[`scripts/tauri.ts`](../scripts/tauri.ts) já limita assinatura Apple ao `process.platform === "darwin"` e executa a CLI Tauri em um processo novo. A parte de interrupção/sinais precisa de teste Windows para não deixar Vite/Cargo/filhos depois de Ctrl+C.

Não executar no Windows `run-signed-macos.sh`, `ci-macos-signing.sh`, `check-macos-keychain.ts`, `codesign` ou comandos `security`. Continuam específicos do macOS.

Proposta: configuração de bundle Windows separada, carregada somente no build apropriado. Manter `productName: "Jarvis"`, identifier e versão coerentes. `bundle.targets: "all"` hoje não significa que há uma política pronta para distribuir todos os formatos em todos os sistemas.

### 18.2 Instalador inicial

Recomenda-se começar com **NSIS por usuário** e `Jarvis` no Menu Iniciar. MSI/WiX pode ser uma etapa posterior para ambientes corporativos. Tauri documenta que MSI requer build em Windows; cross-compilation NSIS existe com ressalvas, mas não é o caminho desta migração.

Definir explicitamente:

- Diretório de instalação por usuário e política de atualização sem elevação desnecessária.
- Ícone `.ico` com múltiplas resoluções, aparência na taskbar, Alt+Tab, Explorer e lista de apps.
- AUMID/atalhos necessários à identidade do app.
- Política WebView2: bootstrapper online ou runtime offline, com mensagens claras se estiver ausente.
- Idioma do instalador e metadados de versão/autor.
- Desinstalar executável sem apagar automaticamente projetos ou histórico `.jarvis`.
- Comportamento quando app/processos estiverem abertos durante instalar/desinstalar.

SemVer e formato de versão de instalador não são idênticos em todos os formatos. Preservar a versão pública `0.x.y-beta` prevista para releases futuras e verificar como NSIS/MSI geram seus campos numéricos. Não alterar silenciosamente para `beta.1` nem reutilizar tag publicada.

### 18.3 Assinaturas diferentes

| Assinatura | Finalidade | Política |
| --- | --- | --- |
| Tauri updater/minisign | Verificar integridade/autenticidade do payload baixado pelo Jarvis | Obrigatória; nunca desabilitar verificação para Windows |
| Authenticode | Identificar editor do `.exe`/instalador no Windows | Escolher certificado/serviço de assinatura e timestamp; requer configuração própria |
| Apple signing/notarização | Identidade de `.app`/DMG no macOS | Mantida apenas no pipeline macOS |

Assinatura do updater não elimina SmartScreen nem equivale a assinatura Authenticode. Mesmo Authenticode não é garantia de ausência de alertas de reputação em uma distribuição nova. Não incluir chave privada no bundle ou no repositório.

### 18.4 Atualização nativa e reabertura automática

O fluxo atual em `updater/mod.rs` baixa/verifica, faz flush, chama `update.install(bytes)`, depois envia `Restarting`, inicia um sucessor e só sai quando o sucessor confirma janela visível.

**Diferença confirmada na dependência auditada `tauri-plugin-updater 2.11.0`:** em Windows, `install()` inicia o instalador e encerra o processo por `std::process::exit(0)`. O plugin oferece `on_before_exit` e `restart_after_install(true)`. Portanto, o código posterior a `install()` não é o local de limpeza/reabertura Windows, nem há garantia de executar destructors ou o evento Tauri normal de encerramento.

Implementação recomendada:

1. Bloquear novas execuções e verificar estado ocioso, incluindo processos persistentes conforme contrato atual de atualização.
2. Baixar, verificar assinatura e preservar barra de progresso.
3. Antes de chamar o instalador, persistir sessão/layout/estado e fazer a limpeza necessária dos recursos.
4. Preparar a política de restart do instalador (`restart_after_install(true)` já é aplicado) e callback de saída quando necessário.
5. Em Windows, deixar o instalador substituir arquivos após a saída e abrir o Jarvis de novo. Não lançar uma segunda instância concorrente antes da substituição do `.exe`.
6. Na primeira abertura nova, reconciliar uma marca persistente de atualização pendente se for necessário mostrar o resultado. Registrar versão esperada, sem credencial de restart permanente.
7. Manter o protocolo atual de confirmação de sucessor para macOS onde ele se aplica; não apagá-lo como efeito colateral da adaptação.

O usuário espera que o app **abra automaticamente depois de atualizar**. Verificar isso com duas versões Windows reais. O sucesso de `download` ou a existência de `restart_after_install(true)` não substitui esse teste.

Testar: cancelamento de UAC quando houver, instalação sem permissão, antivírus segurando executável, app aberto, processo persistente, download interrompido, assinatura inválida, restart lento, versão divergente e instalador executado manualmente.

### 18.5 Manifests e pipeline

`updater::platform_key()` já usa `windows-x86_64` no target x64. O cliente procura assets específicos de plataforma, mas a publicação atual gera somente `latest.json` e `latest-darwin-aarch64.json`.

O pipeline futuro precisa produzir o manifest Windows correspondente, por exemplo `latest-windows-x86_64.json`, cujo conteúdo `platforms` use as chaves/formatos aceitos pela versão do plugin. URL, extensão e assinatura devem corresponder ao payload Windows **real**, sem adaptar um manifest de `.app.tar.gz` por simples troca de nome.

Pontos de generalização:

- `scripts/release-common.ts`: target/workflow/environment hoje macOS.
- `scripts/release-plan.ts`: targets aceitos e chaves dos manifests.
- `scripts/release-ci.ts`: coleta, verificação, assinatura, nomes e publicação de artefatos `.app`/DMG.
- `.github/workflows/release-macos.yml`: preservar jobs atuais; adicionar pipeline Windows somente na fase aprovada.
- `docs/RELEASING.md`: documentar construir local, publicar via CI e instalar/atualizar como operações distintas.

No futuro, um publicador único deve reunir/verificar artefatos de cada plataforma antes de tornar o release público, ou oferecer adição posterior controlada sem apagar manifests existentes. Não deixar jobs de macOS e Windows sobrescreverem `latest.json` um do outro.

O comando `bun run release` atual continua disparando **CI macOS**. Poder executá-lo de um terminal Windows não significa que ele passou a gerar uma versão Windows. Não executar esse comando durante a adaptação achando que é apenas build local: ele cria commit/tag, faz push e publica pelo CI.

## 19. Sequência de implementação recomendada

Esta sequência serve para decompor o trabalho em Beads no ambiente de implementação. Não substitui o tracker e não marca nenhuma adaptação como pronta.

| Fase | Escopo e entregas | Dependências | Critério para avançar |
| --- | --- | --- | --- |
| W0 — Base reproduzível | Registrar SHA/versões, ambiente Windows, arquivos locais necessários e perfil de teste separado | Alterações atuais disponíveis no checkout Windows | Build/teste inicial executados e erros reais registrados, sem editar dependências ao acaso |
| W1 — Plataforma e build | Compilação MSVC, capacidades nativas, configuração de bundle local, adaptação dos testes exclusivos de Unix | W0 | App abre em WebView2; checks que independem de Core/contas funcionam; nenhum erro Windows é escondido por skip genérico |
| W2 — Paths e cofre | Home `.jarvis`, caminhos canônicos, backend seguro e contratos de exclusão | W1 | Preferências e credenciais sobrevivem ao restart em conta de teste; segredos não aparecem em arquivos legíveis |
| W3 — Shell/processos | PowerShell, argv, PATH/shims, Job Objects, timeout, cancelamento e `workflow_check` | W1/W2 para os recursos que persistem estado | Comando finito e servidor com descendentes executam/encerram sem vazamento e sem fechar processos externos |
| W4 — Core e onboarding | Instalar/verificar/reparar os cinco componentes, metadados de plataforma e contrato de hooks | W2/W3 | Perfil limpo conclui onboarding; cada componente executa sua função essencial; pacote inválido bloqueia e pode ser reparado |
| W5 — Integrações e projetos | MCPs, Marketplace, atalhos, Git/diffs, anexos, providers/OAuth e leitura paginada | W2/W3/W4 | Fluxos reais podem trabalhar no projeto com histórico/arquivos corretos |
| W6 — Desktop Windows | Titlebar, menus/atalhos, DPI, persistência, taskbar/notificações e repouso | W1; notificações de fluxo usam W5 | Validação manual da matriz desktop, inclusive background e duas conversas concorrentes |
| W7 — Instalador/update local | NSIS, identidade, assinatura, manifests e reabertura Windows | W2–W6 | Instalação limpa e atualização de versão A para B comprovadas em Windows |
| W8 — Homologação do produto | Regressões dos fluxos, performance, interrupções, limpeza e macOS | W5–W7 | Evidências dos checks automáticos e aceite manual separados; bloqueios abertos resolvidos ou suporte limitado explicitamente |
| W9 — CI Windows | Runner Windows, secrets, coleta/verificação/publicador compartilhado | W8 e autorização para ativar publicação | Pipeline reproduz o pacote homologado e preserva os manifests macOS |

W6 pode ter implementação visual em paralelo com W2/W3 por decisão da equipe, mas uma janela bonita não desbloqueia o requisito de Core e credenciais. Não antecipar W9 para usar um release público como teste de build local.

### Decisões que devem ser registradas durante as fases

| Decisão | Recomendação inicial | Evidência necessária para mudar |
| --- | --- | --- |
| Primeiro target | Windows 11 x64/MSVC | Disponibilidade e homologação de todos os runtimes para outro target |
| Cofre | DPAPI por usuário para payloads variáveis | Necessidade operacional clara e limites de tamanho tratados na alternativa |
| Shell | PowerShell 7 → fallback 5.1 explícito | Projeto requer outro shell e o contrato permite, sem fallback silencioso para WSL |
| Supervisão | Job Objects | Demonstração de que outra solução cobre propriedade, filhos, crashes e timeout |
| Instalador | NSIS por usuário | Requisito de administração corporativa/instalação por máquina |
| Pasta de dados | `<perfil>\.jarvis` | Mudança de produto autorizada; não apenas conveniência de uma biblioteca |
| Links de skills | Symlinks/junctions suportados; `.lnk` exige decisão própria | Testes de resolução, ciclo e exclusão segura |
| Reabertura | Instalador Windows relança; macOS mantém protocolo próprio | Comportamento verificado da versão de updater adotada |

## 20. Plano de testes automatizados

As adaptações devem deixar testes de comportamento, colocalizados, seguindo a estrutura existente. Evitar testes que somente procuram a string `windows` no código ou reproduzem a implementação.

### 20.1 Matriz por subsistema

| Grupo | Casos essenciais | Evidência esperada |
| --- | --- | --- |
| Home/config | Perfil com espaços/acentos, `HOME` ausente, perfil não resolvível, diretório inacessível | Dados sempre na raiz correta ou erro explícito; nenhum fallback no cwd |
| Credenciais | Store/load/delete, ausente, payload grande, corrupção, troca de referência, múltiplas contas | Persistência segura, erro tipado e exclusão idempotente |
| Resolver | `.exe`, `.cmd`, `Path` vs `PATH`, override explícito, caminho com espaço, stub de Store | Executável correto e argumentos preservados |
| Shell | Cmdlet e binário com sucesso/falha, stdout/stderr Unicode, multiline, quoting, 5.1/7 | Exit code correto, sem truncamento silencioso ou interpretação de dados como código |
| Processos | Pai+filho+neto, timeout, cancelamento, app exit, filho mantendo pipe | Árvore própria encerrada; processo externo de controle continua vivo |
| Portas | Livre, ocupada IPv4/IPv6/dual stack, bind wildcard, restrita, corrida entre check/start | Estado de disponibilidade correto e nenhum encerramento de terceiros |
| Core/arquivos | ZIP Windows válido, traversal, prefixo de drive, reservado/ADS, colisão por caixa, arquivo em uso | Extração limitada, diagnóstico claro e recuperação transacional |
| Core/runtime | Node FTS5, Bun, MCP Context-mode, hooks, Context7, Beads embedded | Cada capability essencial funciona com o runtime instalado |
| Skills | CRLF/BOM, symlink, junction, ciclo, duplicata, destino ausente, exclusão de link | Listagem correta sem apagar origem compartilhada |
| Marketplace | Clone, atualização, conflito/lock de arquivo, interrupção no rename | Versão anterior/novo estado coerentes e recuperação de journal |
| Filesystem | Root canônico, `\\?\`, drive relativo, UNC, reparse point, link físico, readonly | Escopo mantido; nenhum escape do projeto/config |
| Diff | LF/CRLF, nomes com acento, commit parcial, edição anterior, arquivo novo/deletado | Só alterações atribuíveis à sessão e ainda pendentes |
| Histórico | JSONL UTF-8, offsets, páginas, compactação, retomada, eventos em background | Sem duplicata/perda e sem leitura integral indevida |
| UI | Variantes de titlebar, atalhos, Enter, foco de modais, estado ativo | Comportamento observável com plataforma injetada |
| Não lidos | Dois projetos, foco em outra conversa, ack atrasado, retomada | Contagem e marcação monotônicas e coerentes |
| Update | Target/manifest, semver beta, assinatura, idle gate, falha libera gate | Payload correto, proteção mantida e contrato de saída por plataforma |

### 20.2 Testes existentes que exigem atenção

- `agent/processes.rs`: há fixtures com `exec sleep`, comandos Bash e sinais; não rodam nativamente como estão em PowerShell.
- Testes de `mcp/executable.rs`, MCP e vários cenários de links usam `cfg(unix)`; adicionar equivalentes Windows, não só manter todos ignorados.
- Testes opt-in de providers, compaction, Vision, Ponytail e skills têm leituras diretas de `HOME`. Usar home injetado/API apropriada; nunca tocar o perfil real por engano.
- `persistence.rs` tem exemplo de caminho `/Users/example`; testar a construção relativa ao perfil sob cada plataforma, sem converter um teste de string macOS em regra global.
- `TitleBar.macos.test.tsx` deve continuar cobrindo macOS; criar/expandir casos Windows sem removê-lo.
- `scripts/macos-signing.test.ts` e fixtures Objective-C são específicos; separar guardas e manter a cobertura do contrato de macOS.
- Checks de release que assumem `.app`, DMG e `darwin-*` precisam de casos Windows quando a publicação for generalizada.

Preferir helpers de processo independentes de shell para testar infraestrutura, por exemplo um executável fixture controlado. Manter testes separados para o parser/semântica PowerShell. Mockar `cfg` ou `navigator.platform` não exercita NTFS, DPAPI, WebView2, Job Objects ou NSIS.

### 20.3 Gates do repositório

Executar no Windows depois de cada conjunto de adaptações aplicável, na ordem do projeto:

```powershell
bun run lint
if ($LASTEXITCODE -ne 0) { throw 'Lint falhou' }
bun run typecheck
if ($LASTEXITCODE -ne 0) { throw 'Typecheck falhou' }
bun run test
if ($LASTEXITCODE -ne 0) { throw 'Testes frontend falharam' }
bun run build
if ($LASTEXITCODE -ne 0) { throw 'Build frontend falhou' }

cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { throw 'Clippy falhou' }
cargo test --locked --manifest-path src-tauri/Cargo.toml
if ($LASTEXITCODE -ne 0) { throw 'Testes Rust falharam' }
```

`bun run check` também executa os quatro primeiros gates. Os `if` nos exemplos evitam que PowerShell 5.1 continue e esconda uma falha no meio de uma sequência de comandos nativos.

Mudanças compartilhadas precisam repetir os gates no macOS. Registrar resultados por plataforma, incluindo testes opt-in ignorados e limitações. Build frontend não equivale a build nativo; build nativo não equivale a instalação/update nem a UAT.

## 21. Matriz de validação manual Windows

Esta é uma matriz de aceite a executar pelo usuário/testador na máquina Windows, sem automação de navegador adicionada ao Jarvis. Todos os cenários abaixo estão **pendentes de execução Windows** nesta auditoria.

| ID | Cenário | Resultado esperado |
| --- | --- | --- |
| MAN-01 | Instalar em usuário padrão com perfil contendo espaço e acento | App abre sem exigir administrador no uso diário; dados na `.jarvis` correta |
| MAN-02 | Onboarding em perfil limpo | Apresentação → cinco componentes com progresso → provedores/ferramentas → workspace |
| MAN-03 | Tentar avançar com componente ausente/Context7 sem chave | Não libera uso; ações de configuração continuam acessíveis |
| MAN-04 | Interromper download e abrir o app novamente | Estado recuperável; nenhuma geração parcial marcada pronta |
| MAN-05 | Tornar um runtime de teste indisponível | Barra de alerta, bloqueio do chat, Solucionar e diagnóstico do componente correto |
| MAN-06 | Reparar/reinstalar componente | Volta a funcionar sem perder chats, anexos ou Beads |
| MAN-07 | Configurar contas Codex, Antigravity e Custom | Salvar e reconectar após reiniciar; nenhuma credencial em logs legíveis |
| MAN-08 | Login com navegador padrão, callback cancelado/porta ocupada | Conclusão ou erro acionável; listener não permanece aberto |
| MAN-09 | Custom nos três endpoints, raciocínio/contexto | Descoberta/configuração e streaming preservados |
| MAN-10 | Web Search/Vision herdado e dedicado, geração de imagem | Usa configuração correta, skeleton e download funcionam |
| MAN-11 | MCP por `npx`/Node em pasta com espaço | Descoberta automática, ferramenta listada e chamada real funciona |
| MAN-12 | Desativar, reativar e excluir MCP | Configuração preservada ao desativar; exclusão sem erro de segredo ausente |
| MAN-13 | Skills normais, junction/symlink e origem ausente | Listagem atualiza no switch, filtros funcionam e nenhum ciclo trava a UI |
| MAN-14 | Instalar/atualizar/excluir skill do Marketplace | Badges/versão corretas; falha transacional recuperável; link não apaga origem |
| MAN-15 | Digitar `/`, selecionar skill e removê-la | Badge e filtragem preservadas; envio anexa a skill correta |
| MAN-16 | Comando PowerShell com texto Unicode e falha nativa | Saída legível e falha não apresentada como sucesso |
| MAN-17 | Servidor por `npm.cmd` ou Node com descendentes | Badge de processos, saída e Parar funcionam; porta é liberada |
| MAN-18 | Porta já ocupada por servidor externo | Nenhum novo processo iniciado; serviço externo permanece intacto |
| MAN-19 | Cancelar/finalizar Jarvis com filhos ativos | Nenhum processo próprio órfão; terminal do usuário continua aberto |
| MAN-20 | Fluxo Padrão e Designer direto | Comportamento normal; sem subagente duplicado e sem servidor não solicitado |
| MAN-21 | Fluxo Planejado e Completo | Delegação, modelos, checks, perguntas, Beads e validação manual completos |
| MAN-22 | Reprovar task de validação e enviar resultado | Planejador recebe motivo, corrige e publica nova rodada; épico não fecha antes do aceite |
| MAN-23 | Sessão altera arquivo com CRLF, depois commit parcial | Diff mostra só mudanças pendentes da sessão |
| MAN-24 | Histórico grande e marcadores de navegação | Carga inicial limitada, scroll incremental, sem mover composer/duplicar scrollbar |
| MAN-25 | Compactação manual e automática | Registro no histórico, neon/progresso e retomada coerentes |
| MAN-26 | Colar texto/imagem, nome longo, arquivo pelo + | Placeholder/skeleton corretos, sem quebra de layout ou duplicata |
| MAN-27 | Modal sobre modal e confirmação por Enter | Blur/foco na modal ativa; Enter não confirma a camada errada |
| MAN-28 | Titlebar, Alt+F4, duplo clique, minimizar/maximizar | Layout Windows, logo à esquerda, controles à direita e estado correto |
| MAN-29 | Snap, Win+Setas/Win+Z, taskbar | Comportamento validado ou limitação explicitamente documentada |
| MAN-30 | Dois monitores com DPI diferente, desconectar um | Janela e painéis continuam visíveis/operáveis e restauram coerentemente |
| MAN-31 | App em background, concluir chat de outro projeto | Notificação correta conforme sistema; badge/não lido e resposta disponíveis na primeira volta |
| MAN-32 | Duas conversas não lidas; abrir somente uma | Apenas a correspondente é reconhecida como lida; contagem restante preservada |
| MAN-33 | Pergunta pendente, erro final e retry transitório | Notifica intervenção/erro final; não notifica cada tentativa nem cada subagente concluído |
| MAN-34 | Desligar notificações/ativar Não incomodar/testar | App respeita configuração e apresenta diagnóstico sem prometer banner |
| MAN-35 | Clicar em notificação | Abre/restaura Jarvis e conversa correta conforme escopo de ativação implementado |
| MAN-36 | Três modos de impedir repouso | Pedidos nativos criados/liberados no momento correto, inclusive erro/cancelamento |
| MAN-37 | Rede perdida/suspensão/troca de janela durante streaming | Retry e retomada sem duplicar resposta, tool call ou mensagem da fila |
| MAN-38 | Limpar sessões antigas | Confirmação, espaço/estado coerentes e última sessão por projeto preservada |
| MAN-39 | Instalar versão A e atualizar para B | Download/progresso, assinatura, substituição e **reabertura automática comprovada** |
| MAN-40 | Update falha, cancelamento de permissão, arquivo bloqueado | Mensagem útil, nenhuma perda de dados; estado recuperável |
| MAN-41 | Desinstalar/reinstalar | Projetos e `.jarvis` preservados conforme política; identidade/notificações não duplicam |
| MAN-42 | Regressão macOS após as abstrações compartilhadas | Keychain, titlebar, fullscreen, notificações, processos e updater macOS preservados |

Registrar por execução: SHA, versão do Jarvis, Windows/build, arquitetura, WebView2, PowerShell, caminho/perfil de teste, cenário, resultado e evidência sanitizada. Evitar capturas com tokens/credenciais visíveis.

## 22. Comandos para começar na máquina Windows

Os comandos desta seção são um roteiro futuro, **não foram executados nesta auditoria**. Usar PowerShell; paths e versões devem refletir o computador de teste.

### 22.1 Inspecionar o ambiente

```powershell
$PSVersionTable
[System.Runtime.InteropServices.RuntimeInformation]::OSDescription
[System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture
$env:USERPROFILE
Get-Command git, bun, cargo, rustup -ErrorAction SilentlyContinue
Get-Command pwsh.exe, powershell.exe -ErrorAction SilentlyContinue
git --version
bun --version
rustc --version
cargo --version
rustup show active-toolchain
```

`Get-Command` vazio indica dependência ausente ou PATH não atualizado. Abrir novo terminal depois de instalar ferramentas; testar também o app iniciado pelo Menu Iniciar. Os ambientes podem diferir.

Instalar Build Tools/SDK e WebView2 pelos instaladores oficiais indicados nas referências. Depois, se a toolchain ainda não estiver preparada:

```powershell
rustup toolchain install stable --profile minimal --component clippy
if ($LASTEXITCODE -ne 0) { throw 'Falha ao preparar Rust' }
rustup target add x86_64-pc-windows-msvc --toolchain stable
if ($LASTEXITCODE -ne 0) { throw 'Falha ao preparar target Windows' }
```

Confirmar que a toolchain ativa é MSVC antes do build. Não sobrescrever uma versão fixada pelo repositório que venha a ser adicionada durante a implementação.

### 22.2 Preparar o checkout

```powershell
$jarvisWorkspace = Join-Path $env:USERPROFILE 'source\jarvis'
if (-not (Test-Path -LiteralPath $jarvisWorkspace)) {
    git clone https://github.com/paulovnas/jarvis.git $jarvisWorkspace
    if ($LASTEXITCODE -ne 0) { throw 'Falha ao clonar Jarvis' }
}
Set-Location -LiteralPath $jarvisWorkspace
git status --short
git log -1 --format='%h %s'
bun install --frozen-lockfile
if ($LASTEXITCODE -ne 0) { throw 'Falha ao instalar dependências' }
```

Se a pasta já existir, o exemplo apenas entra nela: não faz reset/pull nem sobrescreve trabalho local. Confirmar que o SHA contém as alterações auditadas aqui; o HEAD listado no início sozinho não incluía todo o trabalho pendente.

Os projetos de referência em `docs` são **submódulos Git** registrados em `.gitmodules`; um clone comum não materializa seu conteúdo automaticamente. Confirmar a disponibilidade de `docs/metis` antes de implementar, conforme `AGENTS.md`. No checkout novo, inicializar os commits registrados, sem atualizar para versões arbitrárias dos upstreams:

```powershell
git submodule update --init --recursive
if ($LASTEXITCODE -ne 0) { throw 'Falha ao preparar as referências em docs' }
git submodule status
```

Manter esses fontes somente leitura durante a implementação e não usar `git submodule update --remote` como parte do setup. O caminho versionado do OMP é `docs/omp` (minúsculo); o checkout local macOS também pôde ser acessado como `docs/OMP`. Usar o nome versionado nos scripts, sem depender da insensibilidade a caixa do filesystem. Consultar a seção 24 para os SHAs de referência.

Criar branch de trabalho conforme a política do repositório, por exemplo `codex/windows-support`, após conferir o estado local. Manter acompanhamento com `bd prime`/Beads e implementar as fases, não alterar a branch `main` por conveniência.

### 22.3 Desenvolvimento e bundle local

```powershell
bun run tauri dev
```

Esse comando inicia o ambiente de desenvolvimento; não comprova onboarding/credenciais enquanto as adaptações P0 estiverem pendentes. Depois das mudanças e dos gates, preparar bundle local:

```powershell
bun run tauri build --target x86_64-pc-windows-msvc --bundles nsis -- --locked
if ($LASTEXITCODE -ne 0) { throw 'Build nativo Windows falhou' }
```

Quando uma configuração Windows de distribuição for criada, acrescentar `--config` apontando para **esse arquivo existente**. Não reutilizar automaticamente a receita de `.app`/DMG. Produzir payloads do updater requer também a configuração de artefatos e assinatura da fase W7; o comando acima não publica release nem, sozinho, prepara toda essa cadeia.

Não colar comandos macOS como `open ...app`, `codesign`, `chmod +x` ou `/bin/bash` no PowerShell esperando compatibilidade. O instalador local deve ser aberto pelo Explorer ou por invocação Windows apropriada, após conferir o arquivo gerado.

### 22.4 Diagnóstico sem apagar dados

```powershell
$jarvisDataDirectory = Join-Path $env:USERPROFILE '.jarvis'
Get-Item -LiteralPath $jarvisDataDirectory -ErrorAction SilentlyContinue
Get-ChildItem -LiteralPath $jarvisDataDirectory -Force -ErrorAction SilentlyContinue |
    Select-Object Name, Mode, LastWriteTime
Get-NetTCPConnection -LocalPort 5173 -ErrorAction SilentlyContinue |
    Select-Object LocalAddress, LocalPort, State, OwningProcess
```

`5173` é somente exemplo: consultar a porta real do projeto. Esses comandos não autorizam encerrar o PID encontrado ou apagar `.jarvis`. Não imprimir conteúdo de arquivos de credenciais para diagnosticar sua existência.

Para instalação limpa repetível, usar conta Windows/VM de teste ou implementar um diretório de dados de teste explicitamente suportado. Não assumir que mudar `HOME` sozinho redireciona todos os caminhos do Tauri/DPAPI. Não mover o perfil real com o app ou bancos abertos.

## 23. Inventário dos principais arquivos para implementação

| Área | Arquivos | O que revisar |
| --- | --- | --- |
| Build | [`Cargo.toml`](../src-tauri/Cargo.toml), [`Cargo.lock`](../src-tauri/Cargo.lock), [`build.rs`](../src-tauri/build.rs), [`main.rs`](../src-tauri/src/main.rs) | Targets, dependências específicas, features de Job Object e processo sem console |
| Tauri | [`tauri.conf.json`](../src-tauri/tauri.conf.json), [`capabilities/default.json`](../src-tauri/capabilities/default.json), [`lib.rs`](../src-tauri/src/lib.rs) | Janela, bundle, plugins e permissões mínimas |
| Scripts | [`tauri.ts`](../scripts/tauri.ts), [`macos-signing.ts`](../scripts/macos-signing.ts) | Separação macOS e propagação/encerramento de processos |
| Credenciais | [`openai_codex.rs`](../src-tauri/src/openai_codex.rs), [`mcp/mod.rs`](../src-tauri/src/mcp/mod.rs), [`core/context7.rs`](../src-tauri/src/core/context7.rs) | Dois contratos de cofre, namespaces, tamanho de payload e erros |
| Dados | [`persistence.rs`](../src-tauri/src/persistence.rs), [`library.rs`](../src-tauri/src/library.rs), [`desktop.rs`](../src-tauri/src/desktop.rs) | Home, SQLite, arquivos/locks e preferências |
| Limpeza | [`library/deletion.rs`](../src-tauri/src/library/deletion.rs), [`library/cleanup.rs`](../src-tauri/src/library/cleanup.rs) | Escopo de exclusão, reparse points e arquivos em uso |
| Shell | [`agent/tools.rs`](../src-tauri/src/agent/tools.rs), [`agent/processes.rs`](../src-tauri/src/agent/processes.rs) | PowerShell e cancelamento/árvore |
| Portas | [`agent/processes/ports.rs`](../src-tauri/src/agent/processes/ports.rs) | Bind exclusivo, IPv4/IPv6 e diagnósticos Windows |
| Fluxos | [`workflow/dispatch.rs`](../src-tauri/src/agent/workflow/dispatch.rs), [`workflow/contracts.rs`](../src-tauri/src/agent/workflow/contracts.rs), [`workflow/common.md`](../src-tauri/src/agent/workflow/common.md) | `workflow_check`, nome do shell, capabilities e instruções |
| MCP | [`mcp/executable.rs`](../src-tauri/src/mcp/executable.rs), [`mcp/runtime.rs`](../src-tauri/src/mcp/runtime.rs), [`mcp/config.rs`](../src-tauri/src/mcp/config.rs) | PATH, shims, cwd, ambiente, transporte e cancelamento |
| Core geral | [`core/mod.rs`](../src-tauri/src/core/mod.rs), [`core/install.rs`](../src-tauri/src/core/install.rs), [`core/health.rs`](../src-tauri/src/core/health.rs) | Manifest, assets, plataforma, extração, probes e reparo |
| Context-mode | [`core/context.rs`](../src-tauri/src/core/context.rs), [`core/hooks.rs`](../src-tauri/src/core/hooks.rs), [`context-hook.mjs`](../src-tauri/src/core/context-hook.mjs) | Shell/runtime, FTS5, hooks e estado |
| Beads | [`core/beads.rs`](../src-tauri/src/core/beads.rs), [`beads/process.rs`](../src-tauri/src/core/beads/process.rs), [`beads/dashboard.rs`](../src-tauri/src/core/beads/dashboard.rs) | Embedded, ambiente privado e operações de projeto |
| Recursos de agentes | [`core/ponytail.rs`](../src-tauri/src/core/ponytail.rs), [`core/design.rs`](../src-tauri/src/core/design.rs) | Parsing, índices e scripts/templates portáveis |
| Skills | [`skills/catalog.rs`](../src-tauri/src/skills/catalog.rs), [`skills/store.rs`](../src-tauri/src/skills/store.rs), [`skills/mod.rs`](../src-tauri/src/skills/mod.rs) | Links/junctions, cache, transações e toggle |
| Diff | [`agent/diffs.rs`](../src-tauri/src/agent/diffs.rs), [`diffs/working.rs`](../src-tauri/src/agent/diffs/working.rs), [`ChangedFiles.tsx`](../src/components/layout/ChangedFiles.tsx) | Formato de path Git, CRLF e atribuição por sessão |
| Histórico | [`agent/journal.rs`](../src-tauri/src/agent/journal.rs), [`agent/history.rs`](../src-tauri/src/agent/history.rs), [`workflow/storage.rs`](../src-tauri/src/agent/workflow/storage.rs) | IO, paginação, locks, offsets e retomada |
| Anexos | [`agent/attachments.rs`](../src-tauri/src/agent/attachments.rs), [`SkillInput.tsx`](../src/components/chat/SkillInput.tsx) | Clipboard, diálogo, nomes e codificação |
| Janela/menu | [`TitleBar.tsx`](../src/components/layout/TitleBar.tsx), [`app_menu.rs`](../src-tauri/src/app_menu.rs), [`desktop.rs`](../src-tauri/src/desktop.rs) | Controles por OS, atalhos e DPI |
| Sistema | [`system.rs`](../src-tauri/src/system.rs), [`system/notifications.rs`](../src-tauri/src/system/notifications.rs), [`system/unread.rs`](../src-tauri/src/system/unread.rs) | AUMID, toasts, overlay, foco e repouso |
| Retomada frontend | [`use-chat.ts`](../src/hooks/use-chat.ts), [`use-agent-activity.ts`](../src/hooks/use-agent-activity.ts), [`use-unread-conversations.ts`](../src/hooks/use-unread-conversations.ts), [`desktop-resume.ts`](../src/core/desktop-resume.ts) | Resposta/atividade/não lido ao voltar do background |
| Atualizador | [`updater/mod.rs`](../src-tauri/src/updater/mod.rs), [`updater/relaunch.rs`](../src-tauri/src/updater/relaunch.rs), [`AppUpdate.tsx`](../src/components/layout/AppUpdate.tsx) | Contrato Windows de exit/install/restart e progresso |
| Release | [`release-common.ts`](../scripts/release-common.ts), [`release-plan.ts`](../scripts/release-plan.ts), [`release-ci.ts`](../scripts/release-ci.ts), [`release-macos.yml`](../.github/workflows/release-macos.yml), [`RELEASING.md`](RELEASING.md) | Matriz futura, assinaturas, manifests e publicação sem conflito |

## 24. Referências e decisões aproveitáveis

### 24.1 Base de conhecimento local

Metis, OMP e Context-mode forneceram referências diretas de implementação de plataforma; os contratos do Core e o comportamento de skills foram confrontados com a integração atual do Jarvis. O inventário abaixo também registra os demais checkouts disponíveis para aprofundamento durante a migração. Nenhum deles foi alterado ou copiado textualmente. Os SHAs não são versões necessariamente instaladas pelo Jarvis.

| Referência | SHA local | Pontos relevantes |
| --- | --- | --- |
| `docs/metis` | `f099a8c60ddc` | `src/utils/shell.ts`, `child-process.ts`, `paths.ts`, `tools-manager.ts` |
| `docs/omp` | `5964a0f76492` | `packages/utils/src/procmgr.ts`, resolutores/clipboard, histórico de problemas Windows |
| `docs/context-mode` | `aded72c62372` | `src/runtime.ts`, `executor.ts`, `db-base.ts`, documentação de plataformas |
| `docs/ponytail` | `974d940a1c53` | Contratos de instrução, instalação e configuração |
| `docs/beads` | `c0d8da42de5f` | Instalação Windows, capabilities do build e embedded Dolt |
| `docs/open-design` | `3d0d15fc5503` | Recursos/templates/scripts consumidos pelo pacote de design |
| `docs/skills-manager` | `bb926d070f31` | Referência local de Marketplace/gestão de skills; comportamento Jarvis deve seguir seus próprios contratos |

Aprendizados aplicáveis:

- Metis centraliza shell/processos/caminhos; distingue Bash do WSL legado e usa spawn Windows específico. Seu shell padrão procura Git Bash: isso **não atende automaticamente ao objetivo PowerShell do Jarvis**.
- Metis tem tratamento para pipes herdados por descendentes: não basta esperar o PID principal ou destruir streams arbitrariamente.
- OMP distingue `cmd.exe`, PowerShell e POSIX ao montar argumentos. Não passar `-l -c` para PowerShell.
- O histórico do OMP registra problemas de encerramento que atingiam terminais PowerShell alheios. Isso reforça o requisito de propriedade da árvore de processos.
- Context-mode já trata extensões de scripts, janelas de console, runtimes falsos da Store e arquivos SQLite temporários em uso no Windows. Confirmar a presença dessas correções nos bundles efetivamente distribuídos.

### 24.2 Fontes oficiais

Fontes consultadas em 07/09/2026; revisar novamente ao implementar, especialmente documentação de dependências atualizáveis.

- [Tauri — pré-requisitos Windows: C++ Build Tools, Rust e WebView2](https://v2.tauri.app/start/prerequisites/).
- [Tauri — instaladores Windows, NSIS/MSI e WebView2](https://v2.tauri.app/distribute/windows-installer/).
- [Tauri — updater, assinaturas e opções de instalação](https://v2.tauri.app/plugin/updater/).
- [Tauri — customização de janela e controles](https://v2.tauri.app/learn/window-customization/).
- [Microsoft — Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects).
- [Microsoft — DPAPI / CryptProtectData](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata).
- [Microsoft — CREDENTIALW e limite do blob de credenciais](https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentialw).
- [Microsoft — PowerShell.exe e parâmetros de execução/EncodedCommand](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_powershell_exe?view=powershell-5.1).
- [Microsoft — AppUserModelID](https://learn.microsoft.com/en-us/windows/win32/shell/appids).
- [Microsoft — overlay da taskbar](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-itaskbarlist3-setoverlayicon).
- [Microsoft — limites e suporte a caminhos longos](https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation).
- [Node 22.23.2 — checksums oficiais](https://nodejs.org/dist/v22.23.2/SHASUMS256.txt).
- [Beads — release v1.2.2 consultada](https://github.com/gastownhall/beads/releases/tag/v1.2.2).
- [Dolt — release v2.3.2 consultada](https://github.com/dolthub/dolt/releases/tag/v2.3.2).
- [Bun — release 1.4.2 consultada](https://github.com/oven-sh/bun/releases/tag/bun-v1.4.2).

Além das páginas oficiais, foram inspecionados no cache local os fontes de `process-wrap 9.1.0`, `keepawake 0.6.1` e `tauri-plugin-updater 2.11.0`, especialmente os ramos Windows. São referências da versão auditada: conferir o lockfile antes de aplicar uma recomendação a outra versão.

## 25. Critério de conclusão da migração

O suporte Windows só deve ser apresentado como concluído quando existir evidência conjunta de:

1. Compilação/gates nativos sem erros ou warnings, com cobertura Windows dos casos essenciais.
2. Instalação limpa como usuário padrão e onboarding completo com os cinco componentes.
3. Credenciais persistentes, shell/processos/MCPs e fluxos de trabalho funcionais.
4. Histórico, arquivos, Beads, skills e anexos sem regressão/perda de dados.
5. Acabamento Windows, notificações, badge e retomada de background validados manualmente.
6. Atualização entre duas versões instaladas com reabertura automática comprovada.
7. Regressões macOS verificadas e limitações remanescentes registradas no Beads/documentação.

Esta entrega encerra a **auditoria documental**, não esses critérios de implementação. A documentação foi preparada para que a adaptação possa começar em Windows com prioridades, pontos de código e testes definidos, sem depender de reconstruir o histórico da conversa.
