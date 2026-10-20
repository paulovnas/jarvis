import { expect, it } from "vitest";
import ts from "typescript";
import { adoptUi } from "./ui-adapter";

it("translates visible review decisions, diagnostic details and accessible controls without changing their IDs", () => {
  const source = adoptUi('function View(){return <><input aria-label="Add favorite" /><img alt="No runner detail available." /><NumberField label={fieldName} /><span>{message}</span></>;}');
  expect(source).toContain('aria-label="Adicionar aos favoritos"');
  expect(source).toContain('alt="Nenhum detalhe do executor disponível."');
  expect(source).toContain('label={jarvisUiText(fieldName)}');
  const compiled = ts.transpileModule(source, {compilerOptions:{target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.React,module:ts.ModuleKind.CommonJS}}).outputText;
  const runtime = compiled.slice(compiled.indexOf("const jarvisVisibleLabels"), compiled.indexOf("function View"));
  const translate = new Function(`${runtime}; return jarvisUiText;`)() as (value: string) => string;
  const cases: Record<string, string> = {
    winner: "Vencedor", hold: "Aguardar", reject: "Rejeitar", piloting: "Gerando piloto",
    checkerboard: "Transparência", dark: "Escuro", game: "Jogo", start: "Início", peak: "Pico",
    "identity-drift": "Alteração da identidade", "footline-drift": "Deslocamento dos pés",
    "fingerprinting...": "Identificando a referência...",
    "raw center drift 12px": "Deslocamento do centro original: 12 px",
    "normalization resized 25% of frames": "A normalização redimensionou 25% dos quadros",
    "5 retained, largest 3": "5 preservados; maior item: 3",
    "front: walk motion too small (0.4% average frame change)": "Frente: movimento de caminhada insuficiente (0.4% de mudança média entre quadros)",
    "front frame 2: chroma residue 12%": "Frente quadro 2: 12% de resíduos da cor de recorte",
    "front uses explicit direction override: order, durations.": "Frente usa configurações específicas da direção: ordem, durações.",
    "Effect QC quarantined for job-a: all frames are static copies; alpha transparency is missing from all frames. No history or final download item was added.": "O efeito de job-a foi separado por falha na verificação de qualidade: Todos os quadros são cópias estáticas; Todos os quadros estão sem transparência. Nenhum resultado final foi adicionado ao histórico ou disponibilizado para baixar.",
    "history cleared. Open the normal URL to start with a clean local state.": "Histórico: limpeza concluída. Reabra a suíte para começar com o estado local redefinido.",
    "animation library localStorage cleared. IndexedDB was unavailable or blocked; close other tabs and retry if needed.": "Biblioteca de animações: armazenamento local limpo. O IndexedDB estava indisponível ou bloqueado; feche outras abas e tente novamente, se necessário.",
  };
  for (const [original, expected] of Object.entries(cases)) expect(translate(original)).toBe(expected);
  expect(adoptUi('const state = {decision:"winner",background:"checkerboard",reason:"identity-drift"};')).toContain('decision:"winner",background:"checkerboard",reason:"identity-drift"');
});

it("uses registry disclosure controls and preserves their original initial state and content", () => {
  const adapted = adoptUi('function View() { return <details className="advanced" open={ready}><summary>Advanced diagnostics</summary><div>{value}</div></details>; }');
  expect(adapted).toContain('<JarvisDisclosure className="advanced" open={ready}>');
  expect(adapted).toContain("<JarvisDisclosureContent><div>{jarvisUiText(value)}</div></JarvisDisclosureContent>");
  expect(adapted).toContain('from "@/components/ui/collapsible"');
  expect(adapted).toContain("useDisclosureEffect(() => setExpanded(open), [open])");
});

it("uses registry controls while preserving actions, option IDs and checkbox booleans", () => {
  const source = `function View() { return <><button onClick={save}>Save timeline</button><input type="checkbox" checked={playing} onChange={(event) => setPlaying(event.target.checked)} /><select value={mode} onChange={(e) => update(e.target.value)}><option value="front">Front</option></select></>; }`;
  const adapted = adoptUi(source);
  expect(adapted).toContain('<JarvisButton variant="outline" onClick={save}> Salvar linha do tempo </JarvisButton>');
  expect(adapted).toContain("onCheckedChange={(event) => setPlaying(event)}");
  expect(adapted).toContain('<JarvisNativeSelectOption value="front"> Frente </JarvisNativeSelectOption>');
  expect(adapted).toContain("update(e.target.value)");
  expect(adapted).toContain("@/components/ui/checkbox");
});

it("does not translate schema values, generation prompts or image pixels", () => {
  const source = 'const prompt = "Front"; const color = "#00ff00"; const schema = {title:"Animation Timeline Review / Edit"}; const quality = flag ? "high" : "low";';
  expect(adoptUi(source)).toContain(source);
});

it("translates visible metadata and statuses without mutating protocol IDs", () => {
  const adapted = adoptUi('const state = "front"; const modes = [{id:"foot-drift",ja:"接地ずれ",en:"Foot drift"}]; function View(){setMessage("Composite setup saved"); return <><option value={direction}>{direction}</option><span>{event.name}<small>{event.origin}</small></span><small>{`Selected frame ${frameIndex + 1}`}</small></>;}');
  expect(adapted).toContain('const state = "front"');
  expect(adapted).toContain('id:"foot-drift"');
  expect(adapted).toContain('en:"Foot drift"');
  expect(adapted).toContain('setMessage("Configuração da composição salva")');
  expect(adapted).toContain('value={direction}>{jarvisUiText(direction)}');
  expect(adapted).toContain('{jarvisUiText(event.origin)}');
  expect(adapted).toContain('jarvisUiText(`Selected frame ${frameIndex + 1}`)');
});

it("translates dynamic recipe and history presentation while retaining original model inputs", () => {
  const adapted = adoptUi(`const recipe = {title:{en:"Walk Cycle",ja:"歩行"},prompt:"Walk Cycle"};
function View(){return <><strong>{localizedText(recipe.title, language)}</strong><small>{item.source}</small><div>{report.message}</div><pre>{log.error || log.text}</pre><p>{localizedText(preset.prompt, language)}</p></>;}`);
  expect(adapted).toContain('title:{en:"Walk Cycle",ja:"歩行"},prompt:"Walk Cycle"');
  expect(adapted).toContain("{jarvisUiText(localizedText(recipe.title, language))}");
  expect(adapted).toContain("{jarvisUiText(item.source)}");
  expect(adapted).toContain("<pre>{log.error || log.text}</pre>");
  expect(adapted).toContain("<p>{localizedText(preset.prompt, language)}</p>");
});

it("keeps numeric timeline options intact while translating directional labels", () => {
  const source = adoptUi('function View(){return <span>{direction}</span>;}');
  const compiled = ts.transpileModule(source, {compilerOptions:{target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.React}}).outputText;
  const translate = new Function(`${compiled}; return jarvisUiText;`)() as (value: string | number | undefined) => string | number | undefined;
  expect(translate(0.5)).toBe(0.5);
  expect(translate(undefined)).toBeUndefined();
  expect(translate("front")).toBe("Frente");
  expect(translate("Selected frame 3")).toBe("Quadro selecionado 3");
  expect(translate("Frame 3 duration")).toBe("Duração do quadro 3");
  expect(translate("Animation Review loaded: 2/3 candidates ready.")).toBe("Revisão carregada: 2/3 candidatos prontos.");
  expect(translate("Walk Cycle")).toBe("Ciclo de caminhada");
  expect(translate("import")).toBe("importação");
  expect(translate("Supervisor off")).toBe("Hospedagem gerenciada pelo Jarvis");
  expect(translate("Effect: Hit Spark")).toBe("Efeito: Faísca de impacto");
  expect(translate("GOLD / 4f / 96px")).toBe("Ouro / 4 quadros / 96 px");
  expect(translate("inbox")).toBe("caixa de entrada");
  expect(translate("example")).toBe("exemplo");
  expect(adoptUi('function formatTime(value:string){return new Intl.DateTimeFormat(undefined, { month:"short" }).format(new Date(value));}')).toContain('DateTimeFormat("pt-BR"');
});

it("keeps original modal contents and refs inside registry Dialog lifecycle", () => {
  const adapted = adoptUi('function Modal({onClose}) { return <div className="prompt-backdrop" onMouseDown={onClose}><section ref={dialogRef} className="editor" role="dialog" aria-label="Preview"><button onClick={onClose}>Close</button></section></div>; }');
  expect(adapted).toContain('<JarvisDialog open onOpenChange={(open) => { if (!open) onClose(); }}>');
  expect(adapted).toContain('<JarvisDialogContent showCloseButton={false} data-jarvis-suite-dialog="" ref={dialogRef}');
  expect(adapted).toContain("onClick={onClose}");
  expect(adapted).toContain("</JarvisDialogContent></JarvisDialog>");
});
