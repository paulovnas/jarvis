import { FileText } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import type { PendingAuthoring } from "@/core/authoring";

export function ProjectInstructionsReview({ target }: { target: Extract<PendingAuthoring["target"], { kind: "project_instructions" }> }) {
  return <div className="min-w-0 space-y-4">
    <Alert><FileText aria-hidden="true" /><AlertDescription>Instruções apenas para este projeto. O Jarvis atualiza sua seção no arquivo e preserva as demais regras e os arquivos das subpastas.</AlertDescription></Alert>
    <Card className="min-w-0 gap-3 p-4">
      <CardHeader className="p-0"><CardTitle className="font-mono text-sm">{target.path}</CardTitle></CardHeader>
      <CardContent className="p-0"><Tabs defaultValue="proposed" className="min-w-0">
        <TabsList aria-label="Comparar instruções do projeto"><TabsTrigger value="proposed" className="cursor-pointer">Arquivo proposto</TabsTrigger><TabsTrigger value="current" className="cursor-pointer">Arquivo atual</TabsTrigger></TabsList>
        <TabsContent value="proposed"><Textarea aria-label="Conteúdo proposto do AGENTS.md" value={target.after} readOnly className="min-h-72 max-h-[55dvh] resize-y font-mono text-xs leading-6" /></TabsContent>
        <TabsContent value="current">{target.before === null ? <p className="py-4 text-sm text-muted-foreground">Este projeto ainda não possui AGENTS.md.</p> : <Textarea aria-label="Conteúdo atual do AGENTS.md" value={target.before} readOnly className="min-h-72 max-h-[55dvh] resize-y font-mono text-xs leading-6" />}</TabsContent>
      </Tabs></CardContent>
    </Card>
  </div>;
}
