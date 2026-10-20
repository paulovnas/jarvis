import { useEffect, useId, useState } from "react";
import { ChevronDown, Grid2X2, LoaderCircle } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { Toaster } from "@/components/ui/sonner";
import { productionRecipes } from "../../../scripts/pixel-cockpit/production-recipes";

type Data = Record<string, unknown>;
interface Picture { id: string; name?: string; dataUrl: string }
interface Props {
  selected?: Picture | null;
  selectedFrameId?: string;
  history: Picture[];
  frames: Picture[];
  grid: { columns: number; rows: number };
  busy?: boolean;
  execute: (action: string, args: Data) => Promise<unknown>;
  activity: () => unknown;
}
const record = (value: unknown): Data => value && typeof value === "object" ? value as Data : {};

export function PixelProcessingPanel({ selected, selectedFrameId, history, frames, grid, busy, execute, activity }: Props) {
  const id = useId();
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [operation, setOperation] = useState<Data>({});
  const [target, setTarget] = useState("image");
  const [chosenFrameId, setFrameId] = useState<string>();
  const [manual, setManual] = useState(false);
  const [pitchX, setPitchX] = useState(4);
  const [pitchY, setPitchY] = useState(4);
  const [phaseX, setPhaseX] = useState(0);
  const [phaseY, setPhaseY] = useState(0);
  const [colors, setColors] = useState(256);
  const [scale, setScale] = useState(1);
  const [layout, setLayout] = useState<{ key: string; columns: number; rows: number }>();
  const layoutKey = (selected?.id ?? "") + ":" + grid.columns + ":" + grid.rows;
  const columns = layout?.key === layoutKey ? layout.columns : grid.columns;
  const rows = layout?.key === layoutKey ? layout.rows : grid.rows;
  const setColumns = (value: number) => setLayout({ key: layoutKey, columns: value, rows });
  const setRows = (value: number) => setLayout({ key: layoutKey, columns, rows: value });
  const [padding, setPadding] = useState(2);
  const [minArea, setMinArea] = useState(1);
  const [poseCells, setPoseCells] = useState("");
  const [alignment, setAlignment] = useState("preserve");
  const [timeline, setTimeline] = useState(true);
  const [removeChroma, setRemoveChroma] = useState(false);
  const [chroma, setChroma] = useState("#00ff00");
  const [tolerance, setTolerance] = useState(30);
  const [alpha, setAlpha] = useState(16);
  const [recipe, setRecipe] = useState<string>("neutral-anchor");
  const [direction, setDirection] = useState("south");
  const frameId = chosenFrameId !== undefined && (chosenFrameId === "" || frames.some((frame) => frame.id === chosenFrameId))
    ? chosenFrameId : selectedFrameId ?? frames[0]?.id ?? "";
  useEffect(() => {
    const update = () => {
      const next = record(activity());
      setOperation((previous) => JSON.stringify(previous) === JSON.stringify(next) ? previous : next);
    };
    update();
    const timer = setInterval(update, 400);
    return () => clearInterval(timer);
  }, [activity]);
  const waiting = pending || busy || operation.status === "running";
  const hasSource = target === "frame" ? frames.some((frame) => frame.id === frameId) : Boolean(selected);
  const source = target === "frame" ? { frameId } : { sourceId: selected?.id };
  const shared: Data = { alphaThreshold: alpha, tolerance, ...(removeChroma ? { chromaKey: [1, 3, 5].map((offset) => parseInt(chroma.slice(offset, offset + 2), 16)) } : {}) };
  const result = record(operation.result);
  const report = record(result.report);
  const ids = Array.isArray(result.historyIds) ? result.historyIds : [];
  const after = history.find((item) => item.id === ids[0]);
  const before = frames.find((item) => item.id === result.sourceFrameId) ?? history.find((item) => item.id === result.sourceId);
  const warnings = Array.isArray(report.warnings) ? report.warnings.map(String) : [];
  async function run(action: string, args: Data) {
    setPending(true); setError("");
    try {
      if (action === "extract_poses" && poseCells.trim()) {
        const cells=poseCells.split(",").map((value) => Number(value.trim())-1);
        if (cells.some((value) => !Number.isInteger(value) || value<0 || value>=columns*rows) || new Set(cells).size!==cells.length) throw new Error("Informe células válidas, sem repetir, começando em 1.");
        args={...args,options:{...record(args.options),poseCells:cells}};
      }
      await execute(action, args); toast.success(action === "production_recipe" ? "Receita aplicada" : "Derivados criados; original preservado");
    }
    catch (cause) { const message = cause instanceof Error ? cause.message : String(cause); setError(message); toast.error(message); }
    finally { setPending(false); }
  }
  const numberField = (label: string, value: number, change: (value: number) => void, min: number, max: number, step=1) => (
    <Label className="grid gap-1 text-xs">
      {label}<Input aria-label={label} type="number" value={value} min={min} max={max} step={step} disabled={Boolean(waiting)} onChange={(event) => change(Number(event.target.value))} className="h-8"/>
    </Label>
  );
  return (
    <Collapsible data-pixel-processing open={open} onOpenChange={setOpen} className="rounded-lg border border-border bg-muted/20 text-foreground">
      <CollapsibleTrigger render={<Button variant="ghost" className="h-auto w-full cursor-pointer justify-between px-3 py-2 text-sm"/>}>
          <span className="flex items-center gap-2"><Grid2X2 className="size-4" aria-hidden="true"/>Preparar pixel art</span>
          <ChevronDown className="size-4" aria-hidden="true"/>
      </CollapsibleTrigger>
      <CollapsibleContent className="space-y-3 px-3 pb-3">
        <p className="text-xs text-muted-foreground">Processamento local em cópias. As referências e ações existentes são preservadas.</p>
        <Label className="grid gap-1 text-xs">Alvo
          <NativeSelect aria-label="Alvo do processamento" value={target} disabled={Boolean(waiting)} onChange={(event) => setTarget(event.target.value)} className="w-full cursor-pointer">
            <NativeSelectOption value="image">Imagem selecionada</NativeSelectOption>
            <NativeSelectOption value="frame" disabled={!frames.length}>Quadro da linha do tempo</NativeSelectOption>
          </NativeSelect>
        </Label>
        {target === "frame" && <NativeSelect aria-label="Quadro do processamento" value={frameId} onChange={(event) => setFrameId(event.target.value)} className="w-full cursor-pointer">
          <NativeSelectOption value="">Selecione um quadro</NativeSelectOption>
          {frames.map((frame, index) => <NativeSelectOption key={frame.id} value={frame.id}>{frame.name ?? "Quadro " + (index + 1)}</NativeSelectOption>)}
        </NativeSelect>}
        <div className="space-y-2 border-t border-border pt-3">
          <h3 className="text-sm font-medium">Recuperação da grade</h3>
          <Label htmlFor={id + "-manual"} className="flex cursor-pointer items-center gap-2 text-xs">
            <Checkbox id={id + "-manual"} checked={manual} onCheckedChange={(value) => setManual(value === true)} className="cursor-pointer"/>Definir grade manualmente
          </Label>
          {manual && <div className="grid grid-cols-2 gap-2">
            {numberField("Largura do pixel", pitchX, setPitchX, 1, 128, 0.01)}{numberField("Altura do pixel", pitchY, setPitchY, 1, 128, 0.01)}
            {numberField("Origem X da grade", phaseX, setPhaseX, 0, 127.99, 0.01)}{numberField("Origem Y da grade", phaseY, setPhaseY, 0, 127.99, 0.01)}
          </div>}
          <div className="grid grid-cols-2 gap-2">
            {numberField("Máximo de cores", colors, setColors, 2, 256)}{numberField("Ampliação inteira", scale, setScale, 1, 16)}
          </div>
          <Button variant="outline" className="w-full cursor-pointer" disabled={!hasSource || Boolean(waiting)}
            onClick={() => void run("recover_grid", { ...source, options: { ...shared, paletteColors: colors, scale, ...(manual ? { cellWidth: pitchX, cellHeight: pitchY, phaseX, phaseY } : {}) } })}>Recuperar grade</Button>
        </div>
        <div className="space-y-2 border-t border-border pt-3">
          <h3 className="text-sm font-medium">Extração de poses</h3>
          <p className="text-xs text-muted-foreground">A grade orienta o agrupamento; partes que cruzam suas divisões são recuperadas completas.</p>
          <div className="grid grid-cols-2 gap-2">
            {numberField("Colunas de poses", columns, setColumns, 1, 32)}{numberField("Linhas de poses", rows, setRows, 1, 32)}
            {numberField("Margem dos quadros", padding, setPadding, 0, 64)}{numberField("Área mínima da parte", minArea, setMinArea, 1, 10000)}
          </div>
          <Label className="grid gap-1 text-xs">Células com poses
            <Input aria-label="Células com poses" placeholder="Automático; exemplo: 1, 2, 6" value={poseCells} onChange={(event) => setPoseCells(event.target.value)} disabled={Boolean(waiting)}/>
          </Label>
          <p className="text-xs text-muted-foreground">Opcional: numere da esquerda para a direita, de cima para baixo. Use quando partes soltas ocuparem uma célula vazia.</p>
          <NativeSelect aria-label="Alinhamento das poses" value={alignment} onChange={(event) => setAlignment(event.target.value)} className="w-full cursor-pointer">
            <NativeSelectOption value="preserve">Preservar deslocamentos, incluindo saltos</NativeSelectOption>
            <NativeSelectOption value="bottom">Alinhar explicitamente pela base</NativeSelectOption>
          </NativeSelect>
          <Label htmlFor={id + "-timeline"} className="flex cursor-pointer items-center gap-2 text-xs">
            <Checkbox id={id + "-timeline"} checked={timeline} onCheckedChange={(value) => setTimeline(value === true)} className="cursor-pointer"/>Adicionar como uma nova ação
          </Label>
          <Button variant="outline" className="w-full cursor-pointer" disabled={!hasSource || Boolean(waiting)}
            onClick={() => void run("extract_poses", { ...source, addToTimeline: timeline, options: { ...shared, columns, rows, padding, minArea, alignment } })}>Extrair poses</Button>
        </div>
        <div className="space-y-2 border-t border-border pt-3">
          <h3 className="text-sm font-medium">Fundo e transparência</h3>
          <Label htmlFor={id + "-chroma"} className="flex cursor-pointer items-center gap-2 text-xs">
            <Checkbox id={id + "-chroma"} checked={removeChroma} onCheckedChange={(value) => setRemoveChroma(value === true)} className="cursor-pointer"/>Remover a cor escolhida
          </Label>
          {removeChroma && <Input aria-label="Cor de fundo a remover" type="color" value={chroma} onChange={(event) => setChroma(event.target.value)} className="h-8 cursor-pointer"/>}
          <div className="grid grid-cols-2 gap-2">
            {numberField("Tolerância da cor", tolerance, setTolerance, 0, 120)}{numberField("Alfa mínimo", alpha, setAlpha, 1, 255)}
          </div>
        </div>
        <div className="space-y-2 border-t border-border pt-3">
          <h3 className="text-sm font-medium">Receitas de produção</h3>
          <NativeSelect aria-label="Receita de produção" value={recipe} onChange={(event) => setRecipe(event.target.value)} className="w-full cursor-pointer">
            {productionRecipes.map((item) => <NativeSelectOption key={item.id} value={item.id}>{item.title}</NativeSelectOption>)}
          </NativeSelect>
          {recipe === "directional-anchor" && <Input aria-label="Direção da referência" value={direction} onChange={(event) => setDirection(event.target.value)}/>}
          <p className="text-xs text-muted-foreground">As receitas mantêm identidade, assimetrias e fases da ação. Aplicar prepara os parâmetros; a geração continua no botão existente.</p>
          <Button variant="outline" className="w-full cursor-pointer" disabled={Boolean(waiting) || (!selected && productionRecipes.find((item) => item.id === recipe)?.kind === "source")}
            onClick={() => void run("production_recipe", { id: recipe, direction, sourceId: selected?.id })}>Aplicar receita</Button>
        </div>
        {waiting && <div role="status" className="flex items-center gap-2 text-xs"><LoaderCircle className="size-3 animate-spin motion-reduce:animate-none" aria-hidden="true"/>{String(operation.status === "running" ? operation.phase ?? "Processando" : "Preparando processamento")}</div>}
        {error && <p role="alert" className="text-xs text-destructive">{error}</p>}
        {after && <div className="grid grid-cols-2 gap-2" aria-label="Comparação do processamento">
          {before && <figure><img src={before.dataUrl} alt="Imagem original preservada" className="h-24 w-full object-contain [image-rendering:pixelated]"/><figcaption className="text-xs text-muted-foreground">Original</figcaption></figure>}
          <figure><img src={after.dataUrl} alt="Resultado do processamento" className="h-24 w-full object-contain [image-rendering:pixelated]"/><figcaption className="text-xs text-muted-foreground">Derivado</figcaption></figure>
        </div>}
        {warnings.length > 0 && <ul className="space-y-1 text-xs text-muted-foreground">{warnings.map((warning, index) => <li key={index}>{warning}</li>)}</ul>}
        <Toaster position="bottom-right"/>
      </CollapsibleContent>
    </Collapsible>
  );
}
