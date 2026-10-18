import { ChevronDown, FileText, Trash2 } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import type { PendingAuthoring } from "@/core/authoring";

const origins = { jarvis: "Global do Jarvis", agents: "Compartilhada entre apps", project: "Deste projeto" };

export function SkillRemovalReview({ skills }: { skills: Extract<PendingAuthoring["target"], { kind: "skill_removal" }>["skills"] }) {
  return <div className="min-w-0 space-y-4">
    <Alert variant="destructive"><Trash2 aria-hidden="true" /><AlertDescription>A remoção exclui as pastas e os arquivos exibidos e atualiza o catálogo nesta conversa. Skills compartilhadas também deixam de estar disponíveis nos outros apps que usam esta pasta.</AlertDescription></Alert>
    {skills.map(skill => <Card key={skill.id} className="min-w-0 gap-3 p-4">
      <CardHeader className="min-w-0 gap-2 p-0">
        <CardTitle className="break-words text-sm">{skill.name}</CardTitle>
        <p className="break-words text-xs leading-5 text-muted-foreground">{skill.description}</p>
        <Badge variant="outline">{origins[skill.origin]}</Badge>
        <p className="break-all font-mono text-[11px] text-muted-foreground">{skill.path}</p>
      </CardHeader>
      <CardContent className="min-w-0 space-y-3 p-0">
        <p className="break-words text-xs leading-5">{skill.reason}</p>
        <p className="text-xs leading-5 text-muted-foreground">{skill.linked ? "Somente o vínculo será removido. A pasta original e seus arquivos serão preservados." : "A pasta e seus arquivos serão excluídos permanentemente."}</p>
        {!skill.linked && <Collapsible>
          <CollapsibleTrigger render={<Button variant="ghost" size="sm" />} className="group h-auto w-full cursor-pointer justify-start whitespace-normal px-0 text-xs"><FileText aria-hidden="true" className="size-3.5" />{skill.files.length} {skill.files.length === 1 ? "arquivo" : "arquivos"}<ChevronDown aria-hidden="true" className="size-3.5 transition-transform group-data-[panel-open]:rotate-180" /></CollapsibleTrigger>
          <CollapsibleContent><ul aria-label={`Arquivos a remover de ${skill.name}`} className="max-h-40 overflow-y-auto break-all pt-2 font-mono text-[10px] text-muted-foreground">{skill.files.map(file => <li key={file}>{file}</li>)}</ul></CollapsibleContent>
        </Collapsible>}
      </CardContent>
    </Card>)}
  </div>;
}
