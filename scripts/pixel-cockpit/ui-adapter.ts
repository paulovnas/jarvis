import ts from "typescript";
import { localizedLabels } from "./localized-labels";
import { advancedLabels } from "./advanced-labels";
import { assetLabels } from "./asset-labels";
import { promptExampleLabels } from "./prompt-example-labels";
import { statusPatterns } from "./status-patterns";
import { statusLabels } from "./status-labels";

const labels: Record<string, string> = {
  "Image Cockpit for Codex Workflows": "Suíte de pixel art",
  "v0.2.1": "Motor Image Cockpit 0.2.1 · MIT",
  "Animation Timeline Review / Edit": "Revisão e edição da linha do tempo",
  "Edit timing and game metadata without regenerating PNG frames.": "Edite o tempo e os metadados do jogo preservando os quadros PNG.",
  "Saved": "Salvo", "Unsaved": "Não salvo", "Metadata only": "Somente metadados", "Direction": "Direção",
  "Play": "Reproduzir", "Pause": "Pausar", "Speed": "Velocidade", "Undo": "Desfazer", "Redo": "Refazer",
  "Recipe defaults": "Restaurar receita", "Hold +100ms": "Pausar +100 ms", "Duplicate": "Duplicar",
  "Reverse": "Inverter", "Loop": "Repetição", "One shot": "Uma vez", "Events": "Eventos",
  "Recipe / User origin": "Origem: receita / usuário", "Game coordinates": "Coordenadas no jogo",
  "Selected frame": "Quadro selecionado", "Close": "Fechar", "Save timeline": "Salvar linha do tempo",
  "Pack v2 Round-trip": "Verificar pacote v2", "Loop start": "Início da repetição", "Loop seam": "Transição da repetição",
  "Animation Review Cockpit": "Revisão de animações", "Candidate compare, QC and adoption": "Comparação, qualidade e adoção dos candidatos",
  "Calibration JSON": "JSON de calibração", "Close review": "Fechar revisão",
  "Synchronized playback controls": "Controles de reprodução sincronizada", "Previous frame": "Quadro anterior", "Next frame": "Próximo quadro",
  "Previous direction": "Direção anterior", "Next direction": "Próxima direção", "Normalized": "Normalizado", "Raw": "Original",
  "Source": "Referência", "Autoplay paused by reduced motion": "Reprodução automática pausada pela preferência de movimento reduzido",
  "Compare every candidate at the same direction, frame and speed.": "Compare os candidatos na mesma direção, quadro e velocidade.",
  "Loading candidate artifacts…": "Carregando arquivos dos candidatos…", "Failed": "Falhou", "Human Review": "Revisão visual",
  "Onion": "Quadros vizinhos", "Diff": "Diferenças", "Ground": "Chão", "Pivot": "Pivô", "Socket": "Ponto de encaixe",
  "Hitbox": "Área de impacto", "Hurtbox": "Área vulnerável", "Background": "Fundo", "Checker": "Transparência",
  "Light": "Claro", "Dark": "Escuro", "Winner": "Vencedor", "Reject": "Rejeitar", "Hold": "Aguardar",
  "Save review": "Salvar revisão", "Adopt winner": "Adotar vencedor", "Repair direction": "Corrigir direção",
  "VFX Composite Stage": "Composição de efeitos", "Close VFX Composite Stage": "Fechar composição",
  "Character animation result": "Animação do personagem", "Effect animation result": "Animação do efeito",
  "Open character result": "Abrir personagem", "Open effect result": "Abrir efeito", "peak sync": "Sincronizar pico", "start sync": "Sincronizar início",
  "Safe area": "Área segura", "Effect bounds": "Limites do efeito", "Clipping guide": "Guia de recorte",
  "No character frame": "Nenhum quadro de personagem", "Pause composite": "Pausar composição", "Play composite": "Reproduzir composição",
  "Composite frame scrubber": "Posição na composição", "Composite direction": "Direção da composição",
  "Legacy Effect Pack / uncalibrated": "Pacote de efeito legado / sem calibração", "Reset": "Restaurar",
  "Save setup": "Salvar configuração", "Socket / Event": "Ponto de encaixe / evento", "Event": "Evento",
  "Effect event sync mode": "Sincronização do efeito com o evento", "Basic offset": "Deslocamento",
  "Offset X": "Deslocamento X", "Offset Y": "Deslocamento Y", "Layer": "Camada", "Blend": "Mistura",
  "Front": "Frente", "Back": "Atrás", "Additive": "Aditiva", "Screen": "Clarear", "Advanced adjustments": "Ajustes avançados",
  "Scale": "Escala", "Rotation": "Rotação", "Opacity": "Opacidade", "Time scale": "Escala de tempo",
  "Start frame": "Quadro inicial", "Peak frame": "Quadro de pico", "Export": "Exportar", "Import": "Importar",
  "Import composite": "Importar composição", "Export layers": "Exportar camadas", "Export manifest": "Exportar manifesto",
  "Composite Pack": "Pacote da composição", "PNG sheet": "Atlas PNG", "Checkerboard": "Transparência",
  "Preview": "Prévia", "Overlays": "Sobreposições", "Quality": "Qualidade", "Notes": "Notas",
  "Advanced diagnostics": "Diagnóstico avançado",
  "Store human judgment as a separate calibration layer.": "Salve a revisão visual como uma avaliação separada das métricas automáticas.",
  "Reject reason tags": "Motivos da rejeição", "Review note": "Nota da revisão",
  "Visual judgment, adoption reason, regeneration condition": "Avaliação visual, motivo da adoção e condição para gerar novamente",
  "Saving…": "Salvando…", "Save failed": "Falha ao salvar", "Starting…": "Iniciando…",
  "Direction × Frame QC Matrix": "Qualidade por direção e quadro", "Pass": "Aprovado", "Warning": "Aviso", "Fail": "Falha",
  "QC heatmap": "Mapa de qualidade", "Adjacent diff": "Diferença entre quadros vizinhos",
  "Translate X": "Deslocamento X", "Translate Y": "Deslocamento Y",
  "Frame-range Repair": "Corrigir intervalo de quadros",
  "Frame-range repair is disabled because the generator cannot yet preserve the untouched frame range.": "A correção por intervalo está desativada porque o gerador ainda não garante preservar os demais quadros.",
  "Disabled until untouched-frame preservation can be verified.": "Indisponível até ser possível verificar a preservação dos demais quadros.",
  "Preview Studio": "Inspeção visual", "Inspect game coordinates and the loop seam together.": "Inspecione as coordenadas no jogo e a transição da repetição.",
  "Prev/next diff": "Diferenças anterior/seguinte", "Select an adoption candidate": "Selecione um candidato para adotar",
  "Confirm automatic reasons and warnings before adoption.": "Confira os motivos automáticos e avisos antes de adotar.",
  "Adopting…": "Adotando…", "Adopt selected candidate": "Adotar candidato selecionado",
  "Phase 7 / non-destructive layers": "Composição com camadas preservadas",
  "Sync separate character and effect layers by socket and event.": "Sincronize as camadas de personagem e efeito por ponto de encaixe e evento.",
  "Character": "Personagem", "Effect": "Efeito", "Centroid": "Centro de energia", "Palette": "Paleta",
  "non-destructive": "Preserva os arquivos originais", "Separate layers remain reconstructable in-engine": "As camadas separadas podem ser reconstruídas no jogo",
  "Layer Sheets": "Atlas das camadas", "Manifest JSON": "Manifesto JSON", "Reimport Pack": "Reimportar pacote",
  "Combined GIF": "GIF combinado", "Combined APNG": "APNG combinado", "Composite Sheet": "Atlas combinado",
  "Exporting…": "Exportando…", "Composite Pack ZIP": "Pacote combinado ZIP",
  "Pack v2 round-trip: identical": "Pacote v2 exportado e reimportado sem alterações",
  "source": "origem",
  ...localizedLabels,
  ...advancedLabels,
  ...assetLabels,
  ...promptExampleLabels,
  ...statusLabels,
  "Supervisor off": "Hospedagem gerenciada pelo Jarvis",
  "Broken": "Com falha", "Repairing": "Reparando", "Checking": "Verificando",
  "Cockpit:": "Suíte:", "Runner": "Executor",
  "Columns": "Colunas", "Rows": "Linhas", "Gutter": "Espaçamento",
  "Add favorite": "Adicionar aos favoritos", "Remove favorite": "Remover dos favoritos",
  "PILOT ·": "PILOTO ·", "Prompt": "Instruções", "sync": "· sincronização",
  "Safe modeで開く": "Abrir no modo de recuperação",
  "通常起動へ戻る / export": "Voltar ao modo normal / exportar",
  "localhostで新規origin": "Abrir um espaço local limpo",
};

const visibleEnumLabels: Record<string, string> = {
  inbox:"caixa de entrada", example:"exemplo", annotated:"anotação",
  GOLD:"Ouro", SILVER:"Prata", BRONZE:"Bronze", FAILED:"Falhou", BLOCKED:"Bloqueado",
  gold:"Ouro", silver:"Prata", bronze:"Bronze",
  front: "Frente", back: "Costas", side: "Lateral", "front three-quarter": "Frente em 3/4", "back three-quarter": "Costas em 3/4",
  startup: "Preparação", active: "Ativo", impact: "Impacto", recovery: "Recuperação", "loop-point": "Ponto de repetição",
  recipe: "Receita", user: "Usuário", source: "Referência", frame: "Quadro", warning: "Aviso",
  estimated: "Estimado", fallback: "Alternativa", queued: "Na fila", running: "Executando", completed: "Concluído",
  failed: "Falhou", cancelled: "Cancelado", waiting: "Aguardando", review: "Revisão", "pilot-review": "Revisão do piloto",
  pivots: "Pivôs", anchors: "Âncoras", sockets: "Pontos de encaixe", hitboxes: "Áreas de impacto", hurtboxes: "Áreas vulneráveis",
  winner: "Vencedor", hold: "Aguardar", reject: "Rejeitar", piloting: "Gerando piloto", repairing: "Corrigindo",
  checkerboard: "Transparência", light: "Claro", dark: "Escuro", game: "Jogo", start: "Início", peak: "Pico",
  core: "Essenciais", locomotion: "Locomoção", combat: "Combate", magic: "Magia", social: "Interação", utility: "Utilitários",
  pass: "Aprovado", fail: "Falha", "identity-drift": "Alteração da identidade", "palette-drift": "Alteração da paleta",
  "silhouette-break": "Quebra da silhueta", "footline-drift": "Deslocamento dos pés", "loop-seam": "Emenda da repetição",
  "phase-mismatch": "Fases incompatíveis", "motion-weak": "Movimento pouco perceptível", "over-correction": "Correção excessiva",
  clipping: "Recorte indevido", "static-copy": "Cópia estática",
  motion: "movimento", "walk motion": "movimento de caminhada", "idle breathing motion": "respiração em repouso",
  "subtle motion": "movimento sutil", footline: "pés", "hover-height": "altura de flutuação",
  "wing-beat": "batida das asas", "ground-contact": "contato com o chão", "body-contact": "contato do corpo",
  order: "ordem", durations: "durações", events: "eventos", "order, durations": "ordem, durações",
  "order, events": "ordem, eventos", "durations, events": "durações, eventos", "order, durations, events": "ordem, durações, eventos",
};

/** Replace presentation controls only; original callbacks, algorithm data and file schemas stay unchanged. */
export function adoptUi(source: string) {
  source = source.replace('new Intl.DateTimeFormat(undefined, { month:', 'new Intl.DateTimeFormat("pt-BR", { month:');
  const file = ts.createSourceFile("suite.tsx", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const edits: { start: number; end: number; text: string }[] = [];
  const used = new Set<string>();
  const dialogs = new Set<ts.JsxElement>();
  let runtimeLabels = false;
  const edit = (node: ts.Node, text: string) => edits.push({ start: node.getStart(file), end: node.end, text });
  const names: Record<string, string> = {
    button: "JarvisButton", input: "JarvisInput", textarea: "JarvisTextarea",
    select: "JarvisNativeSelect", option: "JarvisNativeSelectOption", optgroup: "JarvisNativeSelectOptGroup",
    details: "JarvisDisclosure", summary: "JarvisDisclosureTrigger",
  };
  function visit(node: ts.Node) {
    if (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) {
      const original = node.tagName.getText(file);
      let name = names[original];
      if (original === "details" && ts.isJsxElement(node.parent)) {
        const summary = node.parent.children.find((child): child is ts.JsxElement =>
          ts.isJsxElement(child) && child.openingElement.tagName.getText(file) === "summary");
        if (summary) {
          edits.push({start:summary.end,end:summary.end,text:"<JarvisDisclosureContent>"});
          edits.push({start:node.parent.closingElement.getStart(file),end:node.parent.closingElement.getStart(file),text:"</JarvisDisclosureContent>"});
        }
      }
      if (ts.isJsxOpeningElement(node) && ts.isJsxElement(node.parent)) {
        const role = node.attributes.properties.find((attribute): attribute is ts.JsxAttribute =>
          ts.isJsxAttribute(attribute) && attribute.name.getText(file) === "role");
        const modal = role?.initializer && ts.isStringLiteral(role.initializer) && role.initializer.text === "dialog";
        const backdrop = node.parent.parent;
        if (modal && ts.isJsxElement(backdrop) && !dialogs.has(backdrop)) {
          dialogs.add(backdrop);
          used.add("JarvisDialog");
          used.add("JarvisDialogContent");
          edit(backdrop.openingElement.tagName, "JarvisDialog");
          edit(backdrop.closingElement.tagName, "JarvisDialog");
          edits.push({start:backdrop.openingElement.tagName.end,end:backdrop.openingElement.attributes.end,text:' open onOpenChange={(open) => { if (!open) onClose(); }}'});
          edit(node.tagName, "JarvisDialogContent");
          edit(node.parent.closingElement.tagName, "JarvisDialogContent");
          edits.push({start:node.tagName.end,end:node.tagName.end,text:' showCloseButton={false} data-jarvis-suite-dialog=""'});
        }
      }
      if (original === "input") {
        const type = node.attributes.properties.find((attribute): attribute is ts.JsxAttribute =>
          ts.isJsxAttribute(attribute) && attribute.name.getText(file) === "type");
        if (type?.initializer && ts.isStringLiteral(type.initializer) && type.initializer.text === "checkbox") {
          name = "JarvisCheckbox";
          edit(type, "");
          const change = node.attributes.properties.find((attribute): attribute is ts.JsxAttribute =>
            ts.isJsxAttribute(attribute) && attribute.name.getText(file) === "onChange");
          if (change?.initializer && ts.isJsxExpression(change.initializer) && change.initializer.expression) {
            const expression = change.initializer.expression;
            const param = ts.isArrowFunction(expression) ? expression.parameters[0]?.name.getText(file) : undefined;
            let handler = expression.getText(file);
            if (param) handler = handler.replaceAll(`${param}.target.checked`, param).replaceAll(`${param}.currentTarget.checked`, param);
            edit(change, `onCheckedChange={${handler}}`);
          }
        }
      }
      if (name) {
        used.add(name);
        edit(node.tagName, name);
        if (original === "button") edits.push({ start: node.tagName.end, end: node.tagName.end, text: ' variant="outline"' });
      }
    } else if (ts.isJsxClosingElement(node)) {
      const name = names[node.tagName.getText(file)];
      if (name) edit(node.tagName, name);
    } else if (ts.isJsxExpression(node) && node.expression) {
      const expression = node.expression;
      const text = expression.getText(file);
      const parent = node.parent;
      const isChild = ts.isJsxElement(parent) || ts.isJsxFragment(parent);
      const isLabelAttribute = ts.isJsxAttribute(parent) && ["title", "aria-label", "alt", "placeholder", "label"].includes(parent.name.getText(file));
      let containsElement = false;
      const scan = (child: ts.Node) => {
        if (ts.isJsxElement(child) || ts.isJsxSelfClosingElement(child) || ts.isJsxFragment(child)) containsElement = true;
        else ts.forEachChild(child, scan);
      };
      scan(expression);
      // Localized recipe records are also used by the generation compiler. Translate
      // their rendered text, never those records, original prompts, or execution logs.
      const originalContent = /\blog\.(?:text|error)\b|(?:\.|\b)(?:prompt|negativePrompt|jobNotes)\b/.test(text);
      if ((isChild || isLabelAttribute) && !containsElement && !originalContent) {
        runtimeLabels = true;
        edit(expression, `jarvisUiText(${text})`);
        return;
      }
    } else if (ts.isJsxText(node)) {
      const text = node.text.trim();
      if (labels[text]) edit(node, ` ${labels[text]} `);
    } else if (ts.isStringLiteral(node) && labels[node.text]) {
      const parent = node.parent;
      const uiCall = ts.isCallExpression(parent) && /^(setMessage|setStatus|setActionError|onStatus)$/.test(parent.expression.getText(file));
      let ancestor: ts.Node | undefined = parent;
      while (ancestor && !ts.isJsxExpression(ancestor) && !ts.isStatement(ancestor)) ancestor = ancestor.parent;
      const uiConditional = ts.isConditionalExpression(parent) && (
        /(?:\blanguage\b|\bisJa\b|\bja\b)/.test(parent.condition.getText(file))
        || Boolean(ancestor && ts.isJsxExpression(ancestor))
      );
      if (ts.isJsxAttribute(parent) && ["title", "aria-label", "alt", "placeholder", "label"].includes(parent.name.getText(file))
        || uiConditional
        || uiCall) {
        edit(node, JSON.stringify(labels[node.text]));
      }
    } else if (ts.isTemplateExpression(node) && /^(Timeline frame |Selected frame |Frame |VFX Composite export:)/.test(node.head.text)) {
      let ancestor: ts.Node | undefined = node.parent;
      while (ancestor && !ts.isJsxExpression(ancestor) && !ts.isCallExpression(ancestor) && !ts.isStatement(ancestor)) ancestor = ancestor.parent;
      if (ancestor && (ts.isJsxExpression(ancestor) || ts.isCallExpression(ancestor) && /^(setMessage|setStatus|onStatus)$/.test(ancestor.expression.getText(file)))) {
        runtimeLabels = true;
        edit(node, `jarvisUiText(${node.getText(file)})`);
        return;
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(file);
  let result = source;
  for (const change of edits.sort((left, right) => right.start - left.start || right.end - left.end)) {
    result = result.slice(0, change.start) + change.text + result.slice(change.end);
  }
  const imports: Record<string, string> = {
    JarvisButton: 'import { Button as JarvisButton } from "@/components/ui/button";',
    JarvisInput: 'import { Input as JarvisInput } from "@/components/ui/input";',
    JarvisTextarea: 'import { Textarea as JarvisTextarea } from "@/components/ui/textarea";',
    JarvisCheckbox: 'import { Checkbox as JarvisCheckbox } from "@/components/ui/checkbox";',
    JarvisDialog: 'import { Dialog as JarvisDialog, DialogContent as JarvisDialogContent } from "@/components/ui/dialog";',
  };
  const native = [...used].filter((name) => name.startsWith("JarvisNative")).map((name) => `${name.slice(6)} as ${name}`);
  const disclosure = used.has("JarvisDisclosure") ? `
import { useEffect as useDisclosureEffect, useState as useDisclosureState, type ComponentProps as DisclosureProps } from "react";
import { Collapsible as JarvisDisclosureRoot, CollapsibleTrigger as JarvisDisclosureTrigger, CollapsibleContent as JarvisDisclosureContent } from "@/components/ui/collapsible";
function JarvisDisclosure({open = false, children, ...props}: Omit<DisclosureProps<typeof JarvisDisclosureRoot>,"onOpenChange">) {
  const [expanded, setExpanded] = useDisclosureState(open);
  useDisclosureEffect(() => setExpanded(open), [open]);
  return <JarvisDisclosureRoot {...props} open={expanded} onOpenChange={setExpanded}>{children}</JarvisDisclosureRoot>;
}
` : "";
  return [...used].filter((name) => imports[name]).map((name) => imports[name]).join("\n")
    + (native.length ? `\nimport { ${native.join(",")} } from "@/components/ui/native-select";\n` : "\n") + disclosure + (runtimeLabels ? presentationRuntimeSource() : "") + result;
}

/** Build-time output for the suite and chat receipts; no compiler dependency reaches the worker. */
export function presentationRuntimeSource() {
  return `
const jarvisVisibleLabels: Record<string,string> = ${JSON.stringify({...labels,...visibleEnumLabels})};
const jarvisVisiblePatterns = ${JSON.stringify([...statusPatterns].sort(([left], [right]) =>
  right.replace(/\$\{\d+\}/g, "").length - left.replace(/\$\{\d+\}/g, "").length).map(([source, target]) => [
  "^" + source.split(/\$\{\d+\}/).map((part) => part.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("([\\s\\S]*?)") + "$",
  target,
]))}.map(([source, target]) => ({pattern:new RegExp(source),target}));
function jarvisUiText<T>(value: T): T {
  if (typeof value !== "string") return value;
  const exact = jarvisVisibleLabels[value];
  if (exact) return exact as T;
  if (value.startsWith("Effect: ")) {
    const category = value.slice(8);
    return ("Efeito: " + (jarvisVisibleLabels[category] ?? category)) as T;
  }
  const effectMeta = /^(GOLD|SILVER|BRONZE) \\/ (\\d+)f \\/ (\\d+)px$/.exec(value);
  if (effectMeta) return (jarvisVisibleLabels[effectMeta[1]] + " / " + effectMeta[2] + " quadros / " + effectMeta[3] + " px") as T;
  const effectImport = /^(?:Effect imported|Efeito importado): (.+) \\((gold|silver|bronze|failed|blocked)(?:, (\\d+) warnings?)?\\)$/.exec(value);
  if (effectImport) return ("Efeito importado: " + effectImport[1] + " (" + jarvisVisibleLabels[effectImport[2]]
    + (effectImport[3] ? ", " + effectImport[3] + " avisos" : "") + ")") as T;
  for (const {pattern,target} of jarvisVisiblePatterns) {
    const match = pattern.exec(value);
    if (match) return target.replace(/\\$\\{(\\d+)\\}/g, (_all, index: string) => {
      const captured = match[Number(index)+1] ?? "";
      return captured.split("; ").map((part) => jarvisUiText(part)).join("; ");
    }) as T;
  }
  const prefixes: Record<string,string> = {"Timeline frame ":"Quadro da linha do tempo ","Selected frame ":"Quadro selecionado ","Frame ":"Quadro ","VFX Composite export:":"Exportação da composição de efeitos:"};
  for (const [prefix, translated] of Object.entries(prefixes)) if (value.startsWith(prefix)) return (translated + value.slice(prefix.length)) as T;
  return value
    .replace(/^(startup|active|impact|recovery|loop-point) \\((recipe|user)\\)$/, (_match, name: string, origin: string) => jarvisVisibleLabels[name] + " (" + jarvisVisibleLabels[origin] + ")")
    .replace(/^Cockpit OK\\. (\\d+) outbox results? available for Recover Results\\.$/, "Suíte funcionando. $1 resultados disponíveis para recuperar.")
    .replace(/^(\\d+) warning$/, "$1 avisos")
    .replace(/^ \\/ (\\d+) unimported$/, " / $1 não importados") as T;
}
`;
}
