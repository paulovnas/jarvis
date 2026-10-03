import { Hint } from "@/components/ui/hint";
import { generationRate, type Generation } from "@/core/generation";

export function GenerationRate({ generation }: { generation?: Generation | null }) {
  const rate = generationRate(generation);
  if (rate === null || !generation) return null;
  const value = rate.toLocaleString("pt-BR", { minimumFractionDigits: 1, maximumFractionDigits: 1 });
  const kind = generation.estimated ? "estimada" : "medida";
  const explanation = `Média ${kind} de tokens de saída por segundo durante a geração do modelo, incluindo raciocínio quando informado pelo provedor. O tempo das ferramentas não entra.${generation.estimated ? " A contagem é aproximada até o provedor informar o uso." : ""}`;
  return <Hint content={explanation}>
    <span aria-label={`Velocidade média ${kind}: ${value} tokens por segundo`} className="shrink-0 whitespace-nowrap font-mono tabular-nums">
      {generation.estimated ? "≈ " : ""}{value} tok/s
    </span>
  </Hint>;
}
