import { Folder, ListChecks, Files, Users } from "lucide-react";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import type { LibrarySnapshot } from "@/core/library";
import type { ChatSnapshot } from "@/core/chat";

export function Inspector({ library, chat }: { library: LibrarySnapshot | null; chat?: ChatSnapshot | null }) {
  const selectedChat = chat?.conversationId === library?.selection.conversationId ? chat : null;
  const turns = selectedChat?.turns ?? [];
  const last = turns[turns.length - 1];
  const tools = turns.flatMap(turn => turn.steps.flatMap(step => step.tools));
  const files = [...new Set(tools.filter(tool => tool.status === "completed" && ["write", "edit"].includes(tool.name)).map(tool => tool.args.path).filter((path): path is string => typeof path === "string"))];
  const lastPlan = [...turns].reverse().find(turn => turn.options.mode === "plan" && turn.status === "completed");
  const usage = last?.steps.reduce((total, step) => ({ input: total.input + (step.usage?.inputTokens ?? 0), output: total.output + (step.usage?.outputTokens ?? 0) }), { input: 0, output: 0 });
  const workspace = library?.workspaces.find(
    (item) => item.id === library.selection.workspaceId,
  );
  const project = library?.projects.find(
    (item) => item.id === library.selection.projectId,
  );
  const conversation = library?.conversations.find(
    (item) => item.id === library.selection.conversationId,
  );
  return (
    <aside
      aria-label="Inspector"
      className="flex h-full min-h-0 flex-col border-l border-border bg-card"
    >
      <header className="border-b border-border px-4 py-4">
        <h2 className="text-sm font-semibold">Contexto</h2>
      </header>
      <ScrollArea className="min-h-0 flex-1">
        <div className="space-y-5 p-4">
          <section aria-label="Seleção atual">
            <h3 className="mb-3 flex items-center gap-2 text-sm font-medium">
              <Folder className="size-4 text-primary" />
              Projeto selecionado
            </h3>
            {workspace ? (
              <dl className="space-y-3 text-xs">
                <div>
                  <dt className="text-muted-foreground">Workspace</dt>
                  <dd className="mt-1 break-words">{workspace.name}</dd>
                </div>
                {project && (
                  <>
                    <div>
                      <dt className="text-muted-foreground">Projeto</dt>
                      <dd className="mt-1 break-words">{project.name}</dd>
                    </div>
                    <div>
                      <dt className="text-muted-foreground">Pasta</dt>
                      <dd className="mt-1 break-all font-mono">
                        {project.path}
                      </dd>
                    </div>
                  </>
                )}
                {conversation && (
                  <div>
                    <dt className="text-muted-foreground">Conversa</dt>
                    <dd className="mt-1 break-words">{conversation.title}</dd>
                  </div>
                )}
              </dl>
            ) : (
              <p className="text-xs text-muted-foreground">
                Nenhum workspace selecionado.
              </p>
            )}
          </section>
          <Separator />
          {last && <section aria-label="Execução atual" className="space-y-2 text-xs">
            <h3 className="text-sm font-medium">Execução</h3>
            <p>{selectedChat?.pendingApproval ? "Aguardando autorização" : ({ running: "Em andamento", completed: "Concluída", cancelled: "Interrompida", interrupted: "Interrompida ao encerrar", error: "Falhou" })[last.status]}</p>
            <p className="break-all text-muted-foreground">{last.options.account} / {last.options.model}</p>
            <p>{last.options.mode === "plan" ? "Plan · Somente leitura" : `Build · ${last.options.approvalMode === "manual" ? "Manual" : "YOLO"}`}</p>
            {last.options.reasoning && <p>Raciocínio: {last.options.reasoning}</p>}
            {last.status !== "running" && <p>Duração: {(last.durationMs / 1000).toFixed(1)}s</p>}
            {usage && last.steps.some(step => step.usage) && <p>Tokens informados: {usage.input.toLocaleString("pt-BR")} entrada · {usage.output.toLocaleString("pt-BR")} saída</p>}
          </section>}
          <section>
            <h3 className="mb-2 flex items-center gap-2 text-sm font-medium">
              <ListChecks className="size-4" />
              Plano
            </h3>
            <p className="text-xs text-muted-foreground">
              {lastPlan ? "A última resposta no modo Plan está disponível no histórico da conversa." : "Nenhum plano nesta conversa."}
            </p>
          </section>
          <section>
            <h3 className="mb-2 flex items-center gap-2 text-sm font-medium">
              <Files className="size-4" />
              Arquivos alterados
            </h3>
            {files.length ? <ul className="space-y-1 text-xs">{files.map(path => <li key={path} className="break-all font-mono">{path}</li>)}</ul> : <p className="text-xs text-muted-foreground">Nenhuma alteração registrada.</p>}
            {tools.some(tool => tool.name === "bash") && <p className="mt-2 text-xs text-muted-foreground">Comandos podem ter alterado outros arquivos. Consulte suas saídas no histórico.</p>}
          </section>
          <section>
            <h3 className="mb-2 flex items-center gap-2 text-sm font-medium">
              <Users className="size-4" />
              Subagentes
            </h3>
            <p className="text-xs text-muted-foreground">
              Nenhum subagente em execução.
            </p>
          </section>
        </div>
      </ScrollArea>
    </aside>
  );
}
