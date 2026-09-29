import { Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/TextInput";
import { Hint } from "@/components/ui/hint";
import type { HttpPair } from "@/core/http-client";

export function HttpPairsEditor({ label, pairs, onChange, disabled = false }: { label: string; pairs: HttpPair[]; onChange: (pairs: HttpPair[]) => void; disabled?: boolean }) {
  return <div className="flex flex-col gap-2">
    {pairs.map((pair, index) => <div key={index} className="flex min-w-0 items-center gap-2">
      <Checkbox aria-label={`Ativar ${label} ${index + 1}`} checked={pair.enabled} disabled={disabled} className="cursor-pointer" onCheckedChange={enabled => onChange(pairs.map((item, i) => i === index ? { ...item, enabled: enabled === true } : item))} />
      <Input aria-label={`Nome ${label} ${index + 1}`} placeholder="Nome" value={pair.name} disabled={disabled} className="min-w-0 flex-1 font-mono text-xs" onChange={event => onChange(pairs.map((item, i) => i === index ? { ...item, name: event.target.value } : item))} />
      <Input aria-label={`Valor ${label} ${index + 1}`} placeholder="Valor ou {{variável}}" value={pair.value} disabled={disabled} className="min-w-0 flex-1 font-mono text-xs" onChange={event => onChange(pairs.map((item, i) => i === index ? { ...item, value: event.target.value } : item))} />
      <Hint content="Remover linha"><Button variant="ghost" size="icon-sm" aria-label={`Remover ${label} ${index + 1}`} disabled={disabled} className="cursor-pointer" onClick={() => onChange(pairs.filter((_, i) => i !== index))}><Trash2 /></Button></Hint>
    </div>)}
    <Button variant="outline" size="sm" disabled={disabled} className="w-fit cursor-pointer" onClick={() => onChange([...pairs, { name: "", value: "", enabled: true }])}><Plus />Adicionar {label}</Button>
  </div>;
}
