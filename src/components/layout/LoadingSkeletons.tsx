import { Skeleton } from "@/components/ui/skeleton";
import { Card } from "@/components/ui/card";
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet";

function Lines({ count = 3 }: { count?: number }) {
  return <div aria-hidden="true" className="flex flex-col gap-2.5">{Array.from({ length: count }, (_, i) => <Skeleton key={i} className={`h-3 ${i === count - 1 ? "w-2/3" : "w-full"}`} />)}</div>;
}

export function CardsSkeleton({ label, columns = false }: { label: string; columns?: boolean }) {
  return <div role="status" aria-label={label} className={`grid gap-3 ${columns ? "sm:grid-cols-2" : "grid-cols-1"}`}>
    {[0, 1, 2, 3].slice(0, columns ? 4 : 2).map(i => <Card key={i} aria-hidden="true" className="gap-3 p-4"><div className="flex items-center gap-3"><Skeleton className="size-5 shrink-0" /><Skeleton className="h-4 w-1/2" /><Skeleton className="ml-auto h-4 w-7 rounded-full" /></div><Lines count={2} /><div className="flex gap-2"><Skeleton className="h-4 w-14 rounded-full" /><Skeleton className="h-4 w-16 rounded-full" /></div></Card>)}
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
    <div aria-hidden="true" className="flex h-[101px] shrink-0 flex-col gap-3 border-b px-5 py-4"><Skeleton className="h-3 w-1/3" /><Skeleton className="h-5 w-1/2" /><Skeleton className="h-3 w-2/5" /></div>
    <div aria-hidden="true" className="flex min-h-0 flex-1 flex-col gap-8 overflow-hidden p-5"><Card className="ml-auto w-2/3 gap-3 rounded-2xl p-4"><Lines count={2} /></Card><div className="flex gap-3"><Skeleton className="size-6 shrink-0 rounded-full" /><div className="flex flex-1 flex-col gap-4 pt-1"><Skeleton className="h-3 w-1/4" /><Lines count={5} /></div></div></div>
    <div aria-hidden="true" className="px-5 pt-2 pb-4"><ComposerSkeleton /></div>
  </div>;
}

export function ComposerSkeleton() {
  return <Card role="status" aria-label="Carregando campo de mensagem" className="h-[130px] justify-between rounded-[22px] p-4"><Skeleton className="h-3 w-1/2" /><div aria-hidden="true" className="flex items-center gap-3"><Skeleton className="size-7 rounded-full" /><Skeleton className="ml-auto h-5 w-20" /><Skeleton className="h-5 w-28" /><Skeleton className="size-7 rounded-full" /></div></Card>;
}

export function HomeSkeleton() {
  return <div role="status" aria-label="Carregando Jarvis" className="flex h-full min-h-0 w-full flex-1 bg-background">
    <aside aria-hidden="true" className="flex w-[22%] min-w-60 flex-col border-r bg-sidebar"><div className="flex h-16 items-center gap-3 border-b p-4"><Skeleton className="h-8 flex-1" /><Skeleton className="size-7" /></div><SidebarSkeleton /><div className="mt-auto flex gap-3 border-t p-4"><Skeleton className="size-6" /><Skeleton className="h-5 w-1/2" /></div></aside>
    <div aria-hidden="true" className="min-w-0 flex-1"><ConversationSkeleton /></div>
    <aside aria-hidden="true" className="flex w-[28%] min-w-60 flex-col gap-6 border-l bg-sidebar p-4"><div className="flex gap-3 border-b pb-4"><Skeleton className="h-5 w-16" /><Skeleton className="h-5 w-20" /></div><Lines count={3} /><Skeleton className="h-4 w-1/2" /><Lines count={4} /><div className="mt-auto flex flex-col gap-3 border-t pt-4"><Skeleton className="h-3 w-1/3" /><Skeleton className="h-2 w-full rounded-full" /></div></aside>
  </div>;
}

export function SettingsSkeleton({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  return <Sheet open={open} onOpenChange={onOpenChange}><SheetContent side="left" className="flex h-full w-[min(960px,85vw)] flex-col gap-0 p-0 data-[side=left]:w-[min(960px,85vw)] data-[side=left]:sm:max-w-none"><SheetHeader className="border-b bg-card px-6 py-4.5"><SheetTitle className="text-xl">Configurações</SheetTitle><SheetDescription className="sr-only">Carregando configurações</SheetDescription></SheetHeader><div role="status" aria-label="Carregando configurações" className="flex min-h-0 flex-1 flex-col gap-6"><div aria-hidden="true" className="flex h-12 shrink-0 items-center gap-5 border-b px-6">{[0, 1, 2, 3].map(i => <Skeleton key={i} className="h-5 w-20" />)}</div><div aria-hidden="true" className="px-6"><CardsSkeleton label="Carregando opções" /></div></div></SheetContent></Sheet>;
}
