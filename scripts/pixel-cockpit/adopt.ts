import ts from "typescript";

export interface ControllerCatalog {
  methods: Record<string, { parameters: string; returns: string }>;
  settings: Record<string, { setter: string; initial: string; type: string }>;
}

export function typeDeclarationCatalog(source: string) {
  const file = ts.createSourceFile("source.tsx", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const types: Record<string, string> = {};
  for (const statement of file.statements) {
    if (ts.isInterfaceDeclaration(statement) || ts.isTypeAliasDeclaration(statement) || ts.isEnumDeclaration(statement)) {
      types[statement.name.text] = statement.getText(file);
    }
  }
  return types;
}

/** Keep UI diagnostics visible and propagate their failure to an active agent receipt. */
export function forwardPublicOperationErrors(source: string, publicMethods: ReadonlySet<string>) {
  const file = ts.createSourceFile("App.tsx", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const edits: { position: number; value: string }[] = [];
  function visit(node: ts.Node, method?: string) {
    const name = ts.isFunctionDeclaration(node) ? node.name?.text : method;
    if (ts.isCatchClause(node) && name && publicMethods.has(name)) {
      const variable = node.variableDeclaration?.name.getText(file);
      if (variable && /^[A-Za-z_$][\w$]*$/.test(variable)) {
        edits.push({ position: node.block.end - 1, value: `\nreportOperationError(${variable});\n` });
      }
    }
    ts.forEachChild(node, (child) => visit(child, name));
  }
  visit(file);
  for (const edit of edits.sort((a, b) => b.position - a.position)) {
    source = source.slice(0, edit.position) + edit.value + source.slice(edit.position);
  }
  return source.replace(/import \{ (bindController(?:, getRecoverableJob)?) \}/,
    'import { $1, reportOperationError }');
}

/** Expose the original algorithms and hooks without changing their implementation. */
export function extractController(source: string, retainInterface = true) {
  const file = ts.createSourceFile("App.tsx", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const app = file.statements.find(
    (statement): statement is ts.FunctionDeclaration =>
      ts.isFunctionDeclaration(statement) && statement.name?.text === "App",
  );
  if (!app?.body) throw new Error("Upstream App controller was not found.");
  const boundary = app.body.statements.find(
    (statement) => ts.isIfStatement(statement) && statement.expression.getText(file).includes('storageScreen.mode !== "normal"'),
  );
  if (!boundary) throw new Error("Upstream App UI boundary changed; review the adoption.");
  const catalog: ControllerCatalog = { methods: {}, settings: {} };
  const state: string[] = [];
  const methods: string[] = [];
  const setters: string[] = [];
  for (const statement of app.body.statements) {
    if (statement.pos >= boundary.pos) break;
    if (ts.isFunctionDeclaration(statement) && statement.name) {
      const name = statement.name.text;
      methods.push(name);
      catalog.methods[name] = {
        parameters: statement.parameters.map((parameter) => parameter.getText(file)).join(", "),
        returns: statement.type?.getText(file) ?? "upstream result",
      };
    }
    if (!ts.isVariableStatement(statement)) continue;
    for (const declaration of statement.declarationList.declarations) {
      if (ts.isArrayBindingPattern(declaration.name)) {
        const entries = declaration.name.elements;
        const name = entries[0]?.getText(file);
        const setter = entries[1]?.getText(file);
        if (!name || !setter || !declaration.initializer?.getText(file).startsWith("useState")) continue;
        state.push(name);
        setters.push(`${JSON.stringify(name)}: ${setter}`);
        const initializer = declaration.initializer;
        catalog.settings[name] = {
          setter,
          initial: ts.isCallExpression(initializer) ? initializer.arguments.map((arg) => arg.getText(file)).join(", ") : "",
          type: ts.isCallExpression(initializer) ? initializer.typeArguments?.map((arg) => arg.getText(file)).join(", ") ?? "" : "",
        };
      }
    }
  }
  catalog.methods.renameUserAnimationItem = { parameters: "id: string, title: string", returns: "void" };
  catalog.methods.exportEnvironmentReport = { parameters: 'kind: "markdown" | "json"', returns: "void" };
  catalog.methods.resumeInterruptedCodexJob = { parameters: 'jobId: string, mode: "process_existing" | "resume"', returns: "CodexRunnerStatus" };
  const bridge = `
  useEffect(() => {
    return bindController({
      state: {${state.join(",")}, selected, activeAction, selectedAnimationAction, selectedEffectAction, selectedMotionRecipe, selectedAnimationDirections,
        animationPresets: animationPresetExamples, promptExamples, motionVariantOptions: MOTION_VARIANT_OPTIONS,
        effectCategories: effectCategoryDefinitions, effectStyles: effectStyleOptions, effectPalettes: effectPaletteOptions,
        effectFrameCounts: effectFrameCountOptions, effectCanvasSizes: effectCanvasSizeOptions},
      setters: {${setters.join(",")}},
      methods: {${methods.join(",")},
        resumeInterruptedCodexJob: async (jobId: string, mode: "process_existing" | "resume") => {
          const pending = getRecoverableJob(jobId) as CodexJobQueueItem | undefined;
          if (!pending) throw new Error("Não há metadados preservados desse job. Inspecione os arquivos e importe o resultado existente; não recrie a geração automaticamente.");
          const response = await fetch("/api/codex/jobs/" + encodeURIComponent(jobId) + "/resume", {
            method:"POST", headers:{"Content-Type":"application/json"}, body:JSON.stringify({confirm:true,mode})
          });
          if (!response.ok) throw new Error(await response.text());
          const result = await response.json() as {status:CodexRunnerStatus};
          if (!shouldWaitForCodexRunner(result.status)) throw new Error(result.status.message);
          setCodexFailureNotices(current => current.filter(item => item.jobId !== jobId));
          setCodexJobs(current => [...current.filter(item => item.id !== jobId), {...pending, state:"running"}]);
          return result.status;
        },
        exportEnvironmentReport: (kind: "markdown" | "json") => {
          if (kind !== "markdown" && kind !== "json") throw new Error("Escolha markdown ou json.");
          const content = kind === "markdown" ? environmentReport.markdown : JSON.stringify(environmentReport.json, null, 2);
          downloadBlob(new Blob([content], {type:kind === "markdown" ? "text/markdown" : "application/json"}), "ambiente-pixel-art." + (kind === "markdown" ? "md" : "json"));
        },
        renameUserAnimationItem: (id: string, title: string) => {
          const name = title.trim();
          if (!name || !userAnimationLibrary.some(item => item.id === id)) throw new Error("Nome ou item da biblioteca inválido.");
          setUserAnimationLibrary(current => current.map(item => item.id === id
            ? {...item, title:name, manifest:{...item.manifest,title:name}, updatedAt:new Date().toISOString()} : item));
        },
        jarvisAddAnnotation: (mode: "brush"|"rect"|"arrow", points: {x:number;y:number}[], comment: string) => {
          if (!selected) throw new Error("Selecione uma imagem antes de marcar uma região.");
          const [width, height] = selected.size.split("x").map(Number);
          const display = imageDisplayRectForCanvas({width, height});
          const annotation = annotationWithSourceImageCoordinates({
            id: createId("anno"), tool:mode, color:annotationColor, width:3, comment,
            points:points.map(point => ({x:display.x+point.x*display.width,y:display.y+point.y*display.height}))
          }, selected);
          setAnnotationsByItem(current => {
            const existing = current[selected.id] ?? [];
            return {...current, [selected.id]: [...existing, {...annotation, number:existing.length+1}]};
          });
          return annotation;
        }
      }
    });
  });
`;
  const transformed =
    'import { bindController, getRecoverableJob } from "../jarvis-worker";\n' +
    source.slice(0, boundary.pos) +
    bridge +
    (retainInterface
      ? source.slice(boundary.pos)
      : '\n  return <canvas ref={canvasRef} width={CANVAS_WIDTH} height={CANVAS_HEIGHT} aria-hidden="true" />;\n}' + source.slice(app.end));
  return { source: transformed, catalog };
}

export function exportedModuleCatalog(source: string) {
  const file = ts.createSourceFile("module.ts", source, ts.ScriptTarget.Latest, true);
  const result: Record<string, { parameters: string; kind: string }> = {};
  for (const statement of file.statements) {
    if (!ts.canHaveModifiers(statement) || !ts.getModifiers(statement)?.some((modifier) => modifier.kind === ts.SyntaxKind.ExportKeyword)) continue;
    if (ts.isFunctionDeclaration(statement) && statement.name) {
      result[statement.name.text] = {
        parameters: statement.parameters.map((parameter) => parameter.getText(file)).join(", "),
        kind: "function",
      };
    }
    if (ts.isVariableStatement(statement)) {
      for (const declaration of statement.declarationList.declarations) {
        if (ts.isIdentifier(declaration.name)) result[declaration.name.text] = { parameters: "", kind: "value" };
      }
    }
  }
  return result;
}

export function exposeSuiteController(source: string, functionName: string, controllerName: string, addImport = true) {
  const file = ts.createSourceFile("suite.tsx", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const fn = file.statements.find((statement): statement is ts.FunctionDeclaration =>
    ts.isFunctionDeclaration(statement) && statement.name?.text === functionName);
  if (!fn?.body) throw new Error(`Upstream suite controller ${functionName} was not found.`);
  const boundary = [...fn.body.statements].reverse().find((statement) => ts.isReturnStatement(statement));
  if (!boundary) throw new Error(`Upstream suite controller ${functionName} has no return.`);
  const state: string[] = [];
  const setters: string[] = [];
  const methods: string[] = [];
  const catalog: ControllerCatalog = { methods: {}, settings: {} };
  for (const statement of fn.body.statements) {
    if (statement.pos >= boundary.pos) break;
    if (ts.isFunctionDeclaration(statement) && statement.name) {
      const name = statement.name.text;
      methods.push(name);
      catalog.methods[name] = { parameters: statement.parameters.map((parameter) => parameter.getText(file)).join(", "), returns: statement.type?.getText(file) ?? "upstream result" };
    }
    if (!ts.isVariableStatement(statement)) continue;
    for (const declaration of statement.declarationList.declarations) {
      if (!ts.isArrayBindingPattern(declaration.name)) continue;
      const name = declaration.name.elements[0]?.getText(file);
      const setter = declaration.name.elements[1]?.getText(file);
      if (!name || !setter || !declaration.initializer?.getText(file).startsWith("useState")) continue;
      state.push(name);
      setters.push(`${JSON.stringify(name)}: ${setter}`);
      catalog.settings[name] = { setter, initial: declaration.initializer.getText(file), type: "" };
    }
  }
  const context = controllerName === "review" ? ["directions", "candidates", "frameCount", "qcCells"]
    : controllerName === "vfx" ? ["characters", "effects", "frameCount", "characterDirections"] : [];
  methods.push("close: onClose");
  catalog.methods.close = { parameters: "", returns: "void" };
  if (controllerName === "timeline") {
    const adapters = {
      resetToRecipe: "() => commit(session.recipeDefault)",
      save: "() => {onSave(draft); setSavedSnapshot(serializeAnimationPackV2(draft));}",
      exportPack: "() => onExport(draft)",
      roundTrip: "async () => {const result = await onRoundTrip(draft); setRoundTripStatus(result); return result;}",
    };
    for (const [name, expression] of Object.entries(adapters)) {
      methods.push(`${name}: ${expression}`);
      catalog.methods[name] = { parameters: "", returns: name === "roundTrip" ? "Promise<string>" : "void" };
    }
  }
  const injection = `\n  useEffect(() => bindController({ state: {${[...state, ...context].join(",")}}, setters: {${setters.join(",")}}, methods: {${methods.join(",")}} }, ${JSON.stringify(controllerName)}));\n`;
  return { catalog, source: (addImport ? 'import { bindController } from "../jarvis-worker";\n' : "") + source.slice(0, boundary.pos) + injection + source.slice(boundary.pos) };
}
