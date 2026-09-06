import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FileCode2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { DiffSkeleton } from "./LoadingSkeletons";
import { fileDiffSchema, type FileChange, type FileDiff } from "@/core/chat";
import { libraryError } from "@/core/library";

function Counts({ additions, deletions }: Pick<FileChange, "additions" | "deletions">) {
  return additions === null || deletions === null ? <span className="text-[10px] text-muted-foreground">Sem base</span> : <span className="flex shrink-0 gap-2 font-mono text-[11px] tabular-nums" aria-label={`${additions} linhas adicionadas, ${deletions} linhas removidas`}><span className="text-[#98c379]">+{additions}</span><span className="text-destructive">−{deletions}</span></span>;
}

function FileButton({ file, selected, onClick }: { file: FileChange; selected?: boolean; onClick: () => void }) {
  const parts = file.path.split("/");
  const name = parts.pop();
  return <Button variant="ghost" onClick={onClick} aria-pressed={selected} aria-label={`Alterações em ${file.path}`} className={`h-auto w-full cursor-pointer justify-start gap-2 px-2 py-2 text-xs ${selected ? "bg-primary/10 text-primary" : ""}`}>
    <FileCode2 aria-hidden="true" className="size-4 shrink-0 text-primary" />
    <span className="min-w-0 flex-1 text-left" title={file.path}><span className="block truncate">{name}</span>{parts.length > 0 && <span className="block truncate text-[10px] text-muted-foreground">{parts.join("/")}</span>}</span>
    <Counts {...file} />
  </Button>;
}

function DiffView({ conversationId, file, revision }: { conversationId: string; file: FileChange; revision: number }) {
  const [result, setResult] = useState<{ diff?: FileDiff; error?: string } | null>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let active = true;
    void invoke<unknown>("get_agent_file_diff", { conversationId, path: file.path }).then(value => {
      const diff = fileDiffSchema.parse(value);
      if (diff.path !== file.path) throw new Error("O diff recebido não corresponde ao arquivo selecionado.");
      if (active) setResult({ diff });
    }).catch((cause: unknown) => { if (active) setResult({ error: libraryError(cause, "Não foi possível carregar as alterações.") }); });
    return () => { active = false; };
  }, [conversationId, file.path, revision, attempt]);
  return <section className="flex min-h-0 min-w-0 flex-1 flex-col" aria-label={`Diff de ${file.path}`}>
    <div className="flex min-w-0 items-center gap-3 border-b border-border px-4 py-3"><p className="min-w-0 flex-1 truncate font-mono text-xs" title={file.path}>{file.path}</p><Counts {...file} /></div>
    {!result ? <DiffSkeleton /> : result.error ? <div className="space-y-3 p-4"><p role="alert" className="text-xs text-destructive">{result.error}</p><Button variant="outline" size="sm" className="cursor-pointer" onClick={() => { setResult(null); setAttempt(value => value + 1); }}>Tentar novamente</Button></div> : result.diff && <>
      <p className="border-b border-border/50 px-4 py-2 text-[11px] text-muted-foreground">Alterações da sessão ainda não commitadas.</p>
      <div className="min-h-0 flex-1 overflow-auto" tabIndex={0} aria-label="Linhas do diff">
        <table className="w-full border-collapse font-mono text-[11px] leading-5"><thead className="sr-only"><tr><th>Linha anterior</th><th>Linha atual</th><th>Alteração</th><th>Conteúdo</th></tr></thead><tbody>
          {result.diff.rows.map((row, index) => row.kind === "gap" ? <tr key={index} className="bg-primary/5 text-muted-foreground"><td colSpan={4} className="px-4 py-1 text-center">··· {row.text} ···</td></tr> : <tr key={index} className={row.kind === "added" ? "bg-[#98c379]/10 text-[#b6d7a2]" : row.kind === "removed" ? "bg-destructive/10 text-[#e8a0a7]" : "text-foreground/80"}>
            <td className="w-10 select-none border-r border-border/30 px-2 text-right text-muted-foreground">{row.oldLine}</td><td className="w-10 select-none border-r border-border/30 px-2 text-right text-muted-foreground">{row.newLine}</td>
            <td className="w-6 select-none px-2" aria-label={row.kind === "added" ? "Adicionada" : row.kind === "removed" ? "Removida" : "Sem alteração"}>{row.kind === "added" ? "+" : row.kind === "removed" ? "−" : " "}</td><td className="whitespace-pre pr-4">{row.text || " "}</td>
          </tr>)}
        </tbody></table>
        {result.diff.base !== "unknown" && !result.diff.rows.length && <p className="p-4 text-xs text-muted-foreground">Nenhuma diferença de conteúdo.</p>}
      </div>
      {result.diff.truncated && <p role="status" className="border-t border-border px-4 py-2 text-xs text-[#e5c07b]">Prévia limitada para manter a navegação rápida. As contagens incluem o arquivo completo.</p>}
    </>}
  </section>;
}

export function ChangedFiles({ files, conversationId }: { files: FileChange[]; conversationId: string }) {
  const [selected, setSelected] = useState<string | null>(null);
  const file = files.find(item => item.path === selected) ?? files[0];
  const totals = files.reduce((sum, item) => ({ additions: sum.additions + (item.additions ?? 0), deletions: sum.deletions + (item.deletions ?? 0) }), { additions: 0, deletions: 0 });
  return <>
    <div className="mb-2 flex items-center justify-between gap-2 px-2 text-[11px] text-muted-foreground"><span>{files.length} {files.length === 1 ? "arquivo" : "arquivos"}</span><Counts {...totals} /></div>
    {files.some(item => item.base === "unknown") && <p className="mb-2 px-2 text-[10px] text-muted-foreground">Totais apenas dos arquivos com base disponível.</p>}
    <div className="space-y-0.5">{files.slice(0, 5).map(item => <FileButton key={item.path} file={item} onClick={() => setSelected(item.path)} />)}</div>
    {files.length > 5 && <Button variant="ghost" size="sm" className="mt-2 w-full cursor-pointer text-xs text-primary" onClick={() => setSelected(files[0].path)}>Ver tudo ({files.length})</Button>}
    <Dialog open={selected !== null} onOpenChange={open => { if (!open) setSelected(null); }}>
      <DialogContent showCloseButton={false} className="flex h-[min(82vh,820px)] w-[calc(100vw-3rem)] max-w-none flex-col gap-0 overflow-hidden p-0 sm:max-w-6xl">
        <DialogHeader className="shrink-0 border-b border-border px-5 py-4 pr-14"><DialogTitle>Arquivos alterados</DialogTitle><DialogDescription className="text-xs">{files.length} {files.length === 1 ? "arquivo alterado" : "arquivos alterados"} nesta conversa. Selecione um arquivo para revisar o diff.</DialogDescription></DialogHeader>
        <DialogClose render={<Button variant="ghost" size="icon" className="absolute right-3 top-3 cursor-pointer" aria-label="Fechar alterações" />}><X className="size-4" /></DialogClose>
        <div className="flex min-h-0 flex-1 flex-col sm:flex-row">
          <nav aria-label="Arquivos para revisar" className="max-h-36 shrink-0 overflow-y-auto border-b border-border bg-sidebar p-2 sm:max-h-none sm:w-64 sm:border-r sm:border-b-0 lg:w-72">{files.map(item => <FileButton key={item.path} file={item} selected={file?.path === item.path} onClick={() => setSelected(item.path)} />)}</nav>
          {file && selected !== null && <DiffView key={`${conversationId}/${file.path}/${file.revision ?? 0}`} conversationId={conversationId} file={file} revision={file.revision ?? 0} />}
        </div>
      </DialogContent>
    </Dialog>
  </>;
}
