import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { chmod, copyFile, lstat, mkdir, readFile, readdir, realpath, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";
import { liveActionPortuguese, livePortuguese, liveTemplatePortuguese } from "./prepare-impeccable.locale";

export const IMPECCABLE_ENGINE_VERSION = "0.1.14";
export const IMPECCABLE_SKILL_VERSION = "4.5.2";
export const IMPECCABLE_CLI_VERSION = "4.0.0";
export const IMPECCABLE_SOURCE_COMMIT = "d631a8827f99414d2b6daba4ef08b7f8701751d7";
export const SKILL_DIRECTORY = ".agents/skills/impeccable";
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const IMPECCABLE_RESOURCE_DIRECTORY = path.join(ROOT, "src-tauri/resources/impeccable");
export const engines = {
  "darwin-arm64": { asset: "impeccable-darwin-arm64", sha256: "abbd69d540532b91e9599c41d3a6127dfbaabae5e24bafd74ff7e2fb18f4f87c" },
  "darwin-x64": { asset: "impeccable-darwin-x64", sha256: "dfa4570309939f49615a441baa9c73f1c2d339bf07494517a6c6eb2eb28aa0f5" },
  "linux-arm64": { asset: "impeccable-linux-arm64", sha256: "396263ee694780db942483fc80933a012ef08bf981f5f26b0da5e16b6208c948" },
  "linux-x64": { asset: "impeccable-linux-x64", sha256: "3d31ec1a664ac35c815ea1d7c10abad1155ce364d0f461157eb837a34368efb6" },
  "windows-x64": { asset: "impeccable-windows-x64.exe", sha256: "38e09451694973ba664a24e67e1502a6913e6d0e533a0a5ae1e04c5963a804b1" },
} as const;
export type EnginePlatform = keyof typeof engines;
export const IMPECCABLE_NATIVE_PATCHES = ["scripts/impeccable-native/queue-cas.patch", "scripts/impeccable-native/canonical-design.patch", "scripts/impeccable-native/accept-write.patch"] as const;
export interface NativeEnginePatch { path: string; sha256: string }
export interface NativeEngineReceipt {
  schemaVersion: 1;
  engineVersion: string;
  cliVersion: string;
  sourceCommit: string;
  target: EnginePlatform;
  patches: NativeEnginePatch[];
  engineSha256: string;
}
interface ResourceHash { path: string; sha256: string }
interface NativeManifest {
  schemaVersion: 1;
  version: string;
  engineVersion: string;
  cliVersion: string;
  source: { repository: string; commit: string; skillDirectory: string };
  adaptations: { locale: "pt-BR"; owner: "Jarvis"; upstreamAlgorithms: "preserved" };
  engines: typeof engines;
  files: ResourceHash[];
}
interface DesignResource { id: string; kind: string; name: string; description: string; files: string[] }
interface PrepareOptions { target?: string; destination?: string; offline?: boolean }

export function sha256(bytes: Uint8Array | string): string {
  return createHash("sha256").update(bytes).digest("hex");
}
export function enginePlatform(target?: string, platform = process.platform, arch = process.arch): EnginePlatform {
  const aliases: Record<string, EnginePlatform> = {
    "aarch64-apple-darwin": "darwin-arm64", "x86_64-apple-darwin": "darwin-x64",
    "aarch64-unknown-linux-gnu": "linux-arm64", "x86_64-unknown-linux-gnu": "linux-x64",
    "x86_64-pc-windows-msvc": "windows-x64", "x86_64-pc-windows-gnu": "windows-x64",
  };
  const key = target !== undefined ? aliases[target] ?? target : `${platform === "win32" ? "windows" : platform}-${arch}`;
  if (!Object.hasOwn(engines, key)) throw new Error(`O Impeccable nativo não oferece engine para ${key}.`);
  return key as EnginePlatform;
}

/** Rewrite string tokens, never protocol keys, identifiers, selectors or control flow. */
export function localizeLive(source: string, vocabulary = false): string {
  const file = ts.createSourceFile("live-browser.js", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
  const edits: { start: number; end: number; text: string }[] = [];
  function visit(node: ts.Node): void {
    if (ts.isStringLiteral(node) && Object.hasOwn(livePortuguese, node.text)) {
      edits.push({ start: node.getStart(file), end: node.end, text: JSON.stringify(livePortuguese[node.text]) });
    } else if (ts.isTemplateHead(node) || ts.isTemplateMiddle(node) || ts.isTemplateTail(node) || ts.isNoSubstitutionTemplateLiteral(node)) {
      const raw = node.getText(file);
      let translated = raw;
      for (const [from, to] of Object.entries(liveTemplatePortuguese)) translated = translated.replaceAll(from, to);
      if (raw !== translated) edits.push({ start: node.getStart(file), end: node.end, text: translated });
    }
    ts.forEachChild(node, visit);
  }
  visit(file);
  let result = source;
  for (const edit of edits.sort((a, b) => b.start - a.start)) result = result.slice(0, edit.start) + edit.text + result.slice(edit.end);
  if (vocabulary) {
    const anchor = "return { value: c.value, label: c.label };";
    if (result.split(anchor).length !== 2) throw new Error("O contrato de vocabulário Live mudou; revise a integração nativa.");
    result = result.replace(anchor, `return { value: c.value, label: (${JSON.stringify(liveActionPortuguese)})[c.value] || c.label };`);
  }
  return result;
}

/** Queue admission is not acceptance: retain the preview until canonical completion and HMR. */
export function bindNativeAcceptLifecycle(source: string): string {
  const file = ts.createSourceFile("live-browser.js", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
  const functions = new Map<string, ts.FunctionDeclaration[]>();
  const switches: ts.SwitchStatement[] = [];
  function visit(node: ts.Node): void {
    if (ts.isFunctionDeclaration(node) && node.name) {
      const named = functions.get(node.name.text) ?? [];
      named.push(node);
      functions.set(node.name.text, named);
    }
    if (ts.isSwitchStatement(node) && node.expression.getText(file) === "msg.type") switches.push(node);
    ts.forEachChild(node, visit);
  }
  visit(file);
  function requiredFunction(name: string): ts.FunctionDeclaration {
    const named = functions.get(name);
    if (named?.length !== 1 || !named[0].body) throw new Error("O contrato de aceitação Live mudou; revise a integração nativa.");
    return named[0];
  }
  function requireText(node: ts.Node, fragments: string[]): void {
    const text = node.getText(file);
    if (fragments.some((fragment) => !text.includes(fragment))) throw new Error("O contrato de aceitação Live mudou; revise a integração nativa.");
  }
  const edits: { start: number; end: number; text: string }[] = [];
  const accept = requiredFunction("handleAccept");
  const thenCallbacks: ts.ArrowFunction[] = [];
  const catchCallbacks: ts.ArrowFunction[] = [];
  function findThen(node: ts.Node): void {
    if (ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression)
      && node.arguments.length === 1 && ts.isArrowFunction(node.arguments[0])) {
      if (node.expression.name.text === "then") thenCallbacks.push(node.arguments[0]);
      if (node.expression.name.text === "catch") catchCallbacks.push(node.arguments[0]);
    }
    ts.forEachChild(node, findThen);
  }
  findThen(accept);
  const sends = accept.body?.statements.filter((statement) => ts.isExpressionStatement(statement) && statement.getText(file).startsWith("sendEvent(acceptPayload,"));
  if (thenCallbacks.length !== 1 || !ts.isBlock(thenCallbacks[0].body) || catchCallbacks.length !== 1
    || !ts.isBlock(catchCallbacks[0].body) || sends?.length !== 1) throw new Error("O contrato de aceitação Live mudou; revise a integração nativa.");
  edits.push({ start: sends[0].getStart(file), end: sends[0].getStart(file), text: "const acceptedAttempt = pendingAcceptedSession;\n    " });
  const queued = thenCallbacks[0].body;
  requireText(queued, ["pendingAcceptedSession", "acceptedSessionId", "markSessionHandled();", "setLiveState('CONFIRMED');", "scheduleAcceptCleanup(pending);"]);
  edits.push({ start: queued.getStart(file), end: queued.end, text: `{
        if (pendingAcceptedSession !== acceptedAttempt || acceptedAttempt.finalizing) return;
        // HTTP success only confirms durable queue admission. Keep the chosen
        // preview, stylesheet and checkpoint until the same request completes.
        awaitingAcceptResult = { id: acceptedSessionId };
      }` });
  const failed = catchCallbacks[0].body;
  const failedGuard = failed.statements[0];
  if (!failedGuard) throw new Error("O contrato de aceitação Live mudou; revise a integração nativa.");
  requireText(failedGuard, ["if (pendingAcceptedSession?.id === acceptedSessionId) pendingAcceptedSession = null;"]);
  edits.push({ start: failedGuard.getStart(file), end: failedGuard.end, text: `if (pendingAcceptedSession !== acceptedAttempt || acceptedAttempt.finalizing) return;
        pendingAcceptedSession = null;
        awaitingAcceptResult = null;` });
  const complete = requiredFunction("maybeCompleteAcceptedSession");
  requireText(complete, ["pending.finalizing = true;", "scheduleAcceptCleanup(pending);", "currentSessionId !== pending.id"]);
  const completeBody = complete.body;
  if (!completeBody) throw new Error("O contrato de aceitação Live mudou; revise a integração nativa.");
  edits.push({ start: completeBody.getStart(file) + 1, end: completeBody.getStart(file) + 1, text: "\n    if (msg?.type !== 'complete') return false;" });
  const eventSwitch = switches.filter((node) => node.caseBlock.clauses.some((clause) => ts.isCaseClause(clause) && ts.isStringLiteral(clause.expression) && clause.expression.text === "complete"));
  if (eventSwitch.length !== 1) throw new Error("O contrato de aceitação Live mudou; revise a integração nativa.");
  const clauses = eventSwitch[0].caseBlock.clauses;
  const position = clauses.findIndex((clause) => ts.isCaseClause(clause) && ts.isStringLiteral(clause.expression) && clause.expression.text === "complete");
  const [completionCase, acceptCase, agentDoneCase] = [clauses[position], clauses[position + 1], clauses[position + 2]];
  if (!completionCase || !acceptCase || !agentDoneCase) throw new Error("O contrato de aceitação Live mudou; revise a integração nativa.");
  requireText(acceptCase, ["case 'accept':", "maybeCompleteAcceptedSession(msg)"]);
  requireText(agentDoneCase, ["case 'agent_done':", "msg.data?.carbonize === true", "maybeCompleteAcceptedSession(msg)"]);
  if (!ts.isCaseClause(completionCase) || completionCase.statements.length !== 0) throw new Error("O contrato de aceitação Live mudou; revise a integração nativa.");
  edits.push({ start: completionCase.getStart(file), end: agentDoneCase.end, text: `case 'complete':
          if (awaitingAcceptResult?.id && msg.id === awaitingAcceptResult.id) awaitingAcceptResult = null;
          if (!maybeCompleteAcceptedSession(msg) && msg.id && msg.id === currentSessionId) {
            // A restored tab has no in-memory accept attempt to finish.
            markSessionHandled();
            if (findAcceptedRuntimeWrappers(msg.id).length > 0) location.reload();
            else cleanupAcceptedSession();
          }
          break;
        case 'accept':
        case 'agent_done':
          // Admission and partial source promotion do not confirm final cleanup.
          break;` });
  const connected = clauses.find((clause) => ts.isCaseClause(clause) && ts.isStringLiteral(clause.expression) && clause.expression.text === "connected");
  if (!connected) throw new Error("O contrato de aceitação Live mudou; revise a integração nativa.");
  const restore = connected.statements.find((statement) => statement.getText(file).startsWith("restoreFromActiveSessions("));
  if (!restore) throw new Error("O contrato de aceitação Live mudou; revise a integração nativa.");
  edits.push({ start: restore.getStart(file), end: restore.getStart(file), text: `// Completed comparisons are absent from the canonical active list.
          if (currentSessionId && (state === 'CYCLING' || state === 'SAVING')
              && Array.isArray(msg.activeSessions)
              && !msg.activeSessions.some(session => session?.id === currentSessionId)) {
            markSessionHandled();
            if (findAcceptedRuntimeWrappers(currentSessionId).length > 0) location.reload();
            else cleanupAcceptedSession();
          }
          ` });
  const restoreSession = requiredFunction("restoreSessionWithoutWrapper");
  const restoreBody = restoreSession.body;
  if (!restoreBody) throw new Error("O contrato de aceitação Live mudou; revise a integração nativa.");
  requireText(restoreSession, ["Array.isArray(activeSessions) && activeSessions.length > 0 && !serverSession"]);
  edits.push({ start: restoreBody.getStart(file), end: restoreBody.end, text: restoreBody.getText(file)
    .replace("{", "{\n    // A local checkpoint alone cannot resurrect a completed comparison.\n    if (!Array.isArray(activeSessions)) return false;")
    .replace("Array.isArray(activeSessions) && activeSessions.length > 0 && !serverSession", "!serverSession") });
  const cleanup = requiredFunction("scheduleAcceptCleanup");
  requireText(cleanup, ["queueMicrotask(function()", "cleanupAcceptedSession();", "watchForHandledRuntimeWrapper", "ensureAcceptedDomClean(accepted, recoveryRevision)", "}, 1200);"]);
  const cleanupBody = cleanup.body;
  if (!cleanupBody) throw new Error("O contrato de aceitação Live mudou; revise a integração nativa.");
  edits.push({ start: cleanupBody.getStart(file), end: cleanupBody.end, text: `{
    const recoveryRevision = liveInteractionRevision;
    let observer = null;
    let expiry = null;
    function releaseCleanPreview() {
      if (deferredRecoverySuperseded(accepted?.id, recoveryRevision)
          || pendingAcceptedSession?.id !== accepted?.id) {
        observer?.disconnect();
        if (expiry) clearTimeout(expiry);
        return true;
      }
      // Keep the visibility stylesheet until HMR removes this comparison.
      // Removing it while wrappers remain can expose every alternative.
      if (findAcceptedRuntimeWrappers(accepted.id).length > 0) return false;
      observer?.disconnect();
      if (expiry) clearTimeout(expiry);
      cleanupAcceptedSession();
      return true;
    }
    if (accepted?.isSvelteComponent) commitAcceptedSvelteComponentToDom(accepted.id);
    queueMicrotask(releaseCleanPreview);
    observer = new MutationObserver(releaseCleanPreview);
    if (document.body) observer.observe(document.body, { childList: true, subtree: true });
    if (!accepted?.isSvelteComponent) {
      // Retain the upstream bounded HMR/reload recovery and static fallback.
      // React/Vue DOM remains owned by their reconcilers throughout recovery.
      watchForHandledRuntimeWrapper(accepted?.id, recoveryRevision);
      setTimeout(function() {
        if (releaseCleanPreview()) return;
        if (!acceptedDomAlreadyClean(accepted)) ensureAcceptedDomClean(accepted, recoveryRevision);
        releaseCleanPreview();
      }, 1200);
    }
    expiry = setTimeout(function() {
      observer?.disconnect();
      releaseCleanPreview();
    }, 12000);
  }` });
  let result = source;
  for (const edit of edits.sort((a, b) => b.start - a.start)) result = result.slice(0, edit.start) + edit.text + result.slice(edit.end);
  return result;
}

async function filesBelow(directory: string, prefix = ""): Promise<string[]> {
  const files: string[] = [];
  for (const name of (await readdir(path.join(directory, prefix))).sort()) {
    const relative = prefix ? `${prefix}/${name}` : name;
    const metadata = await lstat(path.join(directory, relative));
    if (metadata.isSymbolicLink()) throw new Error(`Recurso Impeccable não pode ser um link simbólico: ${relative}`);
    if (metadata.isDirectory()) files.push(...await filesBelow(directory, relative));
    else if (metadata.isFile()) files.push(relative);
  }
  return files.sort();
}

/** Explicit maintainer operation. Ordinary builds consume the committed snapshot without docs/. */
export async function refreshImpeccableAssets(source = path.join(ROOT, "docs/impeccable"), destination = IMPECCABLE_RESOURCE_DIRECTORY): Promise<void> {
  const commit = execFileSync("git", ["-C", source, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
  if (commit !== IMPECCABLE_SOURCE_COMMIT) throw new Error("O snapshot Impeccable não corresponde ao commit validado.");
  const base = path.join(source, ".agents/skills/impeccable");
  const metadata = await readFile(path.join(base, "SKILL.md"), "utf8");
  if (!metadata.includes(`version: ${IMPECCABLE_SKILL_VERSION}`) || (await readFile(path.join(base, "scripts/VERSION"), "utf8")).trim() !== IMPECCABLE_ENGINE_VERSION) {
    throw new Error("As versões de skill e engine do snapshot Impeccable divergem.");
  }
  await mkdir(destination, { recursive: true });
  for (const file of await filesBelow(base)) {
    if (file.startsWith("scripts/bin/") || /(?:^|\/)impeccable(?:\.cmd)?$/.test(file)) continue;
    const output = path.join(destination, SKILL_DIRECTORY, file);
    await mkdir(path.dirname(output), { recursive: true });
    if (file === "scripts/live-browser.js") await writeFile(output, localizeLive(bindNativeAcceptLifecycle(await readFile(path.join(base, file), "utf8")), true));
    else await copyFile(path.join(base, file), output);
  }
  const detector = path.join(destination, SKILL_DIRECTORY, "scripts/detector/detect-antipatterns-browser.js");
  await mkdir(path.dirname(detector), { recursive: true });
  await writeFile(detector, localizeLive(await readFile(path.join(source, "crates/live/assets/detect-antipatterns-browser.js"), "utf8")));
  await copyFile(path.join(source, "LICENSE"), path.join(destination, "LICENSE"));
  await copyFile(path.join(source, "NOTICE.md"), path.join(destination, "UPSTREAM-NOTICE.md"));
  await writeFile(path.join(destination, "NOTICE.md"), `# Impeccable native resources\n\nUpstream: https://github.com/pbakaus/impeccable\nCommit: ${IMPECCABLE_SOURCE_COMMIT}\nSkill: ${IMPECCABLE_SKILL_VERSION}; engine: ${IMPECCABLE_ENGINE_VERSION}; CLI contract: ${IMPECCABLE_CLI_VERSION}.\n\nCopyright Paul Bakaus and Impeccable contributors. Licensed under Apache-2.0 (see LICENSE).\nUpstream third-party notices are preserved in UPSTREAM-NOTICE.md. The unchanged vendored modern-screenshot library is licensed under MIT; see modern-screenshot.LICENSE (source: https://github.com/qq15725/modern-screenshot).\n\nJarvis modifications: Portuguese Live presentation strings and labels, app-owned packaging, a separate native host status adapter, an atomic queue admission/cancellation patch, scoped canonical project design-path binding, and completion-driven accept timing that preserves the chosen preview until canonical acknowledgement and DOM reconciliation. Upstream interaction and variant algorithms are reused. Native resources update together with the Jarvis application, not through independent Core updates. Native engine receipts pin the upstream source commit, the tracked patch SHA-256, the target platform and the resulting binary SHA-256. Original official release checksums remain recorded separately.\n`);
  await writeNativeManifest(destination);
}

async function writeNativeManifest(destination: string): Promise<void> {
  const files = await filesBelow(destination);
  const hashes: ResourceHash[] = [];
  for (const file of files) {
    if (file === "manifest.json" || file === "jarvis-design.json" || file.startsWith("bin/") || file.endsWith(".partial")) continue;
    hashes.push({ path: file, sha256: sha256(await readFile(path.join(destination, file))) });
  }
  const manifest: NativeManifest = {
    schemaVersion: 1, version: IMPECCABLE_SKILL_VERSION, engineVersion: IMPECCABLE_ENGINE_VERSION, cliVersion: IMPECCABLE_CLI_VERSION,
    source: { repository: "https://github.com/pbakaus/impeccable", commit: IMPECCABLE_SOURCE_COMMIT, skillDirectory: SKILL_DIRECTORY },
    adaptations: { locale: "pt-BR", owner: "Jarvis", upstreamAlgorithms: "preserved" }, engines, files: hashes,
  };
  await writeFile(path.join(destination, "manifest.json"), JSON.stringify(manifest, null, 2) + "\n");
}

export async function verifyImpeccableAssets(destination: string): Promise<NativeManifest> {
  const manifest = JSON.parse(await readFile(path.join(destination, "manifest.json"), "utf8")) as NativeManifest;
  if (manifest.schemaVersion !== 1 || manifest.version !== IMPECCABLE_SKILL_VERSION || manifest.engineVersion !== IMPECCABLE_ENGINE_VERSION || manifest.cliVersion !== IMPECCABLE_CLI_VERSION || manifest.source?.commit !== IMPECCABLE_SOURCE_COMMIT || !Array.isArray(manifest.files) || manifest.files.length < 4 || JSON.stringify(manifest.engines) !== JSON.stringify(engines)) throw new Error("Manifesto Impeccable nativo incompatível.");
  const required = ["SKILL.md", "scripts/VERSION", "scripts/live-browser.js", "scripts/live-browser-dom.js", "scripts/live-browser-session.js", "scripts/live-browser-ignores.js"];
  for (const file of required) if (!manifest.files.some((resource) => resource.path === `${SKILL_DIRECTORY}/${file}`)) throw new Error(`Recurso Impeccable obrigatório ausente: ${file}`);
  for (const file of manifest.files) {
    if (typeof file.path !== "string" || !file.path || path.isAbsolute(file.path) || file.path.split(/[\\/]/).some((part) => part === ".." || !part) || !/^[a-f0-9]{64}$/.test(file.sha256)) throw new Error("Caminho ou checksum Impeccable inválido.");
    const target = path.join(destination, file.path);
    const metadata = await lstat(target);
    const canonical = await realpath(target);
    const root = await realpath(destination);
    if (!canonical.startsWith(root + path.sep) || !metadata.isFile() || metadata.isSymbolicLink() || sha256(await readFile(target)) !== file.sha256) throw new Error(`O recurso Impeccable diverge do snapshot validado: ${file.path}`);
  }
  return manifest;
}

export function verifyEngineBytes(platform: EnginePlatform, bytes: Uint8Array, releaseChecksum?: string): void {
  const expected = engines[platform].sha256;
  if (releaseChecksum !== undefined && releaseChecksum.trim().split(/\s+/)[0] !== expected) throw new Error("O checksum oficial do Impeccable diverge da versão fixada.");
  if (bytes.byteLength > 180 * 1024 * 1024 || sha256(bytes) !== expected) throw new Error("O engine Impeccable não passou na verificação SHA-256.");
}
export async function nativeEnginePatches(): Promise<NativeEnginePatch[]> {
  return Promise.all(IMPECCABLE_NATIVE_PATCHES.map(async (relative) => ({ path: relative, sha256: sha256(await readFile(path.join(ROOT, relative))) })));
}

/** A release engine cannot silently replace the host's atomic queue protocol. */
export function verifyNativeEngineReceipt(platform: EnginePlatform, bytes: Uint8Array, value: unknown, patches: readonly NativeEnginePatch[]): NativeEngineReceipt {
  if (typeof value !== "object" || value === null || Array.isArray(value)) throw new Error("Receita do engine Impeccable nativo inválida.");
  const receipt = value as Partial<NativeEngineReceipt>;
  const fields = new Set(["schemaVersion", "engineVersion", "cliVersion", "sourceCommit", "target", "patches", "engineSha256"]);
  if (Object.keys(receipt).some((field) => !fields.has(field))) throw new Error("Receita do engine Impeccable nativo contém campos incompatíveis.");
  if (receipt.schemaVersion !== 1 || receipt.engineVersion !== IMPECCABLE_ENGINE_VERSION || receipt.cliVersion !== IMPECCABLE_CLI_VERSION || receipt.sourceCommit !== IMPECCABLE_SOURCE_COMMIT || receipt.target !== platform || !Array.isArray(receipt.patches) || receipt.patches.length !== patches.length || !patches.length || receipt.patches.some((patch, index) => typeof patch !== "object" || patch === null || Object.keys(patch).length !== 2 || patch.path !== patches[index].path || patch.sha256 !== patches[index].sha256 || !/^[a-f0-9]{64}$/.test(patch.sha256)) || typeof receipt.engineSha256 !== "string" || !/^[a-f0-9]{64}$/.test(receipt.engineSha256) || bytes.byteLength === 0 || bytes.byteLength > 180 * 1024 * 1024 || sha256(bytes) !== receipt.engineSha256) throw new Error("O engine Impeccable nativo não corresponde ao commit, à plataforma e aos patches validados. Reconstrua o engine do Jarvis.");
  return receipt as NativeEngineReceipt;
}

async function preparedNativeEngine(platform: EnginePlatform, destination: string): Promise<{ enginePath: string; receipt: NativeEngineReceipt }> {
  const filename = platform.startsWith("windows-") ? "impeccable.exe" : "impeccable";
  const enginePath = path.join(destination, "bin", filename);
  const receiptPath = path.join(destination, "bin", "jarvis-engine.json");
  const root = await realpath(destination);
  for (const file of [enginePath, receiptPath]) {
    const metadata = await lstat(file);
    if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size > 180 * 1024 * 1024 || !(await realpath(file)).startsWith(root + path.sep)) throw new Error("O engine Impeccable nativo deve permanecer no pacote do aplicativo.");
  }
  const receiptMetadata = await lstat(receiptPath);
  if (receiptMetadata.size > 16_000) throw new Error("Receita do engine Impeccable nativo muito grande.");
  const receipt = verifyNativeEngineReceipt(platform, await readFile(enginePath), JSON.parse(await readFile(receiptPath, "utf8")) as unknown, await nativeEnginePatches());
  if (!platform.startsWith("windows-")) await chmod(enginePath, 0o755);
  return { enginePath, receipt };
}

async function prepareEngine(platform: EnginePlatform, destination: string, offline: boolean): Promise<{ enginePath: string; receipt: NativeEngineReceipt }> {
  const enginePath = path.join(destination, "bin", platform.startsWith("windows-") ? "impeccable.exe" : "impeccable");
  await mkdir(path.dirname(enginePath), { recursive: true });
  if ((await lstat(path.dirname(enginePath))).isSymbolicLink()) throw new Error("A pasta do engine Impeccable não pode ser um link simbólico.");
  try {
    return await preparedNativeEngine(platform, destination);
  } catch (error) {
    if (offline) throw new Error(`Engine Impeccable indisponível para build offline: ${error instanceof Error ? error.message : platform}`, { cause: error });
  }
  // The builder works in a private staging checkout; docs/ remains read-only.
  const { buildImpeccableNativeEngine } = await import("./build-impeccable-native");
  await buildImpeccableNativeEngine({ target: platform, destination, offline: false });
  return preparedNativeEngine(platform, destination);
}

async function writeDesignIndex(destination: string, manifest: NativeManifest, nativeEngine: NativeEngineReceipt): Promise<void> {
  const files = manifest.files.map((file) => file.path).filter((file) => file.startsWith(`${SKILL_DIRECTORY}/`) && file.endsWith(".md")).sort();
  const skill = await readFile(path.join(destination, SKILL_DIRECTORY, "SKILL.md"), "utf8");
  const resources: DesignResource[] = [{ id: "impeccable/skill", kind: "skill", name: "Impeccable", description: skill.match(/^description: (.+)$/m)?.[1]?.slice(0, 600) ?? "Design direction, UX and production UI craft", files: [`${SKILL_DIRECTORY}/SKILL.md`] }];
  for (const file of files.filter((file) => file.startsWith(`${SKILL_DIRECTORY}/reference/`))) {
    const text = await readFile(path.join(destination, file), "utf8");
    const stem = path.posix.basename(file, ".md");
    const name = text.match(/^# (.+)$/m)?.[1] ?? stem;
    resources.push({ id: `impeccable/${stem}`, kind: "craft", name: name.slice(0, 120), description: (text.split(/\r?\n/).map((line) => line.trim()).find((line) => line && !line.startsWith("#")) ?? name).slice(0, 600), files: [file] });
  }
  const digest = createHash("sha256");
  for (const file of manifest.files.filter((file) => file.path.startsWith(`${SKILL_DIRECTORY}/`))) {
    digest.update(file.path);
    digest.update(await readFile(path.join(destination, file.path)));
  }
  const index = {
    version: IMPECCABLE_SKILL_VERSION, engine_version: IMPECCABLE_ENGINE_VERSION, cli_version: IMPECCABLE_CLI_VERSION,
    bundle_engine_version: IMPECCABLE_ENGINE_VERSION, engine_sha256: nativeEngine.engineSha256, native_engine: nativeEngine,
    archive_sha256: digest.digest("hex"), resources: resources.sort((a, b) => a.id.localeCompare(b.id, "en")),
  };
  await writeFile(path.join(destination, "jarvis-design.json"), JSON.stringify(index, null, 2) + "\n");
}

export async function prepareImpeccable(options: PrepareOptions = {}): Promise<{ platform: EnginePlatform; enginePath: string; version: string }> {
  const platform = enginePlatform(options.target);
  const destination = options.destination ?? IMPECCABLE_RESOURCE_DIRECTORY;
  const manifest = await verifyImpeccableAssets(destination);
  const { enginePath, receipt } = await prepareEngine(platform, destination, options.offline ?? false);
  await writeDesignIndex(destination, manifest, receipt);
  return { platform, enginePath, version: IMPECCABLE_SKILL_VERSION };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const targetIndex = args.indexOf("--target");
  try {
    if (targetIndex >= 0 && !args[targetIndex + 1]) throw new Error("Informe a plataforma após --target.");
    if (args.includes("--refresh-assets")) await refreshImpeccableAssets();
    else {
      const result = await prepareImpeccable({ target: targetIndex < 0 ? undefined : args[targetIndex + 1], offline: args.includes("--offline") });
      console.info(`Jarvis: Impeccable nativo ${result.version} preparado para ${result.platform}.`);
    }
  } catch (error) {
    console.error(error instanceof Error ? error.message : "Não foi possível preparar o Impeccable nativo.");
    process.exitCode = 1;
  }
}
