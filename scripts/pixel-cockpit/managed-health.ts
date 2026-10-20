import ts from "typescript";

/** The packaged host owns process recovery; upstream's Vite supervisor is not installed. */
export function adoptManagedHealth(source: string) {
  const file = ts.createSourceFile("App.tsx", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const supervisor = file.statements.find((node): node is ts.FunctionDeclaration =>
    ts.isFunctionDeclaration(node) && node.name?.text === "loadDevSupervisorHealth");
  if (!supervisor?.body) throw new Error("Upstream health adapter changed; review the managed recovery contract.");
  return (source.slice(0, supervisor.body.getStart(file))
    + "{ return undefined; }" + source.slice(supervisor.body.end))
    .replace("await postDevSupervisorRepair();", 'await fetchJsonWithTimeout("/jarvis/repair", { method: "POST" });')
    .replace("const repairAvailable = Boolean(supervisorHealth);", "const repairAvailable = Boolean(apiHealth);")
    .replace("Cockpitを修復します。Vite/API/supervisorの整合性だけを確認し、repo、outbox画像、status、logsは削除しません。running Codex jobは停止しません。",
      "Verificar o serviço, reparar as pastas da sessão e recuperar resultados? Arquivos, logs e gerações em andamento serão preservados.");
}
