import { useId } from "react";
import { SlidersHorizontal } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Hint } from "@/components/ui/hint";
import { Label } from "@/components/ui/label";
import { Popover, PopoverContent, PopoverHeader, PopoverTitle, PopoverTrigger } from "@/components/ui/popover";
import { Switch } from "@/components/ui/switch";
import type { TurnOptions } from "@/core/chat";

type AutomaticPublication = NonNullable<TurnOptions["automaticPublication"]>;

function Setting({ label, hint, checked, disabled, onChange }: { label: string; hint: string; checked: boolean; disabled: boolean; onChange: (value: boolean) => void }) {
  const id = useId();
  return <div className="flex items-center justify-between gap-4"><Hint content={hint}><Label htmlFor={id} className="cursor-pointer text-xs font-normal">{label}</Label></Hint><Switch id={id} size="sm" checked={checked} disabled={disabled} onCheckedChange={onChange} className="cursor-pointer" /></div>;
}

export function ChatBehaviorSettings({ manualAvailable, manualValidation, onManualChange, publication, onPublicationChange, disabled, githubSelected }: {
  manualAvailable: boolean;
  manualValidation: boolean;
  onManualChange: (value: boolean) => void;
  publication: AutomaticPublication | null;
  onPublicationChange: (value: AutomaticPublication | null) => void;
  disabled: boolean;
  githubSelected: boolean;
}) {
  if (githubSelected) return null;
  const active = (manualAvailable && manualValidation) || Boolean(publication);
  const actions = publication ? Object.values(publication).filter(Boolean).length : 0;
  return <Popover>
    <Hint content="Configurar comportamento deste chat"><PopoverTrigger render={<Button type="button" variant="ghost" size="icon" aria-label="Configurações do chat" disabled={disabled} className={`size-7.5 shrink-0 cursor-pointer ${active ? "bg-primary/10 text-primary" : "text-muted-foreground"}`} />}><SlidersHorizontal className="size-3.5" /></PopoverTrigger></Hint>
    <PopoverContent side="top" align="start" sideOffset={10} className="w-80 max-w-[calc(100vw-2rem)] gap-4 p-4">
      <PopoverHeader><PopoverTitle>Comportamento do chat</PopoverTitle></PopoverHeader>
      {manualAvailable && <Setting label="Validação manual" hint="Aguarda sua validação da implementação antes de concluir e iniciar a publicação automática." checked={manualValidation} onChange={onManualChange} disabled={disabled} />}
      <div className="space-y-3">
        <Setting label="Github automático" hint="Ao concluir com sucesso, chama o agente Github com seu modelo configurado para executar as ações selecionadas sem nova aprovação de publicação." checked={Boolean(publication)} disabled={disabled} onChange={checked => onPublicationChange(checked ? { commit: true, push: false, pullRequest: false } : null)} />
        {publication && <div className="space-y-3 rounded-md border border-border bg-secondary/40 p-3">
          {([
            ["commit", "Commit", "Cria um commit das alterações da implementação, seguindo as instruções do projeto."],
            ["push", "Push", "Envia os commits da branch atual ao remoto após sincronizar com a branch de referência."],
            ["pullRequest", "PR", "Cria ou reutiliza uma PR para a branch de referência com o template do projeto. Inclui Push; não executa merge."],
          ] as const).map(([key, label, hint]) => <Setting key={key} label={label} hint={hint} checked={publication[key]} disabled={disabled || (publication[key] && actions === 1) || (key === "push" && publication.pullRequest && !publication.commit)} onChange={checked => onPublicationChange({ ...publication, [key]: checked, ...(key === "pullRequest" && checked ? { push: true } : {}), ...(key === "push" && !checked ? { pullRequest: false } : {}) })} />)}
          <p className="text-[11px] leading-4 text-muted-foreground">As ações selecionadas são autorizadas neste chat. A publicação respeita a validação manual acima.</p>
        </div>}
      </div>
    </PopoverContent>
  </Popover>;
}
