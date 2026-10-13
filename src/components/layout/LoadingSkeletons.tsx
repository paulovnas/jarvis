import { Skeleton } from "@/components/ui/skeleton";
import { Card } from "@/components/ui/card";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";

function Lines({ count = 3 }: { count?: number }) {
  return <div aria-hidden="true" className="flex flex-col gap-2.5">{Array.from({ length: count }, (_, i) => <Skeleton key={i} className={`h-3 ${i === count - 1 ? "w-2/3" : "w-full"}`} />)}</div>;
}

export function CardsSkeleton({ label, columns = false }: { label: string; columns?: boolean }) {
  return <div role="status" aria-label={label} className={columns ? "settings-card-grid" : "grid grid-cols-1 gap-3"}>
    {[0, 1, 2, 3].slice(0, columns ? 4 : 2).map(i => <Card key={i} aria-hidden="true" className="gap-3 p-4"><div className="flex items-center gap-3"><Skeleton className="size-5 shrink-0" /><Skeleton className="h-4 w-1/2" /><Skeleton className="ml-auto h-4 w-7 rounded-full" /></div><Lines count={2} /><div className="flex gap-2"><Skeleton className="h-4 w-14 rounded-full" /><Skeleton className="h-4 w-16 rounded-full" /></div></Card>)}
  </div>;
}

export function RowsSkeleton({ label, count }: { label: string; count: number }) {
  return <div role="status" aria-label={label} className="divide-y divide-border">
    {Array.from({ length: count }, (_, i) => <div key={i} aria-hidden="true" className="flex items-center gap-3 px-4 py-3"><Skeleton className="size-4 shrink-0" /><div className="flex-1 space-y-2"><Skeleton className="h-4 w-28" /><Skeleton className="h-3 w-3/4" /></div><Skeleton className="h-6 w-16" /></div>)}
  </div>;
}

export function DocumentSkeleton({ label = "Carregando conteúdo" }: { label?: string }) {
  return <div role="status" aria-label={label} className="flex min-h-64 flex-col gap-6 py-3"><Skeleton className="h-5 w-2/5" /><Lines count={4} /><Skeleton className="h-4 w-1/3" /><Lines count={5} /><Skeleton className="h-16 w-full rounded-lg" /></div>;
}

export function DiffSkeleton() {
  return <div role="status" aria-label="Carregando alterações" className="flex flex-col gap-2 p-4">{Array.from({ length: 12 }, (_, i) => <div key={i} aria-hidden="true" className="flex gap-4"><Skeleton className="h-3 w-6 shrink-0" /><Skeleton className="h-3 w-6 shrink-0" /><Skeleton className={`h-3 ${i % 3 === 0 ? "w-1/3" : i % 3 === 1 ? "w-2/3" : "w-1/2"}`} /></div>)}</div>;
}

export function SidebarSkeleton() {
  return <div role="status" aria-label="Carregando projetos" className="flex flex-col gap-5 px-3 py-4">{[0, 1, 2].map(i => <div key={i} aria-hidden="true" className="flex flex-col gap-3"><div className="flex gap-2"><Skeleton className="size-4" /><Skeleton className="h-4 w-2/3" /></div><div className="pl-6"><Lines count={2} /></div></div>)}</div>;
}

export function ConversationSkeleton() {
  return <div role="status" aria-label="Abrindo conversa" className="flex h-full min-h-0 w-full flex-1 flex-col">
    <div aria-hidden="true" className="flex h-[72px] shrink-0 flex-col gap-2 border-b px-5 py-3"><Skeleton className="h-3 w-1/3" /><Skeleton className="h-5 w-1/2" /><Skeleton className="h-3 w-2/5" /></div>
    <div aria-hidden="true" className="flex min-h-0 flex-1 flex-col gap-8 overflow-hidden p-5"><Card className="ml-auto w-2/3 gap-3 rounded-lg p-4"><Lines count={2} /></Card><div className="flex gap-3"><Skeleton className="size-6 shrink-0 rounded-full" /><div className="flex flex-1 flex-col gap-4 pt-1"><Skeleton className="h-3 w-1/4" /><Lines count={5} /></div></div></div>
    <div aria-hidden="true" className="px-5 pt-2 pb-4"><ComposerSkeleton /></div>
  </div>;
}

export function ComposerSkeleton() {
  return <Card role="status" aria-label="Carregando campo de mensagem" className="h-[130px] justify-between rounded-2xl p-4"><Skeleton className="h-3 w-1/2" /><div aria-hidden="true" className="flex items-center gap-3"><Skeleton className="size-7 rounded-full" /><Skeleton className="ml-auto h-5 w-20" /><Skeleton className="h-5 w-28" /><Skeleton className="size-7 rounded-full" /></div></Card>;
}

export function PromptRefinementSkeleton({ count = 3 }: { count?: number }) {
  return <div role="status" aria-label="Refinando prompt" aria-live="polite" className="min-h-[84px] px-5 pt-4 pb-2 [&_[data-slot=skeleton]]:bg-muted-foreground/25 motion-reduce:**:animate-none"><span className="sr-only">Refinando prompt…</span><Lines count={count} /></div>;
}

export function HomeSkeleton() {
  return <div role="status" aria-label="Carregando Jarvis" className="flex h-full min-h-0 w-full flex-1 bg-background">
    <aside aria-hidden="true" className="flex w-[20%] min-w-[220px] flex-col border-r bg-sidebar"><div className="flex h-16 items-center gap-3 border-b p-4"><Skeleton className="h-8 flex-1" /><Skeleton className="size-7" /></div><SidebarSkeleton /><div className="mt-auto flex gap-3 border-t p-4"><Skeleton className="size-6" /><Skeleton className="h-5 w-1/2" /></div></aside>
    <div aria-hidden="true" className="min-w-0 flex-1"><ConversationSkeleton /></div>
    <aside aria-hidden="true" className="flex w-[24%] min-w-[280px] flex-col gap-6 border-l bg-sidebar p-4"><div className="flex gap-3 border-b pb-4"><Skeleton className="h-5 w-16" /><Skeleton className="h-5 w-20" /></div><Lines count={3} /><Skeleton className="h-4 w-1/2" /><Lines count={4} /><div className="mt-auto flex flex-col gap-3 border-t pt-4"><Skeleton className="h-3 w-1/3" /><Skeleton className="h-2 w-full rounded-full" /></div></aside>
  </div>;
}

/**
 * Neutral full-screen placeholder shown while the app resolves its boot config
 * and while the onboarding chunk loads. Deliberately not the Home layout: a
 * first-run user must not see the three-pane shell flash before the centered
 * onboarding card appears.
 */
export function BootSkeleton() {
  return <div role="status" aria-label="Iniciando o Jarvis" className="flex h-full min-h-0 w-full flex-1 flex-col items-center justify-center gap-5 bg-background">
    <Skeleton className="h-9 w-44 rounded-md" />
    <div aria-hidden="true" className="flex flex-col items-center gap-2.5"><Skeleton className="h-3 w-56" /><Skeleton className="h-3 w-36" /></div>
    <Skeleton className="mt-2 h-8 w-28 rounded-full" />
  </div>;
}

export function SettingsSkeleton({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  return <Dialog open={open} onOpenChange={onOpenChange}><DialogContent className="flex h-[min(740px,85dvh)] w-[calc(100vw-3rem)] sm:max-w-[960px] flex-col gap-0 p-0 motion-reduce:transition-none"><DialogHeader className="border-b bg-sidebar px-6 py-3"><DialogTitle className="text-sm">Configurações</DialogTitle><DialogDescription className="sr-only">Carregando configurações</DialogDescription></DialogHeader><div role="status" aria-label="Carregando configurações" className="flex min-h-0 flex-1"><aside aria-hidden="true" className="flex w-14 shrink-0 flex-col gap-3 border-r bg-sidebar p-3 sm:w-60 sm:p-4">{Array.from({ length: 12 }, (_, i) => <Skeleton key={i} className="h-6 w-full rounded-md" />)}</aside><div aria-hidden="true" className="min-w-0 flex-1 space-y-6 overflow-y-auto p-6"><Skeleton className="h-5 w-24" /><RowsSkeleton label="Carregando opções" count={6} /><RowsSkeleton label="Carregando limpeza" count={3} /></div></div></DialogContent></Dialog>;
}
