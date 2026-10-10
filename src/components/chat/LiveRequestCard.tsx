import { useState } from "react";
import { ChevronDown, Palette, User, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import type { LiveRequestMessage } from "./live-request-message";

export function LiveRequestCard({ request, timestamp }: { request: LiveRequestMessage; timestamp: string }) {
  const [open, setOpen] = useState(false);
  const [technical, setTechnical] = useState(false);
  return (
    <Dialog open={open} onOpenChange={next => { setOpen(next); if (!next) setTechnical(false); }}>
      <Card size="sm" className="instrument-panel w-full gap-3 rounded-lg border-primary/15 bg-secondary/60 py-3">
        <CardHeader className="gap-1 px-4">
          <CardTitle className="flex items-center gap-2 text-sm">
            <Palette aria-hidden="true" className="size-4 shrink-0 text-onedark-purple" />Designer Live
          </CardTitle>
          <CardDescription className="text-xs">
            {request.action}{request.alternatives !== undefined && ` · ${request.alternatives} ${request.alternatives === 1 ? "alternativa" : "alternativas"}`}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-2 px-4">
          <p className="line-clamp-3 whitespace-pre-wrap break-words text-sm leading-6">{request.instruction.slice(0, 400)}{request.instruction.length > 400 && "…"}</p>
          {request.selection && <p className="truncate font-mono text-[11px] text-muted-foreground">{request.selection}</p>}
        </CardContent>
        <CardFooter className="justify-between gap-3 border-0 bg-transparent px-4 pb-3 pt-0">
          <DialogTrigger render={<Button type="button" variant="ghost" size="sm" className="h-7 cursor-pointer px-2 text-xs text-primary" />}>Ver detalhes</DialogTrigger>
          <span className="flex items-center gap-1.5 font-mono text-[10px] tabular-nums text-muted-foreground"><span>{timestamp}</span><User aria-hidden="true" className="size-3 text-primary" /></span>
        </CardFooter>
      </Card>
      {open && <DialogContent showCloseButton={false} className="flex max-h-[85dvh] min-w-0 flex-col rounded-lg sm:max-w-2xl">
        <DialogHeader className="pr-8">
          <DialogTitle>Solicitação do Designer Live</DialogTitle>
          <DialogDescription>Pedido enviado a partir do modo Live.</DialogDescription>
        </DialogHeader>
        <DialogClose render={<Button type="button" variant="ghost" size="icon-sm" aria-label="Fechar detalhes" className="absolute right-2 top-2 cursor-pointer" />}><X aria-hidden="true" className="size-4" /></DialogClose>
        <ScrollArea className="min-h-0 min-w-0 [&_[data-slot=scroll-area-viewport]]:max-h-[65dvh]">
          <div className="space-y-5 pr-3">
            <section className="space-y-2" aria-label="Pedido">
              <h3 className="text-sm font-medium">Pedido</h3>
              <p className="whitespace-pre-wrap break-words text-sm leading-6">{request.instruction}</p>
            </section>
            <dl className="grid min-w-0 grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-3 text-xs">
              <dt className="text-muted-foreground">Ação</dt><dd>{request.action}</dd>
              {request.page && <><dt className="text-muted-foreground">Página</dt><dd className="break-all font-mono">{request.page}</dd></>}
              {request.selection && <><dt className="text-muted-foreground">Elemento</dt><dd className="break-all font-mono">{request.selection}</dd></>}
              {request.file && <><dt className="text-muted-foreground">Arquivo</dt><dd className="break-all font-mono">{request.file}</dd></>}
              {request.alternatives !== undefined && <><dt className="text-muted-foreground">Alternativas</dt><dd className="font-mono">{request.alternatives}</dd></>}
              <dt className="text-muted-foreground">ID da ação</dt><dd className="break-all font-mono">{request.actionId}</dd>
            </dl>
            <Collapsible open={technical} onOpenChange={setTechnical}>
              <CollapsibleTrigger render={<Button type="button" variant="ghost" size="sm" className="w-full cursor-pointer justify-between px-0 text-xs text-muted-foreground" />}>Mensagem técnica enviada ao agente<ChevronDown aria-hidden="true" className={`size-3.5 ${technical ? "rotate-180" : ""}`} /></CollapsibleTrigger>
              <CollapsibleContent>{technical && <pre className="mt-2 whitespace-pre-wrap break-all rounded-md border border-border bg-background p-3 font-mono text-xs leading-5">{request.raw}</pre>}</CollapsibleContent>
            </Collapsible>
          </div>
        </ScrollArea>
      </DialogContent>}
    </Dialog>
  );
}
