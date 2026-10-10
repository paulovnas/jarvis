import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import ts from "typescript";
import { afterEach, describe, expect, it } from "vitest";
import {
  engines, enginePlatform, IMPECCABLE_CLI_VERSION, IMPECCABLE_ENGINE_VERSION,
  IMPECCABLE_RESOURCE_DIRECTORY, IMPECCABLE_SKILL_VERSION, IMPECCABLE_SOURCE_COMMIT,
  localizeLive, nativeEnginePatches, prepareImpeccable, sha256, SKILL_DIRECTORY, verifyEngineBytes, verifyImpeccableAssets,
  verifyNativeEngineReceipt,
  type NativeEngineReceipt,
} from "./prepare-impeccable";

const temporary: string[] = [];
afterEach(async () => { await Promise.all(temporary.splice(0).map((directory) => rm(directory, { recursive: true, force: true }))); });
async function fixture(): Promise<string> {
  const directory = await mkdtemp(path.join(tmpdir(), "jarvis-impeccable-assets-"));
  temporary.push(directory);
  const files = ["SKILL.md", "scripts/VERSION", "scripts/live-browser.js", "scripts/live-browser-dom.js", "scripts/live-browser-session.js", "scripts/live-browser-ignores.js"];
  const hashes: { path: string; sha256: string }[] = [];
  for (const file of files) {
    const relative = `${SKILL_DIRECTORY}/${file}`;
    await mkdir(path.dirname(path.join(directory, relative)), { recursive: true });
    const text = `fixture ${file}`;
    await writeFile(path.join(directory, relative), text);
    hashes.push({ path: relative, sha256: sha256(text) });
  }
  await writeFile(path.join(directory, "manifest.json"), JSON.stringify({
    schemaVersion: 1, version: IMPECCABLE_SKILL_VERSION, engineVersion: IMPECCABLE_ENGINE_VERSION,
    cliVersion: IMPECCABLE_CLI_VERSION, source: { commit: IMPECCABLE_SOURCE_COMMIT }, engines, files: hashes,
  }));
  return directory;
}

describe("app-owned Impeccable preparation", () => {
  it("chooses the pinned engine for every supported Tauri target and refuses unsupported hosts", () => {
    expect(enginePlatform("aarch64-apple-darwin")).toBe("darwin-arm64");
    expect(enginePlatform("x86_64-apple-darwin")).toBe("darwin-x64");
    expect(enginePlatform("aarch64-unknown-linux-gnu")).toBe("linux-arm64");
    expect(enginePlatform("x86_64-unknown-linux-gnu")).toBe("linux-x64");
    expect(enginePlatform("x86_64-pc-windows-msvc")).toBe("windows-x64");
    expect(enginePlatform(undefined, "win32", "x64")).toBe("windows-x64");
    expect(() => enginePlatform("aarch64-pc-windows-msvc")).toThrow("não oferece");
    expect(() => enginePlatform("")).toThrow("não oferece");
    expect(Object.values(engines).every((engine) => /^[a-f0-9]{64}$/.test(engine.sha256))).toBe(true);
  });

  it("translates the controls, accessibility labels and status while preserving event values and user text", () => {
    const upstream = "button.title='Pick element';button.textContent='✓ Accept';button.setAttribute('aria-label','Voice input');const event={type:'steer',phase:'picked_up',reply:'steer_done',userText:message};const status='Generating '+count+' variants...';";
    const translated = localizeLive(upstream);
    expect(translated).toContain('button.title="Selecionar elemento"');
    expect(translated).toContain('button.textContent="✓ Aceitar"');
    expect(translated).toContain('"Entrada por voz"');
    expect(translated).toContain("type:'steer',phase:'picked_up',reply:'steer_done',userText:message");
    expect(translated).toContain('"Gerando "+count+" alternativas…"');
  });

  it("localizes the command labels without changing action identifiers, templates or expressions", () => {
    const translated = localizeLive("const commands=VOCAB.map(c=>{return { value: c.value, label: c.label };});const empty=`<strong>No DESIGN.md yet</strong>Create one by running <code>${COMMAND} document</code> in your terminal, then re-open this panel.`;", true);
    expect(translated).toContain('"impeccable":"Livre"');
    expect(translated).toContain('"typeset":"Tipografia"');
    expect(translated).toContain("value: c.value");
    expect(translated).toContain("${COMMAND} document</code> no chat do Jarvis");
    expect(translated).not.toContain("in your terminal");
    expect(() => localizeLive("const unknown=true;", true)).toThrow("contrato de vocabulário");
  });

  it("ships a complete immutable snapshot with Portuguese UI and valid JavaScript", async () => {
    const manifest = await verifyImpeccableAssets(IMPECCABLE_RESOURCE_DIRECTORY);
    expect(manifest.source.commit).toBe(IMPECCABLE_SOURCE_COMMIT);
    expect(manifest.files.some((file) => file.path.endsWith("reference/new-work.md"))).toBe(true);
    expect(manifest.files.some((file) => file.path.endsWith("reference/live.md"))).toBe(true);
    expect(manifest.files.some((file) => file.path.endsWith("LICENSE"))).toBe(true);
    const index = JSON.parse(await readFile(path.join(IMPECCABLE_RESOURCE_DIRECTORY, "jarvis-design.json"), "utf8")) as { resources: { id: string; files: string[] }[] };
    expect(index.resources.find((resource) => resource.id === "impeccable/skill")?.files).toEqual([`${SKILL_DIRECTORY}/SKILL.md`]);
    expect(index.resources.find((resource) => resource.id === "impeccable/new-work")?.files).toEqual([`${SKILL_DIRECTORY}/reference/new-work.md`]);
    const ui = await readFile(path.join(IMPECCABLE_RESOURCE_DIRECTORY, SKILL_DIRECTORY, "scripts/live-browser.js"), "utf8");
    expect(ui).toContain("Descreva a alteração na página…");
    expect(ui).toContain("Gerando ");
    expect(ui).not.toContain("'Generate variants'");
    expect(ui).not.toContain("'Agent disconnected - run live-poll.mjs to connect'");
    const compiled = ts.transpileModule(ui, { compilerOptions: { target: ts.ScriptTarget.ES2022 }, reportDiagnostics: true });
    expect(compiled.diagnostics?.filter((diagnostic) => diagnostic.category === ts.DiagnosticCategory.Error)).toEqual([]);
  });

  it("rejects a tampered UI asset before preparing or downloading the engine", async () => {
    const directory = await fixture();
    await writeFile(path.join(directory, SKILL_DIRECTORY, "scripts/live-browser.js"), "unexpected UI");
    await expect(prepareImpeccable({ destination: directory, offline: true })).rejects.toThrow("diverge do snapshot");
  });

  it("refuses a missing script instead of allowing an embedded English fallback", async () => {
    const directory = await fixture();
    const manifestPath = path.join(directory, "manifest.json");
    const manifest = JSON.parse(await readFile(manifestPath, "utf8")) as { files: { path: string; sha256: string }[] };
    manifest.files = manifest.files.filter((file) => !file.path.endsWith("live-browser-ignores.js"));
    await writeFile(manifestPath, JSON.stringify(manifest));
    await expect(verifyImpeccableAssets(directory)).rejects.toThrow("obrigatório ausente");
  });

  it("refuses resource paths escaping the package", async () => {
    const directory = await fixture();
    const manifestPath = path.join(directory, "manifest.json");
    const manifest = JSON.parse(await readFile(manifestPath, "utf8")) as { files: { path: string; sha256: string }[] };
    manifest.files.push({ path: "../outside", sha256: sha256("outside") });
    await writeFile(manifestPath, JSON.stringify(manifest));
    await expect(verifyImpeccableAssets(directory)).rejects.toThrow("Caminho ou checksum");
  });

  it("rejects a changed official checksum and corrupt engine bytes", () => {
    const fakeEngine = new TextEncoder().encode("wrong engine");
    expect(() => verifyEngineBytes("darwin-arm64", fakeEngine, "0".repeat(64))).toThrow("versão fixada");
    expect(() => verifyEngineBytes("darwin-arm64", fakeEngine, engines["darwin-arm64"].sha256)).toThrow("SHA-256");
  });

  it("binds native engine receipts to the source, platform, tracked patches and resulting bytes", async () => {
    const bytes = new TextEncoder().encode("fixture native atomic queue engine");
    const patches = await nativeEnginePatches();
    const receipt: NativeEngineReceipt = {
      schemaVersion: 1, engineVersion: IMPECCABLE_ENGINE_VERSION, cliVersion: IMPECCABLE_CLI_VERSION,
      sourceCommit: IMPECCABLE_SOURCE_COMMIT, target: "darwin-arm64", patches, engineSha256: sha256(bytes),
    };
    expect(verifyNativeEngineReceipt("darwin-arm64", bytes, receipt, patches)).toEqual(receipt);
    expect(patches.map((patch) => patch.path)).toEqual(["scripts/impeccable-native/queue-cas.patch", "scripts/impeccable-native/canonical-design.patch", "scripts/impeccable-native/accept-write.patch"]);
    expect(() => verifyNativeEngineReceipt("darwin-arm64", bytes, { ...receipt, patches: patches.slice(0, 1) }, patches)).toThrow("patches");
    expect(() => verifyNativeEngineReceipt("linux-x64", bytes, receipt, patches)).toThrow("plataforma");
    expect(() => verifyNativeEngineReceipt("darwin-arm64", bytes, { ...receipt, sourceCommit: "0".repeat(40) }, patches)).toThrow("commit");
    expect(() => verifyNativeEngineReceipt("darwin-arm64", bytes, { ...receipt, patches: [] }, patches)).toThrow("patches");
    expect(() => verifyNativeEngineReceipt("darwin-arm64", bytes, { ...receipt, patches: [{ ...patches[0], sha256: "0".repeat(64) }] }, patches)).toThrow("patches");
    expect(() => verifyNativeEngineReceipt("darwin-arm64", bytes, { ...receipt, unvalidatedSource: true }, patches)).toThrow("campos incompatíveis");
    expect(() => verifyNativeEngineReceipt("darwin-arm64", bytes, { ...receipt, patches: [{ ...patches[0], unvalidatedPatch: true }] }, patches)).toThrow("patches");
    expect(() => verifyNativeEngineReceipt("darwin-arm64", new TextEncoder().encode("changed"), receipt, patches)).toThrow("engine");
    expect(() => verifyNativeEngineReceipt("darwin-arm64", bytes, null, patches)).toThrow("Receita");
  });

  it("prepares a verified native engine offline and records its provenance separately from official release pins", async () => {
    const directory = await fixture();
    const bytes = new TextEncoder().encode("fixture engine with native CAS");
    const platform = "darwin-arm64";
    const receipt: NativeEngineReceipt = {
      schemaVersion: 1, engineVersion: IMPECCABLE_ENGINE_VERSION, cliVersion: IMPECCABLE_CLI_VERSION,
      sourceCommit: IMPECCABLE_SOURCE_COMMIT, target: platform, patches: await nativeEnginePatches(), engineSha256: sha256(bytes),
    };
    await mkdir(path.join(directory, "bin"));
    await writeFile(path.join(directory, "bin/impeccable"), bytes);
    await writeFile(path.join(directory, "bin/jarvis-engine.json"), JSON.stringify(receipt));
    const result = await prepareImpeccable({ destination: directory, target: platform, offline: true });
    expect(result.platform).toBe(platform);
    const index = JSON.parse(await readFile(path.join(directory, "jarvis-design.json"), "utf8")) as { engine_sha256: string; native_engine: NativeEngineReceipt };
    expect(index.engine_sha256).toBe(receipt.engineSha256);
    expect(index.native_engine).toEqual(receipt);
    expect(index.engine_sha256).not.toBe(engines[platform].sha256);
    await writeFile(path.join(directory, "bin/impeccable"), "corrupt engine");
    await expect(prepareImpeccable({ destination: directory, target: platform, offline: true })).rejects.toThrow("offline");
  });

  it("refuses to use an unreceipted engine instead of silently losing native queue admission", async () => {
    const directory = await fixture();
    await mkdir(path.join(directory, "bin"));
    await writeFile(path.join(directory, "bin/impeccable"), "unpatched release engine");
    await expect(prepareImpeccable({ destination: directory, target: "darwin-arm64", offline: true })).rejects.toThrow("offline");
    await expect(readFile(path.join(directory, "jarvis-design.json"))).rejects.toThrow();
  });
});
