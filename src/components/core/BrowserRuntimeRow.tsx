import { useState } from "react";
import { Check, ChevronDown, Globe } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import type { BrowserRuntime } from "@/core/core-components";

export function BrowserRuntimeRow({ runtime }: { runtime: BrowserRuntime }) {
  const [open, setOpen] = useState(false);
  const unavailable = runtime.state === "unavailable";
  return <Collapsible open={open} onOpenChange={setOpen} className="min-w-0 px-4 py-3">
    <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
      <Globe aria-hidden="true" className="size-4 shrink-0 text-primary" />
      <div className="min-w-0 flex-1 basis-48">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5"><h3 className="text-sm font-medium">Navegador Chromium</h3><Badge variant="outline" className="text-[10px]">Incluído</Badge><span className="font-mono text-[10px] text-muted-foreground">CEF {runtime.version}</span></div>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">Navegação nas abas do chat e edição visual com Impeccable Live.</p>
        <p className="text-xs leading-5 text-muted-foreground">O Chromium acompanha o Jarvis e é atualizado com o aplicativo.</p>
      </div>
      <div className="ml-auto flex items-center gap-2">
        <span className={`text-xs ${unavailable ? "text-onedark-yellow" : "text-muted-foreground"}`}>{runtime.state === "development" ? "Desenvolvimento" : unavailable ? "Atenção" : "Disponível"}</span>
        <CollapsibleTrigger render={<Button size="icon-sm" variant="ghost" />} aria-label="Verificações do navegador Chromium" className="cursor-pointer text-muted-foreground"><ChevronDown aria-hidden="true" className={`size-3.5 ${open ? "rotate-180" : ""}`} /></CollapsibleTrigger>
      </div>
    </div>
    {unavailable && <div className="ml-7 mt-2 space-y-1"><p role="alert" className="text-xs text-destructive">{runtime.diagnostics.find(check => !check.passed)?.message ?? "O navegador integrado está incompleto."}</p><p className="text-xs leading-5 text-muted-foreground">Atualize ou reinstale o Jarvis para recuperar o navegador integrado.</p></div>}
    {open && <CollapsibleContent className="ml-7 space-y-2 pt-3">
      <p className="max-w-prose text-xs leading-5 text-muted-foreground">Este navegador interativo faz parte do aplicativo. Os runtimes de renderização de vídeo são independentes.</p>
      {runtime.state === "development" && <p className="text-xs leading-5 text-muted-foreground">Executável de desenvolvimento; os arquivos incluídos são verificados no aplicativo empacotado.</p>}
      {runtime.diagnostics.length > 0 && <ul aria-label="Verificações do Chromium" className="space-y-1.5">{runtime.diagnostics.map(check => <li key={check.label} className={`flex items-start gap-2 text-xs ${check.passed ? "text-muted-foreground" : "text-destructive"}`}>{check.passed && <Check aria-hidden="true" className="size-3 shrink-0" />}<span>{check.label}: {check.message}</span></li>)}</ul>}
    </CollapsibleContent>}
  </Collapsible>;
}
