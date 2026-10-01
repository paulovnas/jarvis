# Jarvis — Integração de Graft + Context Mode

## Contexto

O Jarvis é um coding agent extensível e já possui uma arquitetura baseada em agentes, ferramentas, MCPs e gerenciamento persistente de tarefas.

A proposta deste documento é orientar a análise e a futura integração de duas ferramentas complementares:

- **Graft** — conhecimento estrutural persistente do codebase.
- **Context Mode** — redução e gerenciamento do contexto consumido pelo agente.

A hipótese central desta integração é que **Graft e Context Mode não são concorrentes diretos**.

Eles atuam em camadas diferentes do problema de contexto de um coding agent e podem ser utilizados simultaneamente.

---

# Objetivo

Investigar e implementar uma arquitetura em que o Jarvis consiga utilizar:

```text
Graft
+
Context Mode
```

com responsabilidades claramente separadas.

O objetivo NÃO é simplesmente instalar os dois projetos.

O objetivo é integrá-los ao runtime do Jarvis de maneira que:

1. o agente faça menos exploração redundante do codebase;
2. menos conteúdo irrelevante entre na janela de contexto do modelo;
3. o agente consiga entender relações estruturais entre arquivos e símbolos;
4. outputs grandes de ferramentas não sejam enviados integralmente ao modelo;
5. as ferramentas nativas do Jarvis continuem disponíveis quando forem a opção mais eficiente;
6. as integrações não gerem loops, interceptações duplicadas ou aumento desnecessário de latência.

---

# Repositórios de referência

## Graft

Repositório:

```text
https://github.com/trailhq/Graft
```

Package:

```text
@nanonets/graft
```

O Graft constrói um grafo local do repositório contendo informações estruturais sobre o código.

Entre as ferramentas expostas pelo projeto estão:

```text
graft_find_code
graft_file_api
graft_trace_calls
graft_find_all
graft_repo_map
graft_check_freshness
```

O grafo estrutural é regenerável e mantido localmente.

O projeto utiliza Tree-sitter para análise estrutural e pode opcionalmente utilizar modelos para enriquecimento mais profundo do grafo.

---

## Context Mode

Repositório:

```text
https://github.com/mksglu/context-mode
```

O Context Mode atua principalmente no controle do volume de informação enviado ao modelo.

Ele fornece mecanismos como:

```text
sandboxed execution
FTS5/BM25 search
context indexing
session continuity
large-output filtering
tool routing
```

Seu objetivo principal é evitar que outputs grandes de:

```text
shell
testes
logs
APIs
web
MCPs
documentos
```

sejam enviados integralmente para a janela de contexto do modelo.

---

# Diferença conceitual

A divisão principal deve ser entendida desta forma:

```text
Graft
↓
Qual contexto de código é importante?

Context Mode
↓
Quanto desse contexto deve chegar ao modelo?
```

Outra forma de enxergar:

```text
Graft reduz QUANTAS coisas o agente precisa procurar.

Context Mode reduz QUANTO conteúdo dessas operações
precisa entrar na janela do modelo.
```

Esses mecanismos podem ser complementares.

---

# Responsabilidade do Graft

O Graft deve ser considerado a camada de:

```text
CODEBASE KNOWLEDGE
```

Responsabilidades esperadas:

- descoberta de código;
- localização de símbolos;
- entendimento de APIs internas;
- descoberta de dependências;
- navegação estrutural;
- call graph;
- análise de impacto;
- blast radius;
- visão geral do repositório;
- redução de exploração repetitiva.

Exemplo:

Usuário solicita:

```text
"Invalide todos os refresh tokens quando o usuário trocar a senha."
```

Em vez de o agente executar repetidamente:

```text
grep refresh
read auth-service
grep password
read user-service
grep token
read repository
grep invalidate
...
```

o Graft pode ser utilizado para descobrir diretamente relações como:

```text
UserService.changePassword
        │
        ├── UserRepository
        │
        └── RefreshTokenRepository

AuthService.refresh
        │
        └── RefreshTokenRepository
```

Ferramentas relevantes:

```text
graft_find_code
graft_trace_calls
graft_file_api
graft_repo_map
```

---

# Responsabilidade do Context Mode

O Context Mode deve ser considerado a camada de:

```text
CONTEXT BUDGET / CONTEXT GOVERNANCE
```

Responsabilidades esperadas:

- execução de comandos potencialmente verbosos;
- processamento de logs;
- filtragem de outputs de testes;
- filtragem de builds;
- processamento de respostas de APIs;
- processamento de páginas/documentação;
- indexação de conteúdos grandes;
- busca posterior sobre esses conteúdos;
- continuidade de sessão;
- proteção contra consumo desnecessário da context window.

Exemplo:

O agente executa:

```bash
npm test
```

e recebe 10.000 linhas de saída.

O modelo NÃO deveria receber automaticamente as 10.000 linhas.

Fluxo desejado:

```text
npm test
   ↓
Context Mode
   ↓
sandbox / processamento
   ↓
falhas + contexto relevante
   ↓
LLM
```

---

# Arquitetura conceitual para o Jarvis

A arquitetura inicial sugerida é:

```text
                         Jarvis
                           │
                           ▼
                    Agent / Planner
                           │
              ┌────────────┴────────────┐
              │                         │
              ▼                         ▼
            Graft                 Context Mode
              │                         │
       Codebase Knowledge         Context Budget
              │                         │
       ┌──────┼──────┐          ┌──────┼────────┐
       │      │      │          │      │        │
     symbols calls imports     shell   logs    MCP output
       │      │      │          │      │        │
       └──────┴──────┘          └──────┴────────┘
              │                         │
              └────────────┬────────────┘
                           │
                           ▼
                         Model
```

---

# Relação com Beads

Caso o Jarvis continue utilizando Beads para gerenciamento persistente de trabalho, a separação pode ser definida assim:

| Sistema | Responsabilidade |
|---|---|
| Graft | conhecimento estrutural do codebase |
| Context Mode | gerenciamento da janela de contexto e outputs |
| Beads | estado persistente de tarefas, épicos, decisões e progresso |

Conceitualmente:

```text
                    Jarvis

        ┌─────────────┼─────────────┐
        │             │             │
        ▼             ▼             ▼
      Graft      Context Mode      Beads
        │             │             │
   Code Memory   Context Budget   Work Memory
```

Essas três responsabilidades NÃO devem ser misturadas.

---

# Regra de roteamento proposta

O Jarvis deve possuir uma camada explícita de roteamento de contexto/ferramentas.

Exemplo conceitual:

```yaml
context-routing:

  code_discovery:
    provider: graft

  symbol_lookup:
    provider: graft

  code_dependencies:
    provider: graft

  call_graph:
    provider: graft

  blast_radius:
    provider: graft

  repository_overview:
    provider: graft

  shell_execution:
    provider: context-mode

  test_execution:
    provider: context-mode

  build_execution:
    provider: context-mode

  logs:
    provider: context-mode

  large_output_processing:
    provider: context-mode

  document_indexing:
    provider: context-mode

  direct_file_read:
    provider: native

  direct_file_edit:
    provider: native
```

Isso é apenas uma representação conceitual.

A implementação real deve respeitar a arquitetura atual do Jarvis.

---

# Heurística de decisão

O comportamento desejado pode ser resumido assim:

```text
"Quero saber ONDE algo está"
        ↓
      Graft


"Quero entender COMO partes do código se relacionam"
        ↓
      Graft


"Quero saber QUEM chama ou depende deste símbolo"
        ↓
      Graft


"Quero avaliar impacto de uma alteração"
        ↓
      Graft


"Quero EXECUTAR alguma coisa"
        ↓
 Context Mode


"Essa operação pode produzir output grande"
        ↓
 Context Mode


"Quero pesquisar dentro de um output/documento grande"
        ↓
 Context Mode


"Eu já conheço exatamente o arquivo que preciso ler"
        ↓
 ferramenta nativa


"Eu já sei exatamente o arquivo que preciso alterar"
        ↓
 ferramenta nativa
```

---

# Importante: não transformar tudo em Graft

O Graft NÃO deve substituir indiscriminadamente:

```text
read
grep
glob
edit
write
```

Ferramentas nativas continuam sendo importantes.

Exemplo:

Se o agente já sabe que precisa ler:

```text
src/auth/token-service.ts
```

não existe necessariamente vantagem em executar uma sequência de descoberta através do Graft.

Neste caso:

```text
native read
```

pode ser mais barato e rápido.

O Graft deve entrar principalmente quando existe uma pergunta estrutural.

---

# Importante: não transformar tudo em Context Mode

O mesmo vale para Context Mode.

Não deve existir uma arquitetura semelhante a:

```text
tool
 ↓
Jarvis hook
 ↓
Context Mode
 ↓
outro wrapper
 ↓
Graft
 ↓
outro MCP
 ↓
Model
```

Isso cria:

- latência;
- duplicidade;
- comportamento difícil de depurar;
- aumento do número de tool calls;
- system prompts maiores;
- risco de loops;
- perda de previsibilidade.

O roteamento deve ser explícito.

---

# Fluxo de execução desejado

Exemplo completo:

```text
User
 │
 │ "Invalide refresh tokens após alteração de senha"
 ▼
Planner
 │
 │ precisa entender arquitetura
 ▼
Graft
 │
 ├─ encontra changePassword
 ├─ encontra refresh token service
 ├─ encontra repository
 └─ determina blast radius
 │
 ▼
Planner
 │
 ▼
Native Tools
 │
 ├─ read arquivos específicos
 ├─ edit arquivos
 └─ write arquivos
 │
 ▼
Context Mode
 │
 ├─ executar testes
 ├─ executar lint
 └─ executar build
 │
 ▼
Context Mode filtra output
 │
 ▼
Model recebe apenas resultado relevante
 │
 ▼
Graft
 │
 └─ opcionalmente verificar relações/impacto novamente
 │
 ▼
Final
```

---

# Princípio arquitetural

A integração deve privilegiar:

```text
DISCOVERY → Graft

EXECUTION → Context Mode

PRECISE FILE ACCESS → Native Tools

TASK STATE → Beads
```

---

# Possível abstração no Jarvis

Investigar se faz sentido introduzir uma abstração semelhante a:

```text
ContextProvider
```

Exemplo conceitual:

```ts
interface ContextProvider {
  canHandle(request: ContextRequest): boolean

  retrieve(request: ContextRequest): Promise<ContextResult>
}
```

Possíveis providers:

```text
NativeContextProvider
GraftContextProvider
ContextModeProvider
```

Ou, alternativamente, separar explicitamente:

```text
CodeKnowledgeProvider
ExecutionContextProvider
TaskMemoryProvider
```

Por exemplo:

```ts
interface CodeKnowledgeProvider {
  findCode(query: string): Promise<CodeReference[]>

  getFileApi(path: string): Promise<FileApi>

  traceCalls(symbol: string): Promise<CallGraph>

  getBlastRadius(symbol: string): Promise<ImpactAnalysis>
}
```

E:

```ts
interface ExecutionContextProvider {
  execute(command: string): Promise<ExecutionSummary>

  search(index: string, query: string): Promise<SearchResult[]>

  index(content: ContextContent): Promise<ContextIndex>
}
```

Não implementar essas interfaces literalmente sem antes analisar a arquitetura existente do Jarvis.

Elas representam apenas uma possível separação de responsabilidades.

---

# Estratégia para Graft

O Graft já oferece MCP e integração com agentes.

Antes de implementar qualquer adapter próprio, investigar:

```text
docs/graft
```

e identificar:

1. como o MCP server é inicializado;
2. quais tools são disponibilizadas;
3. schemas de input/output;
4. como o grafo é armazenado;
5. como a freshness é verificada;
6. como o grafo reage a alterações de arquivos;
7. como funcionam os hooks;
8. como funciona a integração através de AGENTS.md;
9. se existe API interna reutilizável;
10. se é melhor integrar via MCP, CLI ou package/API.

Ferramentas MCP documentadas atualmente:

```text
graft_find_code
graft_file_api
graft_trace_calls
graft_find_all
graft_repo_map
graft_check_freshness
```

Configuração MCP de referência:

```json
{
  "mcpServers": {
    "graft": {
      "command": "npx",
      "args": ["-y", "@nanonets/graft", "mcp"]
    }
  }
}
```

Não assumir que essa configuração deve ser usada diretamente pelo Jarvis.

Primeiro analisar como o Jarvis já gerencia MCPs.

---

# Estratégia para Context Mode

Antes de implementar integração própria, analisar o projeto e identificar:

1. ferramentas MCP disponíveis;
2. sandbox execution;
3. indexação via SQLite/FTS5;
4. BM25;
5. gerenciamento de sessão;
6. hooks;
7. adapters existentes;
8. integração com Codex/OpenCode;
9. estratégia de interceptação/routing;
10. quais partes podem ser reutilizadas sem transformar Context Mode em um wrapper global de todas as ferramentas.

O Context Mode deve entrar principalmente para operações cujo retorno potencial seja grande.

---

# Fase 1 — Investigação

Antes de alterar o Jarvis:

## Graft

Analisar:

```text
docs/graft
```

Produzir internamente um mapa contendo:

```text
entrypoints
MCP server
tools
graph builder
tree-sitter integration
storage format
freshness mechanism
hooks
agent integrations
public APIs
```

Identificar claramente:

```text
qual é o menor ponto de integração possível com Jarvis?
```

---

## Context Mode

Localizar o código/projeto de referência disponível para Context Mode e mapear:

```text
MCP entrypoint
execution sandbox
indexing
search
session memory
hooks
routing
adapters
```

Responder:

```text
qual parte realmente precisamos integrar?
```

Evitar copiar o projeto inteiro para dentro da arquitetura do Jarvis se um adapter fino for suficiente.

---

# Fase 2 — Design

Após a investigação, criar um design técnico contendo:

```text
Jarvis Tool Router
       │
       ├── Native
       │
       ├── Graft
       │
       └── Context Mode
```

Definir:

- contratos;
- lifecycle;
- fallbacks;
- erros;
- timeouts;
- disponibilidade;
- configuração;
- feature flags;
- observabilidade;
- telemetria local;
- compatibilidade cross-platform.

O Jarvis é multiplataforma.

Considerar:

```text
macOS
Windows
Linux
```

---

# Fase 3 — Integração incremental

Não implementar tudo de uma vez.

Sugestão:

## Etapa A

Integrar apenas:

```text
graft_repo_map
graft_find_code
graft_file_api
```

Validar:

- qualidade dos resultados;
- latência;
- token usage;
- comportamento com projetos pequenos;
- comportamento com monorepos.

---

## Etapa B

Adicionar:

```text
graft_trace_calls
graft_find_all
graft_check_freshness
```

Validar análise de impacto.

---

## Etapa C

Integrar Context Mode somente em:

```text
test
build
lint
logs
```

Não alterar ainda o comportamento de todas as tools.

---

## Etapa D

Adicionar:

```text
index/search
session continuity
large document processing
```

somente se os benefícios forem comprovados.

---

# Fallback obrigatório

Nenhuma dessas integrações deve impedir o funcionamento normal do Jarvis.

Exemplo:

```text
Graft unavailable
      ↓
native grep/read

Context Mode unavailable
      ↓
native shell execution
```

Portanto:

```text
Graft = optimization layer
Context Mode = optimization layer
```

e NÃO dependências obrigatórias do core.

---

# Feature flags

Investigar suporte a configuração semelhante a:

```yaml
context:
  graft:
    enabled: true

  context_mode:
    enabled: true
```

Ou equivalente à configuração atual do Jarvis.

O usuário deve conseguir executar:

```text
Graft ON / Context Mode ON
Graft ON / Context Mode OFF
Graft OFF / Context Mode ON
Graft OFF / Context Mode OFF
```

Isso também é importante para benchmarks.

---

# Observabilidade

Registrar métricas que permitam responder:

```text
Graft realmente reduziu exploração?

Context Mode realmente reduziu contexto?

A latência aumentou?

Quantas tool calls foram evitadas?
```

Métricas interessantes:

```text
native_tool_calls
graft_tool_calls
context_mode_calls
input_tokens
output_tokens
tool_output_bytes
filtered_output_bytes
context_reduction_ratio
task_duration
fallback_count
```

Não assumir que os números divulgados pelos projetos serão reproduzidos no Jarvis.

Medir no runtime real.

---

# Benchmark sugerido

Criar um conjunto de tarefas reais e executar nos quatro modos:

```text
A — baseline
Graft OFF
Context Mode OFF


B — Graft
Graft ON
Context Mode OFF


C — Context Mode
Graft OFF
Context Mode ON


D — combinado
Graft ON
Context Mode ON
```

Comparar:

```text
tokens
tool calls
tempo
qualidade da solução
arquivos desnecessariamente lidos
outputs desnecessariamente enviados
erros
```

O objetivo não é somente reduzir tokens.

Também verificar se existe melhoria em:

```text
code understanding
impact analysis
accuracy
latency
```

---

# Riscos

## 1. Over-routing

Não enviar operações triviais para serviços desnecessários.

---

## 2. Duplicação de contexto

Não permitir:

```text
Graft retorna código
+
Jarvis lê novamente o arquivo inteiro
+
Context Mode indexa novamente
```

sem necessidade.

---

## 3. Hooks concorrentes

Verificar cuidadosamente hooks instalados por ambos os projetos.

O Jarvis deve continuar sendo o coordenador do lifecycle.

---

## 4. Grafo desatualizado

Antes de depender de relações estruturais importantes, garantir freshness adequada.

Utilizar:

```text
graft_check_freshness
```

quando necessário.

---

## 5. Dependência forte

As integrações devem ser opcionais.

O Jarvis deve continuar funcionando caso:

```text
graft
```

ou:

```text
context-mode
```

não estejam disponíveis.

---

# Resultado arquitetural esperado

Idealmente:

```text
                       ┌────────────────────┐
                       │       User         │
                       └─────────┬──────────┘
                                 │
                                 ▼
                       ┌────────────────────┐
                       │   Jarvis Planner   │
                       └─────────┬──────────┘
                                 │
                 ┌───────────────┼───────────────┐
                 │               │               │
                 ▼               ▼               ▼
              Graft        Native Tools    Context Mode
                 │               │               │
        structural context    precise IO     execution context
                 │               │               │
                 └───────────────┼───────────────┘
                                 │
                                 ▼
                              Model
                                 │
                                 ▼
                               Beads
                          task/work memory
```

---

# Definição final das responsabilidades

## Graft

```text
CODE KNOWLEDGE
```

Perguntas que deve responder:

```text
Onde está?
Quem chama?
Do que depende?
Quem depende disso?
Qual API esse arquivo expõe?
Qual o impacto de mudar isso?
Como esse subsistema se conecta ao restante?
```

---

## Context Mode

```text
CONTEXT GOVERNANCE
```

Perguntas que deve responder:

```text
Esse output precisa entrar inteiro no modelo?
Podemos processar isso fora da context window?
Podemos indexar e recuperar apenas o necessário?
Como manter continuidade após compaction?
```

---

## Native Tools

```text
PRECISE OPERATIONS
```

Responsáveis por:

```text
read conhecido
edit conhecido
write
filesystem
ações pontuais
```

---

## Beads

```text
WORK STATE
```

Responsável por:

```text
tasks
epics
decisions
progress
history
```

---

# Regra final

Usar como princípio:

```text
Graft
    reduz exploração.

Context Mode
    reduz ingestão.

Native Tools
    executam operações precisas.

Beads
    preserva o estado do trabalho.
```

A integração Graft + Context Mode deve ser tratada como uma composição de capacidades e não como uma escolha entre ferramentas concorrentes.

---

# Instrução para o Codex

Antes de escrever código:

1. analisar a arquitetura atual do Jarvis;
2. analisar integralmente os pontos relevantes de `docs/graft`;
3. identificar como MCPs e tools são gerenciados atualmente;
4. identificar hooks e lifecycle existentes;
5. localizar qualquer integração já existente com Context Mode;
6. produzir um design de integração compatível com a arquitetura existente;
7. evitar criar abstrações paralelas às já existentes;
8. evitar modificar o core antes de definir os pontos de extensão;
9. manter Graft e Context Mode opcionais;
10. definir fallbacks para ferramentas nativas;
11. propor benchmarks antes de considerar a integração concluída.

Não implementar baseado apenas neste documento.

Este documento define o objetivo e as responsabilidades desejadas.

A implementação deve ser derivada da arquitetura real do Jarvis e dos projetos de referência.
