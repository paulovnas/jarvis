import { FolderPlus, Layers } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import type { LibraryController } from "@/hooks/use-library";
import type { Workspace } from "@/core/library";

export function EmptyWorkspace({ workspace, library }: { workspace: Workspace; library: LibraryController }) {
  return <div className="flex min-h-0 flex-1 items-center justify-center p-6"><Empty className="max-w-lg"><EmptyHeader><EmptyMedia variant="icon" className="border border-onedark-cyan/25 bg-onedark-cyan/10 text-onedark-cyan"><Layers /></EmptyMedia><span className="micro-label text-onedark-cyan">{workspace.name}</span><EmptyTitle>Seu próximo projeto começa aqui</EmptyTitle><EmptyDescription>Selecione a pasta do projeto para começar a trabalhar.</EmptyDescription></EmptyHeader><EmptyContent><Button disabled={library.pending} onClick={() => void library.addProject(workspace.id)}><FolderPlus className="size-4" />Adicionar projeto</Button>{library.error && <p role="alert" className="text-xs text-destructive">{library.error}</p>}</EmptyContent></Empty></div>;
}
