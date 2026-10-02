import { useEffect, useId, useRef } from "react";
import { ArrowUpRight } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Toaster } from "@/components/ui/sonner";
import { companionItemKey, type CompanionItem } from "@/core/companion";
import { LazyChatMarkdown } from "@/components/chat/LazyChatMarkdown";

export function CompanionNotifications({ items, error, onDismiss, onOpen }: {
  items: CompanionItem[];
  error: string | null;
  onDismiss: (item: CompanionItem) => Promise<boolean>;
  onOpen: (item: CompanionItem) => Promise<void>;
}) {
  const toasterId = useId();
  const published = useRef(new Set<string>());
  useEffect(() => {
    const next = new Set<string>();
    const ordered = [...items].sort((a, b) => a.updatedAt - b.updatedAt);
    for (const item of ordered) {
      const id = `${toasterId}/${companionItemKey(item)}/${item.attentionId}`;
      const front = item === ordered[ordered.length - 1];
      next.add(id);
      toast.custom(() => <Card data-state={item.status} aria-hidden={!front} aria-label={`Notificação: ${item.title}`} className="companion-activity-card companion-notification gap-1">
        <p className="truncate text-[10px] text-muted-foreground">{item.projectName}</p>
        <h2 className="truncate text-xs font-semibold">{item.global ? item.status === "failed" ? "Não consegui concluir sua solicitação." : "Sua resposta está pronta." : item.title}</h2>
        <div className="companion-speech-result line-clamp-1 text-[11px] leading-4 text-muted-foreground"><LazyChatMarkdown content={item.status === "failed" ? item.activity || "Não consegui concluir esta solicitação." : item.result || item.activity || "A atividade foi concluída."} /></div>
        {error && <p role="alert" className="line-clamp-1 text-[11px] text-onedark-yellow">{error}</p>}
        <div className="mt-auto flex items-center gap-2"><Button disabled={!front} size="sm" variant="secondary" className="companion-small-action cursor-pointer" onClick={() => { void onOpen(item); }}>Ver atividade<ArrowUpRight className="size-3" /></Button><Button disabled={!front} size="sm" variant="ghost" className="companion-small-action cursor-pointer" onClick={() => { void onDismiss(item); }}>OK</Button></div>
      </Card>, { id, toasterId, duration: Infinity, dismissible: false, className: "companion-notification-toast" });
    }
    for (const id of published.current) if (!next.has(id)) toast.dismiss(id);
    published.current = next;
  }, [items, error, onDismiss, onOpen, toasterId]);
  useEffect(() => () => { for (const id of published.current) toast.dismiss(id); }, []);
  return <div className="companion-notification-stack" role="region" aria-label="Notificações do Jarvito"><Toaster id={toasterId} className="companion-toaster" position="top-center" theme="dark" visibleToasts={3} gap={8} expand={false} offset={0} mobileOffset={0} hotkey={[]} containerAriaLabel="Notificações de atividades" /></div>;
}
