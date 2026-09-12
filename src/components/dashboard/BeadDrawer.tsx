import { useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowUpRight, MessageSquare, Send, X } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Field, FieldLabel } from "@/components/ui/field";
import { Textarea } from "@/components/TextInput";
import { Sheet, SheetContent, SheetHeader, SheetTitle, SheetDescription } from "@/components/ui/sheet";
import { Skeleton } from "@/components/ui/skeleton";
import { Hint } from "@/components/ui/hint";
import { LazyChatMarkdown as ChatMarkdown } from "@/components/chat/LazyChatMarkdown";
import { useDashboardQuery } from "@/hooks/use-dashboard-query";
import { actorName, commentSchema, dashboardError, date, detailSchema, shortId, statusFor, typeName, type BeadDetail } from "@/core/dashboard";

export function BeadDrawer({ projectId, projectName, issueId, onClose, onSelect, onChanged }: { projectId: string; projectName?: string; issueId: string; onClose: () => void; onSelect: (id: string) => void; onChanged: () => Promise<unknown> }) {
  const detail = useDashboardQuery("get_bead_detail", projectId, detailSchema, issueId);
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const [saved, setSaved] = useState<BeadDetail["comments"]>([]);
  const submitting = useRef(false);
  const issue = detail.data?.issue;
  const status = statusFor(issue?.status ?? "open");
  const comments = [...(detail.data?.comments ?? []), ...saved.filter(comment => !detail.data?.comments.some(existing => existing.id === comment.id))];
  async function send() {
    if (submitting.current || !text.trim()) return;
    submitting.current = true; setSending(true); setSendError(null);
    try {
      const comment = commentSchema.parse(await invoke("add_bead_comment", { projectId, issueId, text }));
      setSaved(current => [...current, comment]); setText(""); toast.success("Comentário enviado");
      void detail.refresh(); void onChanged();
    } catch (cause) { setSendError(dashboardError(cause)); }
    finally { submitting.current = false; setSending(false); }
  }
  return <Sheet open onOpenChange={open => { if (!open && !submitting.current) onClose(); }}>
    <SheetContent showCloseButton={false} className="dark gap-0 border-border bg-background data-[side=right]:w-[min(620px,90vw)] data-[side=right]:sm:max-w-[620px]">
      <SheetHeader className="shrink-0 border-b border-border p-6 pr-14"><Hint content={issueId}><SheetDescription className="mb-3 truncate font-mono text-[11px]">{shortId(issueId, projectName)}</SheetDescription></Hint><SheetTitle className="text-xl leading-7">{issue?.title ?? "Detalhes da tarefa"}</SheetTitle>{issue && <div className="mt-3 flex flex-wrap items-center gap-2"><Badge variant="outline" style={{ color: status.color, borderColor: `${status.color}40`, backgroundColor: `${status.color}0a` }}>{status.label}</Badge><Badge variant="outline" className="text-onedark-purple">{typeName(issue.issue_type)}</Badge><Badge variant="secondary" className="font-mono">P{issue.priority}</Badge></div>}</SheetHeader>
      <Button variant="ghost" size="icon-sm" className="absolute top-4 right-4 cursor-pointer" aria-label="Fechar detalhes" disabled={sending} onClick={onClose}><X /></Button>
      <div className="min-h-0 flex-1 overflow-y-auto p-6">
        {detail.error && <div role="alert" className="mb-4 space-y-2 text-xs text-destructive"><p>{detail.error}</p><Button variant="outline" className="cursor-pointer" onClick={() => { void detail.refresh(); }}>Tentar novamente</Button></div>}
        {!issue && !detail.error && <div role="status" aria-label="Carregando tarefa" className="space-y-6"><Skeleton className="h-16" /><Skeleton className="h-36" /><Skeleton className="h-20" /></div>}
        {issue && <div className="space-y-7">
          <dl className="grid grid-cols-2 gap-x-6 gap-y-4 rounded-md border border-border bg-card/50 p-4">{[["Identificador", issue.id], ["Responsável", issue.assignee || "Sem responsável"], ["Criado por", issue.created_by || "—"], ["Criado em", date(issue.created_at, true)], ["Atualizado em", date(issue.updated_at, true)], ...(issue.closed_at ? [["Fechado em", date(issue.closed_at, true)]] : [])].map(([label, value]) => <div key={label} className="min-w-0"><dt className="micro-label mb-1.5 text-muted-foreground">{label}</dt><Hint content={value}><dd className="truncate font-mono text-[11px]">{actorName(value)}</dd></Hint></div>)}</dl>
          {issue.labels.length > 0 && <div className="flex flex-wrap gap-1.5">{issue.labels.map(label => <Badge variant="secondary" key={label}>{label}</Badge>)}</div>}
          {[["Descrição", issue.description], ["Critérios de aceite", issue.acceptance_criteria], ["Design", issue.design], ["Notas de trabalho", issue.notes], ["Conclusão", issue.close_reason]].filter(([, value]) => value.trim()).map(([label, content]) => <section key={label}><h3 className="micro-label mb-3 text-muted-foreground">{label}</h3><div className="bead-prose"><ChatMarkdown content={content} /></div></section>)}
          {issue.parent && <section><h3 className="micro-label mb-2 text-muted-foreground">Épico / tarefa pai</h3><Button variant="outline" className="h-auto max-w-full cursor-pointer py-2 font-mono text-xs" onClick={() => onSelect(issue.parent!)}><span className="truncate">{shortId(issue.parent, projectName)}</span><ArrowUpRight className="size-3" /></Button></section>}
          {([["Dependências", issue.dependencies], ["Relacionadas / subtarefas", issue.dependents]] as const).filter(([, items]) => items.length > 0).map(([label, items]) => <section key={label}><h3 className="micro-label mb-2 text-muted-foreground">{label} · {items.length}</h3><div className="space-y-1">{items.filter(item => item.id).map(item => <Button key={`${item.id}/${item.dependency_type}`} variant="ghost" className="h-auto w-full cursor-pointer justify-start gap-2 border border-border py-3 text-left whitespace-normal" onClick={() => onSelect(item.id)}><span className="size-1.5 shrink-0 rounded-full" style={{ background: statusFor(item.status).color }} /><span className="min-w-0 flex-1"><span className="block text-xs">{item.title || shortId(item.id, projectName)}</span><span className="mt-1 block font-mono text-[10px] text-muted-foreground">{shortId(item.id, projectName)} · {item.dependency_type === "parent-child" ? "Hierarquia" : item.dependency_type === "blocks" ? "Dependência" : item.dependency_type}</span></span><ArrowUpRight className="size-3" /></Button>)}</div></section>)}
          <section className="border-t border-border pt-5"><h3 className="micro-label mb-4 flex items-center gap-2 text-muted-foreground"><MessageSquare className="size-3.5" />Comentários<span className="font-mono">{comments.length}</span></h3><div className="space-y-4">{comments.map(comment => <article key={comment.id} className="rounded-md border border-border bg-card/55 p-4"><div className="mb-3 flex items-center gap-3"><span className="min-w-0 flex-1 truncate text-xs font-medium">{actorName(comment.author)}</span><span className="font-mono text-[10px] text-muted-foreground">{date(comment.created_at, true)}</span></div><div className="bead-prose"><ChatMarkdown content={comment.text} /></div></article>)}{!comments.length && <p className="text-xs text-muted-foreground">Nenhum comentário</p>}</div></section>
        </div>}
      </div>
      {issue && <form className="shrink-0 space-y-3 border-t border-border bg-card/60 p-4" onSubmit={event => { event.preventDefault(); void send(); }}><Field><FieldLabel htmlFor="bead-comment" className="text-xs">Adicionar comentário</FieldLabel><Textarea id="bead-comment" spellCheck autoCorrect="on" autoCapitalize="sentences" value={text} maxLength={10_000} disabled={sending} onChange={event => setText(event.target.value)} placeholder="Escreva um comentário…" className="max-h-36 min-h-20 resize-y text-sm" /></Field>{sendError && <p role="alert" className="text-xs text-destructive">{sendError}</p>}<div className="flex items-center justify-between"><span className="font-mono text-[10px] text-muted-foreground">{text.length > 0 ? `${text.length.toLocaleString("pt-BR")} / 10.000` : ""}</span><Button type="submit" size="sm" className="cursor-pointer" disabled={sending || !text.trim()}><Send className="size-3.5" />{sending ? "Enviando…" : "Comentar"}</Button></div></form>}
    </SheetContent>
  </Sheet>;
}
