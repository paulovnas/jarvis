import { lazy, Suspense, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, FileText } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Hint } from "@/components/ui/hint";
import { DocumentSkeleton } from "@/components/layout/LoadingSkeletons";
import { skillDetailSchema, skillError, type SkillDetail } from "@/core/skills";

const Markdown = lazy(() => import("@/components/chat/ChatMarkdown"));
export type SkillSelection = { name: string; id: string } | { name: string; source: string; skillId: string };
type DetailState =
  | { key: string; status: "loading" }
  | { key: string; status: "ready"; detail: SkillDetail }
  | { key: string; status: "error"; error: string };

function selectionKey(selection: SkillSelection) {
  return "id" in selection ? `local:${selection.id}` : `marketplace:${selection.source}/${selection.skillId}`;
}

export function SkillDetailsDialog({ selection, onClose }: { selection: SkillSelection | null; onClose: () => void }) {
  const [result, setResult] = useState<DetailState | null>(null);
  const [attempt, setAttempt] = useState(0);
  const cache = useRef(new Map<string, SkillDetail>());
  const key = useMemo(() => selection ? selectionKey(selection) : null, [selection]);
  useEffect(() => {
    if (!selection || !key) return;
    const cached = cache.current.get(key);
    if (cached) {
      setResult({ key, status: "ready", detail: cached });
      return;
    }
    let active = true;
    setResult({ key, status: "loading" });
    const request = "id" in selection ? invoke("get_skill_detail", { id: selection.id }) : invoke("get_marketplace_skill", { source: selection.source, skillId: selection.skillId });
    void request.then(value => {
      const detail = skillDetailSchema.parse(value);
      cache.current.set(key, detail);
      if (active) setResult({ key, status: "ready", detail });
    }).catch(cause => { if (active) setResult({ key, status: "error", error: skillError(cause) }); });
    return () => { active = false; };
  }, [selection, key, attempt]);
  const current = result?.key === key ? result : key ? { key, status: "loading" as const } : null;
  const detail = current?.status === "ready" ? current.detail : undefined;
  return <Dialog open={selection !== null} onOpenChange={open => { if (!open) onClose(); }}>
    <DialogContent className="dark flex max-h-[88vh] flex-col gap-4 sm:max-w-3xl">
      <DialogHeader className="pr-8"><DialogTitle className="break-words leading-snug">{selection?.name}</DialogTitle><DialogDescription className="sr-only">Detalhes da skill</DialogDescription></DialogHeader>
      {current?.status === "error" ? <div className="flex min-h-48 flex-col items-center justify-center gap-3 text-center"><p role="alert" className="text-sm text-destructive">{current.error}</p><Button variant="outline" size="sm" className="cursor-pointer" onClick={() => setAttempt(value => value + 1)}>Tentar novamente</Button></div> : !detail ? <div className="min-h-64"><p className="mb-1 font-mono text-[10px] uppercase tracking-wider text-muted-foreground">Carregando arquivos e instruções…</p><DocumentSkeleton label="Carregando skill" /></div> : <>
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <Badge variant="secondary"><FileText className="size-3" aria-hidden="true" />{detail.files.length} {detail.files.length === 1 ? "arquivo" : "arquivos"}</Badge>
          {detail.source && <Button variant="link" size="sm" className="h-auto cursor-pointer p-0 text-xs" onClick={() => { void openUrl(`https://github.com/${detail.source}`).catch(() => toast.error("Não foi possível abrir a origem")); }}><ExternalLink aria-hidden="true" />{detail.source}</Button>}
          {detail.path && <Hint content={detail.path}><span className="w-full truncate font-mono text-[11px] text-muted-foreground">{detail.path}</span></Hint>}
        </div>
        <ScrollArea className="min-h-0 flex-1 [&>[data-slot=scroll-area-viewport]]:max-h-[60vh]">
          <div className="min-w-0 break-words pr-4 text-sm leading-relaxed [&_h1]:mb-3 [&_h1]:text-lg [&_h2]:mt-5 [&_h2]:mb-2 [&_h2]:font-semibold [&_h3]:mt-3 [&_h3]:font-medium [&_p]:my-3 [&_ul]:my-3 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:my-3 [&_ol]:list-decimal [&_ol]:pl-5 [&_pre]:my-3 [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:bg-muted [&_pre]:p-3 [&_code]:font-mono [&_table]:block [&_table]:overflow-x-auto [&_td]:border [&_td]:p-2 [&_th]:border [&_th]:p-2"><Suspense fallback={<DocumentSkeleton />}><Markdown content={detail.content.replace(/^\uFEFF?---\r?\n[\s\S]*?\r?\n(?:---|\.\.\.)\s*(?:\r?\n|$)/, "")} /></Suspense></div>
        </ScrollArea>
      </>}
    </DialogContent>
  </Dialog>;
}
