import { ChevronDown, FileText, Library } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Textarea } from "@/components/ui/textarea";
import type { PendingAuthoring } from "@/core/authoring";

export function SkillInstallationReview({ skills }: { skills: Extract<PendingAuthoring["target"], { kind: "skills" }>["skills"] }) {
  return <div className="min-w-0 space-y-4">
    <Alert><Library aria-hidden="true" /><AlertDescription>Disponíveis em todas as conversas após a instalação. Scripts e dependências não serão executados durante a instalação.</AlertDescription></Alert>
    {skills.map(skill => <Card key={`${skill.source}/${skill.skillId}`} className="min-w-0 gap-3 p-4">
      <CardHeader className="min-w-0 gap-2 p-0">
        <CardTitle className="break-words text-sm">{skill.name}</CardTitle>
        <p className="break-words text-xs leading-5 text-muted-foreground">{skill.description}</p>
        <p className="break-all font-mono text-[11px] text-muted-foreground">{skill.source}</p>
      </CardHeader>
      <CardContent className="min-w-0 space-y-3 p-0">
        <div className="rounded-md border border-border bg-sidebar/55 p-3"><p className="mb-1 text-xs font-medium">Como ajuda nesta tarefa</p><p className="break-words text-xs leading-5 text-muted-foreground">{skill.reason}</p></div>
        <Badge variant="outline" className="text-muted-foreground"><FileText aria-hidden="true" className="size-3" />{skill.files.length} {skill.files.length === 1 ? "arquivo" : "arquivos"}</Badge>
        <Collapsible>
          <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group h-auto w-full cursor-pointer justify-start whitespace-normal px-0 text-xs"><ChevronDown aria-hidden="true" className="size-3.5 transition-transform group-data-[panel-open]:rotate-180" />Ver instruções e arquivos de {skill.name}</CollapsibleTrigger>
          <CollapsibleContent className="space-y-2 pt-2">
            <Textarea aria-label={`Instruções de ${skill.name}`} value={skill.content} readOnly className="min-h-48 max-h-80 resize-y font-mono text-xs leading-5" />
            <ul aria-label={`Arquivos de ${skill.name}`} className="max-h-32 overflow-y-auto break-all font-mono text-[10px] text-muted-foreground">{skill.files.map(file => <li key={file}>{file}</li>)}</ul>
            <p className="break-all font-mono text-[10px] text-muted-foreground">SHA-256: {skill.digest}</p>
          </CollapsibleContent>
        </Collapsible>
      </CardContent>
    </Card>)}
  </div>;
}
