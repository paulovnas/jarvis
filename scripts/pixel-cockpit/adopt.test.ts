import { describe, expect, it } from "vitest";
import { exportedModuleCatalog, extractController, forwardPublicOperationErrors, typeDeclarationCatalog } from "./adopt";

describe("Image Cockpit controller adoption", () => {
  it("retains exact nested field definitions and enums for advanced JSON arguments", () => {
    expect(typeDeclarationCatalog('interface Anchor { id: string; point: {x:number;y:number}; frameIndex?:number } type Mode = "loop" | "once"; const secret = "not a schema";')).toEqual({
      Anchor: "interface Anchor { id: string; point: {x:number;y:number}; frameIndex?:number }",
      Mode: 'type Mode = "loop" | "once";',
    });
  });
  it("forwards caught public failures without changing background recovery handlers", () => {
    const source = `import { bindController } from "../jarvis-worker";
      function revalidate() { try { verify(); } catch (error) { setStatus(error.message); } }
      function backgroundPoll() { try { poll(); } catch (error) { retryLater(); } }`;
    const result = forwardPublicOperationErrors(source, new Set(["revalidate"]));
    expect(result).toContain("setStatus(error.message); \nreportOperationError(error);");
    expect(result).toContain("catch (error) { retryLater(); }");
    expect(result).toContain("bindController, reportOperationError");
  });
  it("preserves business callbacks and state while removing the manually operated interface", () => {
    const source = `
      function App() {
        const [prompt, setPrompt] = useState<string>("");
        async function handleGenerate(input: { count: number }) { return originalGenerate(input); }
        if (storageScreen.mode !== "normal") return <Recovery />;
        return <OriginalManualInterface />;
      }
      export function originalGenerate(input: unknown) { return input; }
      export default App;
    `;
    const extracted = extractController(source, false);
    expect(extracted.source).toContain("originalGenerate(input)");
    expect(extracted.source).toContain("bindController");
    expect(extracted.source).not.toContain("<OriginalManualInterface");
    expect(extracted.source).not.toContain("<Recovery");
    expect(extracted.catalog.methods.handleGenerate.parameters).toContain("count: number");
    expect(extracted.catalog.settings.prompt.type).toBe("string");
    expect(extracted.source).toContain("export default App");
  });

  it("keeps the original suite interface alongside agent commands by default", () => {
    const result = extractController('function App() { if (storageScreen.mode !== "normal") return <Recovery/>; return <Timeline/>; }');
    expect(result.source).toContain("<Timeline");
    expect(result.source).toContain("bindController");
    expect(result.source).toContain("<Recovery");
  });

  it("fails explicitly when the original controller layout changes", () => {
    expect(() => extractController("function App() { return null; }")).toThrow("UI boundary");
  });

  it("catalogs exported library operations and constants, excluding private helpers and types", () => {
    expect(exportedModuleCatalog(`
      export function moveFrame(pack: Pack, index: number) { return pack; }
      function privateHelper() {}
      export const PRESETS = [];
      export type Pack = unknown;
    `)).toEqual({
      moveFrame: { parameters: "pack: Pack, index: number", kind: "function" },
      PRESETS: { parameters: "", kind: "value" },
    });
  });
});
