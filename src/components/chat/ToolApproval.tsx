import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardFooter, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import type { AgentTool } from "@/core/chat";

export function ToolApproval({ tool, projectPath, onAnswer }: { tool: AgentTool; projectPath: string; onAnswer: (approved: boolean) => Promise<boolean> }) {
  const [pending, setPending] = useState(false);
  const answer = async (approved: boolean) => {
    if (pending) return;
    setPending(true);
    const accepted = await onAnswer(approved);
    if (!accepted) setPending(false);
  };
  const subject = tool.name.startsWith("browser_") ? "ação no navegador" : tool.name.startsWith("beads_") ? "alteração de tarefa" : tool.name === "bash" ? "comando" : tool.name === "terminal_close" ? "fechamento de terminal" : "alteração de arquivo";
  return <Card role="region" aria-label="Autorização de ferramenta" className="mb-3 border border-primary/40" size="sm">
    <CardHeader>
      <CardTitle>Autorizar {subject}?</CardTitle>
      <CardDescription className="break-all">{projectPath}</CardDescription>
    </CardHeader>
    <CardContent>
      {Object.entries(tool.args).map(([key, value]) => <div key={key} className="mb-2">
        <p className="mb-1 text-xs text-muted-foreground">{({ path: "Arquivo", command: "Comando", content: "Conteúdo proposto", oldText: "Trecho original", newText: "Substituição", timeoutSeconds: "Tempo limite (segundos)", id: "Terminal", reason: "Motivo" } as Record<string, string>)[key] ?? key}</p>
        <pre className="max-h-44 overflow-auto whitespace-pre-wrap break-all rounded-md bg-background p-2 text-xs">{typeof value === "string" ? value : JSON.stringify(value, null, 2)}</pre>
      </div>)}
    </CardContent>
    <CardFooter className="justify-end gap-2">
      <Button variant="outline" className="cursor-pointer" disabled={pending} onClick={() => { void answer(false); }}>Recusar</Button>
      <Button className="cursor-pointer" disabled={pending} onClick={() => { void answer(true); }}>Autorizar uma vez</Button>
    </CardFooter>
  </Card>;
}
