# Agent Instructions

This project uses **bd** (beads) for issue tracking. Run `bd prime` for full workflow context.

## Jarvis — Coding Agent GUI (Tauri v2)

Jarvis é uma GUI de coding agent construída com **Tauri v2 (Rust) + React 19 + TypeScript + Tailwind CSS v4 + shadcn/ui**. Esta seção é lei: suas regras se aplicam a TODO trabalho no repositório.

### Fonte de conhecimento: docs/metis (OBRIGATÓRIA)

- `docs/metis` contém o código-fonte completo do **metis**, um coding agent (TUI/desktop). Ele é a **BASE DE CONHECIMENTO obrigatória** do Jarvis.
- ANTES de projetar ou implementar qualquer feature (agent loop, sessões, tools, streaming, config, themes, comandos, etc.), ESTUDE como o metis resolve o mesmo problema em `docs/metis/src` e `docs/metis/docs`.
- NÃO é uma cópia: portar ideias, fluxos e decisões arquiteturais — nunca copiar código textualmente. Melhorias são bem-vindas; partes que não se aplicam ao Jarvis ficam de fora.
- `docs/metis` é somente leitura: NUNCA editar, mover ou deletar arquivos lá dentro.

### Stack e arquitetura

- **Frontend:** React 19 + TS estrito, Vite, Tailwind CSS v4, shadcn/ui. Alias de import `@/` → `src/`.
- **Backend:** comandos Tauri em `src-tauri` (Rust). Toda lógica pesada (agent loop, filesystem, processos, providers de IA) vive no Rust; o frontend é fino.
- Pacotes com **bun** (`bun add`), não npm/pnpm/yarn.

### UI: TUDO é shadcn/ui (OBRIGATÓRIO)

- TODO componente de UI DEVE ser shadcn/ui. Se o componente não estiver instalado, instale com `bunx shadcn@latest add <componente>` — nunca reimplemente à mão o que existe no registry.
- NADA é criado do zero "na unha" (botões, inputs, modais, menus...). Exceção única: o shadcn não oferece o componente; nesse caso, construa-o COMPONdo primitivos shadcn/Radix existentes, dentro de `src/components/`.
- **cursor-pointer obrigatório:** todo componente interativo (button, select, menu item, tab, switch, checkbox, link, card clicável, etc.) DEVE indicar interatividade com cursor pointer. O CSS base (`src/index.css`) já cobre elementos nativos, mas qualquer elemento customizado clicável DEVE receber explicitamente a classe `cursor-pointer`.
- Componentes gerados pelo shadcn (`src/components/ui`, hooks gerados como `use-mobile`) são código de registry: não reestilizar à mão nem "consertar" lint neles — já estão isentos no eslint config.
### Design System: Fonte Roboto + Tema One Dark

- **Tipografia:** Fonte padrão é **Roboto** (`@fontsource/roboto`), importada em `src/index.css`. Tanto `--font-sans` quanto `--font-heading` apontam para Roboto.
- **Cores e Tema:** O tema visual padrão do Jarvis é baseado no **One Dark** (Atom / One Dark Pro):
  - Background principal: `#282c34`
  - Superfícies/Cards: `#21252b`
  - Painéis secundários: `#2c313a`
  - Bordas: `#3e4451`
  - Texto principal: `#abb2bf`
  - Destaques semânticos: Azul `#61afef` (primário/ações), Verde `#98c379` (sucesso/terminal), Ciano `#56b6c2` (info/workspace), Amarelo `#e5c07b` (atenção/chaves), Vermelho `#e06c75` (destrutivo/erro), Roxo `#c678dd`.
- **Feedback visual:** Use o componente Sonner (`toast` / `Toaster`) para notificações de ação do usuário.

### Testes são OBRIGATÓRIOS (não opcionais)

- Toda feature ou bugfix DEIXA teste unitário (vitest + @testing-library/react), colocalizado (`*.test.ts(x)` ao lado da fonte).
- Teste comportamento observável (o que o usuário vê/faz), não detalhe de implementação.
- `bun run test` para rodar; configuração em `vitest.config.ts`.

### Quality gates ao final de TODA implementação (OBRIGATÓRIO)

Ao terminar qualquer implementação, rode nesta ordem e só entregue com **zero erros E zero warnings**:

```bash
bun run lint         # eslint . --max-warnings 0
bun run typecheck    # tsc -p tsconfig.json && tsc -p tsconfig.node.json
bun run test         # vitest run
bun run build        # typecheck + vite build
```

Ou simplesmente `bun run check` (roda os quatro). Se tocou em `src-tauri`, rode também `cargo clippy -- -D warnings` e `cargo test` lá dentro.

### Outras regras

- Sem `any`, sem `@ts-ignore`/`@ts-expect-error` sem comentário justificando. TS strict vale para testes também.
- Componentes funcionais + hooks; estado global só quando houver mais de um consumidor real.
- UI em pt-BR quando houver texto visível ao usuário final; código, comentários e docs em inglês, salvo seção de regras deste arquivo.
- Não commitar nem fazer push sem autorização explícita (ver perfil Beads abaixo).

> **Architecture in one line:** Issues live in a local Dolt database
> (`.beads/dolt/`); cross-machine sync uses `bd dolt push/pull` (a
> git-compatible protocol), stored under `refs/dolt/data` on your git
> remote — separate from `refs/heads/*` where your code lives.
> `.beads/issues.jsonl` is a passive export, not the wire protocol.
>
> See [SYNC_CONCEPTS.md](https://github.com/gastownhall/beads/blob/main/docs/SYNC_CONCEPTS.md)
> for the one-screen overview and anti-patterns (don't treat JSONL as the
> source of truth; don't `bd import` during normal operation; don't
> reach for third-party Dolt hosting before trying the default).

## Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work atomically
bd close <id>         # Complete work
bd dolt push          # Push beads data to remote
```

## Non-Interactive Shell Commands

**ALWAYS use non-interactive flags** with file operations to avoid hanging on confirmation prompts.

Shell commands like `cp`, `mv`, and `rm` may be aliased to include `-i` (interactive) mode on some systems, causing the agent to hang indefinitely waiting for y/n input.

**Use these forms instead:**
```bash
# Force overwrite without prompting
cp -f source dest           # NOT: cp source dest
mv -f source dest           # NOT: mv source dest
rm -f file                  # NOT: rm file

# For recursive operations
rm -rf directory            # NOT: rm -r directory
cp -rf source dest          # NOT: cp -r source dest
```

**Other commands that may prompt:**
- `scp` - use `-o BatchMode=yes` for non-interactive
- `ssh` - use `-o BatchMode=yes` to fail instead of prompting
- `apt-get` - use `-y` flag
- `brew` - use `HOMEBREW_NO_AUTO_UPDATE=1` env var

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:970c3bf2 -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/SYNC_CONCEPTS.md for details and anti-patterns.

## Agent Context Profiles

The managed Beads block is task-tracking guidance, not permission to override repository, user, or orchestrator instructions.

- **Conservative (default)**: Use `bd` for task tracking. Do not run git commits, git pushes, or Dolt remote sync unless explicitly asked. At handoff, report changed files, validation, and suggested next commands.
- **Minimal**: Keep tool instruction files as pointers to `bd prime`; use the same conservative git policy unless active instructions say otherwise.
- **Team-maintainer**: Only when the repository explicitly opts in, agents may close beads, run quality gates, commit, and push as part of session close. A current "do not commit" or "do not push" instruction still wins.

## Session Completion

This protocol applies when ending a Beads implementation workflow. It is subordinate to explicit user, repository, and orchestrator instructions.

1. **File issues for remaining work** - Create beads for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **Handle git/sync by active profile**:
   ```bash
   # Conservative/minimal/default: report status and proposed commands; wait for approval.
   git status

   # Team-maintainer opt-in only, unless current instructions forbid it:
   git pull --rebase
   bd dolt push
   git push
   git status
   ```
5. **Hand off** - Summarize changes, validation, issue status, and any blocked sync/commit/push step

**Critical rules:**
- Explicit user or orchestrator instructions override this Beads block.
- Do not commit or push without clear authority from the active profile or the current user request.
- If a required sync or push is blocked, stop and report the exact command and error.
<!-- END BEADS INTEGRATION -->

<!-- BEGIN BEADS CODEX SETUP: generated by bd setup codex -->
## Beads Issue Tracker

Use Beads (`bd`) for durable task tracking in repositories that include it. Use the `beads` skill at `.agents/skills/beads/SKILL.md` (project install) or `~/.agents/skills/beads/SKILL.md` (global install) for Beads workflow guidance, then use the `bd` CLI for issue operations.

### Quick Reference

```bash
bd ready                # Find available work
bd show <id>            # View issue details
bd update <id> --claim  # Claim work
bd close <id>           # Complete work
bd prime                # Refresh Beads context
```

### Rules

- Use `bd` for all task tracking; do not create markdown TODO lists.
- Run `bd prime` when Beads context is missing or stale. Codex 0.129.0+ can load Beads context automatically through native hooks; use `/hooks` to inspect or toggle them.
- Keep persistent project memory in Beads via `bd remember`; do not create ad hoc memory files.

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/SYNC_CONCEPTS.md for details and anti-patterns.
<!-- END BEADS CODEX SETUP -->
