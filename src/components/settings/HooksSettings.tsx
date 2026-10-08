import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AlertTriangle, Anchor, ChevronDown, LockKeyhole, Pencil, Plus, RefreshCw, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { HOOK_EVENTS, HOOK_EVENT_LABELS, hookCatalogSchema, hookError, hookSchema, type Hook, type HookCatalog, type NativeHook } from "@/core/hooks";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Switch } from "@/components/ui/switch";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { AlertDialog, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { ConfirmationDialogContent } from "@/components/ConfirmationDialogContent";
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input, Textarea } from "@/components/TextInput";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { CardsSkeleton } from "@/components/layout/LoadingSkeletons";
import { Spinner } from "@/components/ui/spinner";

type EditingHook = { hook: Hook; revision: number; creating: boolean };

export function HooksSettings({ onBusyChange }: { onBusyChange?: (busy: boolean) => void }) {
  const [catalog, setCatalog] = useState<HookCatalog | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [editor, setEditor] = useState<EditingHook | null>(null);
  const [editorError, setEditorError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<{ hook: Hook; revision: number } | null>(null);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const mounted = useRef(false);
  const pending = useRef(false);
  const request = useRef(0);
  const apply = useCallback((value: unknown) => {
    const next = hookCatalogSchema.parse(value);
    if (mounted.current) setCatalog(current => current && current.revision > next.revision ? current : next);
  }, []);
  const refresh = useCallback(async () => {
    const version = ++request.current;
    try { const value: unknown = await invoke("list_hooks"); if (mounted.current && version === request.current) { apply(value); setError(null); } }
    catch { if (mounted.current && version === request.current) setError("Não foi possível carregar os hooks."); }
    finally { if (mounted.current && version === request.current) setLoading(false); }
  }, [apply]);
  useEffect(() => {
    let active = true;
    mounted.current = true;
    void Promise.resolve().then(() => { if (active) void refresh(); });
    let stop: (() => void) | undefined;
    void listen("hooks:changed", () => { if (active) void refresh(); }).then(unlisten => { if (active) stop = unlisten; else unlisten(); }).catch(() => {});
    return () => { active = false; mounted.current = false; request.current += 1; stop?.(); };
  }, [refresh]);
  useEffect(() => { onBusyChange?.(busy); return () => onBusyChange?.(false); }, [busy, onBusyChange]);

  async function mutate(command: "save_hook" | "delete_hook", args: { hook: Hook; expectedRevision: number } | { id: string; expectedRevision: number }, done: () => void, failed: (message: string) => void) {
    if (pending.current) return;
    pending.current = true; request.current += 1; setBusy(true);
    try { apply(await invoke(command, args)); if (mounted.current) { setError(null); done(); } }
    catch (cause) { if (mounted.current) failed(hookError(cause)); }
    finally { pending.current = false; if (mounted.current) setBusy(false); }
  }
  function closeEditor() { if (!pending.current) { setEditor(null); setEditorError(null); } }
  function edit(hook: Hook, creating = false) { if (!catalog || pending.current) return; setEditorError(null); setEditor({ hook, revision: catalog.revision, creating }); }
  function save(hook: Hook) {
    if (!editor) return;
    void mutate("save_hook", { hook, expectedRevision: editor.revision }, () => { setEditor(null); setEditorError(null); toast.success("Hook salvo"); }, setEditorError);
  }
  const allEvents = [...HOOK_EVENTS, "BeforeAgent"] as const;

  return <section aria-label="Hooks" className="flex min-w-0 flex-col gap-5">
    <div className="flex flex-wrap items-start justify-between gap-3">
      <p className="max-w-2xl text-sm text-muted-foreground">Hooks executam comandos locais nos eventos da conversa, em todos os projetos. O comando recebe um JSON pela entrada padrão. Hooks nativos do Jarvis são somente leitura.</p>
      <div className="flex shrink-0 gap-2">
        <Button variant="ghost" size="icon-sm" aria-label="Atualizar hooks" className="cursor-pointer" disabled={busy || loading} onClick={() => { setLoading(true); void refresh(); }}><RefreshCw /></Button>
        <Button size="sm" className="cursor-pointer" disabled={!catalog || busy || error !== null} onClick={() => edit({ id: crypto.randomUUID().replace(/-/g, ""), name: "", event: "PreToolUse", command: "", matcher: "", timeoutSeconds: 600, enabled: true }, true)}><Plus data-icon="inline-start" />Adicionar hook</Button>
      </div>
    </div>
    {loading ? <CardsSkeleton label="Carregando hooks" /> : error ? <p role="alert" className="text-sm text-destructive">{error}</p> : catalog && <>
      {catalog.hooks.length === 0 && <Empty className="border"><EmptyHeader><EmptyTitle>Nenhum hook manual</EmptyTitle><EmptyDescription>Adicione um comando para personalizar os eventos da conversa.</EmptyDescription></EmptyHeader></Empty>}
      {allEvents.map(event => {
        const manual = catalog.hooks.filter(hook => hook.event === event);
        const native = catalog.nativeHooks.filter(hook => hook.event === event);
        if (!manual.length && !native.length) return null;
        const metadata = HOOK_EVENT_LABELS[event];
        return <section key={event} aria-label={metadata.label} className="flex min-w-0 flex-col gap-3">
          <div className="flex items-start gap-3"><Anchor aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-primary" /><div className="flex min-w-0 flex-col gap-1"><h3 className="text-sm font-medium">{metadata.label}</h3><p className="text-xs text-muted-foreground">{metadata.description}</p></div></div>
          <div className="flex flex-col gap-2">
            {native.map(hook => <HookRow key={hook.id} hook={hook} native busy={busy} />)}
            {manual.map(hook => <HookRow key={hook.id} hook={hook} busy={busy} untrusted={catalog.untrustedIds.includes(hook.id)} onEdit={() => edit(hook)} onDelete={() => { setDeleteError(null); setDeleting({ hook, revision: catalog.revision }); }} onToggle={enabled => { if (catalog.untrustedIds.includes(hook.id)) { edit({ ...hook, enabled: true }); return; } void mutate("save_hook", { hook: { ...hook, enabled }, expectedRevision: catalog.revision }, () => toast.success(enabled ? "Hook ativado" : "Hook desativado"), message => toast.error(message)); }} />)}
          </div>
        </section>;
      })}
    </>}
    <Dialog open={editor !== null} onOpenChange={open => { if (!open) closeEditor(); }}>
      <DialogContent className="dark flex max-h-[85dvh] flex-col overflow-hidden sm:max-w-2xl">
        <DialogHeader className="shrink-0 pr-6"><DialogTitle>{editor?.creating ? "Adicionar hook" : "Editar hook"}</DialogTitle><DialogDescription>O comando será executado no computador, na pasta do projeto. Use apenas comandos em que você confia.</DialogDescription></DialogHeader>
        {editor && <HookEditor key={editor.hook.id} hook={editor.hook} busy={busy} error={editorError} onSave={save} onCancel={closeEditor} />}
      </DialogContent>
    </Dialog>
    <AlertDialog open={deleting !== null} onOpenChange={open => { if (!open && !pending.current) setDeleting(null); }}>
      <ConfirmationDialogContent className="dark"><AlertDialogHeader><AlertDialogTitle>Remover hook {deleting?.hook.name}?</AlertDialogTitle><AlertDialogDescription>O comando deixará de ser executado. Para manter a configuração, desative o hook.</AlertDialogDescription></AlertDialogHeader>
        {deleteError && <p role="alert" className="text-sm text-destructive">{deleteError}</p>}
        <AlertDialogFooter><Button variant="ghost" disabled={busy} className="cursor-pointer" onClick={() => setDeleting(null)}>Cancelar</Button><Button variant="destructive" disabled={busy} className="cursor-pointer" onClick={() => { if (deleting) void mutate("delete_hook", { id: deleting.hook.id, expectedRevision: deleting.revision }, () => { setDeleting(null); toast.success("Hook removido"); }, setDeleteError); }}>{busy && <Spinner data-icon="inline-start" />}Remover hook</Button></AlertDialogFooter>
      </ConfirmationDialogContent>
    </AlertDialog>
  </section>;
}

function HookRow({ hook, native = false, untrusted = false, busy, onEdit, onDelete, onToggle }: { hook: Hook | NativeHook; native?: boolean; untrusted?: boolean; busy: boolean; onEdit?: () => void; onDelete?: () => void; onToggle?: (enabled: boolean) => void }) {
  return <Collapsible render={<Card size="sm" />} className="gap-0 py-0">
    <CardHeader className="flex min-w-0 flex-row items-center gap-2 py-3">
      <CardTitle className="min-w-0 flex-1 break-words">{hook.name}</CardTitle>
      {native ? <Badge variant="outline" className="shrink-0 gap-1"><LockKeyhole aria-hidden="true" className="size-3" />Somente leitura</Badge> : <>
        {untrusted && <Badge variant="outline" className="shrink-0 gap-1"><AlertTriangle aria-hidden="true" className="size-3" />Revisão necessária</Badge>}
        <Button variant="ghost" size="icon-sm" className="cursor-pointer shrink-0" aria-label={`Editar hook ${hook.name}`} disabled={busy} onClick={onEdit}><Pencil /></Button>
        <Switch className="cursor-pointer shrink-0" aria-label={`${untrusted ? "Revisar e ativar" : "Ativar"} hook ${hook.name}`} checked={"enabled" in hook && hook.enabled && !untrusted} disabled={busy} onCheckedChange={onToggle} />
      </>}
      <CollapsibleTrigger render={<Button variant="ghost" size="icon-sm" />} aria-label={`Detalhes do hook ${hook.name}`} className="group cursor-pointer shrink-0"><ChevronDown className="transition-transform group-aria-expanded:rotate-180 motion-reduce:transition-none" /></CollapsibleTrigger>
    </CardHeader>
    <CollapsibleContent><CardContent className="flex min-w-0 flex-col gap-3 border-t py-4">
      {untrusted && <p className="text-xs text-muted-foreground">Este hook foi alterado fora do Jarvis e não será executado. Revise o comando antes de salvar ou ativar novamente.</p>}
      {"description" in hook && <p className="text-xs text-muted-foreground">{hook.description}</p>}
      <dl className="grid min-w-0 gap-x-4 gap-y-2 text-xs sm:grid-cols-[9rem_minmax(0,1fr)]">
        <dt className="text-muted-foreground">Tipo de hook</dt><dd>{HOOK_EVENT_LABELS[hook.event].label}</dd>
        <dt className="text-muted-foreground">Comando</dt><dd className="font-mono whitespace-pre-wrap wrap-anywhere">{hook.command ?? "Interno do Jarvis"}</dd>
        <dt className="text-muted-foreground">Matcher</dt><dd className="font-mono wrap-anywhere">{hook.matcher || "Todos"}</dd>
        <dt className="text-muted-foreground">Tempo limite</dt><dd className="font-mono">{hook.timeoutSeconds === null ? "Interno do Jarvis" : `${hook.timeoutSeconds} s`}</dd>
      </dl>
      {!native && <Button variant="ghost" size="sm" className="cursor-pointer self-end text-destructive" aria-label={`Remover hook ${hook.name}`} disabled={busy} onClick={onDelete}><Trash2 data-icon="inline-start" />Remover</Button>}
    </CardContent></CollapsibleContent>
  </Collapsible>;
}

function HookEditor({ hook, busy, error, onSave, onCancel }: { hook: Hook; busy: boolean; error: string | null; onSave: (hook: Hook) => void; onCancel: () => void }) {
  const [draft, setDraft] = useState(hook);
  const [invalid, setInvalid] = useState<string | null>(null);
  function update(patch: Partial<Hook>) { setDraft(value => ({ ...value, ...patch })); setInvalid(null); }
  function submit() {
    const parsed = hookSchema.safeParse(draft);
    if (!parsed.success) { setInvalid("Confira nome (até 160 bytes), comando (até 16 KB), matcher (até 4 KB) e tempo limite (1 a 600 segundos)."); return; }
    onSave(parsed.data);
  }
  return <form className="flex min-h-0 flex-col gap-4" noValidate onSubmit={event => { event.preventDefault(); submit(); }}>
    <FieldGroup className="min-h-0 gap-4 overflow-y-auto px-1 pb-1">
      <Field data-invalid={invalid !== null}><FieldLabel htmlFor="hook-name">Nome do hook</FieldLabel><Input id="hook-name" value={draft.name} onChange={event => update({ name: event.target.value })} disabled={busy} maxLength={160} autoComplete="off" aria-invalid={invalid !== null && !draft.name.trim()} /></Field>
      <Field><FieldLabel htmlFor="hook-event">Evento</FieldLabel><Select value={draft.event} onValueChange={event => { const parsed = hookSchema.shape.event.safeParse(event); if (parsed.success) update({ event: parsed.data }); }} disabled={busy}><SelectTrigger id="hook-event" className="w-full cursor-pointer"><SelectValue>{HOOK_EVENT_LABELS[draft.event].label}</SelectValue></SelectTrigger><SelectContent><SelectGroup>{HOOK_EVENTS.map(event => <SelectItem key={event} value={event} className="cursor-pointer">{HOOK_EVENT_LABELS[event].label}</SelectItem>)}</SelectGroup></SelectContent></Select><FieldDescription>{HOOK_EVENT_LABELS[draft.event].description}</FieldDescription></Field>
      <Field data-invalid={invalid !== null}><FieldLabel htmlFor="hook-command">Comando</FieldLabel><Textarea id="hook-command" className="min-h-24 font-mono" value={draft.command} onChange={event => update({ command: event.target.value })} maxLength={16_384} disabled={busy} autoComplete="off" aria-invalid={invalid !== null && !draft.command.trim()} /><FieldDescription>Comando de shell. Leia o JSON recebido pela entrada padrão; não inclua senhas ou tokens no comando.</FieldDescription></Field>
      <Field><FieldLabel htmlFor="hook-matcher">Matcher (opcional)</FieldLabel><Input id="hook-matcher" className="font-mono" value={draft.matcher} onChange={event => update({ matcher: event.target.value })} maxLength={4096} disabled={busy} autoComplete="off" placeholder="ex.: bash|write|edit" /><FieldDescription>Expressão regular para nomes de ferramentas. Use | para alternativas ou deixe vazio para todas. Aplicado aos eventos de ferramentas.</FieldDescription></Field>
      <Field data-invalid={invalid !== null}><FieldLabel htmlFor="hook-timeout">Tempo limite (segundos)</FieldLabel><Input id="hook-timeout" type="number" min={1} max={600} step={1} value={draft.timeoutSeconds} onChange={event => update({ timeoutSeconds: Number(event.target.value) })} disabled={busy} aria-invalid={invalid !== null && (draft.timeoutSeconds < 1 || draft.timeoutSeconds > 600)} /></Field>
      <Field orientation="horizontal"><FieldLabel htmlFor="hook-enabled" className="cursor-pointer">Ativar hook</FieldLabel><Switch id="hook-enabled" className="cursor-pointer" checked={draft.enabled} onCheckedChange={enabled => update({ enabled })} disabled={busy} /></Field>
      {(invalid ?? error) && <p role="alert" className="text-sm text-destructive">{invalid ?? error}</p>}
    </FieldGroup>
    <DialogFooter className="shrink-0"><Button type="button" variant="ghost" className="cursor-pointer" disabled={busy} onClick={onCancel}>Cancelar</Button><Button type="submit" className="cursor-pointer" disabled={busy}>{busy && <Spinner data-icon="inline-start" />}Salvar hook</Button></DialogFooter>
  </form>;
}
