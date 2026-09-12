import { useCallback, useEffect, useState, type KeyboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronRight, ChevronsDownUp, Folder, FolderOpen, Link, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { fileError, type DirectoryListing, type FileEntry } from "@/core/project-files";
import { FileIcon } from "./FileIcon";
import { Hint } from "@/components/ui/hint";

type TreeProps = { projectId: string; expanded: Set<string>; selected: string | null; focused: string | null; revision: number; onOpen: (path: string) => void; onToggle: (path: string) => void; onFocus: (path: string) => void };

function DirectoryItems({ path, depth, ...tree }: TreeProps & { path: string; depth: number }) {
  const key = JSON.stringify([tree.projectId, path, tree.revision]);
  const [result, setResult] = useState<{ key: string; data?: DirectoryListing; error?: string }>();
  useEffect(() => {
    let alive = true;
    void invoke<DirectoryListing>("list_project_directory", { projectId: tree.projectId, path }).then(
      data => { if (alive) setResult({ key, data }); },
      error => { if (alive) setResult({ key, error: fileError(error) }); },
    );
    return () => { alive = false; };
  }, [key, path, tree.projectId]);
  const current = result?.key === key ? result : undefined;
  const items = current?.data?.entries ?? [];
  return <ul role={depth === 1 ? "tree" : "group"} aria-label={depth === 1 ? "Arquivos do projeto" : undefined} onKeyDown={depth === 1 ? treeKeys : undefined} className="min-w-0">
    {!current && <li role="none"><div role="status" aria-label="Carregando arquivos" className="space-y-2 px-3 py-2"><Skeleton className="h-5 w-4/5" /><Skeleton className="h-5 w-3/5" /><Skeleton className="h-5 w-4/5" /></div></li>}
    {current?.error && <li role="none"><p role="alert" className="px-3 py-2 text-xs text-destructive">{current.error}</p></li>}
    {current?.data && !items.length && <li role="none"><p className="px-3 py-2 text-xs text-muted-foreground">Pasta vazia.</p></li>}
    {items.map((entry, index) => <TreeRow key={entry.path} {...tree} entry={entry} depth={depth} first={depth === 1 && index === 0} />)}
    {current?.data?.truncated && <li role="none"><p role="status" className="px-3 py-2 text-xs text-onedark-yellow">Esta pasta exibe os primeiros 4.000 itens.</p></li>}
  </ul>;
}

function treeKeys(event: KeyboardEvent<HTMLUListElement>) {
  const target = (event.target as HTMLElement).closest<HTMLButtonElement>('[role="treeitem"]');
  if (!target) return;
  const rows = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="treeitem"]:not(:disabled)'));
  const index = rows.indexOf(target);
  const level = Number(target.getAttribute("aria-level"));
  let next: HTMLButtonElement | undefined;
  if (event.key === "ArrowDown") next = rows[index + 1];
  else if (event.key === "ArrowUp") next = rows[index - 1];
  else if (event.key === "Home") next = rows[0];
  else if (event.key === "End") next = rows[rows.length - 1];
  else if (event.key === "ArrowRight") {
    if (target.getAttribute("aria-expanded") === "false") target.click();
    else if (Number(rows[index + 1]?.getAttribute("aria-level")) > level) next = rows[index + 1];
  } else if (event.key === "ArrowLeft") {
    if (target.getAttribute("aria-expanded") === "true") target.click();
    else next = rows.slice(0, index).reverse().find(row => Number(row.getAttribute("aria-level")) < level);
  } else return;
  event.preventDefault();
  next?.focus();
}

function TreeRow({ entry, depth, first, ...tree }: TreeProps & { entry: FileEntry; depth: number; first: boolean }) {
  const folder = entry.kind === "directory";
  const open = tree.expanded.has(entry.path);
  const activate = () => folder ? tree.onToggle(entry.path) : tree.onOpen(entry.path);
  const focus = () => tree.onFocus(entry.path);
  return <li role="none">
    <Hint content={entry.kind === "link" ? `${entry.path} — link simbólico` : entry.path}><Button type="button" role="treeitem" variant="ghost" aria-level={depth} aria-expanded={folder ? open : undefined} aria-selected={tree.selected === entry.path} disabled={entry.kind === "link"} tabIndex={tree.focused === entry.path || (!tree.focused && first) ? 0 : -1} onClick={activate} onFocus={focus} style={{ paddingLeft: 8 + (depth - 1) * 14 }} className="h-7 w-full min-w-0 cursor-pointer justify-start gap-1.5 rounded-none pr-3 text-xs font-normal aria-selected:bg-primary/15 aria-selected:text-primary">
      {folder ? <ChevronRight aria-hidden="true" className={`size-3 shrink-0 ${open ? "rotate-90" : ""}`} /> : <span className="w-3 shrink-0" />}
      {folder ? open ? <FolderOpen aria-hidden="true" className="size-3.5 shrink-0 text-onedark-cyan" /> : <Folder aria-hidden="true" className="size-3.5 shrink-0 text-onedark-cyan" /> : entry.kind === "link" ? <Link aria-hidden="true" className="size-3.5 shrink-0 text-muted-foreground" /> : <FileIcon path={entry.path} />}
      <span className="truncate font-mono text-[11px]">{entry.name}</span>
    </Button></Hint>
    {folder && open && <DirectoryItems {...tree} path={entry.path} depth={depth + 1} />}
  </li>;
}

export function ProjectExplorer({ projectId, projectName, selected, onOpen }: { projectId: string; projectName: string; selected: string | null; onOpen: (path: string) => void }) {
  const [expanded, setExpanded] = useState(() => new Set<string>());
  const [focused, setFocused] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const toggle = useCallback((path: string) => setExpanded(current => { const next = new Set(current); if (next.has(path)) next.delete(path); else next.add(path); return next; }), []);
  const refresh = () => { setFocused(null); setRevision(current => current + 1); };
  const collapse = () => { setExpanded(new Set()); setFocused(null); };
  return <section aria-label="Explorer do projeto" className="flex h-full min-h-0 flex-col">
    <div className="flex h-9 shrink-0 items-center gap-2 border-b border-border px-3">
      <FolderOpen aria-hidden="true" className="size-3.5 shrink-0 text-onedark-cyan" /><Hint content={projectName}><span className="min-w-0 flex-1 truncate text-xs font-medium">{projectName}</span></Hint>
      <Hint content="Recolher pastas"><Button type="button" variant="ghost" size="icon" aria-label="Recolher pastas" onClick={collapse} className="size-6 cursor-pointer text-muted-foreground"><ChevronsDownUp className="size-3.5" /></Button></Hint>
      <Hint content="Atualizar Explorer"><Button type="button" variant="ghost" size="icon" aria-label="Atualizar Explorer" onClick={refresh} className="size-6 cursor-pointer text-muted-foreground"><RefreshCw className="size-3.5" /></Button></Hint>
    </div>
    <ScrollArea className="min-h-0 flex-1"><div className="py-1"><DirectoryItems projectId={projectId} expanded={expanded} selected={selected} focused={focused} revision={revision} onOpen={onOpen} onToggle={toggle} onFocus={setFocused} path="" depth={1} /></div></ScrollArea>
  </section>;
}
