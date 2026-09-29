import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import type { HttpClientController } from "@/hooks/use-http-client";

export function HttpCloseDialog({ http }: { http: HttpClientController }) {
  const tab = http.tabs.find(tab => tab.draft.id === http.closingId);
  const running = http.snapshot?.runs.some(run => run.draftId === http.closingId && run.status === "running");
  return <Dialog open={!!tab} onOpenChange={open => { if (!open && !http.busy) http.requestClose(null); }}><DialogContent><DialogHeader><DialogTitle>Fechar requisição</DialogTitle><DialogDescription>{running ? "Esta requisição está em execução. Fechar a aba mantém seu resultado no histórico. Cancelar não desfaz efeitos já aplicados pelo servidor." : "As execuções desta requisição continuam disponíveis no histórico."}{tab?.dirty && " As edições não salvas desta aba serão descartadas."}</DialogDescription></DialogHeader><DialogFooter><Button variant="ghost" disabled={!!http.busy} className="cursor-pointer" onClick={() => http.requestClose(null)}>Voltar</Button>{running && <Button variant="outline" disabled={!!http.busy} className="cursor-pointer" onClick={() => void http.close(true)}>Cancelar execução e fechar</Button>}<Button disabled={!!http.busy} className="cursor-pointer" onClick={() => void http.close(false)}>{running ? "Manter execução e fechar" : "Fechar aba"}</Button></DialogFooter></DialogContent></Dialog>;
}
