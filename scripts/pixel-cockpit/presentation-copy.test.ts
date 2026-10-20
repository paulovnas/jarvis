import { readFileSync } from "node:fs";
import { expect, it } from "vitest";
import ts from "typescript";
import { adoptUi } from "./ui-adapter";

const upstream = "src-tauri/resources/pixel-cockpit/upstream/src/";
const compiled = ts.transpileModule(adoptUi("function View(){return <span>{value}</span>}"), {
  compilerOptions: {target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.React},
}).outputText;
const translate = new Function(`${compiled}; return jarvisUiText;`)() as (value: string) => string;

it("translates the original saved footer and every effect grade at their actual display boundaries", () => {
  const app = readFileSync(upstream + "App.tsx", "utf8");
  const composite = readFileSync(upstream + "VfxCompositeStage.tsx", "utf8");
  const initialStatus = app.match(/const \[status, setStatus\] = useState\("([^"]+)"\)/)?.[1];
  expect(initialStatus).toBe("All changes saved locally");
  expect(translate(initialStatus!)).toBe("Todas as alterações foram salvas localmente");

  const types = readFileSync(upstream + "types.ts", "utf8");
  const grades = [...(types.match(/export type EffectQualityRank = ([^;]+);/)?.[1].matchAll(/"([^"]+)"/g) ?? [])].map((match) => match[1]);
  expect(grades).toEqual(["gold", "silver", "bronze", "failed", "blocked"]);
  for (const grade of grades) {
    expect(translate(grade)).not.toBe(grade);
    expect(translate(grade.toUpperCase())).not.toBe(grade.toUpperCase());
  }
  expect(["gold", "silver", "bronze"].map(translate)).toEqual(["Ouro", "Prata", "Bronze"]);
  expect(translate("Effect imported: hit-spark.png (gold)")).toBe("Efeito importado: hit-spark.png (Ouro)");
  expect(translate("Efeito importado: hit-spark.png (silver, 2 warnings)")).toBe("Efeito importado: hit-spark.png (Prata, 2 avisos)");
  const adopted = adoptUi(app);
  expect(adopted).toContain("<span>{jarvisUiText(status)}</span>");
  expect(adopted).toContain('jarvisUiText(selectedEffectMetadata?.qualityRank ?? "checking")');
  expect(adopted).toContain('jarvisUiText((selectedEffectMetadata?.qualityRank ?? "blocked").toUpperCase())');
  expect(adoptUi(composite)).toContain("jarvisUiText(item.effectAnimation.qualityRank)");
  expect(adopted).toContain('const [status, setStatus] = useState("All changes saved locally")');
  expect(adoptUi('const metadata = {qualityRank:"gold",label:{en:"Gold",ja:"金"}};')).toContain('qualityRank:"gold",label:{en:"Gold",ja:"金"}');
});

it("explains the existing-candidate guard without claiming the round failed", () => {
  const message = translate("The initial candidate wave is partial and cannot resume one candidate at a time. Start a new tournament instead.");
  expect(message).toContain("já possui candidatos");
  expect(message).toContain("resultados existentes");
  expect(message).not.toContain("incompleta");
});

it("covers original visible literal labels, accessibility attributes and review reason tags", () => {
  const missing: string[] = [];
  const technicalOrPortuguese = new Set(["Experimental", "Normal", "Markdown", "JSON"]);
  for (const name of ["App.tsx", "AnimationReviewCockpit.tsx", "VfxCompositeStage.tsx"]) {
    const file = ts.createSourceFile(name, readFileSync(upstream + name, "utf8"), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
    const visit = (node: ts.Node) => {
      let text: string | undefined;
      if (ts.isJsxText(node)) text = node.text.trim();
      if (ts.isJsxAttribute(node) && ["title", "aria-label", "alt", "placeholder", "label"].includes(node.name.getText(file))
        && node.initializer && ts.isStringLiteral(node.initializer)) text = node.initializer.text;
      if (text && /[A-Za-z]{3}/.test(text) && !technicalOrPortuguese.has(text) && translate(text) === text) {
        missing.push(`${name}:${file.getLineAndCharacterOfPosition(node.getStart(file)).line + 1}: ${text}`);
      }
      ts.forEachChild(node, visit);
    };
    visit(file);
  }
  const source = readFileSync(upstream + "lib/animationReview.ts", "utf8");
  const tags = source.match(/export const animationReviewReasonTags = \[([\s\S]*?)\] as const/)?.[1].matchAll(/"([^"]+)"/g);
  expect(tags).toBeDefined();
  for (const [, tag] of tags ?? []) if (translate(tag) === tag) missing.push(tag);
  expect(missing).toEqual([]);
});

it("translates every original quality warning and UI-owned error without rewriting report data", () => {
  const missing: string[] = [];
  let checked = 0;
  for (const name of ["App.tsx", "AnimationReviewCockpit.tsx", "VfxCompositeStage.tsx", "lib/animationQuality.ts", "lib/effectQuality.ts"]) {
    const file = ts.createSourceFile(name, readFileSync(upstream + name, "utf8"), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
    const visit = (node: ts.Node) => {
      const qualityMessage = ts.isCallExpression(node) && /(?:warnings|failures|shadowWarnings)\.push$/.test(node.expression.getText(file));
      const uiError = !name.startsWith("lib/") && ts.isNewExpression(node) && node.expression.getText(file) === "Error";
      if (qualityMessage || uiError) {
        const args = (node as ts.CallExpression | ts.NewExpression).arguments ?? [];
        for (const argument of args) {
          const text = ts.isStringLiteral(argument) || ts.isNoSubstitutionTemplateLiteral(argument) ? argument.text
            : ts.isTemplateExpression(argument) ? argument.head.text + argument.templateSpans.map((span, i) => `value${i}` + span.literal.text).join("") : undefined;
          if (!text) continue;
          checked++;
          if (translate(text) === text) missing.push(`${name}: ${text}`);
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(file);
  }
  expect(checked).toBeGreaterThan(65);
  expect(missing).toEqual([]);
});
