import { readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";
import { expect, it, vi } from "vitest";
import ts from "typescript";
import { adoptManagedRunner, assertExplicitResume, cancellationMessage, interruptedRunnerStatus } from "./managed-runner";

const original = "src-tauri/resources/pixel-cockpit/upstream/server/index.ts";
const interrupted = { jobId: "codex-job-test", state: "running", processId: 42, message: "Running" };

it("observes an interrupted job without starting another Codex process", async () => {
  const source = adoptManagedRunner(await readFile(original, "utf8"));
  const ast = ts.createSourceFile("server.ts", source, ts.ScriptTarget.Latest, true);
  const method = ast.statements.find((node): node is ts.FunctionDeclaration =>
    ts.isFunctionDeclaration(node) && node.name?.text === "normalizeRunningStatus");
  expect(method).toBeDefined();
  const start = vi.fn();
  const context = {
    runnerProcesses: new Map(), persistedTournamentCancellationStatus: async () => null,
    resolveJobOutboxDir: async () => undefined, writeRunnerStatus: vi.fn(),
    resumeUntrackedCodexRunner: start,
    interruptedRunnerStatus: (status: typeof interrupted) => interruptedRunnerStatus(status, () => false),
    normalizeRunningStatus: undefined as unknown as (status: typeof interrupted) => Promise<Record<string, unknown>>,
  };
  runInNewContext(ts.transpileModule(method!.getText(ast), {
    compilerOptions: { target: ts.ScriptTarget.ES2022 },
  }).outputText, context);
  const first = await context.normalizeRunningStatus(interrupted);
  const second = await context.normalizeRunningStatus(first as typeof interrupted);
  expect(first).toMatchObject({ state: "failed", resumeRequired: true });
  expect(second).toEqual(first);
  expect(start).not.toHaveBeenCalled();
  expect(source).toContain('pathname.match(/^\\/api\\/codex\\/jobs\\/([^/]+)\\/resume$/)');
});

it("preserves an untracked live process and refuses concurrent or unconfirmed resume", () => {
  expect(interruptedRunnerStatus(interrupted, () => true)).toMatchObject({ state: "running" });
  expect(() => assertExplicitResume(interrupted, false, false, () => false)).toThrow(/confirm/);
  expect(() => assertExplicitResume(interrupted, true, true, () => false)).toThrow(/andamento/);
  expect(() => assertExplicitResume(interrupted, true, false, () => true)).toThrow(/andamento/);
  expect(() => assertExplicitResume({ ...interrupted, state: "completed" }, true, false, () => false)).toThrow(/interrompido/);
  expect(() => assertExplicitResume(interruptedRunnerStatus(interrupted, () => false), true, false, () => false)).not.toThrow();
});

it("distinguishes user cancellation from tournament selection and omits retry diagnostics", async () => {
  const source = adoptManagedRunner(await readFile(original,"utf8"));
  expect(source).not.toContain("Codex runner cancelled after an animation tournament winner was chosen.");
  expect(source).toContain('cancelCodexRunner(jobId, "user")');
  expect(source).toContain('"cancelled" in status && status.cancelled');
  expect(cancellationMessage("user",true)).toBe("Execução cancelada por solicitação do usuário.");
  expect(cancellationMessage("user",false)).toContain("aguardando o encerramento");
  expect(cancellationMessage("tournament",true)).toContain("controle do torneio");
});
