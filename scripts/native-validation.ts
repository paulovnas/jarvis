import { appendFileSync } from "node:fs";
import { command, read, versionFiles } from "./release-common";
import { replaceCargoVersion } from "./release-plan";

try {
  const base = process.env.NATIVE_BASE_SHA ?? "";
  let required = true;
  if (process.env.GITHUB_EVENT_NAME !== "workflow_dispatch" && /^[a-f0-9]{40}$/.test(base) && !/^0+$/.test(base)) {
    const files = command("git", ["diff", "--name-only", base, "HEAD"], true).split("\n").filter(Boolean);
    required = files.some(file => {
      if (!versionFiles.includes(file)) return true;
      const normalize = (contents: string) => {
        if (!file.endsWith(".json")) return replaceCargoVersion(`${contents.trim()}\n`, "0.0.0", file.endsWith(".lock")).trim();
        const data = JSON.parse(contents) as Record<string, unknown>;
        delete data.version;
        return JSON.stringify(data);
      };
      return normalize(command("git", ["show", `${base}:${file}`], true)) !== normalize(read(file));
    });
  }
  if (!process.env.GITHUB_OUTPUT) throw new Error("GITHUB_OUTPUT ausente.");
  appendFileSync(process.env.GITHUB_OUTPUT, `required=${required}\n`);
  console.info(required ? "Validar integrações nativas nas três plataformas." : "Somente a versão mudou; nenhuma integração nativa foi alterada.");
} catch (error) {
  console.error(error instanceof Error ? error.message : "Falha ao identificar alterações nativas.");
  process.exitCode = 1;
}
