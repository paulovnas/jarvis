import { randomUUID, timingSafeEqual } from "node:crypto";
import { access, mkdir, readFile, realpath, rename, stat, writeFile } from "node:fs/promises";
import { constants } from "node:fs";
import type { IncomingMessage, ServerResponse } from "node:http";
import { basename, dirname, extname, isAbsolute, join, relative, resolve, sep } from "node:path";
import type { AddressInfo } from "node:net";

/** Node 22 realpath rejects Rust's verbatim Windows paths; canonical scope checks still run below. */
export function normalizeFilesystemPath(path: string, platform: NodeJS.Platform = process.platform) {
  if (platform !== "win32") return path;
  if (path.startsWith("\\\\?\\UNC\\")) return `\\\\${path.slice(8)}`;
  return path.startsWith("\\\\?\\") ? path.slice(4) : path;
}

const resource = resolve(normalizeFilesystemPath(process.env.JARVIS_PIXEL_RESOURCE_DIR ?? dirname(process.argv[1])));
const session = resolve(normalizeFilesystemPath(process.env.IMAGE_COCKPIT_HANDOFF_DIR ?? "handoff"));
const workspace = resolve(normalizeFilesystemPath(process.env.JARVIS_PIXEL_WORKSPACE ?? process.cwd()));
const token = process.env.JARVIS_PIXEL_SESSION_TOKEN ?? "";
const MAX_BYTES = 96 * 1024 * 1024;
const MIME: Record<string, string> = {
  ".png": "image/png", ".gif": "image/gif", ".webp": "image/webp", ".apng": "image/apng",
  ".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".json": "application/json",
  ".zip": "application/zip", ".txt": "text/plain", ".md": "text/markdown",
};

function sameSecret(value: string) {
  const a = Buffer.from(token);
  const b = Buffer.from(value);
  return a.length > 0 && a.length === b.length && timingSafeEqual(a, b);
}

function send(response: ServerResponse, code: number, value: unknown) {
  response.writeHead(code, { "Content-Type": "application/json", "Cache-Control": "no-store" });
  response.end(JSON.stringify(value));
}

async function body(request: IncomingMessage) {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > MAX_BYTES) throw new Error("O arquivo excede o limite de 96 MiB da operação.");
    chunks.push(chunk);
  }
  return JSON.parse(Buffer.concat(chunks).toString()) as Record<string, unknown>;
}

function within(path: string, root: string) {
  const rel = relative(root, path);
  return rel === "" || (!rel.startsWith(`..${sep}`) && rel !== ".." && !isAbsolute(rel));
}

export function reportJarvisReady(address: string | AddressInfo | null) {
  if (!address || typeof address === "string") throw new Error("Pixel Cockpit did not bind a TCP port.");
  console.log(JSON.stringify({ type: "ready", port: address.port, origin: `http://127.0.0.1:${address.port}` }));
}

export async function handleJarvisRoute(request: IncomingMessage, response: ServerResponse) {
  const url = new URL(request.url ?? "/", "http://127.0.0.1");
  const host = request.headers.host ?? "";
  if (!/^127\.0\.0\.1:\d+$/.test(host) || (request.headers.origin && request.headers.origin !== `http://${host}`)) {
    send(response, 403, { error: "Origem não autorizada para esta sessão Pixel Art." });
    return true;
  }
  const cookie = request.headers.cookie?.split(";").map((part) => part.trim()).find((part) => part.startsWith("jarvis_pixel="))?.slice(13);
  const header = request.headers["x-jarvis-pixel-token"];
  const presented = (typeof header === "string" ? header : "") || cookie || url.searchParams.get("token") || "";
  if (!sameSecret(presented)) {
    send(response, 401, { error: "Sessão Pixel Art inválida." });
    return true;
  }
  response.setHeader("Cache-Control", "no-store");
  response.setHeader("X-Content-Type-Options", "nosniff");
  response.setHeader("Referrer-Policy", "no-referrer");
  response.setHeader("Content-Security-Policy", "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' data: blob:; connect-src 'self' data: blob:; worker-src 'self' blob:; frame-ancestors 'self'");
  if (url.pathname === "/" && url.searchParams.has("token")) {
    response.setHeader("Set-Cookie", `jarvis_pixel=${token}; HttpOnly; SameSite=Strict; Path=/`);
  }
  try {
    if (request.method === "GET" && url.pathname === "/jarvis/health"
      || request.method === "POST" && url.pathname === "/jarvis/repair") {
      const repair = request.method === "POST";
      if (repair) {
        for (const directory of ["artifacts", "logs", "status", "outbox"]) {
          await mkdir(join(session, directory), { recursive: true });
        }
      }
      await access(join(resource, "worker/index.html"), constants.R_OK);
      await access(session, constants.R_OK | constants.W_OK);
      send(response, 200, {
        ok: true, mode: "managed", jobsInterrupted: false,
        checks: { interfaceReadable: true, sessionWritable: true },
        message: repair ? "Pastas da sessão verificadas. Arquivos e jobs preservados." : "Serviço gerenciado disponível.",
        recovery: "Se o processo encerrar, reabra a suíte para restaurar esta sessão sem repetir gerações.",
      });
      return true;
    }
    if (request.method === "POST" && url.pathname === "/jarvis/logs") {
      const data = await body(request);
      if (!Array.isArray(data.jobIds) || data.jobIds.length > 200 || data.jobIds.some((id) => typeof id !== "string" || !/^[a-zA-Z0-9_-]{1,160}$/.test(id))) throw new Error("Identificadores de log inválidos.");
      const artifacts = [];
      for (const id of data.jobIds) {
        const path = join(session, "logs", `${id}.log`);
        try {
          const info = await stat(path);
          if (info.isFile()) artifacts.push({ name: `${id}.log`, path, mime: "text/plain", size: info.size, complete: true });
        } catch (error) {
          if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
        }
      }
      send(response, 200, { artifacts });
      return true;
    }
    if (request.method === "GET" && !url.pathname.startsWith("/api/") && !url.pathname.startsWith("/jarvis/")) {
      const filename = url.pathname === "/" ? "index.html" : decodeURIComponent(url.pathname.slice(1));
      const staticRoot = await realpath(join(resource, "worker"));
      const staticPath = await realpath(resolve(staticRoot, filename));
      if (!within(staticPath, staticRoot)) throw new Error("Recurso fora do diretório autorizado.");
      const extension = extname(filename);
      const mime = extension === ".html" ? "text/html; charset=utf-8" : extension === ".js" ? "application/javascript; charset=utf-8" : extension === ".css" ? "text/css; charset=utf-8" : extension === ".woff2" ? "font/woff2" : MIME[extension] ?? "application/octet-stream";
      response.writeHead(200, { "Content-Type": mime });
      response.end(await readFile(staticPath));
      return true;
    }
    if (request.method === "POST" && url.pathname === "/jarvis/artifacts") {
      const data = await body(request);
      if (typeof data.name !== "string" || typeof data.base64 !== "string" || typeof data.mime !== "string") throw new Error("Arquivo incompleto.");
      // Control bytes are deliberately rejected from managed Windows filenames.
      // eslint-disable-next-line no-control-regex
      const name = basename(data.name).replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_").slice(0, 160);
      if (!name || name === "." || name === "..") throw new Error("Nome de arquivo inválido.");
      const content = Buffer.from(data.base64, "base64");
      if (!content.length || content.length > MAX_BYTES) throw new Error("Arquivo vazio ou grande demais.");
      const destination = join(session, "artifacts", randomUUID(), name);
      await mkdir(dirname(destination), { recursive: true });
      await writeFile(destination, content, { flag: "wx" });
      send(response, 201, { name, path: destination, mime: MIME[extname(name).toLowerCase()] ?? data.mime, size: content.length });
      return true;
    }
    if (url.pathname === "/jarvis/state") {
      const statePath = join(session, "suite-state.json");
      if (request.method === "GET") {
        try { send(response, 200, JSON.parse(await readFile(statePath, "utf8"))); }
        catch (error) {
          if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
          send(response, 200, {});
        }
        return true;
      }
      if (request.method === "POST") {
        const data = await body(request);
        if (![1, 2].includes(Number(data.version)) || !Array.isArray(data.history) || !Array.isArray(data.frames) || !Array.isArray(data.actions)) throw new Error("Estado da suíte inválido.");
        await mkdir(session, { recursive: true });
        const temp = join(session, `suite-state-${randomUUID()}.tmp`);
        await writeFile(temp, JSON.stringify(data), { flag: "wx" });
        await rename(temp, statePath);
        send(response, 200, { saved: true });
        return true;
      }
    }
    if (request.method === "POST" && url.pathname === "/jarvis/import") {
      const data = await body(request);
      if (typeof data.path !== "string") throw new Error("Informe o caminho do arquivo.");
      const path = await realpath(resolve(workspace, normalizeFilesystemPath(data.path)));
      const roots = await Promise.all([workspace, session].map((root) => realpath(root).catch(() => resolve(root))));
      if (!roots.some((root) => within(path, root))) throw new Error("O arquivo está fora do projeto e da sessão autorizados.");
      const info = await stat(path);
      if (!info.isFile() || info.size > MAX_BYTES) throw new Error("O caminho não é um arquivo válido ou excede 96 MiB.");
      const mime = MIME[extname(path).toLowerCase()];
      if (!mime) throw new Error("Formato de arquivo não suportado.");
      send(response, 200, { name: basename(path), mime, dataUrl: `data:${mime};base64,${(await readFile(path)).toString("base64")}` });
      return true;
    }
    if (url.pathname.startsWith("/jarvis/")) {
      send(response, 404, { error: "Operação desconhecida." });
      return true;
    }
    return false;
  } catch (error) {
    send(response, 400, { error: error instanceof Error ? error.message : "Falha ao processar arquivo." });
    return true;
  }
}
