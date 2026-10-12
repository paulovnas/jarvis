import { execFile } from "node:child_process";
import { mkdir, mkdtemp, rm, stat, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { describe, expect, it } from "vitest";

const runFile = promisify(execFile);
const resources = path.resolve("src-tauri/resources/impeccable");
const commands = [
  { name: "live-complete", args: ["live-complete", "--id", "action", "--force"] },
  { name: "live-poll reply", args: ["live-poll", "--reply", "action", "complete"] },
];

async function withHelper(test: (root: string, env: NodeJS.ProcessEnv, requests: unknown[]) => Promise<void>) {
  const root = await mkdtemp(path.join(os.tmpdir(), "jarvis-live-cas-"));
  const requests: unknown[] = [];
  const server = createServer((request, response) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk: Buffer) => chunks.push(chunk));
    request.on("end", () => {
      const body: unknown = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      requests.push(body);
      response.writeHead(200, { "Content-Type": "application/json" });
      response.end('{"ok":true}');
    });
  });
  try {
    await new Promise<void>((resolve, reject) => {
      server.once("error", reject);
      server.listen(0, "127.0.0.1", resolve);
    });
    const address = server.address();
    if (!address || typeof address === "string") throw new Error("Invalid helper address");
    await mkdir(path.join(root, ".impeccable/live"), { recursive: true });
    await writeFile(path.join(root, "package.json"), '{"name":"live-cas-fixture"}');
    await writeFile(path.join(root, ".impeccable/live/server.json"), JSON.stringify({ pid: process.pid, port: address.port, token: "fixture-token" }));
    const env: NodeJS.ProcessEnv = { ...process.env, IMPECCABLE_HOME: path.join(root, "home"), IMPECCABLE_SKILL_DIR: path.join(resources, ".agents/skills/impeccable") };
    for (const key of ["IMPECCABLE_DESIGN_PATH", "IMPECCABLE_DESIGN_ROOT", "IMPECCABLE_PRODUCT_PATH", "IMPECCABLE_PRODUCT_ROOT", "JARVIS_LIVE_REQUEST_SEQ", "JARVIS_LIVE_PENDING_EVENT_SEQ", "JARVIS_LIVE_EVENT_TYPE"]) delete env[key];
    await test(root, env, requests);
  } finally {
    await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
    await rm(root, { recursive: true, force: true });
  }
}

// Verify the host binding on the actual bundled executable, including the
// refusal to emit an unguarded acknowledgement when one binding field is lost.
describe.skipIf(process.env.JARVIS_TEST_NATIVE_ENGINE !== "1" || process.platform === "win32")("native Live acknowledgement binding", () => {
  it.each(commands)("includes the leased phase in $name acknowledgements", async ({ args }) => {
    await withHelper(async (root, env, requests) => {
      env.JARVIS_LIVE_REQUEST_SEQ = "1";
      env.JARVIS_LIVE_PENDING_EVENT_SEQ = "3";
      env.JARVIS_LIVE_EVENT_TYPE = "accept";
      await runFile(path.join(resources, "bin/impeccable"), args, { cwd: root, env, encoding: "utf8", timeout: 10_000 });
      expect(requests).toEqual([{ id: "action", type: "complete", token: "fixture-token", jarvisRequestSeq: 1, jarvisPendingEventSeq: 3, jarvisEventType: "accept" }]);
    });
  });

  it.each(commands)("does not emit or append an unguarded $name acknowledgement", async ({ args }) => {
    await withHelper(async (root, env, requests) => {
      env.JARVIS_LIVE_REQUEST_SEQ = "1";
      env.JARVIS_LIVE_EVENT_TYPE = "accept";
      await expect(runFile(path.join(resources, "bin/impeccable"), args, { cwd: root, env, encoding: "utf8", timeout: 10_000 })).rejects.toThrow();
      expect(requests).toEqual([]);
      expect(await stat(path.join(root, ".impeccable/live/sessions/action.jsonl")).catch(() => null)).toBeNull();
    });
  });
});
