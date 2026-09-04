import { useEffect, useRef, useState } from "react";
import {
  CheckCircle2,
  MessageSquare,
  RotateCcw,
  Sparkles,
} from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { AssistantMessageTurn } from "./AssistantMessageTurn";
import { ChatComposer } from "./ChatComposer";
import type { ChatMessage } from "./types";
import { UserMessageBubble } from "./UserMessageBubble";

const INITIAL_MESSAGES: ChatMessage[] = [
  {
    id: "msg-1",
    role: "user",
    content:
      "Jarvis, implemente a persistência SQLite e o layout de três colunas para o desktop.",
    timestamp: "14:30",
    attachments: [
      {
        id: "att-1",
        name: "docs/PLAN-FEAT-onboarding-app-shell.md",
        size: "14 KB",
        type: "file",
      },
    ],
  },
  {
    id: "msg-2",
    role: "assistant",
    content:
      "### Implementação concluída com sucesso\n\nConcluí a configuração da persistência no banco SQLite do Jarvis e a arquitetura das três colunas verticais do desktop.\n\n- Arquitetura de dados: persistência em `~/.jarvis/jarvis.db` com Drizzle gerando o schema e rusqlite no motor Rust\n- Layout desktop: sidebar de workspaces/projetos à esquerda, área de chat central e inspector de contexto à direita\n- Controles: redimensionamento horizontal suave mantendo limites mínimos para integridade visual\n\n```rust\n#[tauri::command]\npub async fn get_app_config() -> Result<AppConfig, String> {\n    let conn = get_db_connection()?;\n    read_persisted_config(&conn)\n}\n```\n\nOs quality gates estão prontos para execução no seu ambiente.",
    timestamp: "14:31",
    work: {
      durationSeconds: 14,
      thinking:
        "Analisando a estrutura do projeto Tauri v2. Verificando as migrações geradas pelo Drizzle em `drizzle/` e o runtime Rust com rusqlite em `src-tauri/src/persistence.rs`. Precisamos garantir que a inicialização do banco ocorra em `~/.jarvis/jarvis.db` com o schema da tabela `app_config` e criar as três colunas verticais redimensionáveis.",
      tools: [
        {
          id: "tool-1",
          name: "read_file",
          status: "completed",
          durationMs: 45,
          args: { path: "drizzle/schema.ts" },
          output: "export const appConfig = sqliteTable('app_config', { ... });",
        },
        {
          id: "tool-2",
          name: "bash",
          status: "completed",
          durationMs: 820,
          args: { command: "bun run db:generate" },
          output: "Migration 0000_fluffy_iron_man.sql generated successfully.",
        },
        {
          id: "tool-3",
          name: "edit_file",
          status: "completed",
          durationMs: 310,
          diffStats: { added: 112, removed: 34 },
          args: { path: "src/components/layout/Home.tsx" },
          output: "Layout de 3 colunas atualizado com sucesso.",
        },
        {
          id: "tool-4",
          name: "cargo_check",
          status: "completed",
          durationMs: 1200,
          args: { manifest: "src-tauri/Cargo.toml" },
          output: "Checked 4 packages. 0 warnings.",
        },
      ],
    },
  },
  {
    id: "msg-3",
    role: "user",
    content:
      "Execute a suíte de testes do projeto e verifique a integridade do ambiente.",
    timestamp: "14:33",
  },
  {
    id: "msg-4",
    role: "assistant",
    content:
      "### Relatório dos Testes\n\nTodos os **17 testes de frontend** e os **4 testes unitários de persistência Rust** foram aprovados com sucesso.\n\n- Frontend: `bun run check` limpo (lint, typecheck e build OK)\n- Backend: `cargo clippy` sem warnings e testes de banco aprovados\n- O serviço local foi contingenciado para o provedor em nuvem sem impacto nas respostas.",
    timestamp: "14:34",
    work: {
      durationSeconds: 6,
      thinking:
        "Iniciando a execução das rotinas de teste para validar a suíte completa do TypeScript e os testes unitários do motor Rust.",
      tools: [
        {
          id: "tool-5",
          name: "cargo_test",
          status: "completed",
          durationMs: 950,
          args: { manifest: "src-tauri/Cargo.toml" },
          output: "test result: ok. 4 passed; 0 failed; 0 ignored.",
        },
        {
          id: "tool-6",
          name: "bun_test",
          status: "completed",
          durationMs: 1400,
          args: { suite: "vitest" },
          output: "✓ 17 tests passed across 3 test files.",
        },
        {
          id: "tool-7",
          name: "curl_check",
          status: "error",
          durationMs: 200,
          args: { endpoint: "http://127.0.0.1:11434/api/tags" },
          error: "Falha de conexão: servidor Ollama offline na porta 11434.",
        },
      ],
    },
    error: {
      title: "Servidor local de inteligência artificial offline",
      message:
        "O endpoint local do Ollama não respondeu na porta 11434. O Jarvis ativou automaticamente a contingência via nuvem (Google Gemini 2.5 Pro) para manter a sessão ininterrupta.",
      command: "curl -s http://127.0.0.1:11434/api/tags",
    },
  },
];

export function ChatArea() {
  const [messages, setMessages] = useState<ChatMessage[]>(INITIAL_MESSAGES);
  const [isBusy, setIsBusy] = useState(false);
  const scrollBottomRef = useRef<HTMLDivElement>(null);

  const scrollToBottom = () => {
    if (typeof scrollBottomRef.current?.scrollIntoView === "function") {
      scrollBottomRef.current.scrollIntoView({ behavior: "smooth" });
    }
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages]);

  const handleSendMessage = (content: string) => {
    const userMsgId = `user-${Date.now()}`;
    const now = new Date();
    const timeStr = now.toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
    });
    const newUserMsg: ChatMessage = {
      id: userMsgId,
      role: "user",
      content,
      timestamp: timeStr,
    };

    setMessages((prev) => [...prev, newUserMsg]);
    setIsBusy(true);

    // Simulação do agente pensando e respondendo com ferramentas
    setTimeout(() => {
      const assistantMsgId = `assistant-${Date.now()}`;
      const newAssistantMsg: ChatMessage = {
        id: assistantMsgId,
        role: "assistant",
        content: `Recebi sua solicitação: "${content}".\n\nAnalisei o código e executei as operações necessárias com as ferramentas de inspeção do projeto. Todas as verificações retornaram com sucesso.`,
        timestamp: timeStr,
        work: {
          durationSeconds: 3,
          thinking: `Processando instrução: "${content}". Identificando arquivos relevantes no repositório e confirmando os tipos estritos do TypeScript.`,
          tools: [
            {
              id: `tool-dyn-${Date.now()}`,
              name: "read_file",
              status: "completed",
              durationMs: 80,
              args: { query: content },
              output: "Análise de contexto concluída.",
            },
          ],
        },
      };

      setMessages((prev) => [...prev, newAssistantMsg]);
      setIsBusy(false);
      toast.success("Resposta do Jarvis concluída");
    }, 750);
  };

  const handleResetChat = () => {
    setMessages(INITIAL_MESSAGES);
    toast.info("Histórico redefinido para o estado inicial.");
  };

  return (
    <main
      aria-label="Área do chat"
      className="flex h-full min-h-0 w-full flex-1 flex-col overflow-hidden bg-[#282c34]"
    >
      {/* Header do Chat */}
      <header className="flex h-12 shrink-0 items-center justify-between border-b border-[#3e4451] bg-[#21252b] px-5">
        <div className="flex items-center gap-2.5">
          <span className="flex size-7 items-center justify-center rounded-md bg-[#61afef]/15 text-[#61afef]">
            <MessageSquare className="size-4" />
          </span>
          <div>
            <div className="flex items-center gap-2">
              <h2 className="text-xs font-semibold text-[#e6e6e6]">
                Sessão de Engenharia
              </h2>
              <Badge
                variant="outline"
                className="border-[#98c379]/40 bg-[#98c379]/10 text-[10px] text-[#98c379]"
              >
                <CheckCircle2 className="size-2.5 mr-0.5" />
                Ativa
              </Badge>
            </div>
            <p className="text-[10px] text-[#7f848e]">
              Onboarding & App Shell · branch main
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <Badge
            variant="outline"
            className="hidden border-[#3e4451] text-[10px] text-[#7f848e] md:inline-flex"
          >
            Gemini 2.5 Pro
          </Badge>
          <Button
            type="button"
            variant="ghost"
            size="xs"
            onClick={handleResetChat}
            className="cursor-pointer gap-1 text-[11px] text-[#abb2bf] hover:bg-[#2c313a] hover:text-[#e6e6e6]"
          >
            <RotateCcw className="size-3" />
            <span className="hidden sm:inline">Reiniciar</span>
          </Button>
        </div>
      </header>
      {/* Container relativo com mensagens rolando por trás do composer flutuante */}
      <div className="relative min-h-0 flex-1 overflow-hidden">
        {/* Lista de mensagens scrollável */}
        <div className="h-full w-full overflow-y-auto px-4 py-6 sm:px-6">
          <div className="mx-auto w-full max-w-3xl space-y-2 pb-36">
            {/* Divisor temporal inicial */}
            <div className="my-2 mb-6 flex justify-center">
              <span className="rounded-full border border-[#3e4451]/70 bg-[#21252b] px-3 py-1 text-[10.5px] font-medium text-[#7f848e] shadow-xs">
                Hoje · Sessão iniciada
              </span>
            </div>

            {/* Renderização das mensagens */}
            {messages.map((message) =>
              message.role === "user" ? (
                <UserMessageBubble key={message.id} message={message} />
              ) : (
                <AssistantMessageTurn key={message.id} message={message} />
              )
            )}

            {/* Indicador de processamento caso o agente esteja ocupado */}
            {isBusy && (
              <div className="my-4 flex items-center gap-3 rounded-xl border border-[#61afef]/30 bg-[#61afef]/5 p-4 text-xs text-[#61afef]">
                <Sparkles className="size-4 animate-spin" />
                <span>O Jarvis está raciocinando e executando ferramentas…</span>
              </div>
            )}

            <div ref={scrollBottomRef} />
          </div>
        </div>

        {/* Container transparente onde o card do chat flutua */}
        <div className="pointer-events-none absolute inset-x-0 bottom-0 flex justify-center bg-gradient-to-t from-[#282c34] via-[#282c34]/85 to-transparent px-4 pb-4 pt-8 sm:px-6 sm:pb-5">
          <div className="pointer-events-auto w-full max-w-3xl">
            <ChatComposer onSendMessage={handleSendMessage} disabled={isBusy} />
          </div>
        </div>
      </div>
    </main>
  );
}
