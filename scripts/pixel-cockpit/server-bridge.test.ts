import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { createServer, type Server } from "node:http";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { randomUUID } from "node:crypto";

describe("managed Pixel Cockpit HTTP boundary", () => {
  let root: string;
  let origin: string;
  let server: Server;
  const token = randomUUID();
  beforeAll(async () => {
    root = await mkdtemp(join(tmpdir(), "jarvis-pixel-http-"));
    const resource = join(root, "resource");
    await mkdir(join(resource, "worker"), { recursive: true });
    await mkdir(join(root, "project"));
    await mkdir(join(root, "session"));
    await writeFile(join(resource, "worker/index.html"), "<html>Pixel Art</html>");
    await mkdir(join(resource, "worker/fonts"));
    await writeFile(join(resource, "worker/fonts/test.woff2"), Buffer.from([119,79,70,50]));
    await writeFile(join(root, "project/source.png"), Buffer.from([137, 80, 78, 71]));
    await writeFile(join(root, "outside.png"), "private");
    vi.stubEnv("JARVIS_PIXEL_RESOURCE_DIR", resource);
    vi.stubEnv("JARVIS_PIXEL_SESSION_TOKEN", token);
    vi.stubEnv("JARVIS_PIXEL_WORKSPACE", join(root, "project"));
    vi.stubEnv("IMAGE_COCKPIT_HANDOFF_DIR", join(root, "session"));
    const { handleJarvisRoute } = await import("./server-bridge");
    server = createServer((request, response) => {
      void handleJarvisRoute(request, response).then((handled) => {
        if (!handled) { response.writeHead(404); response.end(); }
      });
    });
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    if (!address || typeof address === "string") throw new Error("Test server did not listen");
    origin = `http://127.0.0.1:${address.port}`;
  });
  afterAll(async () => {
    await new Promise<void>((resolve) => server.close(() => resolve()));
    vi.unstubAllEnvs();
    await rm(root, { recursive: true, force: true });
  });
  const post = (path: string, data: unknown) => fetch(`${origin}${path}`, {
    method: "POST", headers: { "Content-Type": "application/json", "X-Jarvis-Pixel-Token": token }, body: JSON.stringify(data),
  });

  it("requires the managed session token and initializes an HttpOnly cookie", async () => {
    expect((await fetch(origin)).status).toBe(401);
    const response = await fetch(`${origin}/?token=${token}`);
    expect(response.status).toBe(200);
    expect(response.headers.get("set-cookie")).toContain("HttpOnly; SameSite=Strict");
  });

  it("imports actual in-project files and refuses paths outside the authorized roots", async () => {
    const allowed = await post("/jarvis/import", { path: "source.png" });
    expect(allowed.status).toBe(200);
    expect(await allowed.json()).toMatchObject({ name: "source.png", mime: "image/png" });
    const denied = await post("/jarvis/import", { path: "../outside.png" });
    expect(denied.status).toBe(400);
    expect(await denied.json()).toMatchObject({ error: expect.stringContaining("fora do projeto") });
  });

  it("normalizes Windows verbatim imports before realpath without bypassing project scope", async () => {
    const {normalizeFilesystemPath} = await import("./server-bridge");
    expect(normalizeFilesystemPath("\\\\?\\C:\\work\\file.png","win32")).toBe("C:\\work\\file.png");
    expect(normalizeFilesystemPath("\\\\?\\UNC\\server\\share\\file.png","win32")).toBe("\\\\server\\share\\file.png");
    expect(normalizeFilesystemPath("/tmp/file.png","linux")).toBe("/tmp/file.png");
    if(process.platform === "win32") {
      expect((await post("/jarvis/import",{path:`\\\\?\\${join(root,"project/source.png")}`})).status).toBe(200);
      expect((await post("/jarvis/import",{path:`\\\\?\\${join(root,"outside.png")}`})).status).toBe(400);
    }
  });

  it("serves bundled fonts with their font MIME type through the same session boundary", async () => {
    expect((await fetch(`${origin}/fonts/test.woff2`)).status).toBe(401);
    const response = await fetch(`${origin}/fonts/test.woff2`, {headers:{"X-Jarvis-Pixel-Token":token}});
    expect(response.status).toBe(200);
    expect(response.headers.get("content-type")).toBe("font/woff2");
    expect(new Uint8Array(await response.arrayBuffer())).toEqual(new Uint8Array([119,79,70,50]));
  });

  it("rejects requests from another local origin even if the browser supplies its session cookie", async () => {
    const response = await fetch(`${origin}/jarvis/state`, {
      method: "POST",
      headers: { Origin: "http://127.0.0.1:9999", Cookie: `jarvis_pixel=${token}`, "Content-Type": "text/plain" },
      body: JSON.stringify({ version: 1, history: [], frames: [], actions: [] }),
    });
    expect(response.status).toBe(403);
  });

  it("persists downloads as unique session artifacts and preserves their bytes", async () => {
    const response = await post("/jarvis/artifacts", { name: "../../frame.png", mime: "image/png", base64: Buffer.from("pixel-art").toString("base64") });
    expect(response.status).toBe(201);
    const artifact = await response.json() as { path: string; name: string };
    expect(artifact.path.startsWith(join(root, "session/artifacts"))).toBe(true);
    expect(artifact.name).toBe("frame.png");
    expect(await readFile(artifact.path, "utf8")).toBe("pixel-art");
  });

  it("retains the suite state across worker origins without replacing the last good snapshot on invalid input", async () => {
    const state = { version: 1, history: [{ id: "generated-1" }], frames: [], actions: [], settings: { prompt: "keep me" } };
    expect((await post("/jarvis/state", state)).status).toBe(200);
    expect((await post("/jarvis/state", { version: 99 })).status).toBe(400);
    const response = await fetch(`${origin}/jarvis/state`, { headers: { "X-Jarvis-Pixel-Token": token } });
    expect(await response.json()).toEqual(state);
  });

  it("persists version two receipts and job links alongside the original suite state", async () => {
    const state = {version:2,history:[],frames:[],actions:[],
      operations:[{id:"operation",status:"running",originalJobIds:["job"]}],
      operationJobs:{operation:["job"]},recoverableJobs:[{id:"job",workflowMode:"effect-animation"}]};
    expect((await post("/jarvis/state",state)).status).toBe(200);
    expect(await (await fetch(`${origin}/jarvis/state`,{headers:{"X-Jarvis-Pixel-Token":token}})).json()).toEqual(state);
  });

  it("returns original full log paths without truncating them or accepting path traversal", async () => {
    await mkdir(join(root, "session/logs"), { recursive: true });
    await writeFile(join(root, "session/logs/job-a.log"), "Original log\n".repeat(10000));
    const response = await post("/jarvis/logs", { jobIds: ["job-a"] });
    const data = await response.json() as { artifacts: {path: string; size: number; complete: boolean}[] };
    expect(data.artifacts[0]).toMatchObject({ size: 130000, complete: true });
    expect(await readFile(data.artifacts[0].path, "utf8")).toHaveLength(130000);
    expect((await post("/jarvis/logs", { jobIds: ["../../outside"] })).status).toBe(400);
  });

  it("repairs managed storage without deleting outputs, snapshots or active job status", async () => {
    await mkdir(join(root, "session/outbox"), { recursive: true });
    await mkdir(join(root, "session/status"), { recursive: true });
    await writeFile(join(root, "session/outbox/keep.png"), "original output");
    await writeFile(join(root, "session/status/active.json"), '{"state":"running"}');
    const state = await readFile(join(root, "session/suite-state.json"), "utf8");
    const response = await post("/jarvis/repair", {});
    expect(response.status).toBe(200);
    expect(await response.json()).toMatchObject({ ok: true, mode: "managed", jobsInterrupted: false });
    expect(await readFile(join(root, "session/outbox/keep.png"), "utf8")).toBe("original output");
    expect(await readFile(join(root, "session/status/active.json"), "utf8")).toBe('{"state":"running"}');
    expect(await readFile(join(root, "session/suite-state.json"), "utf8")).toBe(state);
    expect((await fetch(`${origin}/jarvis/health`, {headers:{"X-Jarvis-Pixel-Token":token}})).status).toBe(200);
  });
});
