import { useState } from "react";
import {
  AlertTriangle,
  Bot,
  Check,
  Copy,
  RotateCcw,
  Sparkles,
} from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { AssistantWorkCollapse } from "./AssistantWorkCollapse";
import type { ChatMessage } from "./types";

interface AssistantMessageTurnProps {
  message: ChatMessage;
}

export function AssistantMessageTurn({ message }: AssistantMessageTurnProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(message.content);
      setCopied(true);
      toast.success("Resposta copiada para a área de transferência");
      setTimeout(() => setCopied(false), 2000);
    } catch {
      toast.error("Não foi possível copiar o texto");
    }
  };

  const handleRetry = () => {
    toast.info("Ação mockada", {
      description: "Tentativa de reexecução acionada.",
    });
  };

  return (
    <div
      data-testid={`assistant-message-${message.id}`}
      className="my-6 flex w-full flex-col gap-3"
    >
      {/* Header do Assistente */}
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2.5">
          <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-[#61afef]/15 text-[#61afef] shadow-xs">
            <Bot className="size-4.5" />
          </span>
          <div className="flex items-center gap-2">
            <span className="text-sm font-semibold text-[#e6e6e6]">Jarvis</span>
            <Badge
              variant="outline"
              className="h-5 border-[#61afef]/30 bg-[#61afef]/5 px-2 font-mono text-[10px] text-[#61afef]"
            >
              Gemini 2.5 Pro
            </Badge>
          </div>
        </div>
        <span className="text-[11px] text-[#7f848e]">{message.timestamp}</span>
      </div>

      {/* Bloco colapsável de trabalho "Trabalhou por {tempo}" */}
      {message.work && (
        <AssistantWorkCollapse
          work={message.work}
          isStreaming={message.streaming}
        />
      )}

      {/* Alerta de erro operacional caso exista */}
      {message.error && (
        <div
          role="alert"
          className="my-3 rounded-xl border border-[#e06c75]/50 bg-[#e06c75]/10 p-4 text-xs text-[#e06c75]"
        >
          <div className="flex items-start justify-between gap-3">
            <div className="flex items-start gap-2.5">
              <AlertTriangle className="mt-0.5 size-4.5 shrink-0 text-[#e06c75]" />
              <div>
                <h4 className="text-[13px] font-semibold text-[#e06c75]">
                  {message.error.title}
                </h4>
                <p className="mt-1.5 leading-relaxed text-[#abb2bf]">
                  {message.error.message}
                </p>
                {message.error.command && (
                  <code className="mt-2.5 block rounded-md bg-[#1e2227] p-2.5 font-mono text-[11px] text-[#e06c75]">
                    $ {message.error.command}
                  </code>
                )}
              </div>
            </div>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={handleRetry}
              className="h-8 shrink-0 cursor-pointer rounded-lg border-[#e06c75]/40 px-3 text-xs text-[#e06c75] transition-colors hover:bg-[#e06c75]/20 hover:text-[#e06c75]"
            >
              <RotateCcw className="size-3 mr-1" />
              Tentar novamente
            </Button>
          </div>
        </div>
      )}

      {/* Conteúdo textual formatado com espaçamento arejado */}
      <div className="rounded-2xl border border-[#3e4451]/70 bg-[#21252b]/80 p-5 sm:p-6 text-[14.5px] leading-relaxed text-[#abb2bf] shadow-sm shadow-black/10">
        <div className="space-y-4">
          {message.content.split("\n\n").map((paragraph, index) => {
            if (paragraph.startsWith("```")) {
              const lines = paragraph.split("\n");
              const language = lines[0].replace("```", "").trim() || "código";
              const codeBody = lines.slice(1, -1).join("\n");

              return (
                <div
                  key={index}
                  className="my-4 overflow-hidden rounded-xl border border-[#3e4451] bg-[#1e2227]"
                >
                  <div className="flex items-center justify-between border-b border-[#3e4451] bg-[#181a1f] px-4 py-2 text-xs">
                    <span className="font-mono text-[#7f848e]">{language}</span>
                    <button
                      type="button"
                      onClick={() => {
                        void navigator.clipboard.writeText(codeBody);
                        toast.success("Código copiado!");
                      }}
                      className="flex cursor-pointer items-center gap-1.5 text-xs text-[#abb2bf] hover:text-[#e6e6e6]"
                    >
                      <Copy className="size-3" />
                      Copiar
                    </button>
                  </div>
                  <pre className="overflow-x-auto p-4 font-mono text-[12.5px] leading-relaxed text-[#98c379]">
                    <code>{codeBody}</code>
                  </pre>
                </div>
              );
            }

            if (paragraph.startsWith("### ")) {
              return (
                <h3
                  key={index}
                  className="mt-2 font-heading text-base font-semibold text-[#e6e6e6]"
                >
                  {paragraph.replace("### ", "")}
                </h3>
              );
            }

            if (paragraph.startsWith("- ")) {
              const items = paragraph.split("\n");
              return (
                <ul key={index} className="list-inside list-disc space-y-1 pl-1">
                  {items.map((item, itemIdx) => (
                    <li key={itemIdx} className="text-[#abb2bf]">
                      {item.replace(/^- /, "")}
                    </li>
                  ))}
                </ul>
              );
            }

            return (
              <p key={index} className="text-[#abb2bf] break-words">
                {paragraph}
              </p>
            );
          })}
        </div>

        {/* Rodapé da resposta com ações */}
        <div className="mt-5 flex items-center justify-between border-t border-[#3e4451]/50 pt-3 text-xs text-[#7f848e]">
          <div className="flex items-center gap-2">
            <span className="flex items-center gap-1.5 text-[11px]">
              <Sparkles className="size-3.5 text-[#56b6c2]" />
              Resposta verificada
            </span>
          </div>
          <Button
            type="button"
            variant="ghost"
            size="xs"
            onClick={handleCopy}
            className="cursor-pointer text-[#abb2bf] hover:bg-[#2c313a] hover:text-[#e6e6e6]"
          >
            {copied ? (
              <>
                <Check className="size-3 mr-1 text-[#98c379]" />
                Copiado
              </>
            ) : (
              <>
                <Copy className="size-3 mr-1" />
                Copiar
              </>
            )}
          </Button>
        </div>
      </div>
    </div>
  );
}
