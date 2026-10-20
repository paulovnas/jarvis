// src-tauri/resources/pixel-cockpit/generated/server-bridge.ts
import { randomUUID, timingSafeEqual } from "node:crypto";
import { access, mkdir, readFile, realpath, rename, stat, writeFile } from "node:fs/promises";
import { constants } from "node:fs";
import { basename, dirname, extname, isAbsolute, join, relative, resolve, sep } from "node:path";
function normalizeFilesystemPath(path, platform = process.platform) {
  if (platform !== "win32")
    return path;
  if (path.startsWith("\\\\?\\UNC\\"))
    return `\\\\${path.slice(8)}`;
  return path.startsWith("\\\\?\\") ? path.slice(4) : path;
}
var resource = resolve(normalizeFilesystemPath(process.env.JARVIS_PIXEL_RESOURCE_DIR ?? dirname(process.argv[1])));
var session = resolve(normalizeFilesystemPath(process.env.IMAGE_COCKPIT_HANDOFF_DIR ?? "handoff"));
var workspace = resolve(normalizeFilesystemPath(process.env.JARVIS_PIXEL_WORKSPACE ?? process.cwd()));
var token = process.env.JARVIS_PIXEL_SESSION_TOKEN ?? "";
var MAX_BYTES = 96 * 1024 * 1024;
var MIME = {
  ".png": "image/png",
  ".gif": "image/gif",
  ".webp": "image/webp",
  ".apng": "image/apng",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".json": "application/json",
  ".zip": "application/zip",
  ".txt": "text/plain",
  ".md": "text/markdown"
};
function sameSecret(value) {
  const a = Buffer.from(token);
  const b = Buffer.from(value);
  return a.length > 0 && a.length === b.length && timingSafeEqual(a, b);
}
function send(response, code, value) {
  response.writeHead(code, { "Content-Type": "application/json", "Cache-Control": "no-store" });
  response.end(JSON.stringify(value));
}
async function body(request) {
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > MAX_BYTES)
      throw new Error("O arquivo excede o limite de 96 MiB da operação.");
    chunks.push(chunk);
  }
  return JSON.parse(Buffer.concat(chunks).toString());
}
function within(path, root) {
  const rel = relative(root, path);
  return rel === "" || !rel.startsWith(`..${sep}`) && rel !== ".." && !isAbsolute(rel);
}
function reportJarvisReady(address) {
  if (!address || typeof address === "string")
    throw new Error("Pixel Cockpit did not bind a TCP port.");
  console.log(JSON.stringify({ type: "ready", port: address.port, origin: `http://127.0.0.1:${address.port}` }));
}
async function handleJarvisRoute(request, response) {
  const url = new URL(request.url ?? "/", "http://127.0.0.1");
  const host = request.headers.host ?? "";
  if (!/^127\.0\.0\.1:\d+$/.test(host) || request.headers.origin && request.headers.origin !== `http://${host}`) {
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
    if (request.method === "GET" && url.pathname === "/jarvis/health" || request.method === "POST" && url.pathname === "/jarvis/repair") {
      const repair = request.method === "POST";
      if (repair) {
        for (const directory of ["artifacts", "logs", "status", "outbox"]) {
          await mkdir(join(session, directory), { recursive: true });
        }
      }
      await access(join(resource, "worker/index.html"), constants.R_OK);
      await access(session, constants.R_OK | constants.W_OK);
      send(response, 200, {
        ok: true,
        mode: "managed",
        jobsInterrupted: false,
        checks: { interfaceReadable: true, sessionWritable: true },
        message: repair ? "Pastas da sessão verificadas. Arquivos e jobs preservados." : "Serviço gerenciado disponível.",
        recovery: "Se o processo encerrar, reabra a suíte para restaurar esta sessão sem repetir gerações."
      });
      return true;
    }
    if (request.method === "POST" && url.pathname === "/jarvis/logs") {
      const data = await body(request);
      if (!Array.isArray(data.jobIds) || data.jobIds.length > 200 || data.jobIds.some((id) => typeof id !== "string" || !/^[a-zA-Z0-9_-]{1,160}$/.test(id)))
        throw new Error("Identificadores de log inválidos.");
      const artifacts = [];
      for (const id of data.jobIds) {
        const path = join(session, "logs", `${id}.log`);
        try {
          const info = await stat(path);
          if (info.isFile())
            artifacts.push({ name: `${id}.log`, path, mime: "text/plain", size: info.size, complete: true });
        } catch (error) {
          if (error.code !== "ENOENT")
            throw error;
        }
      }
      send(response, 200, { artifacts });
      return true;
    }
    if (request.method === "GET" && !url.pathname.startsWith("/api/") && !url.pathname.startsWith("/jarvis/")) {
      const filename = url.pathname === "/" ? "index.html" : decodeURIComponent(url.pathname.slice(1));
      const staticRoot = await realpath(join(resource, "worker"));
      const staticPath = await realpath(resolve(staticRoot, filename));
      if (!within(staticPath, staticRoot))
        throw new Error("Recurso fora do diretório autorizado.");
      const extension = extname(filename);
      const mime = extension === ".html" ? "text/html; charset=utf-8" : extension === ".js" ? "application/javascript; charset=utf-8" : extension === ".css" ? "text/css; charset=utf-8" : extension === ".woff2" ? "font/woff2" : MIME[extension] ?? "application/octet-stream";
      response.writeHead(200, { "Content-Type": mime });
      response.end(await readFile(staticPath));
      return true;
    }
    if (request.method === "POST" && url.pathname === "/jarvis/artifacts") {
      const data = await body(request);
      if (typeof data.name !== "string" || typeof data.base64 !== "string" || typeof data.mime !== "string")
        throw new Error("Arquivo incompleto.");
      const name = basename(data.name).replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_").slice(0, 160);
      if (!name || name === "." || name === "..")
        throw new Error("Nome de arquivo inválido.");
      const content = Buffer.from(data.base64, "base64");
      if (!content.length || content.length > MAX_BYTES)
        throw new Error("Arquivo vazio ou grande demais.");
      const destination = join(session, "artifacts", randomUUID(), name);
      await mkdir(dirname(destination), { recursive: true });
      await writeFile(destination, content, { flag: "wx" });
      send(response, 201, { name, path: destination, mime: MIME[extname(name).toLowerCase()] ?? data.mime, size: content.length });
      return true;
    }
    if (url.pathname === "/jarvis/state") {
      const statePath = join(session, "suite-state.json");
      if (request.method === "GET") {
        try {
          send(response, 200, JSON.parse(await readFile(statePath, "utf8")));
        } catch (error) {
          if (error.code !== "ENOENT")
            throw error;
          send(response, 200, {});
        }
        return true;
      }
      if (request.method === "POST") {
        const data = await body(request);
        if (![1, 2].includes(Number(data.version)) || !Array.isArray(data.history) || !Array.isArray(data.frames) || !Array.isArray(data.actions))
          throw new Error("Estado da suíte inválido.");
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
      if (typeof data.path !== "string")
        throw new Error("Informe o caminho do arquivo.");
      const path = await realpath(resolve(workspace, normalizeFilesystemPath(data.path)));
      const roots = await Promise.all([workspace, session].map((root) => realpath(root).catch(() => resolve(root))));
      if (!roots.some((root) => within(path, root)))
        throw new Error("O arquivo está fora do projeto e da sessão autorizados.");
      const info = await stat(path);
      if (!info.isFile() || info.size > MAX_BYTES)
        throw new Error("O caminho não é um arquivo válido ou excede 96 MiB.");
      const mime = MIME[extname(path).toLowerCase()];
      if (!mime)
        throw new Error("Formato de arquivo não suportado.");
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

// src-tauri/resources/pixel-cockpit/generated/managed-runner.ts
function processIsAlive(id) {
  if (!id || !Number.isInteger(id) || id < 1)
    return false;
  try {
    process.kill(id, 0);
    return true;
  } catch (error) {
    return error.code !== "ESRCH";
  }
}
function interruptedRunnerStatus(status, alive = processIsAlive) {
  if (alive(status.processId))
    return status;
  return {
    ...status,
    state: "failed",
    resumeRequired: true,
    message: "O executor foi interrompido. Os arquivos foram preservados e nenhuma geração foi reiniciada.",
    diagnostic: {
      kind: "runner_failed",
      title: "Execução interrompida",
      userMessage: "O processo do Codex terminou antes de confirmar a entrega. É necessário recuperar este mesmo trabalho.",
      suggestion: "Consulte os arquivos e use resumeInterruptedCodexJob com confirmação explícita. Para finalizar uma imagem existente, use process_existing."
    }
  };
}
function assertExplicitResume(status, confirmed, tracked, alive = processIsAlive) {
  if (!confirmed)
    throw new Error("A retomada exige confirm=true após autorização explícita.");
  if (tracked || alive(status.processId))
    throw new Error("Este executor ainda está em andamento; a retomada não foi iniciada.");
  if (status.cancellationPending || !(status.state === "running" || status.resumeRequired)) {
    throw new Error("Somente um trabalho interrompido pode ser retomado. Consulte seu resultado antes de executar outra ação.");
  }
}
function cancellationMessage(reason, confirmed) {
  const origin = reason === "user" ? "por solicitação do usuário" : "pelo controle do torneio";
  return confirmed ? `Execução cancelada ${origin}.` : `Cancelamento solicitado ${origin}; aguardando o encerramento do executor.`;
}

// src-tauri/resources/pixel-cockpit/generated/server/index.ts
import { spawn } from "node:child_process";
import { createHash as createHash2 } from "node:crypto";
import { closeSync, createWriteStream, existsSync, openSync, readFileSync, readSync, readdirSync, writeFileSync } from "node:fs";
import { copyFile, mkdir as mkdir2, readFile as readFile2, readdir, rename as rename2, stat as stat2, writeFile as writeFile3 } from "node:fs/promises";
import { createServer } from "node:http";
import { basename as basename2, delimiter, extname as extname2, join as join3, resolve as resolve2, sep as sep2 } from "node:path";

// src-tauri/resources/pixel-cockpit/generated/src/lib/animationTournamentGate.ts
var BEST_FIRST_QUALIFIED_MIN_SCORE = 3300;
var BEST_FIRST_QUALIFIED_MIN_IDENTITY_SCORE = 85;
function evaluateBestFirstQualifiedCandidate(candidate) {
  if (!candidate.ready) {
    return { qualified: false, reason: "first Best candidate did not pass every requested direction" };
  }
  if (candidate.warningCount !== 0) {
    return { qualified: false, reason: "first Best candidate has quality warnings" };
  }
  if (candidate.score < BEST_FIRST_QUALIFIED_MIN_SCORE) {
    return { qualified: false, reason: "first Best candidate score is below the strict solo threshold" };
  }
  if (typeof candidate.identityScore !== "number" || candidate.identityScore < BEST_FIRST_QUALIFIED_MIN_IDENTITY_SCORE) {
    return { qualified: false, reason: "first Best candidate identity is below the strict solo threshold" };
  }
  if (candidate.shadowWouldBlock !== false) {
    return { qualified: false, reason: "first Best candidate has a shadow quality block" };
  }
  return {
    qualified: true,
    reason: "first completed Best candidate passed the strict solo gate"
  };
}

// src-tauri/resources/pixel-cockpit/generated/server/local-generator.ts
import { createHash } from "node:crypto";
import { writeFile as writeFile2 } from "node:fs/promises";
import { join as join2 } from "node:path";
import { deflateSync } from "node:zlib";
var pngSignature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
var crcTable = buildCrcTable();
async function generateLocalImages(request, outboxDir, id) {
  const workflowMode = normalizeLocalGenerationWorkflow(request.workflowMode);
  const count = workflowMode === "image-generate" ? clampInteger(request.count ?? 1, 1, 4) : 1;
  const results = [];
  for (let index = 0;index < count; index += 1) {
    const image = workflowMode === "sprite-generate" ? renderSpriteSheet(request, index) : renderPromptImage(request, index);
    const suffix = workflowMode === "sprite-generate" ? "sprite-sheet" : `image-${index + 1}`;
    const name = `${id}-${suffix}.png`;
    const path = join2(outboxDir, name);
    await writeFile2(path, encodePng(image));
    results.push({ name, path, mimeType: "image/png" });
  }
  const sidecarName = `${id}-local-generation.json`;
  await writeFile2(join2(outboxDir, sidecarName), JSON.stringify({
    id,
    createdAt: new Date().toISOString(),
    workflowMode,
    prompt: request.prompt ?? "",
    negativePrompt: request.negativePrompt ?? "",
    jobNotes: request.jobNotes ?? "",
    seed: request.seed ?? "",
    results: results.map((result) => result.name),
    generator: "image-cockpit.local-procedural-v1"
  }, null, 2), "utf8");
  return results;
}
function renderPromptImage(request, index) {
  const { width, height } = parseSize(request.size, 1024, 1024);
  const prompt = `${request.prompt ?? ""} ${request.jobNotes ?? ""}`.toLowerCase();
  const seed = `${request.seed ?? ""}:${request.prompt ?? ""}:${index}`;
  const random = createRng(seed);
  const palette = paletteForPrompt(prompt, random);
  const canvas = createCanvas(width, height);
  drawSoftBackdrop(canvas, palette, random);
  drawPromptMotifs(canvas, prompt, palette, random);
  drawCharacter(canvas, Math.round(width * 0.5), Math.round(height * 0.56), Math.round(Math.min(width, height) * 0.38), {
    palette,
    frame: index,
    variant: prompt,
    random
  });
  drawSparkPixels(canvas, palette.accent, random, Math.round(width * height * 0.00045));
  return canvas;
}
function renderSpriteSheet(request, index) {
  const grid = parseGrid(request.grid);
  const cell = parseCell(request.cell);
  const width = grid.columns * cell.width;
  const height = grid.rows * cell.height;
  const prompt = `${request.prompt ?? ""} ${request.jobNotes ?? ""}`.toLowerCase();
  const random = createRng(`${request.seed ?? ""}:${request.prompt ?? ""}:sprite:${index}`);
  const palette = paletteForPrompt(prompt, random);
  const canvas = createCanvas(width, height);
  const frameCount = grid.columns * grid.rows;
  for (let frame = 0;frame < frameCount; frame += 1) {
    const column = frame % grid.columns;
    const row = Math.floor(frame / grid.columns);
    const x = column * cell.width;
    const y = row * cell.height;
    const phase = frame / Math.max(1, grid.columns - 1) * Math.PI * 2;
    drawFrameGuide(canvas, x, y, cell.width, cell.height, frame, palette);
    drawCharacter(canvas, x + Math.round(cell.width / 2), y + Math.round(cell.height * 0.58 + Math.sin(phase) * 3), Math.round(Math.min(cell.width, cell.height) * 0.58), {
      palette,
      frame,
      variant: `${prompt} ${request.action ?? ""}`,
      random,
      walkPhase: phase
    });
  }
  return canvas;
}
function createCanvas(width, height) {
  return { width, height, pixels: new Uint8Array(width * height * 4) };
}
function drawSoftBackdrop(canvas, palette, random) {
  const cx = canvas.width * (0.42 + random() * 0.16);
  const cy = canvas.height * (0.35 + random() * 0.12);
  const radius = Math.min(canvas.width, canvas.height) * (0.42 + random() * 0.14);
  for (let y = 0;y < canvas.height; y += 1) {
    for (let x = 0;x < canvas.width; x += 1) {
      const distance = Math.hypot(x - cx, y - cy) / radius;
      const alpha = Math.max(0, 1 - distance) * 70;
      if (alpha > 0)
        blendPixel(canvas, x, y, [palette.glow[0], palette.glow[1], palette.glow[2], alpha]);
    }
  }
  fillEllipse(canvas, canvas.width / 2, canvas.height * 0.78, canvas.width * 0.26, canvas.height * 0.045, palette.shadow);
}
function drawPromptMotifs(canvas, prompt, palette, random) {
  const count = prompt.includes("forest") || prompt.includes("森") ? 26 : prompt.includes("space") || prompt.includes("星") ? 34 : 18;
  for (let i = 0;i < count; i += 1) {
    const x = Math.round(canvas.width * (0.14 + random() * 0.72));
    const y = Math.round(canvas.height * (0.09 + random() * 0.58));
    const size = Math.max(3, Math.round(Math.min(canvas.width, canvas.height) * (0.008 + random() * 0.012)));
    if (prompt.includes("fire") || prompt.includes("炎")) {
      fillDiamond(canvas, x, y, size, palette.accent);
    } else if (prompt.includes("water") || prompt.includes("水")) {
      fillEllipse(canvas, x, y, size * 0.8, size * 1.3, [palette.accent[0], palette.accent[1], palette.accent[2], 130]);
    } else {
      fillRect(canvas, x, y, size, size, [palette.accent[0], palette.accent[1], palette.accent[2], 105]);
    }
  }
}
function drawFrameGuide(canvas, x, y, width, height, frame, palette) {
  const guide = frame % 2 === 0 ? [palette.glow[0], palette.glow[1], palette.glow[2], 22] : [palette.primary[0], palette.primary[1], palette.primary[2], 16];
  fillRect(canvas, x + 2, y + 2, width - 4, height - 4, guide);
  fillEllipse(canvas, x + width / 2, y + height * 0.82, width * 0.28, height * 0.04, palette.shadow);
}
function drawCharacter(canvas, cx, cy, size, options) {
  const { palette, frame, variant, walkPhase = frame * 0.75 } = options;
  const pixel = Math.max(2, Math.round(size / 28));
  const bodyW = pixel * 10;
  const bodyH = pixel * 16;
  const headR = pixel * 5;
  const legSwing = Math.round(Math.sin(walkPhase) * pixel * 2);
  const armSwing = Math.round(Math.cos(walkPhase) * pixel * 2);
  const baseY = cy + pixel * 11;
  fillRect(canvas, cx - bodyW / 2, cy - bodyH / 2, bodyW, bodyH, palette.primary);
  fillRect(canvas, cx - bodyW / 2 + pixel * 2, cy - bodyH / 2 + pixel * 3, bodyW - pixel * 4, bodyH - pixel * 3, palette.secondary);
  fillCircle(canvas, cx, cy - bodyH / 2 - headR + pixel, headR, [238, 207, 168, 255]);
  fillRect(canvas, cx - headR, cy - bodyH / 2 - headR * 2, headR * 2, pixel * 3, palette.accent);
  if (variant.includes("mage") || variant.includes("wizard") || variant.includes("魔")) {
    fillTriangle(canvas, cx - headR - pixel, cy - bodyH / 2 - headR * 2, cx, cy - bodyH / 2 - headR * 4, cx + headR + pixel, cy - bodyH / 2 - headR * 2, palette.accent);
    drawLine(canvas, cx + bodyW / 2 + pixel * 2, cy - pixel * 8, cx + bodyW / 2 + pixel * 2, baseY, pixel, [116, 79, 43, 255]);
    fillDiamond(canvas, cx + bodyW / 2 + pixel * 2, cy - pixel * 10, pixel * 3, palette.glow);
  } else if (variant.includes("mech") || variant.includes("robot")) {
    fillRect(canvas, cx - headR, cy - bodyH / 2 - headR * 2, headR * 2, pixel * 2, [225, 232, 240, 255]);
    fillRect(canvas, cx - pixel * 2, cy - bodyH / 2 - headR, pixel * 4, pixel * 2, palette.accent);
  } else {
    fillTriangle(canvas, cx - headR, cy - bodyH / 2 - headR * 2, cx, cy - bodyH / 2 - headR * 3, cx + headR, cy - bodyH / 2 - headR * 2, palette.accent);
  }
  drawLimb(canvas, cx - bodyW / 2, cy - pixel * 2, cx - bodyW / 2 - pixel * 4, cy + pixel * 5 + armSwing, pixel * 2, palette.primary);
  drawLimb(canvas, cx + bodyW / 2, cy - pixel * 2, cx + bodyW / 2 + pixel * 4, cy + pixel * 5 - armSwing, pixel * 2, palette.primary);
  drawLimb(canvas, cx - pixel * 3, cy + bodyH / 2 - pixel, cx - pixel * 4 - legSwing, baseY, pixel * 2, palette.secondary);
  drawLimb(canvas, cx + pixel * 3, cy + bodyH / 2 - pixel, cx + pixel * 4 + legSwing, baseY, pixel * 2, palette.secondary);
  fillRect(canvas, cx - headR + pixel * 2, cy - bodyH / 2 - headR + pixel, pixel * 2, pixel * 2, [42, 45, 54, 255]);
  fillRect(canvas, cx + headR - pixel * 4, cy - bodyH / 2 - headR + pixel, pixel * 2, pixel * 2, [42, 45, 54, 255]);
}
function drawSparkPixels(canvas, color, random, count) {
  const size = Math.max(2, Math.round(Math.min(canvas.width, canvas.height) / 160));
  for (let i = 0;i < count; i += 1) {
    const x = Math.round(random() * canvas.width);
    const y = Math.round(random() * canvas.height);
    fillRect(canvas, x, y, size, size, [color[0], color[1], color[2], 95]);
  }
}
function drawLimb(canvas, x1, y1, x2, y2, width, color) {
  drawLine(canvas, Math.round(x1), Math.round(y1), Math.round(x2), Math.round(y2), Math.max(1, Math.round(width)), color);
}
function drawLine(canvas, x1, y1, x2, y2, width, color) {
  const steps = Math.max(Math.abs(x2 - x1), Math.abs(y2 - y1), 1);
  for (let i = 0;i <= steps; i += 1) {
    const t = i / steps;
    const x = Math.round(x1 + (x2 - x1) * t);
    const y = Math.round(y1 + (y2 - y1) * t);
    fillCircle(canvas, x, y, width, color);
  }
}
function fillRect(canvas, x, y, width, height, color) {
  const left = clampInteger(Math.floor(x), 0, canvas.width);
  const top = clampInteger(Math.floor(y), 0, canvas.height);
  const right = clampInteger(Math.ceil(x + width), 0, canvas.width);
  const bottom = clampInteger(Math.ceil(y + height), 0, canvas.height);
  for (let py = top;py < bottom; py += 1) {
    for (let px = left;px < right; px += 1)
      blendPixel(canvas, px, py, color);
  }
}
function fillCircle(canvas, cx, cy, radius, color) {
  fillEllipse(canvas, cx, cy, radius, radius, color);
}
function fillEllipse(canvas, cx, cy, rx, ry, color) {
  const left = Math.floor(cx - rx);
  const right = Math.ceil(cx + rx);
  const top = Math.floor(cy - ry);
  const bottom = Math.ceil(cy + ry);
  for (let y = top;y <= bottom; y += 1) {
    for (let x = left;x <= right; x += 1) {
      const dx = (x - cx) / Math.max(1, rx);
      const dy = (y - cy) / Math.max(1, ry);
      if (dx * dx + dy * dy <= 1)
        blendPixel(canvas, x, y, color);
    }
  }
}
function fillDiamond(canvas, cx, cy, radius, color) {
  for (let y = Math.floor(cy - radius);y <= Math.ceil(cy + radius); y += 1) {
    for (let x = Math.floor(cx - radius);x <= Math.ceil(cx + radius); x += 1) {
      if (Math.abs(x - cx) + Math.abs(y - cy) <= radius)
        blendPixel(canvas, x, y, color);
    }
  }
}
function fillTriangle(canvas, ax, ay, bx, by, cx, cy, color) {
  const minX = Math.floor(Math.min(ax, bx, cx));
  const maxX = Math.ceil(Math.max(ax, bx, cx));
  const minY = Math.floor(Math.min(ay, by, cy));
  const maxY = Math.ceil(Math.max(ay, by, cy));
  const area = edge(ax, ay, bx, by, cx, cy);
  if (area === 0)
    return;
  for (let y = minY;y <= maxY; y += 1) {
    for (let x = minX;x <= maxX; x += 1) {
      const w0 = edge(bx, by, cx, cy, x, y);
      const w1 = edge(cx, cy, ax, ay, x, y);
      const w2 = edge(ax, ay, bx, by, x, y);
      if (w0 >= 0 && w1 >= 0 && w2 >= 0 || w0 <= 0 && w1 <= 0 && w2 <= 0)
        blendPixel(canvas, x, y, color);
    }
  }
}
function edge(ax, ay, bx, by, cx, cy) {
  return (cx - ax) * (by - ay) - (cy - ay) * (bx - ax);
}
function blendPixel(canvas, x, y, color) {
  if (x < 0 || y < 0 || x >= canvas.width || y >= canvas.height)
    return;
  const offset = (Math.floor(y) * canvas.width + Math.floor(x)) * 4;
  const sourceAlpha = color[3] / 255;
  const targetAlpha = canvas.pixels[offset + 3] / 255;
  const outputAlpha = sourceAlpha + targetAlpha * (1 - sourceAlpha);
  if (outputAlpha <= 0)
    return;
  canvas.pixels[offset] = Math.round((color[0] * sourceAlpha + canvas.pixels[offset] * targetAlpha * (1 - sourceAlpha)) / outputAlpha);
  canvas.pixels[offset + 1] = Math.round((color[1] * sourceAlpha + canvas.pixels[offset + 1] * targetAlpha * (1 - sourceAlpha)) / outputAlpha);
  canvas.pixels[offset + 2] = Math.round((color[2] * sourceAlpha + canvas.pixels[offset + 2] * targetAlpha * (1 - sourceAlpha)) / outputAlpha);
  canvas.pixels[offset + 3] = Math.round(outputAlpha * 255);
}
function encodePng(canvas) {
  const stride = canvas.width * 4;
  const raw = Buffer.alloc((stride + 1) * canvas.height);
  for (let y = 0;y < canvas.height; y += 1) {
    const rowOffset = y * (stride + 1);
    raw[rowOffset] = 0;
    Buffer.from(canvas.pixels.buffer, canvas.pixels.byteOffset + y * stride, stride).copy(raw, rowOffset + 1);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(canvas.width, 0);
  ihdr.writeUInt32BE(canvas.height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  ihdr[10] = 0;
  ihdr[11] = 0;
  ihdr[12] = 0;
  return Buffer.concat([
    pngSignature,
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", deflateSync(raw, { level: 9 })),
    pngChunk("IEND", Buffer.alloc(0))
  ]);
}
function pngChunk(type, data) {
  const typeBuffer = Buffer.from(type, "ascii");
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length, 0);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([typeBuffer, data])), 0);
  return Buffer.concat([length, typeBuffer, data, crc]);
}
function crc32(buffer) {
  let crc = 4294967295;
  for (const byte of buffer)
    crc = crcTable[(crc ^ byte) & 255] ^ crc >>> 8;
  return (crc ^ 4294967295) >>> 0;
}
function buildCrcTable() {
  const table = new Uint32Array(256);
  for (let i = 0;i < 256; i += 1) {
    let value = i;
    for (let bit = 0;bit < 8; bit += 1)
      value = value & 1 ? 3988292384 ^ value >>> 1 : value >>> 1;
    table[i] = value >>> 0;
  }
  return table;
}
function paletteForPrompt(prompt, random) {
  if (prompt.includes("fire") || prompt.includes("炎"))
    return createPalette([205, 72, 47, 255], [85, 36, 45, 255], [255, 189, 77, 235]);
  if (prompt.includes("water") || prompt.includes("水"))
    return createPalette([49, 135, 202, 255], [42, 68, 136, 255], [143, 229, 245, 225]);
  if (prompt.includes("space") || prompt.includes("星"))
    return createPalette([98, 82, 184, 255], [34, 38, 86, 255], [238, 210, 113, 225]);
  if (prompt.includes("mech") || prompt.includes("robot"))
    return createPalette([108, 125, 143, 255], [43, 54, 68, 255], [104, 232, 215, 230]);
  if (prompt.includes("cute") || prompt.includes("かわいい"))
    return createPalette([232, 116, 163, 255], [105, 73, 132, 255], [255, 213, 126, 220]);
  const hue = Math.floor(random() * 5);
  const options = [
    [[46, 142, 97, 255], [35, 78, 67, 255], [155, 224, 122, 220]],
    [[180, 91, 77, 255], [84, 54, 68, 255], [247, 189, 97, 220]],
    [[76, 121, 190, 255], [40, 57, 102, 255], [137, 213, 228, 220]],
    [[136, 93, 184, 255], [68, 49, 103, 255], [227, 171, 255, 220]],
    [[190, 132, 64, 255], [79, 61, 44, 255], [241, 214, 117, 220]]
  ];
  return createPalette(...options[hue]);
}
function createPalette(primary, secondary, accent) {
  return {
    primary,
    secondary,
    accent,
    shadow: [24, 28, 34, 78],
    glow: [accent[0], accent[1], accent[2], 150]
  };
}
function parseSize(value, fallbackWidth, fallbackHeight) {
  const match = value?.match(/^(\d{2,4})x(\d{2,4})$/);
  if (!match)
    return { width: fallbackWidth, height: fallbackHeight };
  return {
    width: clampInteger(Number(match[1]), 256, 1536),
    height: clampInteger(Number(match[2]), 256, 1536)
  };
}
function parseGrid(value) {
  const grid = value && typeof value === "object" ? value : {};
  return {
    columns: clampInteger(Number(grid.columns ?? 8), 1, 12),
    rows: clampInteger(Number(grid.rows ?? 4), 1, 8)
  };
}
function parseCell(value) {
  const cell = value && typeof value === "object" ? value : {};
  return {
    width: clampInteger(Number(cell.width ?? 128), 32, 256),
    height: clampInteger(Number(cell.height ?? 128), 32, 256)
  };
}
function normalizeLocalGenerationWorkflow(value) {
  return value === "sprite-generate" ? "sprite-generate" : "image-generate";
}
function clampInteger(value, min, max) {
  if (!Number.isFinite(value))
    return min;
  return Math.max(min, Math.min(max, Math.round(value)));
}
function createRng(seed) {
  const digest = createHash("sha256").update(seed || "image-cockpit").digest();
  let state = digest.readUInt32LE(0) || 2654435769;
  return () => {
    state |= 0;
    state = state + 1831565813 | 0;
    let value = Math.imul(state ^ state >>> 15, 1 | state);
    value ^= value + Math.imul(value ^ value >>> 7, 61 | value);
    return ((value ^ value >>> 14) >>> 0) / 4294967296;
  };
}

// src-tauri/resources/pixel-cockpit/generated/server/index.ts
loadDotEnv(resolve2(".env"));
var port = Number(process.env.IMAGE_COCKPIT_API_PORT ?? 8787);
var handoffRoot = resolve2(process.env.IMAGE_COCKPIT_HANDOFF_DIR ?? "codex-handoff");
var inboxDir = join3(handoffRoot, "inbox");
var outboxDir = join3(handoffRoot, "outbox");
var tournamentWorkRootDir = join3(outboxDir, ".tournaments");
var assetsDir = join3(handoffRoot, "assets");
var statusDir = join3(handoffRoot, "status");
var logsDir = join3(handoffRoot, "logs");
var codexAutoRun = process.env.IMAGE_COCKPIT_CODEX_AUTORUN !== "0";
var codexCommand = process.env.IMAGE_COCKPIT_CODEX_COMMAND ?? "codex";
var codexCommandCandidates = resolveCommandCandidates(codexCommand);
var codexLaunchCommand = selectCodexLaunchCommand(codexCommand, codexCommandCandidates);
var codexSandbox = process.env.IMAGE_COCKPIT_CODEX_SANDBOX ?? "workspace-write";
var codexApproval = process.env.IMAGE_COCKPIT_CODEX_APPROVAL ?? "never";
var codexHelpArgs = parseJsonStringArray("IMAGE_COCKPIT_CODEX_HELP_ARGS_JSON", ["--help"]);
var codexExecArgs = parseJsonStringArray("IMAGE_COCKPIT_CODEX_EXEC_ARGS_JSON", [
  "exec",
  "-c",
  `approval_policy=${JSON.stringify(codexApproval)}`,
  "--sandbox",
  codexSandbox,
  "-"
]);
var codexRunnerMode = detectCodexRunnerMode(codexCommand, codexLaunchCommand, codexHelpArgs, codexExecArgs);
var allowMockRunner = process.env.IMAGE_COCKPIT_ALLOW_MOCK_RUNNER === "1";
var runnerStaleTimeoutMs = parsePositiveNumber("IMAGE_COCKPIT_CODEX_STALE_MS", 30 * 60 * 1000);
var runnerStaleLogIdleMs = parsePositiveNumber("IMAGE_COCKPIT_CODEX_STALE_LOG_IDLE_MS", 5 * 60 * 1000);
var runnerCapacityCooldownMs = parsePositiveNumber("IMAGE_COCKPIT_CODEX_CAPACITY_COOLDOWN_MS", 15 * 60 * 1000);
var runnerLogTailDefaultBytes = 24 * 1024;
var runnerLogTailMaxBytes = 96 * 1024;
var resultRoutePrefix = "/api/codex/results/";
var runnerPreflightTimeoutMs = 4000;
var directionSplitSlugs = ["front", "front-three-quarter", "side", "back-three-quarter", "back"];
var directionSplitNames = ["front", "front three-quarter", "side", "back three-quarter", "back"];
var directionSplitManifestSchema = "image-cockpit.direction-split-animation.v1";
var artifactStableMs = parsePositiveNumber("IMAGE_COCKPIT_ARTIFACT_STABLE_MS", 1500);
var maxActiveCodexJobs = 3;
var runnerStatuses = new Map;
var runnerProcesses = new Map;
var resumingRunnerJobs = new Map;
var cancellingRunnerJobIds = new Set;
var animationTournamentLocks = new Map;
var codexRunnerAdmissionTail = Promise.resolve();
var animationTournamentRegistrationTail = Promise.resolve();
var cachedRunnerCapacityBlock = null;

class HttpError extends Error {
  statusCode;
  code;
  details;
  constructor(statusCode, message, code, details = {}) {
    super(message);
    this.name = "HttpError";
    this.statusCode = statusCode;
    this.code = code;
    this.details = details;
  }
}
var server = createServer(async (request, response) => {
  if (await handleJarvisRoute(request, response))
    return;
  response.setHeader("Access-Control-Allow-Methods", "GET,POST,OPTIONS");
  response.setHeader("Access-Control-Allow-Headers", "Content-Type");
  if (request.method === "OPTIONS") {
    response.writeHead(204);
    response.end();
    return;
  }
  try {
    await ensureHandoffDirs();
    const requestUrl = new URL(request.url ?? "/", `http://${request.headers.host ?? "127.0.0.1"}`);
    const pathname = requestUrl.pathname;
    if (request.method === "GET" && pathname === "/api/health") {
      const runner = await checkCodexRunnerPreflight();
      sendJson(response, 200, {
        app: "image-cockpit",
        version: "0.2.1",
        role: "api",
        port,
        handoffRoot,
        inboxReadable: await isDirectoryReadable(inboxDir),
        outboxReadable: await isDirectoryReadable(outboxDir),
        statusReadable: await isDirectoryReadable(statusDir),
        logsReadable: await isDirectoryReadable(logsDir),
        runner: {
          state: runner.state,
          message: runner.message,
          checkedAt: runner.checkedAt,
          autorun: runner.autorun,
          mode: runner.mode,
          mockRunnerAllowed: runner.mockRunnerAllowed
        }
      });
      return;
    }
    if (request.method === "GET" && pathname === "/api/providers") {
      sendJson(response, 200, {
        providers: [
          { id: "local-file", label: "Local File", enabled: true, message: "Use images from this machine" },
          {
            id: "local-generator",
            label: "Local Generator",
            enabled: true,
            path: outboxDir,
            message: "Generate local PNG images without external services"
          },
          {
            id: "codex-handoff",
            label: "Codex Handoff",
            enabled: true,
            path: inboxDir,
            message: codexAutoRun ? `Write local jobs and start ${codexCommand} exec when available` : "Write local jobs for manual Codex pickup"
          },
          {
            id: "local-inbox",
            label: "Local Inbox",
            enabled: true,
            path: outboxDir,
            message: "Import results returned by Codex"
          }
        ]
      });
      return;
    }
    if (request.method === "POST" && pathname === "/api/generate") {
      const rawBody = await readJson(request);
      const validation = validateLocalGenerationRequest(rawBody);
      if (!validation.ok) {
        sendJson(response, 400, { error: validation.error });
        return;
      }
      const body2 = validation.request;
      const createdAt = new Date().toISOString();
      const id = `local-gen-${createdAt.replace(/[:.]/g, "-")}`;
      const results = await generateLocalImages(body2, outboxDir, id);
      const responseResults = await Promise.all(results.map(async (result) => {
        const [bytes, fileStat] = await Promise.all([readFile2(result.path), stat2(result.path)]);
        return {
          ...result,
          size: fileStat.size,
          modifiedAt: fileStat.mtime.toISOString(),
          dataUrl: `data:${result.mimeType};base64,${bytes.toString("base64")}`
        };
      }));
      sendJson(response, 200, { id, createdAt, outboxPath: outboxDir, results: responseResults });
      return;
    }
    if (request.method === "GET" && pathname === "/api/codex/runner") {
      sendJson(response, 200, { runner: await checkCodexRunnerPreflight() });
      return;
    }
    if (request.method === "GET" && pathname === "/api/codex/capacity") {
      const active = await activeCodexRunnerCount();
      sendJson(response, 200, {
        capacity: {
          limit: maxActiveCodexJobs,
          active,
          available: Math.max(0, maxActiveCodexJobs - active)
        }
      });
      return;
    }
    if (request.method === "GET" && pathname === "/api/codex/jobs") {
      const files = await readdir(inboxDir);
      sendJson(response, 200, {
        inboxPath: inboxDir,
        jobs: files.filter((file) => file.endsWith(".json")).sort().reverse().slice(0, 20)
      });
      return;
    }
    const jobResumeMatch = pathname.match(/^\/api\/codex\/jobs\/([^/]+)\/resume$/);
    if (request.method === "POST" && jobResumeMatch) {
      const jobId = decodeURIComponent(jobResumeMatch[1]);
      if (!isSafeJobId(jobId)) {
        sendJson(response, 400, { error: "Identificador de trabalho inválido." });
        return;
      }
      const body2 = await readJson(request);
      const status = await getRunnerStatus(jobId);
      assertExplicitResume(status, body2.confirm === true, runnerProcesses.has(jobId) || resumingRunnerJobs.has(jobId));
      if (body2.mode !== "process_existing" && body2.mode !== "resume") {
        sendJson(response, 400, { error: "Escolha process_existing para finalizar artefatos existentes ou resume para retomar a tarefa original." });
        return;
      }
      managedRecoveryModes.set(jobId, body2.mode);
      try {
        sendJson(response, 200, { status: await resumeUntrackedCodexRunner(status) });
      } finally {
        managedRecoveryModes.delete(jobId);
      }
      return;
    }
    const jobStatusMatch = pathname.match(/^\/api\/codex\/jobs\/([^/]+)\/status$/);
    if (request.method === "GET" && jobStatusMatch) {
      const jobId = decodeURIComponent(jobStatusMatch[1]);
      if (!isSafeJobId(jobId)) {
        sendJson(response, 400, { error: "Unsupported or unsafe job id" });
        return;
      }
      sendJson(response, 200, { status: await getRunnerStatus(jobId) });
      return;
    }
    const jobCancelMatch = pathname.match(/^\/api\/codex\/jobs\/([^/]+)\/cancel$/);
    if (request.method === "POST" && jobCancelMatch) {
      const jobId = decodeURIComponent(jobCancelMatch[1]);
      if (!isSafeJobId(jobId)) {
        sendJson(response, 400, { error: "Unsupported or unsafe job id" });
        return;
      }
      sendJson(response, 200, await cancelCodexRunner(jobId, "user"));
      return;
    }
    const jobLogMatch = pathname.match(/^\/api\/codex\/jobs\/([^/]+)\/log$/);
    if (request.method === "GET" && jobLogMatch) {
      const jobId = decodeURIComponent(jobLogMatch[1]);
      if (!isSafeJobId(jobId)) {
        sendJson(response, 400, { error: "Unsupported or unsafe job id" });
        return;
      }
      const requestedBytes = Number(requestUrl.searchParams.get("bytes") ?? runnerLogTailDefaultBytes);
      sendJson(response, 200, await readRunnerLogTail(jobId, requestedBytes));
      return;
    }
    const qualityGateMatch = pathname.match(/^\/api\/codex\/artifacts\/([^/]+)\/quality-gate$/);
    if (request.method === "POST" && qualityGateMatch) {
      const jobId = decodeURIComponent(qualityGateMatch[1]);
      if (!isSafeJobId(jobId)) {
        sendJson(response, 400, { error: "Unsupported or unsafe job id" });
        return;
      }
      const gate = qualityGateFromRequest(await readJson(request));
      const writeResult = await writeDirectionSplitQualityGate(jobId, gate);
      sendJson(response, 200, writeResult);
      return;
    }
    const animationQualityMatch = pathname.match(/^\/api\/codex\/artifacts\/([^/]+)\/animation-quality$/);
    if (request.method === "POST" && animationQualityMatch) {
      const jobId = decodeURIComponent(animationQualityMatch[1]);
      if (!isSafeJobId(jobId)) {
        sendJson(response, 400, { error: "Unsupported or unsafe job id" });
        return;
      }
      const report = animationQualityFromRequest(await readJson(request));
      if (!report) {
        sendJson(response, 400, { error: "Invalid or unsupported animation quality report" });
        return;
      }
      sendJson(response, 200, await writeDirectionSplitAnimationQuality(jobId, report));
      return;
    }
    if (request.method === "GET" && pathname === "/api/codex/results") {
      const requestedLimit = Number(requestUrl.searchParams.get("limit") ?? 20);
      const limit = Number.isFinite(requestedLimit) ? Math.min(Math.max(Math.floor(requestedLimit), 1), 200) : 20;
      sendJson(response, 200, {
        outboxPath: outboxDir,
        results: (await listOutboxResults()).slice(0, limit)
      });
      return;
    }
    const jobResultsMatch = pathname.match(/^\/api\/codex\/jobs\/([^/]+)\/results$/);
    if (request.method === "GET" && jobResultsMatch) {
      const jobId = decodeURIComponent(jobResultsMatch[1]);
      if (!isSafeJobId(jobId)) {
        sendJson(response, 400, { error: "Unsupported or unsafe job id" });
        return;
      }
      const jobOutboxDir = await resolveJobOutboxDir(jobId);
      if (!jobOutboxDir) {
        sendJson(response, 404, { error: "Job outbox was not found" });
        return;
      }
      const requestedLimit = Number(requestUrl.searchParams.get("limit") ?? 20);
      const limit = Number.isFinite(requestedLimit) ? Math.min(Math.max(Math.floor(requestedLimit), 1), 200) : 20;
      sendJson(response, 200, {
        outboxPath: jobOutboxDir,
        results: (await listOutboxResults(jobOutboxDir)).slice(0, limit)
      });
      return;
    }
    if (request.method === "GET" && pathname.startsWith(resultRoutePrefix)) {
      const name = decodeURIComponent(pathname.slice(resultRoutePrefix.length));
      const filePath = resolveOutboxFile(name);
      const mimeType = mimeTypeForOutboxResult(name);
      if (!filePath || !mimeType) {
        sendJson(response, 400, { error: "Unsupported or unsafe outbox file" });
        return;
      }
      const [bytes, fileStat] = await Promise.all([readFile2(filePath), stat2(filePath)]);
      sendJson(response, 200, {
        name,
        path: filePath,
        size: fileStat.size,
        modifiedAt: fileStat.mtime.toISOString(),
        mimeType,
        dataUrl: `data:${mimeType};base64,${bytes.toString("base64")}`
      });
      return;
    }
    const jobResultFileMatch = pathname.match(/^\/api\/codex\/jobs\/([^/]+)\/results\/(.+)$/);
    if (request.method === "GET" && jobResultFileMatch) {
      const jobId = decodeURIComponent(jobResultFileMatch[1]);
      const name = decodeURIComponent(jobResultFileMatch[2]);
      if (!isSafeJobId(jobId)) {
        sendJson(response, 400, { error: "Unsupported or unsafe job id" });
        return;
      }
      const jobOutboxDir = await resolveJobOutboxDir(jobId);
      const filePath = jobOutboxDir ? resolveOutboxFileInDir(jobOutboxDir, name) : null;
      const mimeType = mimeTypeForOutboxResult(name);
      if (!filePath || !mimeType) {
        sendJson(response, 400, { error: "Unsupported or unsafe outbox file" });
        return;
      }
      const [bytes, fileStat] = await Promise.all([readFile2(filePath), stat2(filePath)]);
      sendJson(response, 200, {
        name,
        path: filePath,
        size: fileStat.size,
        modifiedAt: fileStat.mtime.toISOString(),
        mimeType,
        dataUrl: `data:${mimeType};base64,${bytes.toString("base64")}`
      });
      return;
    }
    if (request.method === "GET" && pathname === "/api/codex/tournaments") {
      const manifests = await listAnimationTournamentManifests();
      sendJson(response, 200, { tournaments: manifests });
      return;
    }
    if (request.method === "POST" && pathname === "/api/codex/tournaments") {
      const registration = await readJson(request);
      const result = registration.startInitialCandidates === true ? await registerAndStartAnimationTournament(registration) : await registerAnimationTournament(registration);
      sendJson(response, result.created ? 201 : 200, result);
      return;
    }
    const tournamentManifestMatch = pathname.match(/^\/api\/codex\/tournaments\/([^/]+)$/);
    if (request.method === "GET" && tournamentManifestMatch) {
      const tournamentId = decodeURIComponent(tournamentManifestMatch[1]);
      if (!isSafeTournamentId(tournamentId)) {
        sendJson(response, 400, { error: "Unsupported or unsafe tournament id" });
        return;
      }
      const manifest = await refreshAnimationTournamentManifest(tournamentId);
      if (!manifest) {
        sendJson(response, 404, { error: "Tournament manifest was not found" });
        return;
      }
      sendJson(response, 200, { tournament: manifest });
      return;
    }
    const tournamentInitialCandidatesMatch = pathname.match(/^\/api\/codex\/tournaments\/([^/]+)\/initial-candidates$/);
    if (request.method === "POST" && tournamentInitialCandidatesMatch) {
      const tournamentId = decodeURIComponent(tournamentInitialCandidatesMatch[1]);
      if (!isSafeTournamentId(tournamentId)) {
        sendJson(response, 400, { error: "Unsupported or unsafe tournament initial-candidate request" });
        return;
      }
      const result = await startAnimationTournamentInitialCandidateBundle(tournamentId);
      sendJson(response, result.reused ? 200 : 201, result);
      return;
    }
    const tournamentCandidateMatch = pathname.match(/^\/api\/codex\/tournaments\/([^/]+)\/candidates$/);
    if (request.method === "POST" && tournamentCandidateMatch) {
      const tournamentId = decodeURIComponent(tournamentCandidateMatch[1]);
      const body2 = await readJson(request);
      const candidateIndex = normalizeCandidateIndex(body2.candidateIndex);
      if (!isSafeTournamentId(tournamentId)) {
        sendJson(response, 400, { error: "Unsupported or unsafe tournament candidate request" });
        return;
      }
      const result = await startAnimationTournamentCandidate(tournamentId, candidateIndex, typeof body2.reason === "string" ? body2.reason : undefined);
      sendJson(response, result.reused ? 200 : 201, result);
      return;
    }
    const tournamentEvaluationMatch = pathname.match(/^\/api\/codex\/tournaments\/([^/]+)\/evaluation$/);
    if (request.method === "POST" && tournamentEvaluationMatch) {
      const tournamentId = decodeURIComponent(tournamentEvaluationMatch[1]);
      if (!isSafeTournamentId(tournamentId)) {
        sendJson(response, 400, { error: "Unsupported or unsafe tournament evaluation request" });
        return;
      }
      const body2 = await readJson(request);
      sendJson(response, 200, {
        tournament: await recordAnimationTournamentEvaluation(tournamentId, body2)
      });
      return;
    }
    const tournamentCancelMatch = pathname.match(/^\/api\/codex\/tournaments\/([^/]+)\/cancel$/);
    if (request.method === "POST" && tournamentCancelMatch) {
      const tournamentId = decodeURIComponent(tournamentCancelMatch[1]);
      if (!isSafeTournamentId(tournamentId)) {
        sendJson(response, 400, { error: "Unsupported or unsafe tournament cancel request" });
        return;
      }
      sendJson(response, 200, await cancelAnimationTournament(tournamentId));
      return;
    }
    const tournamentRepairMatch = pathname.match(/^\/api\/codex\/tournaments\/([^/]+)\/repairs$/);
    if (request.method === "POST" && tournamentRepairMatch) {
      const tournamentId = decodeURIComponent(tournamentRepairMatch[1]);
      const body2 = await readJson(request);
      if (!isSafeTournamentId(tournamentId)) {
        sendJson(response, 400, { error: "Unsupported or unsafe tournament repair request" });
        return;
      }
      const result = await startAnimationDirectionRepair(tournamentId, body2.directions);
      sendJson(response, result.reused ? 200 : 201, result);
      return;
    }
    const tournamentRepairAcceptMatch = pathname.match(/^\/api\/codex\/tournaments\/([^/]+)\/repairs\/accept$/);
    if (request.method === "POST" && tournamentRepairAcceptMatch) {
      const tournamentId = decodeURIComponent(tournamentRepairAcceptMatch[1]);
      const body2 = await readJson(request);
      const jobId = typeof body2.jobId === "string" ? body2.jobId : "";
      if (!isSafeTournamentId(tournamentId) || !isSafeJobId(jobId)) {
        sendJson(response, 400, { error: "Unsupported or unsafe tournament repair acceptance request" });
        return;
      }
      sendJson(response, 200, await acceptAnimationDirectionRepair(tournamentId, jobId));
      return;
    }
    const tournamentReviewMatch = pathname.match(/^\/api\/codex\/tournaments\/([^/]+)\/review$/);
    if (request.method === "POST" && tournamentReviewMatch) {
      const tournamentId = decodeURIComponent(tournamentReviewMatch[1]);
      if (!isSafeTournamentId(tournamentId)) {
        sendJson(response, 400, { error: "Unsupported or unsafe tournament review request" });
        return;
      }
      const body2 = await readJson(request);
      sendJson(response, 200, { tournament: await recordAnimationHumanReview(tournamentId, body2.review) });
      return;
    }
    const motionPilotExpandMatch = pathname.match(/^\/api\/codex\/tournaments\/([^/]+)\/pilot\/expand$/);
    if (request.method === "POST" && motionPilotExpandMatch) {
      const tournamentId = decodeURIComponent(motionPilotExpandMatch[1]);
      const body2 = await readJson(request);
      const jobId = typeof body2.jobId === "string" ? body2.jobId : "";
      if (!isSafeTournamentId(tournamentId) || !isSafeJobId(jobId)) {
        sendJson(response, 400, { error: "Unsupported or unsafe Motion Pilot expansion request" });
        return;
      }
      const result = await startMotionPilotExpansion(tournamentId, jobId);
      sendJson(response, result.reused ? 200 : 201, result);
      return;
    }
    const motionPilotFallbackMatch = pathname.match(/^\/api\/codex\/tournaments\/([^/]+)\/pilot\/fallback$/);
    if (request.method === "POST" && motionPilotFallbackMatch) {
      const tournamentId = decodeURIComponent(motionPilotFallbackMatch[1]);
      const body2 = await readJson(request);
      if (!isSafeTournamentId(tournamentId)) {
        sendJson(response, 400, { error: "Unsupported or unsafe Motion Pilot fallback request" });
        return;
      }
      sendJson(response, 201, await fallbackMotionPilotTournament(tournamentId, normalizeShortText(body2.reason) ?? "human review requested fallback"));
      return;
    }
    const tournamentWinnerMatch = pathname.match(/^\/api\/codex\/tournaments\/([^/]+)\/winner$/);
    if (request.method === "POST" && tournamentWinnerMatch) {
      const tournamentId = decodeURIComponent(tournamentWinnerMatch[1]);
      const body2 = await readJson(request);
      const jobId = typeof body2.jobId === "string" ? body2.jobId : "";
      if (!isSafeTournamentId(tournamentId) || !isSafeJobId(jobId)) {
        sendJson(response, 400, { error: "Unsupported or unsafe tournament winner request" });
        return;
      }
      const { result, tournament, cancellationResults } = await publishAndAcceptTournamentWinner(tournamentId, jobId, body2.decision);
      sendJson(response, 200, { ...result, tournament, cancellationResults });
      return;
    }
    if (request.method === "POST" && pathname === "/api/codex/jobs") {
      const body2 = await readJson(request);
      if (!body2.prompt?.trim()) {
        sendJson(response, 400, { error: "Prompt is required for a Codex handoff job" });
        return;
      }
      if (hasTournamentScopedJobFields(body2)) {
        throw new HttpError(409, "Tournament-scoped jobs must start through the animation tournament endpoints.", "tournament_job_endpoint_required", { retryable: false });
      }
      if (normalizeWorkflowMode(body2.workflowMode) === "sprite-generate" && body2.spriteVariant !== "hatch-pet" && body2.spriteVariant !== "directional-hatch-pet") {
        throw new HttpError(409, "Standard animation generation must start through the animation tournament endpoint.", "animation_tournament_endpoint_required", { retryable: false });
      }
      const result = await createCodexJob(body2);
      sendJson(response, result.reused ? 200 : 201, result);
      return;
    }
    sendJson(response, 404, { error: "Not found" });
  } catch (error) {
    sendJson(response, error instanceof HttpError ? error.statusCode : 500, {
      error: error instanceof Error ? error.message : "Internal server error",
      ...error instanceof HttpError && error.code ? { code: error.code } : {},
      ...error instanceof HttpError ? error.details : {}
    });
  }
});
server.listen(port, "127.0.0.1", () => {
  reportJarvisReady(server.address());
  console.log(`Codex handoff inbox: ${inboxDir}`);
});
async function ensureHandoffDirs() {
  await mkdir2(inboxDir, { recursive: true });
  await mkdir2(outboxDir, { recursive: true });
  await mkdir2(join3(outboxDir, ".staging"), { recursive: true });
  await mkdir2(tournamentWorkRootDir, { recursive: true });
  await mkdir2(assetsDir, { recursive: true });
  await mkdir2(statusDir, { recursive: true });
  await mkdir2(logsDir, { recursive: true });
}
async function isDirectoryReadable(path) {
  try {
    await readdir(path);
    return true;
  } catch {
    return false;
  }
}
async function createCodexJob(body2, options = {}) {
  if (options.admissionLockHeld)
    return createCodexJobUnlocked(body2, options);
  return withCodexRunnerAdmissionLock(() => createCodexJobUnlocked(body2, options));
}
async function createCodexJobUnlocked(body2, options = {}) {
  const workflowMode = normalizeWorkflowMode(body2.workflowMode);
  const tournamentId = resolveTournamentIdForJobRequest(workflowMode, body2);
  if (tournamentId && body2.idempotencyKey) {
    const manifest = await readAnimationTournamentManifest(tournamentId);
    const existing = manifest?.candidates.find((candidate) => candidate.idempotencyKey === body2.idempotencyKey && candidate.jobId);
    if (existing?.jobId)
      return codexJobResponse(existing.jobId, true);
  }
  if (!options.skipSlotCheck)
    await assertCodexRunnerSlotAvailable();
  const createdAt = new Date().toISOString();
  const id = createCodexJobId(createdAt);
  const includeSelectedImage = workflowUsesSelectedImage(workflowMode);
  const includeSpriteContext = workflowUsesSpriteContext(workflowMode);
  const includeAnnotations = workflowMode === "image-edit";
  const jobOutboxDir = tournamentId ? tournamentJobOutboxDir(tournamentId, id) : outboxDir;
  if (tournamentId)
    await mkdir2(jobOutboxDir, { recursive: true });
  const selectedImageAsset = includeSelectedImage ? await writeSelectedImageAsset(id, body2) : null;
  const annotations = includeAnnotations && Array.isArray(body2.annotations) ? body2.annotations : [];
  const repairDirections = normalizeDirectionNames(body2.repairDirections);
  const standardSpriteDirections = normalizeStandardSpriteJobDirections(workflowMode, body2.spriteVariant ?? "standard", body2.directions, repairDirections, tournamentId);
  const job = {
    id,
    createdAt,
    kind: "image-cockpit.codex-handoff",
    workflowMode,
    intent: workflowIntent(workflowMode),
    prompt: body2.prompt,
    negativePrompt: body2.negativePrompt ?? "",
    jobNotes: body2.jobNotes ?? "",
    generationHints: {
      seed: body2.seed ?? "",
      size: body2.size ?? "1024x1024",
      count: body2.count ?? 1,
      quality: body2.quality ?? "auto"
    },
    selectedImage: {
      name: includeSelectedImage ? body2.selectedImageName ?? "" : "",
      size: includeSelectedImage ? body2.selectedImageSize ?? "" : "",
      source: includeSelectedImage ? body2.selectedImageSource ?? "" : "",
      assetPath: selectedImageAsset?.path ?? "",
      fingerprint: selectedImageAsset?.fingerprint ?? body2.sourceFingerprint ?? "",
      mimeType: selectedImageAsset?.mimeType ?? "",
      originalSource: selectedImageAsset?.source ?? ""
    },
    annotationContext: {
      annotations,
      annotationCount: annotations.length,
      coordinateSpace: "Image Cockpit canvas coordinates plus source image normalized and pixel rectangles",
      canvasSize: { width: 920, height: 520 }
    },
    spriteContext: {
      action: includeSpriteContext ? body2.action ?? "" : "",
      frames: includeSpriteContext ? body2.frames ?? 0 : 0,
      framesPerDirection: includeSpriteContext ? normalizeMotionFrameCount(body2.framesPerDirection) : undefined,
      grid: includeSpriteContext ? body2.grid ?? null : null,
      cell: includeSpriteContext ? body2.cell ?? null : null,
      chromaKey: includeSpriteContext ? body2.chromaKey ?? "" : "",
      variant: includeSpriteContext ? body2.spriteVariant ?? "standard" : "",
      directions: includeSpriteContext ? standardSpriteDirections ?? (Array.isArray(body2.directions) ? body2.directions : []) : [],
      motionRecipe: includeSpriteContext ? normalizeMotionRecipeContext(body2) : undefined
    },
    effectContext: workflowMode === "effect-animation" ? body2.effectContext ?? null : null,
    tournament: tournamentId ? {
      id: tournamentId,
      candidateIndex: normalizeCandidateIndex(body2.tournamentCandidateIndex),
      candidateCount: normalizeCandidateCount(body2.tournamentCandidateCount),
      generationProfile: normalizeAnimationGenerationProfile(body2.generationProfile),
      idempotencyKey: body2.idempotencyKey ?? "",
      repairOfJobId: isSafeJobId(body2.repairOfJobId ?? "") ? body2.repairOfJobId : undefined,
      repairDirections,
      hiddenOutbox: true
    } : undefined,
    returnTo: {
      outboxDir: jobOutboxDir,
      expected: ["png", "webp", "gif", "json"]
    },
    notes: [
      "This app does not call OpenAI APIs directly.",
      "Codex or the user should perform generation/editing externally and place results in the outbox or import them through the UI.",
      ...workflowNotes(workflowMode)
    ]
  };
  const path = join3(inboxDir, `${id}.json`);
  await writeFile3(path, JSON.stringify(job, null, 2), "utf8");
  const runner = await startCodexRunner({ id, createdAt, path, outboxDir: jobOutboxDir });
  options.onJobCreated?.(id);
  if (tournamentId) {
    await attachJobToAnimationTournament(tournamentId, normalizeCandidateIndex(body2.tournamentCandidateIndex), id, createdAt, runner, repairDirections);
  }
  return { id, path, inboxPath: inboxDir, outboxPath: jobOutboxDir, createdAt, runner, reused: false };
}
async function codexJobResponse(jobId, reused) {
  const path = join3(inboxDir, `${jobId}.json`);
  const parsed = parseJsonText(await readFile2(path, "utf8"));
  const runner = await getRunnerStatus(jobId);
  return {
    id: jobId,
    path,
    inboxPath: inboxDir,
    outboxPath: resolveSafeOutboxSubdir(parsed.returnTo?.outboxDir) ?? outboxDir,
    createdAt: parsed.createdAt ?? runner.startedAt ?? new Date().toISOString(),
    runner,
    reused
  };
}
function animationTournamentDir(tournamentId) {
  return join3(tournamentWorkRootDir, tournamentId);
}
function animationTournamentManifestPath(tournamentId) {
  return join3(animationTournamentDir(tournamentId), "tournament.json");
}
function animationTournamentTemplatePath(tournamentId) {
  return join3(animationTournamentDir(tournamentId), "template.json");
}
async function registerAndStartAnimationTournament(registration) {
  const tournamentId = typeof registration.tournamentId === "string" ? registration.tournamentId : "";
  const idempotencyKey = typeof registration.idempotencyKey === "string" ? registration.idempotencyKey.trim() : "";
  if (!isSafeTournamentId(tournamentId) || !isSafeIdempotencyKey(idempotencyKey)) {
    throw new Error("Tournament id and idempotency key are required and must be safe.");
  }
  const registrationOutcome = await withAnimationTournamentRegistrationLock(() => withAnimationTournamentLock(tournamentId, async () => {
    const existing = await readAnimationTournamentManifest(tournamentId);
    if (existing) {
      if (existing.idempotencyKey !== idempotencyKey) {
        throw new Error("Tournament id already exists with a different idempotency key.");
      }
      return {
        started: false,
        registered: {
          created: false,
          tournament: await refreshAnimationTournamentManifestUnlocked(tournamentId) ?? existing
        }
      };
    }
    const prepared = await prepareAnimationTournamentRegistration(registration, tournamentId);
    const semanticMatch = await findUnfinishedAnimationTournamentBySemanticKey(prepared.semanticKey, tournamentId);
    if (semanticMatch) {
      return {
        started: false,
        registered: {
          created: false,
          deduplicated: true,
          tournament: await refreshAnimationTournamentManifest(semanticMatch.tournamentId) ?? semanticMatch
        }
      };
    }
    return withCodexRunnerAdmissionLock(async () => {
      await assertAnimationInitialBundleAdmissionAvailable(prepared.initialCandidateCount);
      const registered2 = await createPreparedAnimationTournament(registration, tournamentId, idempotencyKey, prepared);
      const bundle = await startAnimationTournamentInitialCandidateBundleUnlocked(tournamentId);
      return {
        started: true,
        response: {
          created: registered2.created,
          reused: bundle.reused,
          jobs: bundle.jobs,
          tournament: bundle.tournament
        }
      };
    });
  }));
  if (registrationOutcome.started)
    return registrationOutcome.response;
  const registered = registrationOutcome.registered;
  const canonicalTournamentId = registered.tournament.tournamentId;
  return withAnimationTournamentLock(canonicalTournamentId, async () => {
    const manifest = await readAnimationTournamentManifestRequired(canonicalTournamentId);
    const initialCandidates = manifest.candidates.slice(0, manifest.initialCandidateCount);
    const startedCandidates = initialCandidates.filter((candidate) => Boolean(candidate.jobId));
    if (startedCandidates.length === initialCandidates.length) {
      const refreshed = await refreshAnimationTournamentManifestUnlocked(canonicalTournamentId) ?? manifest;
      assertAnimationInitialBundleReusable(refreshed);
      return {
        created: registered.created,
        reused: true,
        jobs: await Promise.all(startedCandidates.map((candidate) => codexJobResponse(candidate.jobId, true))),
        tournament: refreshed
      };
    }
    if (startedCandidates.length > 0) {
      throw initialAnimationBundlePartialError(manifest.initialCandidateCount, startedCandidates.length);
    }
    if (manifest.state !== "queued") {
      throw new HttpError(409, "Only a queued animation tournament can start its initial candidate bundle.", "animation_tournament_not_queued", {
        retryable: false
      });
    }
    return withCodexRunnerAdmissionLock(async () => {
      const profile = normalizeAnimationGenerationProfile(manifest.generationProfile);
      const profilePlan = animationGenerationProfilePlan(profile);
      assertAnimationGenerationProfileRequestCounts(registration, profile, profilePlan);
      assertAnimationGenerationProfileManifestCounts(manifest);
      await assertAnimationInitialBundleAdmissionAvailable(manifest.initialCandidateCount);
      const bundle = await startAnimationTournamentInitialCandidateBundleUnlocked(canonicalTournamentId);
      return {
        created: registered.created,
        reused: bundle.reused,
        jobs: bundle.jobs,
        tournament: bundle.tournament
      };
    });
  });
}
async function startAnimationTournamentInitialCandidateBundle(tournamentId) {
  return withAnimationTournamentLock(tournamentId, () => withCodexRunnerAdmissionLock(() => startAnimationTournamentInitialCandidateBundleUnlocked(tournamentId)));
}
async function startAnimationTournamentInitialCandidateBundleUnlocked(tournamentId) {
  const manifest = await readAnimationTournamentManifestRequired(tournamentId);
  assertAnimationGenerationProfileManifestCounts(manifest);
  const initialCandidates = manifest.candidates.slice(0, manifest.initialCandidateCount);
  const startedCandidates = initialCandidates.filter((candidate) => Boolean(candidate.jobId));
  if (startedCandidates.length === initialCandidates.length) {
    const refreshed = await refreshAnimationTournamentManifestUnlocked(tournamentId) ?? manifest;
    assertAnimationInitialBundleReusable(refreshed);
    return {
      reused: true,
      jobs: await Promise.all(startedCandidates.map((candidate) => codexJobResponse(candidate.jobId, true))),
      tournament: refreshed
    };
  }
  if (startedCandidates.length > 0)
    throw initialAnimationBundlePartialError(initialCandidates.length, startedCandidates.length);
  if (manifest.state !== "queued") {
    throw new HttpError(409, "Only a queued animation tournament can start its initial candidate bundle.", "animation_tournament_not_queued", {
      retryable: false
    });
  }
  await assertAnimationInitialBundleAdmissionAvailable(initialCandidates.length);
  const jobs = [];
  const createdJobIds = new Set;
  try {
    for (const candidate of initialCandidates) {
      const result = await startAnimationTournamentCandidateUnlocked(tournamentId, candidate.index, undefined, {
        allowInitialCandidate: true,
        admissionLockHeld: true,
        skipSlotCheck: true,
        onJobCreated: (jobId) => createdJobIds.add(jobId)
      });
      if (!["running", "completed", "disabled"].includes(result.job.runner?.state ?? "unknown")) {
        throw new HttpError(409, `Initial animation candidate ${candidate.index + 1} did not enter the running state.`, "initial_candidate_runner_not_started", {
          jobId: result.job.id,
          runnerState: result.job.runner?.state ?? "unknown",
          retryable: true
        });
      }
      jobs.push(result.job);
    }
  } catch (error) {
    await rollbackAnimationInitialCandidateBundle(tournamentId, [...createdJobIds]);
    throw error;
  }
  return {
    reused: false,
    jobs,
    tournament: await refreshAnimationTournamentManifestUnlocked(tournamentId)
  };
}
function initialAnimationBundlePartialError(requiredSlots, startedSlots) {
  return new HttpError(409, "The initial animation candidate wave is partial and cannot be resumed one candidate at a time. Cancel it and start a new tournament when all runner slots are free.", "partial_initial_candidate_bundle", { requiredSlots, startedSlots, retryable: false });
}
function assertAnimationInitialBundleReusable(manifest) {
  if (manifest.state === "accepted")
    return;
  const initialCandidates = manifest.candidates.slice(0, manifest.initialCandidateCount);
  const invalidCandidate = initialCandidates.find((candidate) => candidate.state === "failed" || candidate.state === "cancelled" || candidate.reason?.startsWith("Initial candidate bundle failed") || candidate.reason?.startsWith("Initial candidate bundle rolled back"));
  if (manifest.state === "failed" || manifest.state === "cancelled" || invalidCandidate) {
    throw new HttpError(409, "The existing initial animation candidate wave is failed or cancelled and cannot be reused.", "initial_candidate_bundle_not_reusable", { retryable: false });
  }
}
async function rollbackAnimationInitialCandidateBundle(tournamentId, jobIds) {
  const cancellationResults = await Promise.all(jobIds.map(async (jobId) => {
    const status = await getRunnerStatus(jobId).catch(() => null);
    if (!status || status.state !== "running" && !status.cancellationPending) {
      return { ok: true, jobId, status };
    }
    return cancelCodexRunner(jobId).catch((error) => ({
      ok: false,
      jobId,
      status,
      message: error instanceof Error ? error.message : "Runner cancellation failed."
    }));
  }));
  const manifest = await readAnimationTournamentManifestRequired(tournamentId);
  const rolledBackJobIds = new Set(jobIds);
  const unconfirmedJobIds = new Set(cancellationResults.filter((result) => !result.ok && (result.status?.state === "running" || result.status?.cancellationPending)).map((result) => result.jobId));
  manifest.candidates.slice(0, manifest.initialCandidateCount).forEach((candidate) => {
    if (!candidate.jobId || !rolledBackJobIds.has(candidate.jobId))
      return;
    candidate.state = unconfirmedJobIds.has(candidate.jobId) ? "running" : "cancelled";
    candidate.reason = unconfirmedJobIds.has(candidate.jobId) ? "Initial candidate bundle failed and runner cancellation remains unconfirmed." : "Initial candidate bundle rolled back after one candidate failed to start.";
    candidate.updatedAt = new Date().toISOString();
  });
  manifest.state = "failed";
  await writeAnimationTournamentManifest(manifest);
  if (unconfirmedJobIds.size > 0) {
    throw new HttpError(409, `Initial candidate rollback could not confirm cancellation for: ${[...unconfirmedJobIds].join(", ")}.`, "initial_bundle_rollback_unconfirmed", { jobIds: [...unconfirmedJobIds], retryable: true });
  }
}
async function registerAnimationTournament(registration) {
  const tournamentId = typeof registration.tournamentId === "string" ? registration.tournamentId : "";
  const idempotencyKey = typeof registration.idempotencyKey === "string" ? registration.idempotencyKey.trim() : "";
  if (!isSafeTournamentId(tournamentId) || !isSafeIdempotencyKey(idempotencyKey)) {
    throw new Error("Tournament id and idempotency key are required and must be safe.");
  }
  return withAnimationTournamentRegistrationLock(() => withAnimationTournamentLock(tournamentId, () => registerAnimationTournamentUnlocked(registration, tournamentId, idempotencyKey)));
}
async function registerAnimationTournamentUnlocked(registration, tournamentId, idempotencyKey) {
  const existing = await readAnimationTournamentManifest(tournamentId);
  if (existing) {
    if (existing.idempotencyKey !== idempotencyKey)
      throw new Error("Tournament id already exists with a different idempotency key.");
    return { created: false, tournament: await refreshAnimationTournamentManifestUnlocked(tournamentId) ?? existing };
  }
  const prepared = await prepareAnimationTournamentRegistration(registration, tournamentId);
  const semanticMatch = await findUnfinishedAnimationTournamentBySemanticKey(prepared.semanticKey, tournamentId);
  if (semanticMatch) {
    return {
      created: false,
      deduplicated: true,
      tournament: await refreshAnimationTournamentManifest(semanticMatch.tournamentId) ?? semanticMatch
    };
  }
  return createPreparedAnimationTournament(registration, tournamentId, idempotencyKey, prepared);
}
async function prepareAnimationTournamentRegistration(registration, tournamentId) {
  const profile = normalizeAnimationGenerationProfile(registration.generationProfile);
  const profilePlan = animationGenerationProfilePlan(profile);
  assertAnimationGenerationProfileRequestCounts(registration, profile, profilePlan);
  const maximumCandidateCount = profilePlan.maximumCandidates;
  const initialCandidateCount = profilePlan.initialCandidates;
  const requestedDirections = normalizeDirectionNames(registration.requestedDirections);
  if (![1, 3, 5].includes(requestedDirections.length))
    throw new Error("Animation tournaments require 1, 3, or 5 directions.");
  const pilotMode = registration.pilotMode === true;
  const selectionPolicy = profile === "best" && !pilotMode && registration.selectionPolicy !== "exhaustive" ? "smart-race" : "exhaustive";
  if (pilotMode && requestedDirections.length === 1) {
    throw new Error("Motion Pilot requires more than one requested direction.");
  }
  const requestedPilotDirection = normalizeDirectionNames([registration.pilotDirection])[0];
  const pilotDirection = pilotMode ? requestedDirections.includes(requestedPilotDirection) ? requestedPilotDirection : requestedDirections.includes("side") ? "side" : requestedDirections[0] : undefined;
  if (!registration.jobTemplate || typeof registration.jobTemplate !== "object")
    throw new Error("Tournament job template is required.");
  const jobTemplate = JSON.parse(JSON.stringify(registration.jobTemplate));
  if (!jobTemplate.prompt?.trim())
    throw new Error("Tournament job template prompt is required.");
  const selectedImageAsset = await writeSelectedImageAsset(tournamentId, jobTemplate);
  const sourceFingerprint = typeof registration.sourceFingerprint === "string" && registration.sourceFingerprint.trim() ? registration.sourceFingerprint.trim() : selectedImageAsset?.fingerprint ?? "";
  if (!sourceFingerprint)
    throw new Error("Tournament source fingerprint is required.");
  const motionRecipeId = normalizeShortText(registration.motionRecipeId);
  const motionRecipeVersion = normalizePositiveInteger(registration.motionRecipeVersion);
  const motionRecipeCompilerVersion = normalizeShortText(registration.motionRecipeCompilerVersion);
  const presetId = normalizeShortText(registration.presetId);
  jobTemplate.selectedImageDataUrl = "";
  jobTemplate.selectedImageAssetPath = selectedImageAsset?.path;
  jobTemplate.sourceFingerprint = sourceFingerprint;
  jobTemplate.generationProfile = profile;
  jobTemplate.directions = requestedDirections;
  const semanticKey = animationTournamentSemanticKey({
    sourceFingerprint,
    motionRecipeId,
    motionRecipeVersion,
    motionRecipeCompilerVersion,
    presetId,
    generationProfile: profile,
    requestedDirections,
    maximumCandidateCount,
    initialCandidateCount,
    pilotMode,
    pilotDirection,
    jobTemplate
  });
  return {
    profile,
    selectionPolicy,
    requestedDirections,
    maximumCandidateCount,
    initialCandidateCount,
    pilotMode,
    pilotDirection,
    jobTemplate,
    selectedImageAsset,
    sourceFingerprint,
    motionRecipeId,
    motionRecipeVersion,
    motionRecipeCompilerVersion,
    presetId,
    semanticKey
  };
}
async function createPreparedAnimationTournament(registration, tournamentId, idempotencyKey, prepared) {
  const {
    profile,
    selectionPolicy,
    requestedDirections,
    maximumCandidateCount,
    initialCandidateCount,
    pilotMode,
    pilotDirection,
    jobTemplate,
    selectedImageAsset,
    sourceFingerprint,
    motionRecipeId,
    motionRecipeVersion,
    motionRecipeCompilerVersion,
    presetId,
    semanticKey
  } = prepared;
  const now = new Date().toISOString();
  const manifest = {
    schema: "image-cockpit.animation-tournament.v1",
    schemaVersion: 1,
    tournamentId,
    idempotencyKey,
    semanticKey,
    sourceFingerprint,
    sourceAssetRef: selectedImageAsset?.path ? basename2(selectedImageAsset.path) : undefined,
    motionRecipeId,
    motionRecipeVersion,
    motionRecipeCompilerVersion,
    presetId,
    generationProfile: profile,
    selectionPolicy,
    requestedDirections,
    maximumCandidateCount,
    initialCandidateCount,
    candidates: Array.from({ length: maximumCandidateCount }, (_, index) => ({
      index,
      state: "queued",
      idempotencyKey: `${idempotencyKey}:candidate:${index}`,
      updatedAt: now
    })),
    directionStates: Object.fromEntries(requestedDirections.map((direction) => [direction, { state: "queued", updatedAt: now }])),
    qualityReportRefs: [],
    acceptedDirectionHashes: {},
    retryCount: 0,
    pilotMode,
    pilotState: pilotMode ? "piloting" : undefined,
    pilotDirection,
    pilotCandidateIds: pilotMode ? [] : undefined,
    expansionDirectionIds: pilotMode ? requestedDirections.filter((direction) => direction !== pilotDirection) : undefined,
    directionOutputCount: 0,
    totalCandidateJobs: 0,
    repairCount: 0,
    state: "queued",
    templateRef: "template.json",
    clientContext: registration.clientContext && typeof registration.clientContext === "object" ? JSON.parse(JSON.stringify(registration.clientContext)) : undefined,
    createdAt: now,
    updatedAt: now
  };
  await mkdir2(animationTournamentDir(tournamentId), { recursive: true });
  await writeJsonAtomic(animationTournamentTemplatePath(tournamentId), jobTemplate);
  await writeAnimationTournamentManifest(manifest);
  return { created: true, tournament: manifest };
}
function animationTournamentSemanticKey(input) {
  const semanticTemplate = JSON.parse(JSON.stringify(input.jobTemplate));
  [
    "idempotencyKey",
    "selectedImageAssetPath",
    "selectedImageDataUrl",
    "selectedImageName",
    "selectedImageSize",
    "selectedImageSource",
    "sourceFingerprint",
    "tournamentCandidateCount",
    "tournamentCandidateIndex",
    "tournamentId"
  ].forEach((key) => delete semanticTemplate[key]);
  return createHash2("sha256").update(stableJsonStringify({
    sourceFingerprint: input.sourceFingerprint,
    motionRecipeId: input.motionRecipeId,
    motionRecipeVersion: input.motionRecipeVersion,
    motionRecipeCompilerVersion: input.motionRecipeCompilerVersion,
    presetId: input.presetId,
    generationProfile: input.generationProfile,
    requestedDirections: input.requestedDirections,
    maximumCandidateCount: input.maximumCandidateCount,
    initialCandidateCount: input.initialCandidateCount,
    pilotMode: input.pilotMode,
    pilotDirection: input.pilotDirection,
    jobTemplate: semanticTemplate
  })).digest("hex");
}
async function animationTournamentSemanticKeyFromManifest(manifest) {
  if (manifest.semanticKey)
    return manifest.semanticKey;
  const jobTemplate = parseJsonText(await readFile2(animationTournamentTemplatePath(manifest.tournamentId), "utf8"));
  return animationTournamentSemanticKey({
    sourceFingerprint: manifest.sourceFingerprint,
    motionRecipeId: manifest.motionRecipeId,
    motionRecipeVersion: manifest.motionRecipeVersion,
    motionRecipeCompilerVersion: manifest.motionRecipeCompilerVersion,
    presetId: manifest.presetId,
    generationProfile: manifest.generationProfile,
    requestedDirections: manifest.requestedDirections,
    maximumCandidateCount: manifest.maximumCandidateCount,
    initialCandidateCount: manifest.initialCandidateCount,
    pilotMode: manifest.pilotMode === true,
    pilotDirection: manifest.pilotDirection,
    jobTemplate
  });
}
async function findUnfinishedAnimationTournamentBySemanticKey(semanticKey, excludedTournamentId) {
  const entries = await readdir(tournamentWorkRootDir, { withFileTypes: true });
  for (const entry of entries) {
    if (!entry.isDirectory() || entry.name === excludedTournamentId || !isSafeTournamentId(entry.name))
      continue;
    const manifest = await readAnimationTournamentManifest(entry.name);
    if (!manifest || !animationTournamentStateIsUnfinished(manifest.state))
      continue;
    const existingSemanticKey = await animationTournamentSemanticKeyFromManifest(manifest).catch(() => "");
    if (existingSemanticKey !== semanticKey)
      continue;
    const refreshed = await refreshAnimationTournamentManifest(manifest.tournamentId).catch(() => manifest);
    if (refreshed && animationTournamentStateIsUnfinished(refreshed.state))
      return refreshed;
  }
  return null;
}
function animationTournamentStateIsUnfinished(state) {
  return state === "queued" || state === "running" || state === "pilot-review" || state === "pilot-expanding";
}
function stableJsonStringify(value) {
  return JSON.stringify(stableJsonValue(value));
}
function stableJsonValue(value) {
  if (Array.isArray(value))
    return value.map(stableJsonValue);
  if (!value || typeof value !== "object")
    return value;
  return Object.keys(value).sort().reduce((result, key) => {
    const normalized = stableJsonValue(value[key]);
    if (normalized !== undefined)
      result[key] = normalized;
    return result;
  }, {});
}
async function startAnimationTournamentCandidate(tournamentId, candidateIndex, reason) {
  return withAnimationTournamentLock(tournamentId, () => withCodexRunnerAdmissionLock(() => startAnimationTournamentCandidateUnlocked(tournamentId, candidateIndex, reason, { allowInitialCandidate: false, admissionLockHeld: true, skipSlotCheck: false })));
}
async function startAnimationTournamentCandidateUnlocked(tournamentId, candidateIndex, reason, options = {}) {
  const manifest = await readAnimationTournamentManifestRequired(tournamentId);
  assertAnimationGenerationProfileManifestCounts(manifest);
  const candidate = manifest.candidates[candidateIndex];
  if (!candidate)
    throw new Error("Tournament candidate index is outside the registered plan.");
  if (candidate.jobId)
    return { reused: true, job: await codexJobResponse(candidate.jobId, true), tournament: await refreshAnimationTournamentManifestUnlocked(tournamentId) };
  if (candidateIndex < manifest.initialCandidateCount && !options.allowInitialCandidate) {
    throw new HttpError(409, "Initial animation candidates must start together through the initial-candidates bundle endpoint.", "initial_batch_admission_required", { requiredSlots: manifest.initialCandidateCount, retryable: true });
  }
  const normalizedReason = normalizeShortText(reason);
  if (candidateIndex >= manifest.initialCandidateCount) {
    if (manifest.pilotMode || manifest.generationProfile !== "balanced" || candidateIndex !== manifest.initialCandidateCount || manifest.maximumCandidateCount !== manifest.initialCandidateCount + 1) {
      throw new HttpError(409, "This animation generation profile does not have an adaptive candidate slot.", "adaptive_candidate_not_available", { retryable: false });
    }
    const initialCandidates = manifest.candidates.slice(0, manifest.initialCandidateCount);
    const startedCandidates = initialCandidates.filter((initialCandidate) => Boolean(initialCandidate.jobId));
    const terminalCandidates = initialCandidates.filter((initialCandidate) => initialCandidate.state === "quality-evaluated" || initialCandidate.state === "failed");
    if (manifest.state === "accepted" || manifest.state === "cancelled" || startedCandidates.length !== initialCandidates.length || terminalCandidates.length !== initialCandidates.length) {
      throw new HttpError(409, "The adaptive animation candidate can start only after the complete initial wave has finished Quality Gate evaluation.", "adaptive_candidate_initial_wave_required", {
        requiredCandidates: initialCandidates.length,
        startedCandidates: startedCandidates.length,
        terminalCandidates: terminalCandidates.length,
        retryable: true
      });
    }
    if (!normalizedReason) {
      throw new HttpError(409, "The adaptive animation candidate requires a persisted reason.", "adaptive_candidate_reason_required", { retryable: false });
    }
    await assertAnimationInitialBundleAdmissionAvailable(1);
  }
  const template = parseJsonText(await readFile2(animationTournamentTemplatePath(tournamentId), "utf8"));
  const candidateLabel = `candidate ${candidateIndex + 1}/${manifest.maximumCandidateCount}`;
  const pilotDirections = manifest.pilotMode && manifest.pilotDirection ? [manifest.pilotDirection] : undefined;
  const candidateDirections = pilotDirections ?? manifest.requestedDirections;
  const candidateFrameCount = normalizeMotionFrameCount(template.framesPerDirection) ?? 8;
  const body2 = {
    ...template,
    directions: candidateDirections,
    grid: { columns: candidateFrameCount, rows: candidateDirections.length, gutter: 0 },
    frames: candidateFrameCount * candidateDirections.length,
    framesPerDirection: candidateFrameCount,
    jobNotes: [
      template.jobNotes ?? "",
      `Animation tournament ${tournamentId}, ${candidateLabel}, profile ${manifest.generationProfile}.`,
      "Generate this candidate independently from the same source and motion contract. Do not copy another candidate.",
      manifest.pilotMode ? `Motion Pilot experimental candidate. Generate only representative direction ${manifest.pilotDirection}. Preserve the full Recipe phase contract so the accepted pilot can expand to ${manifest.expansionDirectionIds?.join(", ")}.` : "",
      normalizedReason ? `Adaptive candidate reason: ${normalizedReason}.` : ""
    ].filter(Boolean).join(`
`),
    tournamentId,
    tournamentCandidateIndex: candidateIndex,
    tournamentCandidateCount: manifest.maximumCandidateCount,
    generationProfile: manifest.generationProfile,
    idempotencyKey: candidate.idempotencyKey,
    sourceFingerprint: manifest.sourceFingerprint
  };
  if (candidateIndex >= manifest.initialCandidateCount && normalizedReason)
    manifest.thirdCandidateReason = normalizedReason;
  candidate.reason = normalizedReason;
  candidate.updatedAt = new Date().toISOString();
  await writeAnimationTournamentManifest(manifest);
  const job = await createCodexJob(body2, {
    admissionLockHeld: options.admissionLockHeld,
    skipSlotCheck: options.skipSlotCheck,
    onJobCreated: options.onJobCreated
  });
  return { reused: false, job, tournament: await refreshAnimationTournamentManifestUnlocked(tournamentId) };
}
async function attachJobToAnimationTournament(tournamentId, candidateIndex, jobId, createdAt, runner, repairDirections = []) {
  const manifest = await readAnimationTournamentManifest(tournamentId);
  const candidate = manifest?.candidates[candidateIndex];
  if (!manifest || !candidate)
    return;
  candidate.jobId = candidate.jobId ?? jobId;
  candidate.createdAt = candidate.createdAt ?? createdAt;
  candidate.state = runner.state === "running" ? "running" : runner.state === "completed" ? "artifact-ready" : "failed";
  candidate.updatedAt = new Date().toISOString();
  if (manifest.pilotMode && !candidate.repairDirections?.length && !manifest.pilotCandidateIds?.includes(jobId)) {
    manifest.pilotCandidateIds = [...manifest.pilotCandidateIds ?? [], jobId];
  }
  manifest.totalCandidateJobs = manifest.candidates.filter((item) => Boolean(item.jobId)).length;
  manifest.state = candidate.state === "running" ? manifest.pilotMode && candidate.repairDirections?.length ? "pilot-expanding" : "running" : manifest.state;
  if (candidate.state === "running") {
    const activeDirections = repairDirections.length > 0 ? repairDirections : manifest.pilotMode && manifest.pilotDirection ? [manifest.pilotDirection] : manifest.requestedDirections;
    activeDirections.forEach((direction) => {
      manifest.directionStates[direction] = { state: "running", updatedAt: candidate.updatedAt, jobId };
    });
  }
  await writeAnimationTournamentManifest(manifest);
}
async function recordAnimationTournamentEvaluation(tournamentId, body2) {
  return withAnimationTournamentLock(tournamentId, () => recordAnimationTournamentEvaluationUnlocked(tournamentId, body2));
}
function normalizeAnimationHumanReviewForManifest(value, manifest) {
  if (!value || typeof value !== "object")
    throw new Error("Animation human review payload is required.");
  const source = value;
  const candidateIds = new Set(manifest.candidates.flatMap((candidate) => candidate.jobId ? [candidate.jobId] : []));
  const manualWinnerJobId = typeof source.manualWinnerJobId === "string" && candidateIds.has(source.manualWinnerJobId) ? source.manualWinnerJobId : undefined;
  const decisions = Array.isArray(source.decisions) ? source.decisions.flatMap((entry) => {
    if (!entry || typeof entry !== "object")
      return [];
    const item = entry;
    if (typeof item.jobId !== "string" || !candidateIds.has(item.jobId))
      return [];
    const decision = item.decision === "winner" || item.decision === "reject" ? item.decision : "hold";
    const reasonTags = Array.isArray(item.reasonTags) ? [...new Set(item.reasonTags.filter((tag) => typeof tag === "string" && tag.trim().length > 0).map((tag) => tag.trim().slice(0, 48)))].slice(0, 12) : [];
    return [{
      jobId: item.jobId,
      decision,
      reasonTags,
      note: typeof item.note === "string" ? item.note.trim().slice(0, 1000) : ""
    }];
  }) : [];
  return {
    updatedAt: new Date().toISOString(),
    ...manualWinnerJobId ? { manualWinnerJobId } : {},
    decisions
  };
}
async function recordAnimationHumanReview(tournamentId, value) {
  return withAnimationTournamentLock(tournamentId, async () => {
    const manifest = await readAnimationTournamentManifestRequired(tournamentId);
    manifest.humanReview = normalizeAnimationHumanReviewForManifest(value, manifest);
    await writeAnimationTournamentManifest(manifest);
    return manifest;
  });
}
async function recordAnimationTournamentEvaluationUnlocked(tournamentId, body2) {
  const manifest = await readAnimationTournamentManifestRequired(tournamentId);
  const jobId = typeof body2.jobId === "string" ? body2.jobId : "";
  const candidate = manifest.candidates.find((item) => item.jobId === jobId);
  if (!candidate)
    throw new Error("Tournament evaluation job was not registered.");
  if (manifest.state === "accepted" && manifest.winnerCandidateId && !manifest.pilotMode && !candidate.repairDirections?.length) {
    const terminalState = candidate.jobId === manifest.winnerCandidateId ? "accepted" : "cancelled";
    if (candidate.state !== terminalState) {
      candidate.state = terminalState;
      candidate.updatedAt = new Date().toISOString();
      await writeAnimationTournamentManifest(manifest);
    }
    return manifest;
  }
  if (body2.ready !== true && candidate.state === "quality-evaluated")
    return manifest;
  candidate.state = body2.ready === true ? "quality-evaluated" : "failed";
  candidate.score = typeof body2.score === "number" && Number.isFinite(body2.score) ? body2.score : undefined;
  candidate.warningCount = body2.ready === true && typeof body2.warningCount === "number" && Number.isFinite(body2.warningCount) ? Math.min(999, Math.max(0, Math.floor(body2.warningCount))) : 0;
  if (body2.ready === true && typeof body2.identityScore === "number" && Number.isFinite(body2.identityScore)) {
    candidate.identityScore = Math.min(100, Math.max(0, body2.identityScore));
  }
  if (body2.ready === true && typeof body2.shadowWouldBlock === "boolean") {
    candidate.shadowWouldBlock = body2.shadowWouldBlock;
  }
  candidate.reason = typeof body2.reason === "string" ? body2.reason.slice(0, 400) : candidate.reason;
  candidate.qualityReportRef = typeof body2.qualityReportRef === "string" ? body2.qualityReportRef.slice(0, 200) : candidate.qualityReportRef;
  if (candidate.qualityReportRef && !manifest.qualityReportRefs.includes(candidate.qualityReportRef))
    manifest.qualityReportRefs.push(candidate.qualityReportRef);
  candidate.updatedAt = new Date().toISOString();
  if (body2.ready === true && manifest.state === "failed" && !manifest.winnerCandidateId) {
    manifest.state = "running";
  }
  const evaluationDirections = candidate.repairDirections?.length ? candidate.repairDirections : manifest.pilotMode && manifest.pilotDirection ? [manifest.pilotDirection] : manifest.requestedDirections;
  if (body2.ready === true) {
    evaluationDirections.forEach((direction) => {
      manifest.directionStates[direction] = {
        state: "quality-evaluated",
        updatedAt: candidate.updatedAt,
        jobId,
        reason: candidate.warningCount ? `${candidate.warningCount} candidate warning(s)` : "candidate passed Quality Gate v2"
      };
    });
  }
  if (candidate.repairDirections?.length && manifest.winnerCandidateId) {
    manifest.state = manifest.pilotMode && manifest.pilotState === "expanding" ? "pilot-expanding" : "accepted";
    if (manifest.pilotMode && body2.ready === true) {
      manifest.directionOutputCount = (manifest.pilotCandidateIds?.length ?? manifest.initialCandidateCount) + candidate.repairDirections.length;
      manifest.totalCandidateJobs = manifest.candidates.filter((item) => Boolean(item.jobId)).length;
    }
    manifest.requestedDirections.filter((direction) => body2.ready !== true || !candidate.repairDirections?.includes(direction)).forEach((direction) => {
      const acceptedPilotDirection = manifest.pilotMode && direction === manifest.pilotDirection;
      manifest.directionStates[direction] = {
        state: acceptedPilotDirection || !manifest.pilotMode ? "accepted" : manifest.directionStates[direction]?.state ?? "queued",
        updatedAt: candidate.updatedAt,
        jobId: manifest.winnerCandidateId,
        reason: acceptedPilotDirection ? "accepted pilot direction preserved during expansion" : body2.ready === true ? "untargeted direction preserved from accepted winner" : "restored accepted winner after failed Direction Repair"
      };
    });
  }
  if (manifest.pilotMode && !candidate.repairDirections?.length && !manifest.pilotWinnerId) {
    const pilotCandidates = manifest.candidates.slice(0, manifest.initialCandidateCount);
    const allPilotCandidatesTerminal = pilotCandidates.every((item) => item.state === "quality-evaluated" || item.state === "failed");
    if (allPilotCandidatesTerminal) {
      const usablePilotCandidates = pilotCandidates.filter((item) => item.state === "quality-evaluated");
      manifest.directionOutputCount = usablePilotCandidates.length;
      manifest.totalCandidateJobs = pilotCandidates.filter((item) => Boolean(item.jobId)).length;
      manifest.pilotState = usablePilotCandidates.length > 0 ? "review" : "failed";
      manifest.state = usablePilotCandidates.length > 0 ? "pilot-review" : "failed";
      if (usablePilotCandidates.length === 0)
        manifest.fallbackReason = "all pilot candidates failed Quality Gate v2";
    }
  }
  await writeAnimationTournamentManifest(manifest);
  return manifest;
}
function rankAnimationTournamentCandidates(candidates) {
  return candidates.slice().sort((left, right) => {
    const scoreOrder = (right.score ?? Number.NEGATIVE_INFINITY) - (left.score ?? Number.NEGATIVE_INFINITY);
    if (scoreOrder !== 0)
      return scoreOrder;
    const warningOrder = (left.warningCount ?? Number.MAX_SAFE_INTEGER) - (right.warningCount ?? Number.MAX_SAFE_INTEGER);
    if (warningOrder !== 0)
      return warningOrder;
    const identityOrder = (right.identityScore ?? Number.NEGATIVE_INFINITY) - (left.identityScore ?? Number.NEGATIVE_INFINITY);
    if (identityOrder !== 0)
      return identityOrder;
    return left.index - right.index;
  });
}
async function normalizeAnimationTournamentWinnerDecision(value, manifest, winnerJobId) {
  if (!value)
    return;
  if (manifest.generationProfile !== "best" || manifest.selectionPolicy !== "smart-race" || manifest.pilotMode) {
    throw new HttpError(409, "Smart Race decisions are only valid for new non-Pilot Best tournaments.");
  }
  const mode = value.mode === "first-qualified" || value.mode === "early-accept" || value.mode === "full-compare" ? value.mode : undefined;
  if (!mode)
    throw new HttpError(400, "Smart Race winner decision mode is invalid.");
  const registeredJobIds = new Set(manifest.candidates.flatMap((candidate) => candidate.jobId ? [candidate.jobId] : []));
  const comparedJobIds = Array.isArray(value.comparedJobIds) ? [...new Set(value.comparedJobIds.filter((jobId) => typeof jobId === "string" && registeredJobIds.has(jobId)))].slice(0, 3) : [];
  if (!comparedJobIds.includes(winnerJobId)) {
    throw new HttpError(409, "Smart Race winner must be included in the compared candidates.");
  }
  const comparedCandidates = comparedJobIds.map((jobId) => manifest.candidates.find((candidate) => candidate.jobId === jobId)).filter((candidate) => Boolean(candidate));
  const rankedUsable = rankAnimationTournamentCandidates(comparedCandidates.filter((candidate) => candidate.state === "quality-evaluated" && typeof candidate.score === "number"));
  const winner = rankedUsable[0];
  if (!winner || winner.jobId !== winnerJobId) {
    throw new HttpError(409, "Smart Race winner does not match the persisted candidate ranking.");
  }
  const runnerUp = rankedUsable[1];
  const scoreGap = runnerUp && typeof winner.score === "number" && typeof runnerUp.score === "number" ? winner.score - runnerUp.score : undefined;
  if (mode === "first-qualified") {
    const strictDecision = evaluateBestFirstQualifiedCandidate({
      ready: winner.state === "quality-evaluated",
      score: winner.score ?? Number.NEGATIVE_INFINITY,
      warningCount: winner.warningCount ?? Number.MAX_SAFE_INTEGER,
      identityScore: winner.identityScore,
      shadowWouldBlock: winner.shadowWouldBlock
    });
    if (comparedCandidates.length !== 1 || rankedUsable.length !== 1 || !strictDecision.qualified) {
      throw new HttpError(409, "Persisted candidate metrics do not satisfy the strict First Qualified gate.");
    }
    const remainingCandidates = manifest.candidates.filter((candidate) => candidate.jobId && !candidate.repairDirections?.length && !comparedJobIds.includes(candidate.jobId));
    if (remainingCandidates.length !== manifest.maximumCandidateCount - 1) {
      throw new HttpError(409, "First Qualified acceptance requires every other Best candidate to remain active.");
    }
    const remainingStatuses = await Promise.all(remainingCandidates.map((candidate) => readRunnerStatusSnapshot(candidate.jobId)));
    if (remainingStatuses.some((status) => status.state !== "running")) {
      throw new HttpError(409, "Another Best candidate is already terminal; continue with Smart Race comparison.");
    }
  } else if (mode === "early-accept") {
    if (comparedCandidates.length !== 2 || rankedUsable.length !== 2 || typeof winner.score !== "number" || winner.score < 3000 || typeof scoreGap !== "number" || scoreGap < 50 || typeof winner.identityScore !== "number" || winner.identityScore < 82 || winner.shadowWouldBlock !== false) {
      throw new HttpError(409, "Persisted candidate metrics do not satisfy the Smart Race early-accept thresholds.");
    }
    const remainingCandidates = manifest.candidates.filter((candidate) => candidate.jobId && !candidate.repairDirections?.length && !comparedJobIds.includes(candidate.jobId));
    if (remainingCandidates.length !== 1 || !remainingCandidates[0].jobId) {
      throw new HttpError(409, "Smart Race early acceptance requires exactly one remaining candidate.");
    }
    const remainingStatus = await readRunnerStatusSnapshot(remainingCandidates[0].jobId);
    if (remainingStatus.state !== "running") {
      throw new HttpError(409, "The remaining Smart Race candidate is already terminal; compare all candidates instead.");
    }
  } else {
    const startedCandidates = manifest.candidates.filter((candidate) => candidate.jobId && !candidate.repairDirections?.length);
    const allTerminal = startedCandidates.length === manifest.maximumCandidateCount && startedCandidates.every((candidate) => candidate.state === "quality-evaluated" || candidate.state === "failed");
    if (!allTerminal || startedCandidates.some((candidate) => !candidate.jobId || !comparedJobIds.includes(candidate.jobId))) {
      throw new HttpError(409, "Full comparison requires every started Best candidate to be terminal and recorded.");
    }
  }
  return {
    mode,
    decidedAt: new Date().toISOString(),
    comparedJobIds,
    winnerJobId,
    scoreGap,
    reason: typeof value.reason === "string" && value.reason.trim() ? value.reason.trim().slice(0, 400) : mode === "early-accept" ? "two ready Best candidates have a clear Smart Race winner" : mode === "first-qualified" ? "first completed Best candidate passed the strict solo gate" : "all Best candidates were compared"
  };
}
async function acceptAnimationTournamentWinner(tournamentId, jobId, decision) {
  const manifest = await readAnimationTournamentManifestRequired(tournamentId);
  const winner = manifest.candidates.find((candidate) => candidate.jobId === jobId);
  if (!winner)
    throw new Error("Tournament winner job is not registered in the tournament manifest.");
  const jobOutboxDir = tournamentJobOutboxDir(tournamentId, jobId);
  const hashes = {};
  for (const direction of manifest.requestedDirections) {
    const slug = directionSlug(direction);
    const candidate = await findDirectionSplitCandidateFile(jobId, slug, jobOutboxDir);
    if (!candidate)
      throw new Error(`Tournament winner is missing ${direction}.`);
    hashes[direction] = await hashFile(candidate.path);
    manifest.directionStates[direction] = { state: "accepted", updatedAt: new Date().toISOString(), jobId };
  }
  manifest.acceptedDirectionHashes = hashes;
  manifest.winnerCandidateId = jobId;
  manifest.state = "accepted";
  if (decision && !manifest.smartRaceDecision)
    manifest.smartRaceDecision = decision;
  manifest.candidates.forEach((candidate) => {
    candidate.state = candidate.jobId === jobId ? "accepted" : candidate.jobId ? "cancelled" : candidate.state;
    candidate.updatedAt = new Date().toISOString();
  });
  await writeAnimationTournamentManifest(manifest);
  return manifest;
}
async function cancelAnimationTournament(tournamentId) {
  return withAnimationTournamentLock(tournamentId, () => cancelAnimationTournamentUnlocked(tournamentId));
}
async function cancelAnimationTournamentUnlocked(tournamentId) {
  const manifest = await readAnimationTournamentManifestRequired(tournamentId);
  const results = [];
  for (const candidate of manifest.candidates) {
    if (!candidate.jobId) {
      candidate.state = "cancelled";
      continue;
    }
    const status = await getRunnerStatus(candidate.jobId);
    if (status.state === "running" || status.cancellationPending) {
      if (runnerProcesses.has(candidate.jobId)) {
        results.push(await cancelCodexRunner(candidate.jobId));
      } else {
        results.push(await cancelUntrackedRunnerStatus(status, "Codex runner cancellation persisted after the animation tournament was cancelled.", "Codex runner restart was blocked after the animation tournament was cancelled"));
      }
    }
    candidate.state = "cancelled";
    candidate.updatedAt = new Date().toISOString();
  }
  manifest.state = "cancelled";
  await writeAnimationTournamentManifest(manifest);
  return { ok: true, tournament: manifest, results };
}
async function startMotionPilotExpansion(tournamentId, winnerJobId) {
  return withAnimationTournamentLock(tournamentId, () => withCodexRunnerAdmissionLock(() => startMotionPilotExpansionUnlocked(tournamentId, winnerJobId)));
}
async function startMotionPilotExpansionUnlocked(tournamentId, winnerJobId) {
  const manifest = await readAnimationTournamentManifestRequired(tournamentId);
  if (!manifest.pilotMode || !manifest.pilotDirection || manifest.pilotState !== "review") {
    throw new Error("Motion Pilot expansion requires pilot candidates in human review.");
  }
  const winner = manifest.candidates.find((candidate) => candidate.jobId === winnerJobId && !candidate.repairDirections?.length);
  if (!winner || winner.state !== "quality-evaluated")
    throw new Error("Motion Pilot winner must pass Quality Gate v2 before expansion.");
  if (manifest.humanReview?.manualWinnerJobId && manifest.humanReview.manualWinnerJobId !== winnerJobId) {
    throw new Error("Motion Pilot winner differs from the saved human review winner.");
  }
  if (manifest.expansionJobId) {
    return { reused: true, job: await codexJobResponse(manifest.expansionJobId, true), tournament: await refreshAnimationTournamentManifestUnlocked(tournamentId) };
  }
  await assertCodexRunnerSlotAvailable();
  const winnerDir = tournamentJobOutboxDir(tournamentId, winnerJobId);
  const pilotFile = await findDirectionSplitCandidateFile(winnerJobId, directionSlug(manifest.pilotDirection), winnerDir);
  if (!pilotFile)
    throw new Error(`Motion Pilot winner is missing ${manifest.pilotDirection}.`);
  const pilotHash = await hashFile(pilotFile.path);
  const expansionDirections = manifest.requestedDirections.filter((direction) => direction !== manifest.pilotDirection);
  if (expansionDirections.length === 0)
    throw new Error("Motion Pilot has no remaining directions to expand.");
  const template = parseJsonText(await readFile2(animationTournamentTemplatePath(tournamentId), "utf8"));
  const frameCount = normalizeMotionFrameCount(template.framesPerDirection) ?? 8;
  const expansionIndex = manifest.candidates.length;
  const idempotencyKey = `${manifest.idempotencyKey}:pilot-expansion:${directionSlug(manifest.pilotDirection)}`;
  const now = new Date().toISOString();
  manifest.winnerCandidateId = winnerJobId;
  manifest.pilotWinnerId = winnerJobId;
  manifest.pilotState = "expanding";
  manifest.state = "pilot-expanding";
  manifest.expansionDirectionIds = expansionDirections;
  manifest.acceptedDirectionHashes = { [manifest.pilotDirection]: pilotHash };
  manifest.directionStates[manifest.pilotDirection] = { state: "accepted", updatedAt: now, jobId: winnerJobId, reason: "human-reviewed Motion Pilot winner" };
  expansionDirections.forEach((direction) => {
    manifest.directionStates[direction] = { state: "repairing", updatedAt: now, reason: "expanding accepted pilot winner" };
  });
  manifest.candidates.forEach((candidate) => {
    if (candidate.repairDirections?.length)
      return;
    candidate.state = candidate.jobId === winnerJobId ? "accepted" : candidate.jobId ? "cancelled" : candidate.state;
    candidate.updatedAt = now;
  });
  manifest.candidates.push({
    index: expansionIndex,
    state: "repairing",
    idempotencyKey,
    updatedAt: now,
    repairDirections: expansionDirections,
    reason: "Motion Pilot winner expansion"
  });
  await writeAnimationTournamentManifest(manifest);
  const body2 = {
    ...template,
    directions: expansionDirections,
    grid: { columns: frameCount, rows: expansionDirections.length, gutter: 0 },
    frames: frameCount * expansionDirections.length,
    framesPerDirection: frameCount,
    tournamentId,
    tournamentCandidateIndex: expansionIndex,
    tournamentCandidateCount: expansionIndex + 1,
    generationProfile: manifest.generationProfile,
    idempotencyKey,
    sourceFingerprint: manifest.sourceFingerprint,
    repairDirections: expansionDirections,
    repairOfJobId: winnerJobId,
    jobNotes: [
      template.jobNotes ?? "",
      `Motion Pilot expansion for tournament ${tournamentId}.`,
      `Accepted pilot winner: ${winnerJobId}; reference direction: ${manifest.pilotDirection}; SHA-256: ${pilotHash}.`,
      `Generate only remaining directions: ${expansionDirections.join(", ")}.`,
      `Treat the accepted pilot frames in ${winnerDir} as the locked identity, palette, silhouette, timing, motion phase, topology, and contact reference.`,
      "Do not regenerate the accepted pilot direction. Preserve the full Motion Recipe phase contract across every expansion direction."
    ].filter(Boolean).join(`
`)
  };
  const job = await createCodexJob(body2, { admissionLockHeld: true, skipSlotCheck: true });
  const updated = await readAnimationTournamentManifestRequired(tournamentId);
  updated.expansionJobId = job.id;
  updated.directionOutputCount = updated.pilotCandidateIds?.length ?? updated.initialCandidateCount;
  updated.totalCandidateJobs = (updated.pilotCandidateIds?.length ?? updated.initialCandidateCount) + 1;
  updated.repairCount = updated.retryCount;
  await writeAnimationTournamentManifest(updated);
  return { reused: false, job, tournament: updated };
}
async function fallbackMotionPilotTournament(tournamentId, reason) {
  return withAnimationTournamentLock(tournamentId, async () => {
    const manifest = await readAnimationTournamentManifestRequired(tournamentId);
    if (!manifest.pilotMode || manifest.pilotState !== "review" && manifest.pilotState !== "failed") {
      throw new Error("Motion Pilot fallback requires review-ready or failed pilot candidates.");
    }
    const template = parseJsonText(await readFile2(animationTournamentTemplatePath(tournamentId), "utf8"));
    const fallbackTournamentId = `${tournamentId}-fallback`.slice(0, 96);
    const fallback = await registerAnimationTournament({
      tournamentId: fallbackTournamentId,
      idempotencyKey: `${manifest.idempotencyKey}:fallback:balanced`,
      sourceFingerprint: manifest.sourceFingerprint,
      motionRecipeId: manifest.motionRecipeId,
      motionRecipeVersion: manifest.motionRecipeVersion,
      motionRecipeCompilerVersion: manifest.motionRecipeCompilerVersion,
      presetId: manifest.presetId,
      generationProfile: "balanced",
      requestedDirections: manifest.requestedDirections,
      maximumCandidateCount: 3,
      initialCandidateCount: 2,
      pilotMode: false,
      jobTemplate: template,
      clientContext: { ...manifest.clientContext ?? {}, motionPilotFallbackFrom: tournamentId, fallbackReason: reason }
    });
    if (!fallback.tournament)
      throw new Error("Motion Pilot fallback registration did not return a canonical tournament.");
    manifest.pilotState = "fallback";
    manifest.fallbackReason = reason;
    manifest.fallbackTournamentId = fallback.tournament.tournamentId;
    manifest.state = "cancelled";
    manifest.elapsedTime = Math.max(0, Date.now() - Date.parse(manifest.createdAt));
    manifest.directionOutputCount = manifest.candidates.slice(0, manifest.initialCandidateCount).filter((candidate) => Boolean(candidate.jobId && candidate.qualityReportRef)).length;
    manifest.totalCandidateJobs = manifest.candidates.filter((candidate) => Boolean(candidate.jobId)).length;
    manifest.repairCount = manifest.retryCount;
    manifest.candidates.forEach((candidate) => {
      if (candidate.state !== "accepted")
        candidate.state = "cancelled";
      candidate.updatedAt = new Date().toISOString();
    });
    await writeAnimationTournamentManifest(manifest);
    return { ok: true, tournament: manifest, fallbackTournament: fallback.tournament };
  });
}
async function startAnimationDirectionRepair(tournamentId, requestedDirections) {
  return withAnimationTournamentLock(tournamentId, () => withCodexRunnerAdmissionLock(() => startAnimationDirectionRepairUnlocked(tournamentId, requestedDirections)));
}
async function startAnimationDirectionRepairUnlocked(tournamentId, requestedDirections) {
  const manifest = await readAnimationTournamentManifestRequired(tournamentId);
  if (manifest.state !== "accepted" || !manifest.winnerCandidateId)
    throw new Error("Direction Repair requires an accepted tournament winner.");
  if (manifest.retryCount >= 2)
    throw new Error("Direction Repair retry limit reached; return to human review.");
  const directions = normalizeDirectionNames(requestedDirections).filter((direction) => manifest.requestedDirections.includes(direction));
  if (directions.length === 0)
    throw new Error("Select at least one accepted direction to repair.");
  const existing = manifest.candidates.find((candidate) => candidate.repairDirections?.length === directions.length && candidate.repairDirections.every((direction) => directions.includes(direction)) && candidate.state !== "failed" && candidate.state !== "cancelled");
  if (existing?.jobId) {
    return { reused: true, job: await codexJobResponse(existing.jobId, true), tournament: await refreshAnimationTournamentManifestUnlocked(tournamentId) };
  }
  await assertCodexRunnerSlotAvailable();
  const template = parseJsonText(await readFile2(animationTournamentTemplatePath(tournamentId), "utf8"));
  const repairFrameCount = normalizeMotionFrameCount(template.framesPerDirection) ?? 8;
  const repairIndex = manifest.candidates.length;
  const idempotencyKey = `${manifest.idempotencyKey}:repair:${manifest.retryCount + 1}:${directions.map(directionSlug).sort().join("+")}`;
  const now = new Date().toISOString();
  manifest.retryCount += 1;
  manifest.candidates.push({
    index: repairIndex,
    state: "repairing",
    idempotencyKey,
    updatedAt: now,
    repairDirections: directions,
    reason: "direction repair requested"
  });
  directions.forEach((direction) => {
    manifest.directionStates[direction] = { state: "repairing", updatedAt: now, reason: "direction repair requested" };
  });
  await writeAnimationTournamentManifest(manifest);
  const body2 = {
    ...template,
    directions,
    grid: { columns: repairFrameCount, rows: directions.length, gutter: 0 },
    frames: repairFrameCount * directions.length,
    framesPerDirection: repairFrameCount,
    tournamentId,
    tournamentCandidateIndex: repairIndex,
    tournamentCandidateCount: repairIndex + 1,
    generationProfile: manifest.generationProfile,
    idempotencyKey,
    sourceFingerprint: manifest.sourceFingerprint,
    repairDirections: directions,
    repairOfJobId: manifest.winnerCandidateId,
    jobNotes: [
      template.jobNotes ?? "",
      `Direction Repair ${manifest.retryCount}/2 for tournament ${tournamentId}.`,
      `Regenerate only: ${directions.join(", ")}.`,
      `Accepted winner: ${manifest.winnerCandidateId}. Preserve all untargeted direction hashes exactly.`,
      `Accepted direction hashes: ${JSON.stringify(manifest.acceptedDirectionHashes)}.`,
      `Use the same source fingerprint, motion recipe or preset, ${repairFrameCount}-frame contract, cell size, chroma key, adjacent direction pose language, timing, palette, silhouette, and topology-specific contact line.`
    ].filter(Boolean).join(`
`)
  };
  const job = await createCodexJob(body2, { admissionLockHeld: true, skipSlotCheck: true });
  return { reused: false, job, tournament: await refreshAnimationTournamentManifestUnlocked(tournamentId) };
}
async function acceptAnimationDirectionRepair(tournamentId, repairJobId) {
  return withAnimationTournamentLock(tournamentId, () => acceptAnimationDirectionRepairUnlocked(tournamentId, repairJobId));
}
async function acceptAnimationDirectionRepairUnlocked(tournamentId, repairJobId) {
  const manifest = await readAnimationTournamentManifestRequired(tournamentId);
  const repair = manifest.candidates.find((candidate) => candidate.jobId === repairJobId && candidate.repairDirections?.length);
  if (!repair?.repairDirections || !manifest.winnerCandidateId)
    throw new Error("Direction Repair job is not registered or the winner is missing.");
  const isPilotExpansion = manifest.pilotMode && manifest.pilotState === "expanding" && manifest.expansionJobId === repairJobId;
  const repairStatus = await getRunnerStatus(repairJobId);
  if (repairStatus.state === "running")
    throw new Error("Direction Repair is still running.");
  const repairDir = tournamentJobOutboxDir(tournamentId, repairJobId);
  const repairArtifact = await inspectDirectionSplitArtifact(repairJobId, repairDir);
  if (!repairArtifact.ready || !repairArtifact.verified)
    throw new Error(`Direction Repair is not a verified artifact: ${repairArtifact.reason}`);
  if (!repairArtifact.animationQuality)
    throw new Error("Direction Repair must pass client Quality Gate v2 evaluation before acceptance.");
  const winnerJobId = manifest.winnerCandidateId;
  const winnerDir = tournamentJobOutboxDir(tournamentId, winnerJobId);
  const beforeHashes = { ...manifest.acceptedDirectionHashes };
  for (const direction of repair.repairDirections) {
    const slug = directionSlug(direction);
    const [repairFile, existingWinnerFile] = await Promise.all([
      findDirectionSplitCandidateFile(repairJobId, slug, repairDir),
      findDirectionSplitCandidateFile(winnerJobId, slug, winnerDir)
    ]);
    if (!repairFile)
      throw new Error(`Direction Repair is missing ${direction}.`);
    const winnerPath = existingWinnerFile?.path ?? (isPilotExpansion ? join3(winnerDir, repairFile.finalName.replace(repairJobId, winnerJobId)) : "");
    if (!winnerPath)
      throw new Error(`Accepted winner is missing ${direction}.`);
    await copyFile(repairFile.path, winnerPath);
  }
  const afterHashes = {};
  const changedUntargeted = [];
  for (const direction of manifest.requestedDirections) {
    const winnerFile = await findDirectionSplitCandidateFile(winnerJobId, directionSlug(direction), winnerDir);
    if (!winnerFile)
      throw new Error(`Accepted winner is missing ${direction} after repair.`);
    afterHashes[direction] = await hashFile(winnerFile.path);
    if (!repair.repairDirections.includes(direction) && beforeHashes[direction] && beforeHashes[direction] !== afterHashes[direction])
      changedUntargeted.push(direction);
  }
  if (changedUntargeted.length > 0)
    throw new Error(`Direction Repair changed untargeted directions: ${changedUntargeted.join(", ")}`);
  if (isPilotExpansion)
    await updateMotionPilotWinnerExpectedDirections(winnerJobId, manifest.requestedDirections);
  await new Promise((resolveDelay) => setTimeout(resolveDelay, Math.min(artifactStableMs + 50, 2000)));
  const published = await publishTournamentWinnerUnlocked(tournamentId, winnerJobId);
  const updated = await readAnimationTournamentManifestRequired(tournamentId);
  updated.acceptedDirectionHashes = afterHashes;
  repair.state = "accepted";
  repair.updatedAt = new Date().toISOString();
  repair.repairDirections.forEach((direction) => {
    updated.directionStates[direction] = { state: "accepted", updatedAt: new Date().toISOString(), jobId: repairJobId };
  });
  updated.requestedDirections.filter((direction) => !repair.repairDirections?.includes(direction)).forEach((direction) => {
    updated.directionStates[direction] = {
      state: "accepted",
      updatedAt: new Date().toISOString(),
      jobId: winnerJobId,
      reason: "untargeted direction preserved from accepted winner"
    };
  });
  const updatedRepair = updated.candidates.find((candidate) => candidate.jobId === repairJobId);
  if (updatedRepair) {
    updatedRepair.state = "accepted";
    updatedRepair.updatedAt = new Date().toISOString();
  }
  updated.state = "accepted";
  if (isPilotExpansion) {
    updated.pilotState = "completed";
    updated.expansionDirectionIds = repair.repairDirections;
    updated.expansionJobId = repairJobId;
    updated.directionOutputCount = (updated.pilotCandidateIds?.length ?? updated.initialCandidateCount) + repair.repairDirections.length;
    updated.totalCandidateJobs = (updated.pilotCandidateIds?.length ?? updated.initialCandidateCount) + 1;
    updated.elapsedTime = Math.max(0, Date.now() - Date.parse(updated.createdAt));
    updated.repairCount = updated.retryCount;
  }
  await writeAnimationTournamentManifest(updated);
  return {
    ok: true,
    tournament: updated,
    published,
    repairedDirections: repair.repairDirections,
    beforeHashes,
    afterHashes,
    unchangedDirections: manifest.requestedDirections.filter((direction) => !repair.repairDirections?.includes(direction))
  };
}
async function updateMotionPilotWinnerExpectedDirections(jobId, directions) {
  const path = join3(inboxDir, `${jobId}.json`);
  const job = parseJsonText(await readFile2(path, "utf8"));
  const spriteContext = job.spriteContext && typeof job.spriteContext === "object" ? job.spriteContext : {};
  const framesPerDirection = normalizeMotionFrameCount(spriteContext.framesPerDirection) ?? 8;
  job.spriteContext = {
    ...spriteContext,
    directions,
    grid: { columns: framesPerDirection, rows: directions.length, gutter: 0 },
    frames: framesPerDirection * directions.length
  };
  await writeFile3(path, JSON.stringify(job, null, 2), "utf8");
}
async function refreshAnimationTournamentManifest(tournamentId) {
  return withAnimationTournamentLock(tournamentId, () => refreshAnimationTournamentManifestUnlocked(tournamentId));
}
async function refreshAnimationTournamentManifestUnlocked(tournamentId) {
  const manifest = await readAnimationTournamentManifest(tournamentId);
  if (!manifest)
    return null;
  let changed = false;
  for (const candidate of manifest.candidates) {
    if (typeof candidate.warningCount === "number" && (!Number.isFinite(candidate.warningCount) || candidate.warningCount > 999)) {
      candidate.warningCount = 0;
      changed = true;
    }
    if (!candidate.jobId || candidate.state === "accepted" || candidate.state === "cancelled")
      continue;
    const status = await getRunnerStatus(candidate.jobId);
    let nextState = candidate.state;
    if (status.state === "running")
      nextState = "running";
    else if (status.state === "completed") {
      const artifact = await inspectDirectionSplitArtifact(candidate.jobId, tournamentJobOutboxDir(tournamentId, candidate.jobId));
      nextState = artifact.ready ? artifact.animationQuality ? "quality-evaluated" : "artifact-ready" : "failed";
      if (artifact.animationQuality) {
        candidate.qualityReportRef = `${candidate.jobId}-manifest.json#animationQuality`;
        if (!manifest.qualityReportRefs.includes(candidate.qualityReportRef))
          manifest.qualityReportRefs.push(candidate.qualityReportRef);
      }
      candidate.reason = artifact.reason;
    } else if (status.state === "failed" || status.state === "unavailable" || status.state === "disabled")
      nextState = "failed";
    if (nextState !== candidate.state) {
      candidate.state = nextState;
      candidate.updatedAt = new Date().toISOString();
      changed = true;
    }
  }
  if (manifest.state !== "accepted" && manifest.state !== "cancelled" && manifest.state !== "pilot-review" && manifest.state !== "pilot-expanding") {
    const started = manifest.candidates.filter((candidate) => candidate.jobId);
    const hasActiveCandidate = started.some((candidate) => ["running", "artifact-ready", "quality-evaluated", "repairing"].includes(candidate.state));
    const nextState = hasActiveCandidate ? "running" : manifest.winnerCandidateId ? "accepted" : started.length > 0 && started.every((candidate) => candidate.state === "failed") ? "failed" : manifest.state;
    if (nextState !== manifest.state) {
      manifest.state = nextState;
      changed = true;
    }
    if (nextState === "failed") {
      const updatedAt = new Date().toISOString();
      manifest.requestedDirections.forEach((direction) => {
        const directionState = manifest.directionStates[direction];
        if (directionState?.state === "failed")
          return;
        manifest.directionStates[direction] = {
          state: "failed",
          updatedAt,
          reason: "all tournament candidates reached a failed terminal state"
        };
        changed = true;
      });
    }
  }
  if (manifest.pilotMode && manifest.pilotState === "review" && manifest.state !== "pilot-review") {
    manifest.state = "pilot-review";
    changed = true;
  }
  if (manifest.pilotMode && manifest.pilotState === "expanding" && manifest.state !== "pilot-expanding") {
    manifest.state = "pilot-expanding";
    changed = true;
  }
  const hasPendingDirectionRepair = manifest.candidates.some((candidate) => candidate.repairDirections?.length && ["running", "repairing", "artifact-ready", "quality-evaluated"].includes(candidate.state));
  if (manifest.state === "accepted" && manifest.winnerCandidateId && !hasPendingDirectionRepair) {
    const updatedAt = new Date().toISOString();
    manifest.requestedDirections.forEach((direction) => {
      const directionState = manifest.directionStates[direction];
      if (directionState?.state === "accepted")
        return;
      manifest.directionStates[direction] = {
        state: "accepted",
        updatedAt,
        jobId: manifest.winnerCandidateId,
        reason: "restored accepted winner after terminal Direction Repair"
      };
      changed = true;
    });
  }
  if (manifest.pilotMode && !manifest.pilotWinnerId) {
    const completedPilotOutputs = manifest.candidates.slice(0, manifest.initialCandidateCount).filter((candidate) => Boolean(candidate.jobId && candidate.qualityReportRef)).length;
    if (manifest.directionOutputCount !== completedPilotOutputs) {
      manifest.directionOutputCount = completedPilotOutputs;
      changed = true;
    }
  }
  const totalCandidateJobs = manifest.candidates.filter((candidate) => Boolean(candidate.jobId)).length;
  if (manifest.totalCandidateJobs !== totalCandidateJobs) {
    manifest.totalCandidateJobs = totalCandidateJobs;
    changed = true;
  }
  if (manifest.repairCount !== manifest.retryCount) {
    manifest.repairCount = manifest.retryCount;
    changed = true;
  }
  if (changed)
    await writeAnimationTournamentManifest(manifest);
  return manifest;
}
async function listAnimationTournamentManifests() {
  const entries = await readdir(tournamentWorkRootDir, { withFileTypes: true });
  const manifests = await Promise.all(entries.filter((entry) => entry.isDirectory() && isSafeTournamentId(entry.name)).map((entry) => refreshAnimationTournamentManifest(entry.name).catch(() => null)));
  return manifests.filter((manifest) => Boolean(manifest)).sort((left, right) => Date.parse(right.updatedAt) - Date.parse(left.updatedAt));
}
async function readAnimationTournamentManifest(tournamentId) {
  try {
    const manifest = parseJsonText(await readFile2(animationTournamentManifestPath(tournamentId), "utf8"));
    return manifest.schema === "image-cockpit.animation-tournament.v1" ? manifest : null;
  } catch {
    return null;
  }
}
async function readAnimationTournamentManifestRequired(tournamentId) {
  const manifest = await readAnimationTournamentManifest(tournamentId);
  if (!manifest)
    throw new Error("Tournament manifest was not found.");
  return manifest;
}
async function writeAnimationTournamentManifest(manifest) {
  manifest.updatedAt = new Date().toISOString();
  await mkdir2(animationTournamentDir(manifest.tournamentId), { recursive: true });
  await writeJsonAtomic(animationTournamentManifestPath(manifest.tournamentId), manifest);
}
async function writeJsonAtomic(path, value) {
  const temporaryPath = `${path}.${process.pid}.${Date.now()}.tmp`;
  await writeFile3(temporaryPath, JSON.stringify(value, null, 2), "utf8");
  await rename2(temporaryPath, path);
}
async function withAnimationTournamentRegistrationLock(action) {
  const previous = animationTournamentRegistrationTail;
  let release = () => {};
  const current = new Promise((resolveLock) => {
    release = resolveLock;
  });
  const queued = previous.then(() => current);
  animationTournamentRegistrationTail = queued;
  await previous;
  try {
    return await action();
  } finally {
    release();
    if (animationTournamentRegistrationTail === queued) {
      animationTournamentRegistrationTail = Promise.resolve();
    }
  }
}
async function withAnimationTournamentLock(tournamentId, action) {
  const previous = animationTournamentLocks.get(tournamentId) ?? Promise.resolve();
  let release = () => {};
  const current = new Promise((resolveLock) => {
    release = resolveLock;
  });
  const queued = previous.then(() => current);
  animationTournamentLocks.set(tournamentId, queued);
  await previous;
  try {
    return await action();
  } finally {
    release();
    if (animationTournamentLocks.get(tournamentId) === queued)
      animationTournamentLocks.delete(tournamentId);
  }
}
async function withCodexRunnerAdmissionLock(action) {
  const previous = codexRunnerAdmissionTail;
  let release = () => {};
  const current = new Promise((resolveLock) => {
    release = resolveLock;
  });
  codexRunnerAdmissionTail = previous.then(() => current);
  await previous;
  try {
    return await action();
  } finally {
    release();
  }
}
function normalizeAnimationGenerationProfile(value) {
  return value === "fast" || value === "balanced" || value === "best" ? value : "best";
}
function animationGenerationProfilePlan(profile) {
  if (profile === "fast")
    return { initialCandidates: 1, maximumCandidates: 1 };
  if (profile === "balanced")
    return { initialCandidates: 2, maximumCandidates: 3 };
  return { initialCandidates: 3, maximumCandidates: 3 };
}
function assertAnimationGenerationProfileRequestCounts(registration, profile, profilePlan = animationGenerationProfilePlan(profile)) {
  if (registration.maximumCandidateCount !== undefined && registration.maximumCandidateCount !== profilePlan.maximumCandidates || registration.initialCandidateCount !== undefined && registration.initialCandidateCount !== profilePlan.initialCandidates) {
    throw new HttpError(400, `${profile} animation tournaments require exactly ${profilePlan.initialCandidates} initial and ${profilePlan.maximumCandidates} maximum candidates.`, "animation_profile_candidate_count_mismatch", {
      generationProfile: profile,
      requiredInitialCandidateCount: profilePlan.initialCandidates,
      requiredMaximumCandidateCount: profilePlan.maximumCandidates,
      retryable: false
    });
  }
}
function assertAnimationGenerationProfileManifestCounts(manifest) {
  const profilePlan = animationGenerationProfilePlan(manifest.generationProfile);
  const extraCandidates = manifest.candidates.slice(profilePlan.maximumCandidates);
  const hasUnexpectedExtraCandidate = extraCandidates.some((candidate) => !candidate.repairDirections?.length);
  if (manifest.initialCandidateCount !== profilePlan.initialCandidates || manifest.maximumCandidateCount !== profilePlan.maximumCandidates || manifest.candidates.length < profilePlan.maximumCandidates || hasUnexpectedExtraCandidate) {
    throw new HttpError(409, "The saved animation tournament candidate plan does not match its generation profile.", "animation_profile_candidate_count_mismatch", {
      generationProfile: manifest.generationProfile,
      requiredInitialCandidateCount: profilePlan.initialCandidates,
      requiredMaximumCandidateCount: profilePlan.maximumCandidates,
      retryable: false
    });
  }
}
function normalizeDirectionNames(value) {
  if (!Array.isArray(value))
    return [];
  const allowed = new Set(directionSplitNames);
  return Array.from(new Set(value.filter((item) => typeof item === "string" && allowed.has(item))));
}
function normalizeStandardSpriteJobDirections(workflowMode, spriteVariant, value, repairDirections, tournamentId) {
  if (workflowMode !== "sprite-generate" || spriteVariant !== "standard")
    return;
  const directions = normalizeDirectionNames(value);
  const targetedSubset = Boolean(tournamentId && repairDirections.length > 0);
  if (targetedSubset) {
    if (directions.length < 1 || directions.length > directionSplitNames.length || directions.length !== repairDirections.length || directions.some((direction) => !repairDirections.includes(direction))) {
      throw new HttpError(400, "Direction Repair and Motion Pilot expansion directions must be the same canonical 1-5 direction subset.");
    }
    return directions;
  }
  const initialDirections = directions.length > 0 ? directions : [...directionSplitNames];
  if (![1, 3, 5].includes(initialDirections.length)) {
    throw new HttpError(400, "Standard animation generation requires 1, 3, or 5 unique canonical directions.");
  }
  return initialDirections;
}
function normalizeBoundedInteger(value, fallback, minimum, maximum) {
  return typeof value === "number" && Number.isFinite(value) ? Math.min(maximum, Math.max(minimum, Math.floor(value))) : fallback;
}
function normalizeMotionFrameCount(value) {
  const parsed = typeof value === "number" && Number.isFinite(value) ? Math.floor(value) : Number(value);
  return parsed === 4 || parsed === 6 || parsed === 8 || parsed === 12 || parsed === 16 || parsed === 20 ? parsed : undefined;
}
function directionSplitGridForFrameCount(value) {
  if (value === 6)
    return { columns: 3, rows: 2, gutter: 0 };
  return { columns: 4, rows: Math.max(1, Math.ceil(value / 4)), gutter: 0 };
}
function normalizeShortText(value) {
  return typeof value === "string" && value.trim() ? value.trim().slice(0, 160) : undefined;
}
function normalizePositiveInteger(value) {
  return typeof value === "number" && Number.isFinite(value) && value > 0 ? Math.floor(value) : undefined;
}
function normalizeMotionRecipeContext(body2) {
  const raw = body2.motionRecipe && typeof body2.motionRecipe === "object" ? body2.motionRecipe : undefined;
  const id = normalizeShortText(raw?.id ?? body2.motionRecipeId ?? body2.presetId);
  if (!id)
    return;
  const modifiers = raw?.modifiers && typeof raw.modifiers === "object" ? Object.fromEntries(Object.entries(raw.modifiers).filter((entry) => typeof entry[1] === "string").slice(0, 12)) : undefined;
  return {
    id,
    version: normalizePositiveInteger(raw?.version ?? body2.motionRecipeVersion) ?? 1,
    compilerVersion: normalizeShortText(raw?.compilerVersion ?? body2.motionRecipeCompilerVersion) ?? "unknown",
    qualityProfile: normalizeShortText(raw?.qualityProfile ?? body2.motionRecipeQualityProfile),
    bodyTopology: normalizeShortText(raw?.bodyTopology),
    frameCount: normalizeMotionFrameCount(raw?.frameCount ?? body2.framesPerDirection),
    modifiers,
    experimental: raw?.experimental === true || undefined
  };
}
function isSafeIdempotencyKey(value) {
  return /^[A-Za-z0-9_.:-]{8,200}$/.test(value);
}
function directionSlug(direction) {
  const index = directionSplitNames.indexOf(direction);
  return directionSplitSlugs[index] ?? direction.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}
async function hashFile(path) {
  return createHash2("sha256").update(await readFile2(path)).digest("hex");
}
async function writeSelectedImageAsset(jobId, body2) {
  const existingAssetPath = resolveSafeAssetPath(body2.selectedImageAssetPath);
  if (existingAssetPath) {
    const bytes = await readFile2(existingAssetPath);
    const mimeType = mimeTypeForImage(existingAssetPath) ?? "application/octet-stream";
    return { path: existingAssetPath, mimeType, source: "content-addressed-asset", fingerprint: createHash2("sha256").update(bytes).digest("hex") };
  }
  const value = body2.selectedImageDataUrl;
  if (!value)
    return null;
  const dataUrlMatch = value.match(/^data:([^;,]+);base64,(.+)$/);
  if (dataUrlMatch) {
    const mimeType = dataUrlMatch[1];
    const extension = extensionForMimeType(mimeType);
    if (!extension)
      return null;
    const bytes = Buffer.from(dataUrlMatch[2], "base64");
    const fingerprint = createHash2("sha256").update(bytes).digest("hex");
    const path = join3(assetsDir, `source-${fingerprint}${extension}`);
    if (!existsSync(path))
      await writeFile3(path, bytes);
    return { path, mimeType, source: "data-url", fingerprint };
  }
  if (value.startsWith("/")) {
    const sourcePath = resolve2("public", value.replace(/^\/+/, ""));
    const root = `${resolve2("public")}${sep2}`;
    if (!sourcePath.startsWith(root))
      return null;
    const mimeType = mimeTypeForImage(sourcePath);
    const extension = extname2(sourcePath).toLowerCase();
    if (!mimeType || !extension)
      return null;
    const bytes = await readFile2(sourcePath);
    const fingerprint = createHash2("sha256").update(bytes).digest("hex");
    const path = join3(assetsDir, `source-${fingerprint}${extension}`);
    if (!existsSync(path))
      await copyFile(sourcePath, path);
    return { path, mimeType, source: value, fingerprint };
  }
  return null;
}
function resolveSafeAssetPath(value) {
  if (typeof value !== "string" || !value)
    return null;
  const rootDir = resolve2(assetsDir);
  const candidate = resolve2(value);
  const root = rootDir.endsWith(sep2) ? rootDir : `${rootDir}${sep2}`;
  return candidate.startsWith(root) && existsSync(candidate) ? candidate : null;
}
function normalizeWorkflowMode(value) {
  if (value === "image-generate" || value === "image-edit" || value === "sprite-generate" || value === "sprite-edit" || value === "effect-animation") {
    return value;
  }
  return "image-generate";
}
function workflowUsesSelectedImage(mode) {
  return mode === "image-edit" || mode === "sprite-generate";
}
function workflowUsesSpriteContext(mode) {
  return mode === "sprite-generate" || mode === "sprite-edit" || mode === "effect-animation";
}
function workflowIntent(mode) {
  if (mode === "image-edit") {
    return "Ask local Codex to revise the selected source image using annotations and edit notes, then return image files to the outbox.";
  }
  if (mode === "sprite-generate") {
    return "Ask local Codex to inspect the selected source image and use imagegen / built-in image_gen to create a chroma-key animation sprite sheet.";
  }
  if (mode === "sprite-edit") {
    return "Ask local Codex to revise sprite-sheet frames or metadata when available, or record sprite edit context for manual handoff.";
  }
  if (mode === "effect-animation") {
    return "Ask local Codex to use imagegen / built-in image_gen to create a transparent game VFX animation sheet and metadata.";
  }
  return "Ask local Codex to use imagegen / built-in image_gen to generate a real pixel-art image from the prompt, then return image files to the outbox.";
}
function workflowNotes(mode) {
  const blockerSidecarNote = "If built-in imagegen / image_gen is unavailable, do not create any procedural, SVG, canvas, diagram, geometric, or placeholder image. Return a blocked JSON sidecar with reasonKind=imagegen_unavailable instead. If image generation/editing is blocked by safety or policy, do not create a placeholder image. Write only a small JSON sidecar with status=blocked, reasonKind, userMessage, and suggestion.";
  if (mode === "image-edit") {
    return [
      "Use selectedImage.assetPath as the source image when present.",
      "Use imagegen / built-in image_gen editing when available so the result is a real edited raster image.",
      "Use annotationContext.annotations, numbered region comments, prompt, and jobNotes as the user's edit instructions.",
      "Use annotationContext.annotations[].imageRectNormalized and imageRectPixels when present to target the source image region, while retaining the original canvas coordinate rect for visual context.",
      "Preserve the original canvas size and aspect ratio. Do not zoom in, crop, or reframe the image into a portrait/detail shot.",
      "Keep the full character visible, including head, hair, hands, equipment, and both feet.",
      "Treat selectedImage.assetPath as the exact base image to edit, not inspiration for a new variant.",
      "Preserve transparency when present; if transparency cannot be preserved, use a flat chroma fallback background.",
      "Preserve unrelated pixels when possible, change only requested regions, and return the edited image as a real PNG or WebP with the job id filename prefix.",
      "Do not create a placeholder, SVG, diagram, or text-only result.",
      blockerSidecarNote
    ];
  }
  if (mode === "sprite-generate") {
    return [
      "Use selectedImage.assetPath as the source character image.",
      "Use imagegen / built-in image_gen when available to create a real raster sprite sheet from that source image; never create a procedural placeholder.",
      "Extract only the character from the source image, then generate the requested motion as a sprite sheet.",
      "Use spriteContext.grid, spriteContext.cell, spriteContext.action, spriteContext.frames, spriteContext.directions, and spriteContext.chromaKey exactly when they are populated.",
      "Treat spriteContext.grid and spriteContext.cell as strict cut lines: no gutters, no extra sheet margin, no character pixels crossing cell borders.",
      "The default direction-row order is front, front three-quarter, side, back three-quarter, back.",
      "Every cell must contain exactly one full-body character with head, hair, hands, equipment, and both feet visible, centered with at least 10% inner padding.",
      "Each direction image is an animation sheet, not a still direction reference image: all populated cells must show the requested action progressing over time. Do not paste, upscale, or lightly nudge one still pose into every frame.",
      "Before publishing standard animation output, compare neighboring frames in each direction. If the frame-to-frame motion is nearly identical, regenerate that direction with clearer pose, limb, cloth, hair, equipment, or breathing changes while keeping the same character and grounded baseline.",
      "Reject and retry the sprite sheet if any cell has a cropped head, missing feet, multiple heads, a head below the feet, inconsistent scale, body fragments, or a different character.",
      "Use the requested chroma-key background color as a flat simple background in every cell so Image Cockpit can remove it after import.",
      "For standard direction-split output, keep every intermediate, source, QA, and candidate file under outbox/.staging/<job-id>/ or another non-root work folder while work is still in progress. Do not write, copy, or manifest any root outbox <job-id>-*.png or <job-id>-manifest.json until the complete requested direction set is normalized, self-checked, and no further regeneration is planned. The final step must publish only the requested final direction PNG/WebP files plus the final manifest into the root outbox, with the manifest written last. Keep *-qa.json, work files, temporary files, contact sheets, comparison sheets, and debug images outside the root.",
      "Avoid readable text, logos, watermarks, labels, UI words, numbers, scenery, and complex backgrounds.",
      blockerSidecarNote
    ];
  }
  if (mode === "sprite-edit") {
    return [
      "Use spriteContext.grid, spriteContext.action, and spriteContext.frames when they are populated.",
      "Use jobNotes for frame, transparency, anchor, or export requirements."
    ];
  }
  if (mode === "effect-animation") {
    return [
      "Use imagegen / built-in image_gen when available to create a real raster transparent PNG sprite sheet for a game visual effect.",
      "Use effectContext as the strict output contract: category, type, style, palette, frameCount, frameSize, layout, loopMode, anchor, blendMode, and sheetSize.",
      "Return one final transparent PNG sheet with the job id filename prefix. Include metadata JSON and preview GIF when feasible.",
      "The final sheet must use real alpha transparency. Do not bake checkerboard, solid matte backgrounds, UI panels, labels, frame numbers, text, logos, watermarks, or border guides into the image.",
      "Each populated frame must show visible temporal progression. Do not copy one still effect into every frame.",
      "Keep the effect inside each cell with alpha padding, no clipping at bounds, no overlap into neighboring cells, and no extra gutters.",
      blockerSidecarNote
    ];
  }
  return [
    "For prompt-only pixel art generation, use the imagegen skill default built-in image generation path when it is available.",
    "Use the job prompt as the creative brief. Interpret complex prompts literally and preserve concrete subject, style, palette, composition, and production constraints.",
    "For character or creature assets, keep the full body inside the image with clear transparent or chroma-key padding around the head, hair, hands, props, and both feet; reject and retry if the subject is cropped by the canvas edge.",
    "Create a real raster image. Do not create a procedural placeholder, SVG, diagram, or text-only result.",
    "Avoid readable text, logos, watermarks, labels, UI words, and numbers unless the user explicitly asks for them.",
    "If the first result contains unwanted text or numbers, retry once with stricter no-text/no-number constraints.",
    "Write the final image with the job id prefix. If you include notes, prefer a short Markdown sidecar and do not place *-qa.json, work files, temporary files, contact sheets, comparison sheets, or debug images in the outbox root.",
    blockerSidecarNote,
    "This is a text-to-image style generation job. Do not treat the current UI sample image as a source image unless selectedImage.assetPath is populated.",
    "Use prompt, negativePrompt, generationHints, and jobNotes as the generation brief."
  ];
}
async function startCodexRunner(job, resume) {
  const statusPath = join3(statusDir, `${job.id}.json`);
  const logPath = join3(logsDir, `${job.id}.log`);
  if (!codexAutoRun) {
    const status2 = {
      jobId: job.id,
      state: "disabled",
      message: "Codex autorun is disabled. Run the job manually or set IMAGE_COCKPIT_CODEX_AUTORUN=1.",
      statusPath,
      logPath
    };
    await writeRunnerStatus(status2);
    return status2;
  }
  const capacityBlock = await getRunnerCapacityBlock();
  if (capacityBlock) {
    const finishedAt = new Date().toISOString();
    const status2 = {
      jobId: job.id,
      state: "unavailable",
      message: capacityBlock.userMessage,
      statusPath,
      logPath,
      finishedAt,
      diagnostic: diagnosticFromRunnerCapacityBlock(capacityBlock)
    };
    await writeFile3(logPath, `[${finishedAt}] Codex runner was not started: ${capacityBlock.title}: ${capacityBlock.userMessage}
`, "utf8");
    await writeRunnerStatus(status2);
    return status2;
  }
  if (isMockRunnerBlocked()) {
    const finishedAt = new Date().toISOString();
    const diagnostic = mockRunnerBlockedDiagnostic();
    const status2 = {
      jobId: job.id,
      state: "unavailable",
      message: diagnostic.userMessage,
      command: codexLaunchCommand,
      requestedCommand: codexCommand,
      statusPath,
      logPath,
      outboxDir: job.outboxDir,
      finishedAt,
      diagnostic
    };
    await writeFile3(logPath, `[${finishedAt}] Codex runner was not started: ${diagnostic.title}: ${diagnostic.userMessage}
`, "utf8");
    await writeRunnerStatus(status2);
    return status2;
  }
  const startedAt = new Date().toISOString();
  const status = {
    jobId: job.id,
    state: "running",
    message: resume ? `Resumed ${codexCommand} exec for ${job.id} after the API process restarted` : `Started ${codexCommand} exec for ${job.id}`,
    command: codexLaunchCommand,
    requestedCommand: codexCommand,
    startedAt,
    initialStartedAt: resume?.initialStartedAt ?? startedAt,
    resumedAt: resume ? startedAt : undefined,
    resumeCount: resume?.resumeCount ?? 0,
    statusPath,
    logPath,
    outboxDir: job.outboxDir
  };
  runnerStatuses.set(job.id, status);
  await writeRunnerStatus(status);
  const logStream = createWriteStream(logPath, { flags: "a" });
  const prompt = buildCodexRunnerPrompt(job, Boolean(resume));
  logStream.write(`[${startedAt}] Starting ${codexLaunchCommand} exec for ${job.id}
`);
  logStream.write(`Job path: ${job.path}
`);
  logStream.write(`Outbox: ${job.outboxDir}

`);
  let settled = false;
  try {
    const child = spawn(codexLaunchCommand, codexExecArgs, {
      cwd: process.cwd(),
      env: {
        ...process.env,
        IMAGE_COCKPIT_JOB_ID: job.id,
        IMAGE_COCKPIT_JOB_PATH: job.path,
        IMAGE_COCKPIT_OUTBOX_DIR: job.outboxDir
      },
      windowsHide: true
    });
    if (typeof child.pid === "number" && Number.isSafeInteger(child.pid) && child.pid > 0) {
      status.processId = child.pid;
      runnerStatuses.set(job.id, status);
      try {
        writeFileSync(statusPath, JSON.stringify(status, null, 2), "utf8");
      } catch (error) {
        child.kill("SIGTERM");
        throw error;
      }
    }
    runnerProcesses.set(job.id, child);
    child.stdout.pipe(logStream, { end: false });
    child.stderr.pipe(logStream, { end: false });
    child.stdin.on("error", () => {});
    child.stdin.end(prompt, "utf8");
    child.on("error", (error) => {
      if (settled)
        return;
      settled = true;
      runnerProcesses.delete(job.id);
      cancellingRunnerJobIds.delete(job.id);
      const finishedAt = new Date().toISOString();
      const errorStatus = {
        ...status,
        state: isRunnerUnavailableError(error) ? "unavailable" : "failed",
        message: error.message,
        finishedAt
      };
      writeRunnerStatus(errorStatus);
      logStream.write(`
[${finishedAt}] Runner error: ${error.message}
`);
      logStream.end();
    });
    child.on("close", (exitCode, signal) => {
      if (settled)
        return;
      settled = true;
      runnerProcesses.delete(job.id);
      const wasCancelled = cancellingRunnerJobIds.delete(job.id);
      const finishedAt = new Date().toISOString();
      const completedStatus = {
        ...status,
        state: wasCancelled ? "failed" : exitCode === 0 ? "completed" : "failed",
        cancelled: wasCancelled || undefined,
        cancellationReason: wasCancelled ? managedCancellationReasons.get(job.id) : undefined,
        message: wasCancelled ? cancellationMessage(managedCancellationReasons.get(job.id), true) : exitCode === 0 ? "Codex exec completed" : `Codex exec exited with code ${exitCode}`,
        finishedAt,
        exitCode,
        signal
      };
      writeRunnerStatus(completedStatus);
      logStream.write(`
[${finishedAt}] ${completedStatus.message}
`);
      logStream.end();
    });
  } catch (error) {
    const finishedAt = new Date().toISOString();
    const errorStatus = {
      ...status,
      state: isRunnerUnavailableError(error) ? "unavailable" : "failed",
      message: error instanceof Error ? error.message : "Could not start Codex exec",
      finishedAt
    };
    await writeRunnerStatus(errorStatus);
    logStream.write(`
[${finishedAt}] ${errorStatus.message}
`);
    logStream.end();
    return errorStatus;
  }
  return status;
}
async function checkCodexRunnerPreflight() {
  const base = createRunnerPreflightBase();
  if (!codexAutoRun) {
    return {
      ...base,
      state: "disabled",
      message: "Codex autorun is disabled. Jobs will be written for manual pickup.",
      setupHint: "Set IMAGE_COCKPIT_CODEX_AUTORUN=1 to let Image Cockpit try to start codex exec."
    };
  }
  const capacityBlock = await getRunnerCapacityBlock();
  if (capacityBlock) {
    return {
      ...base,
      state: "unavailable",
      message: capacityBlock.userMessage,
      errorCode: capacityBlock.kind,
      setupHint: capacityBlock.suggestion
    };
  }
  if (isMockRunnerBlocked()) {
    const diagnostic = mockRunnerBlockedDiagnostic();
    return {
      ...base,
      state: "unavailable",
      message: diagnostic.userMessage,
      errorCode: "mock_runner",
      setupHint: diagnostic.suggestion
    };
  }
  return new Promise((resolve3) => {
    let settled = false;
    let stderrText = "";
    let timeoutId;
    let child = null;
    const finish = (preflight) => {
      if (settled)
        return;
      settled = true;
      if (timeoutId)
        clearTimeout(timeoutId);
      resolve3({ ...base, ...preflight });
    };
    timeoutId = setTimeout(() => {
      if (child && !child.killed)
        child.kill();
      finish({
        state: "unavailable",
        message: `Codex runner check timed out after ${runnerPreflightTimeoutMs}ms.`,
        setupHint: codexRunnerSetupHint()
      });
    }, runnerPreflightTimeoutMs);
    try {
      child = spawn(codexLaunchCommand, codexHelpArgs, {
        cwd: process.cwd(),
        stdio: ["ignore", "ignore", "pipe"],
        windowsHide: true
      });
      child.stderr?.on("data", (chunk) => {
        stderrText = `${stderrText}${chunk.toString("utf8")}`.slice(0, 1200);
      });
      child.on("error", (error) => {
        finish({
          state: "unavailable",
          message: error.message,
          errorCode: error.code,
          setupHint: codexRunnerSetupHint(error)
        });
      });
      child.on("close", (exitCode) => {
        if (exitCode === 0) {
          finish({
            state: "ready",
            message: codexRunnerMode === "mock" ? "Mock/test runner is executable. This does not prove that Codex imagegen is available." : `${codexLaunchCommand} is executable from the local handoff server.`
          });
          return;
        }
        finish({
          state: "unavailable",
          message: stderrText.trim() || `${codexLaunchCommand} ${codexHelpArgs.join(" ")} exited with code ${exitCode}`,
          errorCode: exitCode === null ? undefined : String(exitCode),
          setupHint: codexRunnerSetupHint()
        });
      });
    } catch (error) {
      finish({
        state: "unavailable",
        message: error instanceof Error ? error.message : "Could not start Codex runner check.",
        errorCode: error && typeof error === "object" && "code" in error ? String(error.code) : undefined,
        setupHint: codexRunnerSetupHint(error)
      });
    }
  });
}
function createRunnerPreflightBase() {
  return {
    state: "unavailable",
    message: "",
    command: codexCommand,
    launchCommand: codexLaunchCommand,
    mode: codexRunnerMode,
    mockRunnerAllowed: allowMockRunner,
    checkedAt: new Date().toISOString(),
    autorun: codexAutoRun,
    sandbox: codexSandbox,
    approval: codexApproval,
    resolvedCommandPaths: codexCommandCandidates.slice(0, 8)
  };
}
function parseJsonStringArray(envKey, fallback) {
  const rawValue = process.env[envKey];
  if (!rawValue)
    return fallback;
  try {
    const parsed = JSON.parse(rawValue);
    if (Array.isArray(parsed) && parsed.every((item) => typeof item === "string")) {
      return parsed;
    }
  } catch {}
  return fallback;
}
function parsePositiveNumber(envKey, fallback) {
  const rawValue = process.env[envKey];
  if (!rawValue)
    return fallback;
  const parsed = Number(rawValue);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : fallback;
}
function codexRunnerSetupHint(error) {
  if (error && typeof error === "object" && "code" in error) {
    const code = error.code;
    if (code === "ENOENT") {
      return "Install Codex CLI or set IMAGE_COCKPIT_CODEX_COMMAND to the full executable path.";
    }
    if (code === "EACCES" || code === "EPERM") {
      if (isWindowsAppsLaunchLikely()) {
        return "The resolved Codex command is the WindowsApps Codex Desktop executable, which may be blocked from subprocess launch. Use manual handoff, or set IMAGE_COCKPIT_CODEX_COMMAND to a terminal-runnable Codex CLI or wrapper.";
      }
      return "Check that the configured Codex command can run from this shell, or set IMAGE_COCKPIT_CODEX_COMMAND to a runnable executable path.";
    }
  }
  return "Set IMAGE_COCKPIT_CODEX_COMMAND to a runnable Codex CLI path, or set IMAGE_COCKPIT_CODEX_AUTORUN=0 for manual handoff.";
}
function resolveCommandCandidates(command) {
  const hasPathSeparator = command.includes("/") || command.includes("\\");
  const commandNames = commandHasExtension(command) ? [command] : commandExtensions().map((extension) => `${command}${extension}`);
  const dirs = hasPathSeparator ? [""] : (process.env.PATH ?? "").split(delimiter).filter(Boolean);
  const candidates = [];
  dirs.forEach((dir) => {
    commandNames.forEach((name) => {
      const candidate = hasPathSeparator ? resolve2(name) : join3(dir, name);
      if (existsSync(candidate))
        candidates.push(candidate);
    });
  });
  return Array.from(new Set([...candidates, ...knownCodexCliCandidates(command)]));
}
function selectCodexLaunchCommand(command, candidates) {
  if (command.includes("/") || command.includes("\\") || !isCodexCommandName(command))
    return command;
  return candidates.find(isLocalOpenAiCodexCliCommand) ?? candidates.find((candidate) => !isWindowsAppsCodexCommand(candidate)) ?? command;
}
function detectCodexRunnerMode(command, launchCommand, helpArgs, execArgs) {
  const modeProbe = [
    command,
    launchCommand,
    ...helpArgs,
    ...execArgs,
    process.env.IMAGE_COCKPIT_MOCK_RUNNER_DELAY_MS ? "IMAGE_COCKPIT_MOCK_RUNNER_DELAY_MS" : ""
  ].join(`
`).toLowerCase();
  if (modeProbe.includes("mock-codex-runner") || modeProbe.includes("review-mock-runner") || modeProbe.includes("image-cockpit-ui-smoke") || modeProbe.includes("image_cockpit_mock_runner_delay_ms")) {
    return "mock";
  }
  if (isCodexCommandName(basename2(command)) || isCodexCommandName(basename2(launchCommand)))
    return "codex";
  return "custom";
}
function isMockRunnerBlocked() {
  return codexRunnerMode === "mock" && !allowMockRunner;
}
function mockRunnerBlockedDiagnostic() {
  return {
    kind: "runner_failed",
    title: "Mock runner configured",
    userMessage: "A mock/test Codex runner is configured, so Image Cockpit did not start it as a real image generation backend.",
    suggestion: "Use a real Codex CLI command for imagegen, or set IMAGE_COCKPIT_CODEX_AUTORUN=0 for manual handoff. Set IMAGE_COCKPIT_ALLOW_MOCK_RUNNER=1 only for automated smoke tests."
  };
}
function knownCodexCliCandidates(command) {
  if (!isCodexCommandName(command))
    return [];
  const roots = [process.env.LOCALAPPDATA, process.env.USERPROFILE ? join3(process.env.USERPROFILE, "AppData", "Local") : ""].filter(Boolean).map((root) => join3(root, "OpenAI", "Codex", "bin"));
  const candidates = [];
  roots.forEach((root) => {
    if (!existsSync(root))
      return;
    try {
      readdirSync(root, { withFileTypes: true }).filter((entry) => entry.isDirectory()).forEach((entry) => {
        ["codex.exe", "codex"].forEach((file) => {
          const candidate = join3(root, entry.name, file);
          if (existsSync(candidate))
            candidates.push(candidate);
        });
      });
    } catch {}
  });
  return candidates;
}
function commandExtensions() {
  if (process.platform !== "win32")
    return [""];
  const pathExt = process.env.PATHEXT?.split(";").filter(Boolean) ?? [".COM", ".EXE", ".BAT", ".CMD"];
  return ["", ...pathExt.map((extension) => extension.toLowerCase())];
}
function commandHasExtension(command) {
  return Boolean(extname2(command));
}
function isCodexCommandName(command) {
  return /^codex(?:\.exe)?$/i.test(command);
}
function isLocalOpenAiCodexCliCommand(candidate) {
  return /[\\/]AppData[\\/]Local[\\/]OpenAI[\\/]Codex[\\/]bin[\\/][^\\/]+[\\/]codex(?:\.exe)?$/i.test(candidate);
}
function isWindowsAppsCodexCommand(candidate) {
  return /[\\/]WindowsApps[\\/]OpenAI\.Codex_/i.test(candidate) && /[\\/]codex(?:\.exe)?$/i.test(candidate);
}
function hasWindowsAppsCodexCandidate() {
  return codexCommandCandidates.some(isWindowsAppsCodexCommand);
}
function isWindowsAppsLaunchLikely() {
  const explicitLaunchCommandIsWindowsApps = isWindowsAppsCodexCommand(codexLaunchCommand);
  const bareCommandWouldResolveThroughWindowsApps = isCodexCommandName(codexLaunchCommand) && hasWindowsAppsCodexCandidate() && !codexCommandCandidates.some(isLocalOpenAiCodexCliCommand);
  return explicitLaunchCommandIsWindowsApps || bareCommandWouldResolveThroughWindowsApps;
}
var managedCancellationReasons = new Map;
var managedRecoveryModes = new Map;
function buildCodexRunnerPrompt(job, resumed = false) {
  return [
    "You are processing an Image Cockpit for Codex Workflows handoff job.",
    "",
    `Read this local JSON job file: ${job.path}`,
    ...managedRecoveryModes.get(job.id) === "process_existing" ? [
      "MANAGED RECOVERY: process the existing image artifacts only. Do not call imagegen or any image-generation tool, API, or provider.",
      "Use the original raw image in outbox/.staging. Preserve identity and finalize normalization, validation, manifests, and exports for THIS SAME job.",
      "If an essential source is absent, report that precise dependency and stop without generating another image."
    ] : [],
    ...resumed ? [
      "This is a resumable retry after the API process restarted. Inspect the exact job outbox and its .staging subtree before generating anything new.",
      "Reuse direction sheets and intermediate files that already belong to this job and pass the job contract. Continue only missing or invalid directions, then verify the full set and write the final manifest last.",
      "Do not restart completed direction generation merely because this is a fresh Codex process, and never borrow files from another candidate or job."
    ] : [],
    "If selectedImage.assetPath is populated, inspect that source image before editing it.",
    "If selectedImage.assetPath is empty, treat the job as prompt-only unless the job notes say otherwise.",
    "Trust the job JSON as the generation contract. Do not perform broad repository audits or read unrelated project docs/source files when the job prompt and jobNotes already provide the needed contract.",
    "On Windows, do not use System.Drawing for image inspection because it may be unavailable. Use Python/Pillow, PNG header bytes, or another local image tool instead.",
    "For workflowMode=image-generate, use the imagegen skill default built-in image generation path with built-in image_gen when available. Create a real raster image from the job prompt, never a procedural placeholder and never a procedural, SVG, canvas, diagram, geometric, or placeholder image.",
    "For workflowMode=image-generate, if built-in imagegen / image_gen is unavailable, write only a small blocked JSON sidecar into the outbox with reasonKind=imagegen_unavailable and do not create a fake image.",
    "For workflowMode=image-edit, inspect selectedImage.assetPath, use imagegen / built-in image_gen editing when available, follow numbered annotationContext region comments plus prompt/jobNotes, use imageRectNormalized/imageRectPixels when present, preserve the original canvas size/aspect ratio, keep the full character including head, hair, hands, equipment, and both feet visible, do not zoom, crop, or reframe into a portrait/detail shot, preserve transparency or use a flat chroma fallback, change only requested regions when possible, and return a real edited PNG or WebP with the job id filename prefix. Never create a procedural, SVG, canvas, diagram, geometric, or placeholder image.",
    "For workflowMode=sprite-generate, inspect selectedImage.assetPath, then use imagegen / built-in image_gen when available to create the requested sprite sheet assets from the source character image. Never create a procedural, SVG, canvas, diagram, geometric, or placeholder image.",
    "For workflowMode=sprite-generate, follow spriteContext.grid, spriteContext.cell, spriteContext.directions, spriteContext.variant, and spriteContext.chromaKey exactly. Keep one full-body character centered inside each strict cell with padding, no cropping, no duplicated heads, and no body parts crossing cells.",
    "For workflowMode=sprite-generate with spriteContext.variant=standard, return exactly one separate direction PNG/WebP image per entry in spriteContext.directions, using the direction name with spaces replaced by dashes as the filename suffix (full set: front, front-three-quarter, side, back-three-quarter, back). If spriteContext.directions is empty, return all five. Follow spriteContext.framesPerDirection exactly: 4 frames use 4x1, 6 frames use 3x2, 8 frames use 4x2, 12 frames use 4x3, 16 frames use 4x4, and 20 frames use 4x5, with spriteContext.cell dimensions and no gutters. Do not return only one combined multi-direction sheet, and do not return directions that were not requested.",
    "For workflowMode=sprite-generate with spriteContext.variant=standard, keep this candidate as one Codex job. In the first generation wave, submit one independent built-in image_gen tool call per requested direction in one assistant turn. A normal initial candidate requests 1, 3, or 5 directions, so its initial maximum internal fan-out is 1, 3, or 5; five requested directions means five concurrent image_gen calls inside this one candidate. A Direction Repair or Motion Pilot expansion may request any 1-5 direction subset, and its initial maximum internal fan-out equals that pending subset count. Do not spawn direction child jobs, extra Codex jobs, or extra agents.",
    "For every standard direction call, reserve a unique job-local staging destination under outbox/.staging/<job-id>/<direction-slug>/attempt-<n>/. Capture the exact path returned by that call and bind it one-to-one to that direction before copying or normalizing it. Never share one returned path between directions, identify output by a globally newest-file heuristic, or borrow another candidate's output.",
    "For standard direction attempts, maintain outbox/.staging/<job-id>/<job-id>-direction-attempts.json and copy a sanitized summary into the final manifest as directionAttempts keyed by canonical direction slug. Each attempt must record attempt number, status accepted/failed/timeout/capacity, and ISO-8601 start and finish timestamps. An accepted attempt must record its artifactPath in the exact job-relative form .staging/<job-id>/<direction-slug>/attempt-<n>/<file>; a failed/timeout/capacity attempt must instead use one failureKind from imagegen_capacity, imagegen_timeout, imagegen_failed, no_image_returned, policy_or_safety, quality_failed, normalization_failed, or unknown. Do not expose absolute paths, URI paths, or another job's staging path in the final manifest.",
    "As soon as one standard direction passes normalization and QA, mark it accepted and freeze its artifact. Never submit image_gen for it again and never overwrite or regenerate it. Retry only directions whose own attempt failed, timed out, or returned a capacity error; regenerating the complete requested direction set because one direction failed is prohibited.",
    "When multiple standard direction calls fail, time out, or hit capacity together, retry only those affected directions. Track a concurrency ceiling separately from the actual wave size: a normal initial request starts at ceiling 5, 3, or 1 for its 5, 3, or 1 directions, and a Direction Repair or Motion Pilot subset starts at its requested count from 1 through 5; each actual wave size is min(concurrency ceiling, pending direction count). On capacity, reduce ceiling 5 or 4 to 3, then reduce ceiling 3 or 2 to 1, while ceiling 1 stays at 1. Thus two pending failures after a five-direction wave run together under the reduced ceiling of 3, not as a new ceiling of 2. After the ceiling shrinks, leave pending directions beyond the current wave size for the next assistant-turn retry wave and never fan back out above the reduced ceiling. This fallback must remain inside the existing Codex candidate job and must not create more Codex jobs.",
    "Standard direction fan-out is runner-only. Do not create direction child-job UI, partial direction previews, or cross-job generation caches; same-job staging used for freeze and resumable retry is allowed.",
    "For workflowMode=sprite-generate with spriteContext.variant=standard, every populated cell in each direction image must be a distinct animation frame for spriteContext.action, not a repeated still pose. Static or nearly static rows are failed material even when the directions, padding, and chroma key are otherwise correct.",
    "For workflowMode=sprite-generate with spriteContext.motionRecipe.bodyTopology, use its topology-specific contact and anchor contract. Never invent humanoid feet for quadruped, serpentine/body-contact, floating, winged-flying, or multi-leg characters; evaluate paw contact, body contact, hover height, wing beat, or multi-contact support instead.",
    "For idle breathing, the feet must stay planted but most of the requested direction sheets must show readable frame-to-frame breathing or secondary motion: 2-4px shoulder/chest/head change plus hair, scarf, cape, cloth, or equipment follow-through. Regenerate any direction whose frames look nearly identical before writing the final manifest.",
    "For standard direction-split output, keep generated direction images, source manifests, contact sheets, comparison sheets, QA files, and all candidates under outbox/.staging/<job-id>/ or another non-root work folder while work is still in progress. Do not write, copy, or manifest root outbox <job-id>-*.png or <job-id>-manifest.json until the complete requested direction set is normalized, self-checked, and no further regeneration is planned. The final runner step must publish only the requested final direction PNG/WebP files into the root outbox and write the final <job-id>-manifest.json last. Image Cockpit will verify artifacts after the runner has finished and may rewrite the manifest after completion.",
    "For workflowMode=sprite-generate, inspect all cells before writing the final file and retry if any head is cut off, feet are missing, a head appears below feet, scale changes wildly, or the background is not flat chroma key.",
    "For workflowMode=sprite-generate with spriteContext.variant=hatch-pet, use the installed hatch-pet skill/scripts when available. Build a Codex pet atlas with 8 columns x 9 rows, 192x208 cells, 1536x1872 total, transparent unused cells, contact-sheet QA, and final spritesheet PNG/WebP returned with the job id filename prefix. Include pet.json as a sidecar if produced.",
    "For workflowMode=sprite-generate with spriteContext.variant=directional-hatch-pet, use the installed hatch-pet skill/scripts when available and return exactly five separate Codex pet atlas images: direction-01-front, direction-02-front-three-quarter, direction-03-side, direction-04-back-three-quarter, and direction-05-back. Each atlas must be 8 columns x 9 rows, 192x208 cells, 1536x1872 total, transparent unused cells, and use the job id filename prefix plus the direction suffix. Do not return only one giant combined sheet.",
    "For workflowMode=effect-animation, use imagegen / built-in image_gen when available to create one real transparent PNG game VFX animation sheet. Never create a procedural, SVG, canvas, diagram, geometric, or placeholder image.",
    "For workflowMode=effect-animation, follow effectContext exactly: frameCount, frameSize, layout columns/rows, sheetSize, loopMode, anchor, style, palette, category, type, and blendMode. The final PNG must use the job id filename prefix and preferably include '-effect-sheet' in the name.",
    "For workflowMode=effect-animation, do not bake checkerboard, solid matte backgrounds, preview backgrounds, UI panels, text, logos, watermarks, frame numbers, labels, arrows, or border guides into the image. Use real alpha transparency and no gutters.",
    "For workflowMode=effect-animation, every populated frame must show readable temporal progression. Reject and retry if frames are static copies, clipped at cell bounds, overlap neighboring cells, or do not match the requested sheet layout.",
    "For workflowMode=effect-animation, include a compact metadata JSON sidecar and a GIF preview when feasible, both using the same job id filename prefix. The PNG sheet is the required final artifact.",
    "Use jobNotes, annotationContext, spriteContext, and effectContext only when those fields are populated for the workflow.",
    "When using imagegen / built-in image_gen, isolate generated artifacts per job. Record the generated image path returned by the tool when available; otherwise record a before timestamp and only copy files created after that invocation. Do not blindly copy the newest file from CODEX_HOME/generated_images, because multiple tournament candidates may run in parallel. If you cannot confidently identify the image produced for this exact job and direction, regenerate that direction or return a blocker sidecar instead of mixing another candidate's image.",
    `Write final image result files only into this outbox directory: ${job.outboxDir}`,
    `Use this filename prefix for returned assets: ${job.id}`,
    "",
    "Important constraints:",
    "- The Image Cockpit app itself must not call OpenAI APIs directly.",
    "- Do not modify project source files, package files, docs, git metadata, or configuration.",
    "- Do not run git status, git diff, git clean, Remove-Item cleanup, or other repository/cleanup commands for this handoff job.",
    "- Do not write API keys, access tokens, model weights, or license-unclear assets.",
    "- If built-in imagegen / image_gen is unavailable in this Codex environment, write no procedural, SVG, canvas, diagram, geometric, or placeholder image.",
    "- Prefer PNG or WebP for still images. If you produce notes, write them as a small Markdown sidecar in the outbox. Do not place *-qa.json, work files, .tmp files, candidate-contact sheets, contact sheets, preview grids, AB galleries, or debug images in the outbox root.",
    "- If image generation/editing is blocked by safety, policy, or unavailable imagegen capability, do not create a placeholder image.",
    "- Write a small JSON blocker sidecar only, using this schema:",
    '{ "status": "blocked", "reasonKind": "policy_or_safety" | "imagegen_unavailable" | "unknown", "userMessage": "A short user-safe reason.", "suggestion": "A short retry suggestion." }',
    "- Do not include hidden policy text, internal traces, API keys, access tokens, local absolute paths, or the full prompt in the blocker sidecar.",
    "",
    "When finished, make sure all required usable image files are present in the outbox if generation/editing succeeded."
  ].join(`
`);
}
async function writeRunnerStatus(status) {
  runnerStatuses.set(status.jobId, status);
  const enrichedStatus = await enrichRunnerStatus(status);
  runnerStatuses.set(enrichedStatus.jobId, enrichedStatus);
  rememberRunnerCapacityBlock(enrichedStatus);
  const statusPath = enrichedStatus.statusPath ?? join3(statusDir, `${enrichedStatus.jobId}.json`);
  await writeFile3(statusPath, JSON.stringify(enrichedStatus, null, 2), "utf8");
}
async function getRunnerCapacityBlock() {
  if (isRunnerCapacityBlockActive(cachedRunnerCapacityBlock))
    return cachedRunnerCapacityBlock;
  cachedRunnerCapacityBlock = null;
  if (runnerCapacityCooldownMs <= 0)
    return null;
  let latestBlock = null;
  try {
    const names = (await readdir(statusDir)).filter((name) => name.endsWith(".json"));
    await Promise.all(names.map(async (name) => {
      try {
        const status = parseJsonText(await readFile2(join3(statusDir, name), "utf8"));
        const block = runnerCapacityBlockFromStatus(status);
        if (!block)
          return;
        if (!latestBlock || Date.parse(block.seenAt) > Date.parse(latestBlock.seenAt))
          latestBlock = block;
      } catch {}
    }));
  } catch {
    return null;
  }
  cachedRunnerCapacityBlock = latestBlock;
  return latestBlock;
}
function rememberRunnerCapacityBlock(status) {
  const block = runnerCapacityBlockFromStatus(status);
  if (block) {
    cachedRunnerCapacityBlock = block;
    return;
  }
  if (status.state === "completed" && !status.diagnostic)
    cachedRunnerCapacityBlock = null;
}
function runnerCapacityBlockFromStatus(status) {
  if (runnerCapacityCooldownMs <= 0 || status.diagnostic?.kind !== "usage_limit")
    return null;
  const seenAt = status.finishedAt ?? status.startedAt ?? new Date().toISOString();
  const seenAtMs = Date.parse(seenAt);
  if (!Number.isFinite(seenAtMs))
    return null;
  const block = {
    kind: "usage_limit",
    title: status.diagnostic.title,
    userMessage: status.diagnostic.userMessage,
    suggestion: status.diagnostic.suggestion,
    jobId: status.jobId,
    seenAt,
    expiresAt: new Date(seenAtMs + runnerCapacityCooldownMs).toISOString()
  };
  return isRunnerCapacityBlockActive(block) ? block : null;
}
function isRunnerCapacityBlockActive(block) {
  return Boolean(block && Date.parse(block.expiresAt) > Date.now());
}
function diagnosticFromRunnerCapacityBlock(block) {
  return {
    kind: block.kind,
    title: block.title,
    userMessage: block.userMessage,
    suggestion: block.suggestion
  };
}
async function cancelCodexRunner(jobId, reason = "tournament") {
  const currentStatus = await getRunnerStatus(jobId);
  const child = runnerProcesses.get(jobId);
  if (!child) {
    return {
      ok: false,
      jobId,
      status: currentStatus,
      message: currentStatus.state === "running" ? "Runner process is not tracked by this server instance." : "Runner is not running."
    };
  }
  const finishedAt = new Date().toISOString();
  cancellingRunnerJobIds.add(jobId);
  managedCancellationReasons.set(jobId, reason);
  const cancelledStatus = {
    ...currentStatus,
    state: "failed",
    cancellationPending: true,
    message: cancellationMessage(reason, false),
    cancelled: true,
    cancellationReason: reason,
    finishedAt,
    exitCode: null,
    signal: "SIGTERM"
  };
  await writeRunnerStatus(cancelledStatus);
  const closeConfirmation = new Promise((resolveClose) => {
    if (child.exitCode !== null || child.signalCode !== null) {
      resolveClose(true);
      return;
    }
    let settled = false;
    const finish = (confirmed) => {
      if (settled)
        return;
      settled = true;
      resolveClose(confirmed);
    };
    child.once("close", () => finish(true));
    setTimeout(() => finish(false), 5500);
  });
  child.kill("SIGTERM");
  setTimeout(() => {
    if (runnerProcesses.get(jobId) === child)
      child.kill("SIGKILL");
  }, 5000);
  const stopped = await closeConfirmation;
  const terminalStatus = await getRunnerStatus(jobId).catch(() => cancelledStatus);
  return {
    ok: stopped,
    jobId,
    status: terminalStatus,
    ...!stopped ? { message: "Runner termination is still unconfirmed after SIGTERM and SIGKILL requests." } : {}
  };
}
async function activeCodexRunnerCount() {
  const statusNames = await readdir(statusDir).catch(() => []);
  const persistedActiveJobIds = (await Promise.all(statusNames.filter((name) => name.endsWith(".json")).map(async (name) => {
    const jobId = name.slice(0, -5);
    if (!isSafeJobId(jobId))
      return null;
    try {
      const status = parseJsonText(await readFile2(join3(statusDir, name), "utf8"));
      return status.state === "running" || status.cancellationPending ? jobId : null;
    } catch {
      return null;
    }
  }))).filter((jobId) => Boolean(jobId));
  const statuses = await Promise.all(persistedActiveJobIds.map((jobId) => getRunnerStatus(jobId).catch(() => null)));
  const activeJobIds = new Set(statuses.filter((status) => status?.state === "running" || status?.cancellationPending).map((status) => status?.jobId).filter((jobId) => Boolean(jobId)));
  runnerProcesses.forEach((_child, jobId) => activeJobIds.add(jobId));
  resumingRunnerJobs.forEach((_resume, jobId) => activeJobIds.add(jobId));
  cancellingRunnerJobIds.forEach((jobId) => activeJobIds.add(jobId));
  return activeJobIds.size;
}
async function assertCodexRunnerSlotAvailable() {
  const activeCount = await activeCodexRunnerCount();
  if (activeCount >= maxActiveCodexJobs) {
    throw new HttpError(409, `Codex runner slots are full (${activeCount}/${maxActiveCodexJobs}). Wait for a job to finish before starting another.`, "runner_slots_full", {
      requiredSlots: 1,
      capacity: {
        limit: maxActiveCodexJobs,
        active: activeCount,
        available: Math.max(0, maxActiveCodexJobs - activeCount)
      },
      retryable: true
    });
  }
}
async function assertAnimationInitialBundleAdmissionAvailable(requiredSlots) {
  const activeCount = await activeCodexRunnerCount();
  const availableSlots = Math.max(0, maxActiveCodexJobs - activeCount);
  if (activeCount > 0 || requiredSlots > availableSlots) {
    throw new HttpError(409, `Animation generation requires an idle ${maxActiveCodexJobs}-slot runner pool before its ${requiredSlots} initial candidate${requiredSlots === 1 ? "" : "s"} can start.`, "insufficient_runner_slots", {
      requiredSlots,
      capacity: {
        limit: maxActiveCodexJobs,
        active: activeCount,
        available: availableSlots
      },
      retryable: true
    });
  }
}
async function getRunnerStatus(jobId) {
  const liveStatus = runnerStatuses.get(jobId);
  if (liveStatus)
    return enrichRunnerStatus(await normalizeRunningStatus(liveStatus));
  const statusPath = join3(statusDir, `${jobId}.json`);
  try {
    return enrichRunnerStatus(await normalizeRunningStatus(parseJsonText(await readFile2(statusPath, "utf8"))));
  } catch {
    return enrichRunnerStatus({
      jobId,
      state: "unknown",
      message: "No runner status has been recorded for this job.",
      statusPath
    });
  }
}
async function normalizeRunningStatus(status) {
  if (status.cancellationPending) {
    const reconciledCancellation = await persistedTournamentCancellationStatus(status);
    if (reconciledCancellation) {
      await writeRunnerStatus(reconciledCancellation);
      return reconciledCancellation;
    }
  }
  if (status.state !== "running")
    return status;
  if (!runnerProcesses.has(status.jobId)) {
    const acceptedLoserStatus = await persistedTournamentCancellationStatus(status);
    if (acceptedLoserStatus) {
      await writeRunnerStatus(acceptedLoserStatus);
      return acceptedLoserStatus;
    }
    const resultDir = await resolveJobOutboxDir(status.jobId);
    if (resultDir) {
      const artifact = await inspectDirectionSplitArtifact(status.jobId, resultDir).catch(() => null);
      if (artifact?.ready && artifact.verified) {
        const recoveredStatus = {
          ...status,
          state: "completed",
          message: "Recovered a verified artifact after the API process restarted.",
          finishedAt: new Date().toISOString(),
          exitCode: null
        };
        await writeRunnerStatus(recoveredStatus);
        return recoveredStatus;
      }
    }
    const interrupted = interruptedRunnerStatus(status);
    if (interrupted !== status)
      await writeRunnerStatus(interrupted);
    return interrupted;
  }
  const stale = await isRunnerStatusStale(status);
  if (!stale)
    return status;
  if (await hasOutboxImageForJob(status.jobId, status.outboxDir ?? outboxDir)) {
    return {
      ...status,
      state: "completed",
      message: "Codex returned an image before runner status could be finalized.",
      finishedAt: new Date().toISOString(),
      exitCode: null
    };
  }
  return {
    ...status,
    state: "failed",
    message: "Codex runner timed out after log output stopped; no outbox result was returned.",
    finishedAt: new Date().toISOString(),
    exitCode: null
  };
}
async function persistedTournamentCancellationStatus(status) {
  try {
    const job = parseJsonText(await readFile2(join3(inboxDir, `${status.jobId}.json`), "utf8"));
    const tournamentId = typeof job.tournament?.id === "string" ? job.tournament.id : "";
    if (!isSafeTournamentId(tournamentId))
      return null;
    const manifest = await readAnimationTournamentManifest(tournamentId);
    const candidate = manifest?.candidates.find((item) => item.jobId === status.jobId);
    const isAcceptedLoser = manifest?.state === "accepted" && Boolean(manifest.winnerCandidateId) && manifest.winnerCandidateId !== status.jobId && candidate?.state === "cancelled";
    const isCancelledTournamentJob = manifest?.state === "cancelled" && candidate?.state === "cancelled";
    if (!isAcceptedLoser && !isCancelledTournamentJob)
      return null;
    const termination = await terminatePersistedRunnerProcess(status);
    return {
      ...status,
      state: "failed",
      message: termination.stopped ? "Codex runner cancellation was recovered from the animation tournament manifest." : `Codex runner restart was blocked by the animation tournament manifest, but process termination was not confirmed: ${termination.message}`,
      cancellationPending: termination.stopped ? undefined : true,
      finishedAt: termination.stopped ? new Date().toISOString() : undefined,
      exitCode: null,
      signal: termination.stopped ? "SIGTERM" : undefined
    };
  } catch {
    return null;
  }
}
async function terminatePersistedRunnerProcess(status) {
  const processId = status.processId;
  if (!Number.isSafeInteger(processId) || !processId || processId <= 0 || processId === process.pid) {
    return { stopped: false, message: "No safe persisted runner process id was available." };
  }
  if (process.platform !== "win32") {
    return { stopped: false, message: "Persisted runner process termination is currently verified only on Windows." };
  }
  const lookup = await readWindowsRunnerProcessIdentity(processId);
  if (!lookup.checked)
    return { stopped: false, message: lookup.message };
  if (!lookup.identity)
    return { stopped: true, message: "The persisted runner process had already exited." };
  if (!persistedRunnerIdentityMatches(status, lookup.identity)) {
    return {
      stopped: false,
      message: "The persisted process id no longer matched the original runner executable and start time."
    };
  }
  const windowsRoot = process.env.SystemRoot ?? process.env.WINDIR ?? "C:\\Windows";
  const taskkillPath = join3(windowsRoot, "System32", "taskkill.exe");
  const killed = await runHiddenCommand(taskkillPath, ["/PID", String(processId), "/T", "/F"], 5000);
  const afterKill = await readWindowsRunnerProcessIdentity(processId);
  if (afterKill.checked && !afterKill.identity) {
    return {
      stopped: true,
      message: killed.exitCode === 0 ? "The persisted runner process tree was terminated." : "The persisted runner process exited while cancellation was being reconciled."
    };
  }
  if (!afterKill.checked || !afterKill.identity || !persistedRunnerIdentityMatches(status, afterKill.identity)) {
    return {
      stopped: false,
      message: afterKill.checked ? "The runner process identity changed while cancellation was being reconciled." : afterKill.message
    };
  }
  try {
    process.kill(processId, "SIGTERM");
    const afterNodeKill = await readWindowsRunnerProcessIdentity(processId);
    if (afterNodeKill.checked && !afterNodeKill.identity) {
      return {
        stopped: true,
        message: "The verified persisted runner process was terminated after process-tree termination was unavailable."
      };
    }
    return {
      stopped: false,
      message: afterNodeKill.checked ? "The verified runner process was still present after the fallback termination request." : afterNodeKill.message
    };
  } catch (error) {
    const nodeTerminationError = error instanceof Error ? error.message : "Node process termination failed.";
    return {
      stopped: false,
      message: `${killed.error || killed.stderr.trim() || `taskkill exited with code ${killed.exitCode ?? "unknown"}`}; ${nodeTerminationError}`
    };
  }
}
function persistedRunnerIdentityMatches(status, identity) {
  const expectedCommand = status.command ?? "";
  const expectedStartedAt = status.startedAt ? Date.parse(status.startedAt) : NaN;
  const actualStartedAt = Date.parse(identity.startedAt);
  const executableMatches = Boolean(expectedCommand && identity.executablePath && resolve2(expectedCommand).toLowerCase() === resolve2(identity.executablePath).toLowerCase());
  const startTimeMatches = Number.isFinite(expectedStartedAt) && Number.isFinite(actualStartedAt) && Math.abs(actualStartedAt - expectedStartedAt) <= 5000;
  return executableMatches && startTimeMatches;
}
async function readWindowsRunnerProcessIdentity(processId) {
  const windowsRoot = process.env.SystemRoot ?? process.env.WINDIR ?? "C:\\Windows";
  const powershellPath = join3(windowsRoot, "System32", "WindowsPowerShell", "v1.0", "powershell.exe");
  const script = [
    "$process = $null",
    `try { $process = [System.Diagnostics.Process]::GetProcessById(${processId}) } catch [System.ArgumentException] { [pscustomobject]@{ found = $false } | ConvertTo-Json -Compress; exit 0 } catch { [Console]::Error.WriteLine($_.Exception.Message); exit 2 }`,
    "$executablePath = ''",
    "$startedAt = ''",
    "try { $executablePath = [string]$process.MainModule.FileName; $startedAt = $process.StartTime.ToUniversalTime().ToString('o') } catch { [Console]::Error.WriteLine($_.Exception.Message); exit 3 }",
    "[pscustomobject]@{ found = $true; processId = [int]$process.Id; executablePath = $executablePath; startedAt = $startedAt } | ConvertTo-Json -Compress",
    "exit 0"
  ].join("; ");
  const result = await runHiddenCommand(powershellPath, ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script], 5000);
  if (result.exitCode !== 0) {
    return {
      checked: false,
      identity: null,
      message: result.error || result.stderr.trim() || `Could not inspect the persisted runner process (exit ${result.exitCode ?? "unknown"}).`
    };
  }
  try {
    if (!result.stdout.trim())
      throw new Error("Process lookup returned no identity response.");
    const parsed = JSON.parse(result.stdout);
    if (parsed.found === false) {
      return { checked: true, identity: null, message: "The persisted runner process was not found." };
    }
    if (parsed.found !== true || parsed.processId !== processId || typeof parsed.executablePath !== "string" || typeof parsed.startedAt !== "string") {
      throw new Error("Process identity fields were incomplete.");
    }
    return {
      checked: true,
      identity: {
        processId,
        executablePath: parsed.executablePath,
        startedAt: parsed.startedAt
      },
      message: "Persisted runner process identity matched the lookup response."
    };
  } catch (error) {
    return {
      checked: false,
      identity: null,
      message: `Could not parse the persisted runner process identity: ${error instanceof Error ? error.message : "unknown error"}`
    };
  }
}
function runHiddenCommand(command, args, timeoutMs) {
  return new Promise((resolveResult) => {
    let settled = false;
    let stdout = "";
    let stderr = "";
    let timeoutId;
    const finish = (result) => {
      if (settled)
        return;
      settled = true;
      if (timeoutId)
        clearTimeout(timeoutId);
      resolveResult({ ...result, stdout, stderr });
    };
    try {
      const child = spawn(command, args, {
        cwd: process.cwd(),
        stdio: ["ignore", "pipe", "pipe"],
        windowsHide: true
      });
      child.stdout?.on("data", (chunk) => {
        stdout = `${stdout}${chunk.toString("utf8")}`.slice(-16384);
      });
      child.stderr?.on("data", (chunk) => {
        stderr = `${stderr}${chunk.toString("utf8")}`.slice(-16384);
      });
      child.on("error", (error) => {
        finish({ exitCode: null, error: error.message });
      });
      child.on("close", (exitCode) => {
        finish({ exitCode });
      });
      timeoutId = setTimeout(() => {
        child.kill("SIGTERM");
        finish({ exitCode: null, timedOut: true, error: `Command timed out after ${timeoutMs}ms.` });
      }, timeoutMs);
    } catch (error) {
      finish({
        exitCode: null,
        error: error instanceof Error ? error.message : "Could not start the process inspection command."
      });
    }
  });
}
async function isRunnerStatusStale(status) {
  if (runnerStaleTimeoutMs <= 0)
    return false;
  const startedAtMs = status.startedAt ? Date.parse(status.startedAt) : NaN;
  if (!Number.isFinite(startedAtMs))
    return false;
  const now = Date.now();
  if (now - startedAtMs < runnerStaleTimeoutMs)
    return false;
  if (!status.logPath)
    return true;
  try {
    const logStats = await stat2(status.logPath);
    return now - logStats.mtimeMs >= runnerStaleLogIdleMs;
  } catch {
    return true;
  }
}
async function isRunnerStatusActivelyRunning(jobId) {
  let status = runnerStatuses.get(jobId);
  if (!status) {
    try {
      status = parseJsonText(await readFile2(join3(statusDir, `${jobId}.json`), "utf8"));
    } catch {
      return false;
    }
  }
  if (status.state !== "running")
    return false;
  return !await isRunnerStatusStale(status);
}
async function enrichRunnerStatus(status) {
  if ("cancelled" in status && status.cancelled) {
    const clean = { ...status };
    delete clean.diagnostic;
    return clean;
  }
  if ("resumeRequired" in status && status.resumeRequired)
    return status;
  if (!shouldBuildDiagnostic(status))
    return status;
  const diagnostic = await getJobDiagnostic(status);
  if (diagnostic)
    return { ...status, diagnostic };
  const statusWithoutDiagnostic = { ...status };
  delete statusWithoutDiagnostic.diagnostic;
  return statusWithoutDiagnostic;
}
function shouldBuildDiagnostic(status) {
  return status.state === "failed" || status.state === "unavailable" || status.state === "completed" || status.state === "unknown";
}
async function getJobDiagnostic(status) {
  const resultDir = status.outboxDir ?? outboxDir;
  const [sidecar, logText, hasReturnedImage, directionSplitArtifact] = await Promise.all([
    findJobSidecar(status.jobId, resultDir),
    readShortFile(status.logPath),
    hasOutboxImageForJob(status.jobId, resultDir),
    inspectDirectionSplitArtifact(status.jobId, resultDir)
  ]);
  if (status.state === "completed" && (hasReturnedImage || directionSplitArtifact.detected))
    return null;
  const sidecarKind = normalizeFailureKind(sidecar?.reasonKind);
  const diagnosticLogTail = logText.split(/\r?\n/).slice(-12).join(`
`);
  const combinedText = [status.message, sidecar?.text, diagnosticLogTail].filter(Boolean).join(`
`).toLowerCase();
  const kind = sidecarKind ?? classifyFailureKind(status, combinedText, hasReturnedImage);
  if (!kind)
    return null;
  return {
    ...diagnosticForKind(kind),
    sidecarPath: sidecar?.path,
    logPath: status.logPath
  };
}
function classifyFailureKind(status, text, hasReturnedImage) {
  if (status.state === "completed" && !hasReturnedImage)
    return "no_image_returned";
  if (!hasReturnedImage && matchesAny(text, ["stale", "timed out", "no outbox result", "without returning an outbox image", "no returned image was found"])) {
    return "no_image_returned";
  }
  if (matchesAny(text, ["usage limit", "you've hit your usage limit", "try again at"])) {
    return "usage_limit";
  }
  if (matchesAny(text, ["selected model is at capacity", "model is at capacity", "model capacity is currently unavailable"])) {
    return "runner_failed";
  }
  if (matchesAny(text, [
    "imagegen unavailable",
    "imagegen is unavailable",
    "imagegen / image_gen is unavailable",
    "image generation unavailable",
    "image generation is not available",
    "built-in image generation is not available",
    "built-in image_gen unavailable",
    "built-in image_gen is unavailable",
    "tool unavailable",
    "image_gen unavailable",
    "image_gen is unavailable",
    "unavailable imagegen capability"
  ])) {
    return "imagegen_unavailable";
  }
  if (matchesAny(text, ["policy", "safety", "content policy", "disallowed", "not allowed", "blocked", "moderation", "cannot comply", "can't help"])) {
    return "policy_or_safety";
  }
  if (status.state === "failed" || status.state === "unavailable")
    return "runner_failed";
  if (status.state === "unknown")
    return "unknown";
  return null;
}
function diagnosticForKind(kind) {
  if (kind === "policy_or_safety") {
    return {
      kind,
      title: "Generation failed",
      userMessage: "The image could not be generated. It may have been blocked by safety or usage-policy checks.",
      suggestion: "Revise the prompt to remove sensitive, explicit, or disallowed details, then try again."
    };
  }
  if (kind === "imagegen_unavailable") {
    return {
      kind,
      title: "Image generation unavailable",
      userMessage: "Image generation is not available in this Codex environment.",
      suggestion: "Use manual handoff or another local provider, then return an image to the outbox."
    };
  }
  if (kind === "usage_limit") {
    return {
      kind,
      title: "Codex usage limit reached",
      userMessage: "Codex could not run this generation because the configured account hit its usage limit.",
      suggestion: "Wait for the usage window to reset, upgrade the account, or switch to a runner/provider with available capacity."
    };
  }
  if (kind === "runner_failed") {
    return {
      kind,
      title: "Codex runner failed",
      userMessage: "Codex runner stopped before returning an image.",
      suggestion: "Check the runner setup or retry the job after adjusting the prompt."
    };
  }
  if (kind === "no_image_returned") {
    return {
      kind,
      title: "No image returned",
      userMessage: "Codex runner finished or stopped, but no returned image was found.",
      suggestion: "Retry the job, or place a returned image with the job id prefix in the outbox."
    };
  }
  return {
    kind,
    title: "Generation failed",
    userMessage: "The image could not be generated, and no specific reason was returned.",
    suggestion: "Retry with a simpler prompt or use manual handoff."
  };
}
async function findJobSidecar(jobId, resultDir = outboxDir) {
  try {
    const entries = await readdir(resultDir, { withFileTypes: true });
    const candidates = entries.filter((entry) => entry.isFile()).filter((entry) => isJobOutboxFileName(jobId, entry.name)).filter((entry) => !shouldIgnoreOutboxResultName(entry.name)).filter((entry) => !isDirectionSplitManifestFileName(entry.name)).filter((entry) => [".json", ".md", ".txt"].includes(extname2(entry.name).toLowerCase())).sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of candidates) {
      const path = join3(resultDir, entry.name);
      const text = await readShortFile(path);
      if (!text)
        continue;
      return {
        path,
        text,
        reasonKind: readSidecarReasonKind(entry.name, text)
      };
    }
  } catch {}
  return null;
}
function readSidecarReasonKind(name, text) {
  if (extname2(name).toLowerCase() !== ".json")
    return;
  try {
    const parsed = JSON.parse(text);
    return typeof parsed.reasonKind === "string" ? parsed.reasonKind : typeof parsed.kind === "string" ? parsed.kind : undefined;
  } catch {
    return;
  }
}
async function hasOutboxImageForJob(jobId, resultDir = outboxDir) {
  try {
    const entries = await readdir(resultDir, { withFileTypes: true });
    const directionSplitArtifact = await inspectDirectionSplitArtifact(jobId, resultDir);
    if (directionSplitArtifact.detected) {
      const classification = directionSplitArtifact.qualityGate?.classification;
      return directionSplitArtifact.ready || directionSplitArtifact.quality === "bronze" || classification === "quality-failed" || classification === "quarantined-candidate";
    }
    const genericImageReturned = entries.filter((entry) => entry.isFile()).map((entry) => entry.name).some((name) => isGenericImageOutboxFileName(jobId, name));
    if (genericImageReturned)
      return true;
    return false;
  } catch {
    return false;
  }
}
function isGenericImageOutboxFileName(jobId, name) {
  return isJobOutboxFileName(jobId, name) && !shouldIgnoreOutboxResultName(name) && Boolean(mimeTypeForImage(name)) && !isDirectionSplitDirectionFileName(name, jobId);
}
function isJobOutboxFileName(jobId, name) {
  return name.startsWith(`${jobId}-`) || name.startsWith(`${jobId}.`);
}
function isDirectionSplitDirectionFileName(name, jobId, expectedSlug) {
  if (!mimeTypeForImage(name))
    return false;
  const normalized = name.toLowerCase().replace(/\.[^.]+$/, "").replace(jobId.toLowerCase(), "").replace(/[_\s]+/g, "-").replace(/^-+/, "");
  return expectedSlug ? normalized === expectedSlug : directionSplitSlugs.some((slug) => normalized === slug);
}
function directionSplitJobIdFromFileName(name) {
  const lowerName = name.toLowerCase();
  if (lowerName.endsWith("-manifest.json")) {
    const jobId2 = name.replace(/-manifest\.json$/i, "");
    return isSafeJobId(jobId2) ? jobId2 : null;
  }
  const baseName = name.replace(/\.[^.]+$/, "");
  const matchedSlug = directionSplitSlugs.slice().sort((left, right) => right.length - left.length).find((slug) => baseName.toLowerCase().endsWith(`-${slug}`));
  if (!matchedSlug)
    return null;
  const jobId = baseName.slice(0, -(matchedSlug.length + 1));
  return isSafeJobId(jobId) ? jobId : null;
}
async function inspectAllDirectionSplitArtifacts(resultDir = outboxDir) {
  const jobIds = new Set;
  try {
    const entries = await readdir(resultDir, { withFileTypes: true });
    entries.filter((entry) => entry.isFile()).map((entry) => directionSplitJobIdFromFileName(entry.name)).filter((jobId) => Boolean(jobId)).forEach((jobId) => jobIds.add(jobId));
  } catch {}
  try {
    const stagingEntries = await readdir(join3(resultDir, ".staging"), { withFileTypes: true });
    stagingEntries.filter((entry) => entry.isDirectory()).map((entry) => entry.name).filter(isSafeJobId).forEach((jobId) => jobIds.add(jobId));
  } catch {}
  const statuses = await Promise.all(Array.from(jobIds).map((jobId) => inspectDirectionSplitArtifact(jobId, resultDir)));
  return new Map(statuses.filter((status) => status.detected).map((status) => [status.jobId, status]));
}
async function inspectDirectionSplitArtifact(jobId, resultDir = outboxDir) {
  const missingDirections = [];
  const warnings = [];
  const files = [];
  const bySlug = new Map;
  let sourceManifest = null;
  const expectedSpriteContext = await readJobExpectedSpriteContext(jobId);
  const expectedSlugs = expectedSpriteContext.directionSlugs ?? directionSplitSlugs;
  for (const slug of expectedSlugs) {
    const candidate = await findDirectionSplitCandidateFile(jobId, slug, resultDir);
    if (candidate) {
      bySlug.set(slug, candidate);
      files.push(candidate.finalName);
    } else {
      missingDirections.push(directionNameForSlug(slug));
    }
  }
  sourceManifest = await findDirectionSplitSourceManifest(jobId, resultDir);
  const detected = bySlug.size > 0 || Boolean(sourceManifest);
  if (!detected)
    return emptyDirectionSplitArtifactStatus(jobId);
  const candidateCount = bySlug.size + (sourceManifest ? 1 : 0);
  const expectedChromaKey = expectedSpriteContext.chromaKey;
  const expectedAction = expectedSpriteContext.action ?? normalizeActionValue(sourceManifest?.parsed?.action);
  const expectedFramesPerDirection = expectedSpriteContext.framesPerDirection ?? normalizeMotionFrameCount(sourceManifest?.parsed?.framesPerDirection) ?? expectedSpriteContext.motionRecipe?.frameCount ?? 8;
  const manifestChromaKey = normalizeChromaKeyValue(readManifestChromaKey(sourceManifest?.parsed));
  const manifestQualityGate = sourceManifest ? qualityGateFromManifest(sourceManifest.parsed) : null;
  const animationQuality = sourceManifest ? animationQualityFromManifest(sourceManifest.parsed) : undefined;
  const newestCandidateMtimeMs = Math.max(0, ...Array.from(bySlug.values()).map((candidate) => candidate.mtimeMs));
  const manifestQualityGateIsStale = sourceManifest && manifestQualityGate && manifestQualityGate.classification !== "usable-final" && newestCandidateMtimeMs > manifestQualityGateRecordedMtimeMs(sourceManifest) + 50;
  const manifestQualityGateWasSuperseded = manifestQualityGate && manifestQualityGate.classification !== "usable-final" && isSupersededSoftClientQualityGate(manifestQualityGate);
  const chromaWarning = expectedChromaKey && manifestChromaKey && expectedChromaKey !== manifestChromaKey ? `manifest chroma key ${manifestChromaKey} differs from pending job chroma key ${expectedChromaKey}` : undefined;
  if (chromaWarning)
    warnings.push(chromaWarning);
  if (manifestQualityGateIsStale) {
    warnings.push("stale manifest quality gate was ignored because newer direction image candidates were detected");
  }
  if (manifestQualityGateWasSuperseded) {
    warnings.push("soft client bbox variation quality gate was rechecked under the current animation QA policy");
  }
  if (manifestQualityGate && !manifestQualityGateIsStale && !manifestQualityGateWasSuperseded && manifestQualityGate.classification !== "usable-final") {
    return {
      jobId,
      artifactKind: "direction-split",
      detected: true,
      ready: false,
      verified: false,
      quality: manifestQualityGate.classification === "quarantined-candidate" ? "bronze" : "blocked",
      reason: manifestQualityGate.reason,
      missingDirections,
      warnings,
      files,
      manifestName: sourceManifest?.name,
      stable: false,
      candidateCount,
      animationQuality,
      qualityGate: manifestQualityGate,
      chromaKey: {
        expected: expectedChromaKey,
        manifest: manifestChromaKey,
        warning: chromaWarning
      }
    };
  }
  if (missingDirections.length > 0) {
    const reason2 = `missing ${missingDirections.join(", ")}`;
    return {
      jobId,
      artifactKind: "direction-split",
      detected: true,
      ready: false,
      verified: false,
      quality: "waiting",
      reason: reason2,
      missingDirections,
      warnings,
      files,
      manifestName: sourceManifest?.name,
      stable: false,
      candidateCount,
      animationQuality,
      qualityGate: makeQualityGate("running", reason2, "direction-split-missing-directions", false, false, true, warnings),
      chromaKey: {
        expected: expectedChromaKey,
        manifest: manifestChromaKey,
        warning: chromaWarning
      }
    };
  }
  const candidates = expectedSlugs.map((slug) => bySlug.get(slug)).filter((candidate) => Boolean(candidate));
  const newestMtimeMs = Math.max(...candidates.map((candidate) => candidate.mtimeMs));
  const stable = artifactStableMs <= 0 || Date.now() - newestMtimeMs >= artifactStableMs;
  if (!stable) {
    const reason2 = "waiting for stable verified artifacts";
    return {
      jobId,
      artifactKind: "direction-split",
      detected: true,
      ready: false,
      verified: false,
      quality: "waiting",
      reason: reason2,
      missingDirections,
      warnings,
      files,
      manifestName: sourceManifest?.name,
      stable,
      candidateCount,
      animationQuality,
      qualityGate: makeQualityGate("running", reason2, "direction-split-waiting-stable", false, false, true, warnings),
      chromaKey: {
        expected: expectedChromaKey,
        manifest: manifestChromaKey,
        warning: chromaWarning
      }
    };
  }
  const imageInfos = await Promise.all(candidates.map((candidate) => readImageDimensions(candidate.path).then((dimensions) => ({ candidate, dimensions, error: "" }), (error) => ({ candidate, dimensions: null, error: error instanceof Error ? error.message : "image decode failed" }))));
  const decodeFailures = imageInfos.filter((info) => info.error);
  if (decodeFailures.length > 0) {
    const reason2 = `raw direction candidate needs review: ${decodeFailures.map((info) => `${info.candidate.slug} ${info.error}`).join("; ")}`;
    return {
      jobId,
      artifactKind: "direction-split",
      detected: true,
      ready: false,
      verified: false,
      quality: "bronze",
      reason: reason2,
      missingDirections,
      warnings,
      files,
      manifestName: sourceManifest?.name,
      stable,
      candidateCount,
      animationQuality,
      qualityGate: makeQualityGate("quarantined-candidate", reason2, "raw-direction-decode-failed", false, false, true, warnings),
      chromaKey: {
        expected: expectedChromaKey,
        manifest: manifestChromaKey,
        warning: chromaWarning
      }
    };
  }
  const firstDimensions = imageInfos[0]?.dimensions;
  if (firstDimensions) {
    const mismatched = imageInfos.filter((info) => info.dimensions && (info.dimensions.width !== firstDimensions.width || info.dimensions.height !== firstDimensions.height));
    if (mismatched.length > 0) {
      warnings.push(`direction image dimensions differ: ${mismatched.map((info) => `${info.candidate.slug} ${info.dimensions?.width}x${info.dimensions?.height}`).join(", ")}`);
    }
  }
  const manifestOrderWarning = sourceManifest && sourceManifest.mtimeMs < Math.max(...candidates.map((candidate) => candidate.mtimeMs)) ? "source manifest was older than one or more direction images; server manifest was regenerated" : undefined;
  if (manifestOrderWarning)
    warnings.push(manifestOrderWarning);
  if (await isRunnerStatusActivelyRunning(jobId)) {
    const reason2 = "waiting for Codex runner to finish finalizing direction artifacts";
    return {
      jobId,
      artifactKind: "direction-split",
      detected: true,
      ready: false,
      verified: false,
      quality: "waiting",
      reason: reason2,
      missingDirections,
      warnings,
      files,
      manifestName: sourceManifest?.name,
      stable: true,
      candidateCount,
      animationQuality,
      qualityGate: makeQualityGate("running", reason2, "direction-split-runner-finalizing", false, false, true, warnings),
      chromaKey: {
        expected: expectedChromaKey,
        manifest: manifestChromaKey,
        warning: chromaWarning
      }
    };
  }
  const manifestName = await publishVerifiedDirectionSplitArtifact(jobId, candidates, sourceManifest, expectedChromaKey, expectedAction, expectedSpriteContext.motionRecipe, expectedFramesPerDirection, warnings, resultDir, expectedSlugs);
  const reason = warnings.length > 0 ? "server verified with warnings" : "server verified";
  return {
    jobId,
    artifactKind: "direction-split",
    detected: true,
    ready: true,
    verified: true,
    quality: warnings.length > 0 ? "silver" : "gold",
    reason,
    missingDirections,
    warnings,
    files: expectedSlugs.map((slug) => `${jobId}-${slug}${extname2(bySlug.get(slug)?.finalName ?? ".png") || ".png"}`),
    manifestName,
    stable: true,
    candidateCount,
    animationQuality,
    qualityGate: makeQualityGate("usable-final", reason, "direction-split-server-verified", true, true, false, warnings),
    chromaKey: {
      expected: expectedChromaKey,
      manifest: manifestChromaKey,
      warning: chromaWarning
    }
  };
}
function emptyDirectionSplitArtifactStatus(jobId) {
  return {
    jobId,
    artifactKind: "direction-split",
    detected: false,
    ready: false,
    verified: false,
    quality: "waiting",
    reason: "no direction split candidate",
    missingDirections: directionSplitNames.slice(),
    warnings: [],
    files: [],
    stable: false,
    candidateCount: 0,
    qualityGate: makeQualityGate("running", "no direction split candidate", "direction-split-not-detected", false, false, true)
  };
}
async function findDirectionSplitCandidateFile(jobId, slug, resultDir = outboxDir) {
  const candidateNames = directionSplitCandidateNames(jobId, slug);
  const stagingDir = join3(resultDir, ".staging", jobId);
  const candidates = [];
  for (const candidateName of candidateNames.outbox) {
    const candidate = await statDirectionSplitCandidate(join3(resultDir, candidateName), slug, candidateName, candidateName, false);
    if (candidate)
      candidates.push(candidate);
  }
  for (const candidateName of candidateNames.staging) {
    const finalName = `${jobId}-${slug}${extname2(candidateName) || ".png"}`;
    const candidate = await statDirectionSplitCandidate(join3(stagingDir, candidateName), slug, candidateName, finalName, true);
    if (candidate)
      candidates.push(candidate);
  }
  return candidates.sort(sortDirectionSplitCandidates)[0] ?? null;
}
function sortDirectionSplitCandidates(left, right) {
  const timeDifference = right.mtimeMs - left.mtimeMs;
  if (Math.abs(timeDifference) > 1)
    return timeDifference;
  if (left.fromStaging !== right.fromStaging)
    return left.fromStaging ? -1 : 1;
  return left.name.localeCompare(right.name);
}
function directionSplitCandidateNames(jobId, slug) {
  const extensions = [".png", ".webp", ".jpg", ".jpeg"];
  const directionIndex = directionSplitSlugs.indexOf(slug) + 1;
  return {
    outbox: extensions.map((extension) => `${jobId}-${slug}${extension}`),
    staging: extensions.flatMap((extension) => [
      `${jobId}-${slug}${extension}`,
      `${slug}${extension}`,
      `direction-${String(directionIndex).padStart(2, "0")}-${slug}${extension}`,
      `direction-${directionIndex}-${slug}${extension}`
    ])
  };
}
async function statDirectionSplitCandidate(path, slug, name, finalName, fromStaging) {
  try {
    const fileStat = await stat2(path);
    if (!fileStat.isFile())
      return null;
    return {
      slug,
      name,
      finalName,
      path,
      size: fileStat.size,
      mtimeMs: fileStat.mtimeMs,
      modifiedAt: fileStat.mtime.toISOString(),
      fromStaging
    };
  } catch {
    return null;
  }
}
async function findDirectionSplitSourceManifest(jobId, resultDir = outboxDir) {
  const candidates = [
    { path: join3(resultDir, `${jobId}-manifest.json`), name: `${jobId}-manifest.json`, fromStaging: false },
    { path: join3(resultDir, ".staging", jobId, `${jobId}-manifest.json`), name: `${jobId}-manifest.json`, fromStaging: true },
    { path: join3(resultDir, ".staging", jobId, "manifest.json"), name: "manifest.json", fromStaging: true }
  ];
  const loaded = [];
  for (const candidate of candidates) {
    try {
      const [text, fileStat] = await Promise.all([readFile2(candidate.path, "utf8"), stat2(candidate.path)]);
      const parsed = JSON.parse(text);
      if (!isDirectionSplitSourceManifestObject(parsed))
        continue;
      loaded.push({
        ...candidate,
        parsed,
        mtimeMs: fileStat.mtimeMs
      });
    } catch {}
  }
  return loaded.sort(sortDirectionSplitManifests)[0] ?? null;
}
function isDirectionSplitSourceManifestObject(value) {
  return value.schema === directionSplitManifestSchema;
}
function sortDirectionSplitManifests(left, right) {
  const timeDifference = right.mtimeMs - left.mtimeMs;
  if (Math.abs(timeDifference) > 1)
    return timeDifference;
  if (left.fromStaging !== right.fromStaging)
    return left.fromStaging ? 1 : -1;
  return left.name.localeCompare(right.name);
}
async function publishVerifiedDirectionSplitArtifact(jobId, candidates, sourceManifest, expectedChromaKey, expectedAction, expectedMotionRecipe, expectedFramesPerDirection, warnings, targetDir = outboxDir, expectedSlugs = directionSplitSlugs) {
  await mkdir2(targetDir, { recursive: true });
  for (const candidate of candidates) {
    const targetPath = join3(targetDir, candidate.finalName);
    if (resolve2(candidate.path) !== resolve2(targetPath))
      await copyFile(candidate.path, targetPath);
  }
  const manifestName = `${jobId}-manifest.json`;
  const manifestPath = join3(targetDir, manifestName);
  const sourceManifestVerified = sourceManifest?.parsed?.serverVerified === true;
  const normalizedDirectionAttempts = readManifestDirectionAttempts(sourceManifest?.parsed, jobId, expectedSlugs);
  const directionAttemptsAreCurrent = JSON.stringify(sourceManifest?.parsed?.directionAttempts ?? null) === JSON.stringify(normalizedDirectionAttempts ?? null);
  const manifestIsCurrent = sourceManifest && !sourceManifest.fromStaging && sourceManifest.name === manifestName && resolve2(sourceManifest.path) === resolve2(manifestPath) && sourceManifestVerified && directionAttemptsAreCurrent && sourceManifest.mtimeMs >= Math.max(...candidates.map((candidate) => candidate.mtimeMs));
  if (!manifestIsCurrent) {
    const serverManifest = {
      schema: directionSplitManifestSchema,
      jobId,
      serverVerified: true,
      verifiedAt: new Date().toISOString(),
      quality: warnings.length > 0 ? "silver" : "gold",
      qualityGate: makeQualityGate("usable-final", warnings.length > 0 ? "server verified with warnings" : "server verified", "direction-split-server-verified", true, true, false, warnings),
      warnings,
      directions: expectedSlugs.map(directionNameForSlug),
      action: expectedAction,
      motionRecipe: expectedMotionRecipe ?? readManifestMotionRecipe(sourceManifest?.parsed),
      framesPerDirection: expectedFramesPerDirection,
      grid: directionSplitGridForFrameCount(expectedFramesPerDirection),
      files: Object.fromEntries(expectedSlugs.map((slug, index) => [directionNameForSlug(slug), `${jobId}-${slug}${extname2(candidates[index]?.finalName ?? ".png") || ".png"}`])),
      chromaKey: expectedChromaKey ? { name: expectedChromaKey } : undefined,
      directionAttempts: normalizedDirectionAttempts,
      animationQuality: sourceManifest ? animationQualityFromManifest(sourceManifest.parsed) : undefined,
      animationQualityRecordedAt: sourceManifest && typeof sourceManifest.parsed.animationQualityRecordedAt === "string" ? sourceManifest.parsed.animationQualityRecordedAt : undefined,
      sourceManifest: sourceManifest ? {
        name: sourceManifest.name,
        fromStaging: sourceManifest.fromStaging,
        serverRewritten: true
      } : undefined
    };
    await writeFile3(manifestPath, JSON.stringify(serverManifest, null, 2), "utf8");
  }
  return manifestName;
}
function readManifestDirectionAttempts(manifest, jobId, expectedSlugs) {
  const source = manifest?.directionAttempts;
  if (!source || typeof source !== "object" || Array.isArray(source))
    return;
  const attemptsByDirection = {};
  const allowedStatuses = new Set(["accepted", "failed", "timeout", "capacity"]);
  for (const slug of expectedSlugs) {
    const rawAttempts = source[slug];
    if (!Array.isArray(rawAttempts))
      continue;
    const attempts = rawAttempts.slice(0, 32).flatMap((rawAttempt, index) => {
      if (!rawAttempt || typeof rawAttempt !== "object" || Array.isArray(rawAttempt))
        return [];
      const attempt = rawAttempt;
      const status = typeof attempt.status === "string" && allowedStatuses.has(attempt.status) ? attempt.status : undefined;
      if (!status)
        return [];
      const attemptNumber = normalizeBoundedInteger(attempt.attempt, index + 1, 1, 999);
      const startedAt = normalizeDirectionAttemptTimestamp(attempt.startedAt);
      const finishedAt = normalizeDirectionAttemptTimestamp(attempt.finishedAt);
      if (!startedAt || !finishedAt || Date.parse(finishedAt) < Date.parse(startedAt))
        return [];
      const failureKind = normalizeDirectionAttemptFailureKind(attempt.failureKind);
      const artifactPath = normalizeDirectionAttemptArtifactPath(attempt.artifactPath, jobId, slug, attemptNumber);
      if (status === "accepted" ? !artifactPath : !failureKind)
        return [];
      const normalized = {
        attempt: attemptNumber,
        status,
        startedAt,
        finishedAt
      };
      if (status === "accepted")
        normalized.artifactPath = artifactPath;
      else
        normalized.failureKind = failureKind;
      return [normalized];
    });
    if (attempts.length > 0)
      attemptsByDirection[slug] = attempts;
  }
  return Object.keys(attemptsByDirection).length > 0 ? attemptsByDirection : undefined;
}
function normalizeDirectionAttemptTimestamp(value) {
  if (typeof value !== "string" || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,3})?(?:Z|[+-]\d{2}:\d{2})$/.test(value)) {
    return;
  }
  const timestamp = Date.parse(value);
  return Number.isFinite(timestamp) ? new Date(timestamp).toISOString() : undefined;
}
function normalizeDirectionAttemptFailureKind(value) {
  const allowedFailureKinds = new Set([
    "imagegen_capacity",
    "imagegen_timeout",
    "imagegen_failed",
    "no_image_returned",
    "policy_or_safety",
    "quality_failed",
    "normalization_failed",
    "unknown"
  ]);
  return typeof value === "string" && allowedFailureKinds.has(value) ? value : undefined;
}
function normalizeDirectionAttemptArtifactPath(value, jobId, slug, attemptNumber) {
  if (typeof value !== "string")
    return;
  const normalized = value.trim().replace(/\\/g, "/");
  const segments = normalized.split("/");
  if (!normalized || normalized.length > 320 || normalized.startsWith("/") || normalized.includes(":") || /^[a-z][a-z0-9+.-]*:/i.test(normalized) || segments.some((segment) => !segment || segment === "." || segment === "..") || segments.length < 5 || segments[0] !== ".staging" || segments[1] !== jobId || segments[2] !== slug || segments[3] !== `attempt-${attemptNumber}`) {
    return;
  }
  return normalized;
}
async function readJobExpectedSpriteContext(jobId) {
  try {
    const text = await readFile2(join3(inboxDir, `${jobId}.json`), "utf8");
    const parsed = JSON.parse(text);
    return {
      action: normalizeActionValue(parsed.spriteContext?.action),
      chromaKey: normalizeChromaKeyValue(parsed.spriteContext?.chromaKey),
      directionSlugs: normalizeDirectionSlugsValue(parsed.spriteContext?.directions),
      motionRecipe: readManifestMotionRecipeValue(parsed.spriteContext?.motionRecipe),
      framesPerDirection: normalizeMotionFrameCount(parsed.spriteContext?.framesPerDirection)
    };
  } catch {
    return {};
  }
}
function directionSlugForName(name) {
  return name.trim().toLowerCase().replace(/[_\s]+/g, "-");
}
function directionNameForSlug(slug) {
  const index = directionSplitSlugs.indexOf(slug);
  return index >= 0 ? directionSplitNames[index] : slug;
}
function normalizeDirectionSlugsValue(value) {
  if (!Array.isArray(value))
    return;
  const requested = value.filter((item) => typeof item === "string").map(directionSlugForName);
  const ordered = directionSplitSlugs.filter((slug) => requested.includes(slug));
  return ordered.length > 0 && ordered.length < directionSplitSlugs.length ? ordered : undefined;
}
function normalizeActionValue(value) {
  if (typeof value !== "string")
    return;
  const normalized = value.trim().toLowerCase();
  return normalized || undefined;
}
function readManifestChromaKey(manifest) {
  if (!manifest)
    return;
  const chromaKey = manifest.chromaKey;
  if (typeof chromaKey === "string")
    return chromaKey;
  if (chromaKey && typeof chromaKey === "object") {
    const value = chromaKey;
    return value.name ?? value.color ?? value.hex;
  }
  const image = manifest.image;
  if (image && typeof image === "object") {
    const value = image;
    return value.background ?? value.chromaKey;
  }
  return manifest.background;
}
function normalizeChromaKeyValue(value) {
  if (typeof value !== "string")
    return;
  const normalized = value.trim().toLowerCase();
  if (!normalized)
    return;
  if (normalized.includes("magenta") || normalized.includes("#ff00ff") || normalized.includes("255,0,255"))
    return "magenta";
  if (normalized.includes("green") || normalized.includes("#00ff00") || normalized.includes("0,255,0"))
    return "green";
  return;
}
async function readImageDimensions(path) {
  const bytes = await readFile2(path);
  if (bytes.length >= 24 && bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) {
    return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20), format: "png" };
  }
  if (bytes.length >= 10 && bytes.subarray(0, 3).toString("ascii") === "GIF") {
    return { width: bytes.readUInt16LE(6), height: bytes.readUInt16LE(8), format: "gif" };
  }
  if (bytes.length >= 30 && bytes.subarray(0, 4).toString("ascii") === "RIFF" && bytes.subarray(8, 12).toString("ascii") === "WEBP") {
    const chunk = bytes.subarray(12, 16).toString("ascii");
    if (chunk === "VP8X") {
      const width = 1 + bytes.readUIntLE(24, 3);
      const height = 1 + bytes.readUIntLE(27, 3);
      return { width, height, format: "webp" };
    }
    if (chunk === "VP8 " && bytes.length >= 30) {
      return { width: bytes.readUInt16LE(26) & 16383, height: bytes.readUInt16LE(28) & 16383, format: "webp" };
    }
    if (chunk === "VP8L" && bytes.length >= 25) {
      const bits = bytes.readUInt32LE(21);
      return { width: (bits & 16383) + 1, height: (bits >> 14 & 16383) + 1, format: "webp" };
    }
  }
  if (bytes.length >= 4 && bytes[0] === 255 && bytes[1] === 216) {
    let offset = 2;
    while (offset < bytes.length - 9) {
      if (bytes[offset] !== 255) {
        offset += 1;
        continue;
      }
      const marker = bytes[offset + 1];
      const length = bytes.readUInt16BE(offset + 2);
      if (marker >= 192 && marker <= 195) {
        return { width: bytes.readUInt16BE(offset + 7), height: bytes.readUInt16BE(offset + 5), format: "jpeg" };
      }
      offset += 2 + length;
    }
  }
  throw new Error("unsupported or unreadable image header");
}
async function readShortFile(path) {
  if (!path)
    return "";
  try {
    return (await readFile2(path, "utf8")).slice(0, 32768);
  } catch {
    return "";
  }
}
async function readRunnerLogTail(jobId, requestedBytes) {
  const logPath = join3(logsDir, `${jobId}.log`);
  const bytesToRead = clampRunnerLogBytes(requestedBytes);
  const readAt = new Date().toISOString();
  try {
    const logStats = await stat2(logPath);
    const fileSize = logStats.size;
    const readLength = Math.min(bytesToRead, fileSize);
    const start = Math.max(0, fileSize - readLength);
    const buffer = Buffer.alloc(readLength);
    const fd = openSync(logPath, "r");
    try {
      readSync(fd, buffer, 0, readLength, start);
    } finally {
      closeSync(fd);
    }
    return {
      jobId,
      exists: true,
      path: logPath,
      size: fileSize,
      modifiedAt: logStats.mtime.toISOString(),
      readAt,
      truncated: start > 0,
      text: sanitizeRunnerLogText(buffer.toString("utf8"))
    };
  } catch {
    return {
      jobId,
      exists: false,
      path: logPath,
      size: 0,
      modifiedAt: "",
      readAt,
      truncated: false,
      text: ""
    };
  }
}
function clampRunnerLogBytes(value) {
  if (!Number.isFinite(value) || value <= 0)
    return runnerLogTailDefaultBytes;
  return Math.min(Math.max(Math.floor(value), 1024), runnerLogTailMaxBytes);
}
function sanitizeRunnerLogText(text) {
  const userProfile = process.env.USERPROFILE ?? "";
  const shortenedRoots = [
    [userProfile, "~"],
    [handoffRoot, "<handoff>"]
  ];
  let sanitized = text.replace(/\u001b\[[0-9;]*m/g, "");
  for (const [from, to] of shortenedRoots) {
    if (!from)
      continue;
    sanitized = sanitized.split(from).join(to);
  }
  sanitized = sanitized.split(/\r?\n/).slice(-240).map((line) => {
    const cleanLine = line.replace(/[\x00-\x08\x0b\x0c\x0e-\x1f]/g, "");
    return cleanLine.length > 720 ? `${cleanLine.slice(0, 720)} ...[truncated]` : cleanLine;
  }).join(`
`);
  return sanitized.trimEnd();
}
function normalizeFailureKind(value) {
  if (value === "policy_or_safety" || value === "imagegen_unavailable" || value === "runner_failed" || value === "no_image_returned" || value === "unknown") {
    return value;
  }
  return null;
}
function matchesAny(value, markers) {
  return markers.some((marker) => value.includes(marker));
}
function makeQualityGate(classification, reason, code = classification, historyAllowed = classification === "usable-final", downloadAllowed = classification === "usable-final", retryable = classification !== "usable-final", warnings) {
  return {
    classification,
    reason,
    code,
    historyAllowed,
    downloadAllowed,
    retryable,
    warnings: warnings && warnings.length > 0 ? warnings : undefined
  };
}
function qualityGateFromManifest(manifest) {
  const explicitGate = manifest.qualityGate;
  if (explicitGate && typeof explicitGate === "object") {
    const gate = explicitGate;
    const classification = normalizeQualityClassification(gate.classification);
    if (classification) {
      return makeQualityGate(classification, typeof gate.reason === "string" && gate.reason.trim() ? gate.reason : qualityGateDefaultReason(classification), typeof gate.code === "string" ? gate.code : `manifest-${classification}`, gate.historyAllowed === true, gate.downloadAllowed === true, gate.retryable !== false, Array.isArray(gate.warnings) ? gate.warnings.filter((warning) => typeof warning === "string") : undefined);
    }
  }
  const quality = normalizeQualityClassification(manifest.classification ?? manifest.quality ?? manifest.status);
  if (!quality || quality === "usable-final" || quality === "running")
    return null;
  return makeQualityGate(quality, qualityGateDefaultReason(quality), `manifest-${quality}`, false, false, quality !== "debug-artifact");
}
function readManifestMotionRecipe(manifest) {
  return readManifestMotionRecipeValue(manifest?.motionRecipe);
}
function readManifestMotionRecipeValue(value) {
  if (!value || typeof value !== "object" || Array.isArray(value))
    return;
  const record = value;
  const id = normalizeShortText(record.id);
  if (!id)
    return;
  const modifiers = record.modifiers && typeof record.modifiers === "object" && !Array.isArray(record.modifiers) ? Object.fromEntries(Object.entries(record.modifiers).filter((entry) => typeof entry[1] === "string").slice(0, 12)) : undefined;
  return {
    id,
    version: normalizePositiveInteger(record.version) ?? 1,
    compilerVersion: normalizeShortText(record.compilerVersion) ?? "unknown",
    qualityProfile: normalizeShortText(record.qualityProfile),
    bodyTopology: normalizeShortText(record.bodyTopology),
    frameCount: normalizeMotionFrameCount(record.frameCount),
    modifiers,
    experimental: record.experimental === true || undefined
  };
}
async function resumeUntrackedCodexRunner(status) {
  const existing = resumingRunnerJobs.get(status.jobId);
  if (existing)
    return existing;
  const resumePromise = (async () => {
    const path = join3(inboxDir, `${status.jobId}.json`);
    try {
      const job = parseJsonText(await readFile2(path, "utf8"));
      const resolvedOutboxDir = resolveSafeOutboxSubdir(job.returnTo?.outboxDir) ?? status.outboxDir ?? outboxDir;
      const nextResumeCount = Math.max(0, status.resumeCount ?? 0) + 1;
      return startCodexRunner({
        id: status.jobId,
        createdAt: job.createdAt ?? status.initialStartedAt ?? status.startedAt ?? new Date().toISOString(),
        path,
        outboxDir: resolvedOutboxDir
      }, {
        initialStartedAt: status.initialStartedAt ?? status.startedAt,
        resumeCount: nextResumeCount
      });
    } catch (error) {
      const failedStatus = {
        ...status,
        state: "failed",
        message: `Could not resume the untracked Codex runner: ${error instanceof Error ? error.message : "unknown error"}`,
        finishedAt: new Date().toISOString(),
        exitCode: null
      };
      await writeRunnerStatus(failedStatus);
      return failedStatus;
    }
  })();
  resumingRunnerJobs.set(status.jobId, resumePromise);
  try {
    return await resumePromise;
  } finally {
    resumingRunnerJobs.delete(status.jobId);
  }
}
function animationQualityFromRequest(body2) {
  return normalizeAnimationQualityReport(body2.report);
}
function animationQualityFromManifest(manifest) {
  return normalizeAnimationQualityReport(manifest.animationQuality) ?? undefined;
}
function normalizeAnimationQualityReport(value) {
  if (!value || typeof value !== "object" || Array.isArray(value))
    return null;
  const report = value;
  if (report.metricVersion !== "image-cockpit.animation-quality.v2" || report.policyVersion !== "shadow-v1")
    return null;
  if (report.hardGateUnchanged !== true)
    return null;
  try {
    const serialized = JSON.stringify(report);
    if (serialized.length > 512000)
      return null;
    return JSON.parse(serialized);
  } catch {
    return null;
  }
}
function manifestQualityGateRecordedMtimeMs(sourceManifest) {
  const recordedAt = sourceManifest.parsed.qualityGateRecordedAt;
  const recordedTime = typeof recordedAt === "string" ? Date.parse(recordedAt) : NaN;
  return Number.isFinite(recordedTime) ? recordedTime : sourceManifest.mtimeMs;
}
function isSupersededSoftClientQualityGate(qualityGate) {
  if (qualityGate.code !== "client-quality-gate-failed")
    return false;
  const reason = qualityGate.reason.toLowerCase();
  if (!/\bbbox (?:width|height) variation\b/.test(reason))
    return false;
  return !/chroma key removal failed|transparency damage|detached component|blank normalized cell|no primary character|no character pixels|feet touch|top margin|expected \d+x\d+, got|missing direction|could not|decode failed/i.test(reason);
}
function qualityGateFromRequest(body2) {
  const requested = normalizeQualityClassification(body2.classification);
  const classification = requested && requested !== "usable-final" && requested !== "running" ? requested : "quality-failed";
  const reason = typeof body2.reason === "string" && body2.reason.trim() ? trimQualityGateText(body2.reason, 600) : qualityGateDefaultReason(classification);
  const code = typeof body2.code === "string" && body2.code.trim() ? trimQualityGateText(body2.code, 120) : `client-${classification}`;
  const warnings = Array.isArray(body2.warnings) ? body2.warnings.filter((warning) => typeof warning === "string" && warning.trim().length > 0).map((warning) => trimQualityGateText(warning, 220)).slice(0, 12) : undefined;
  return makeQualityGate(classification, reason, code, false, false, classification !== "debug-artifact", warnings);
}
function trimQualityGateText(value, maxLength) {
  return value.replace(/[\x00-\x08\x0b\x0c\x0e-\x1f]/g, " ").replace(/\s+/g, " ").trim().slice(0, maxLength);
}
async function writeDirectionSplitQualityGate(jobId, qualityGate) {
  const resultDir = await resolveJobOutboxDir(jobId) ?? outboxDir;
  const manifestPaths = [
    join3(resultDir, `${jobId}-manifest.json`),
    join3(resultDir, ".staging", jobId, `${jobId}-manifest.json`),
    join3(resultDir, ".staging", jobId, "manifest.json")
  ];
  const written = [];
  const existingManifests = await Promise.all(manifestPaths.map(async (manifestPath) => ({
    manifestPath,
    manifest: await readManifestObject(manifestPath)
  })));
  const targets = existingManifests.some((item) => item.manifest) ? existingManifests.filter((item) => item.manifest) : [{ manifestPath: manifestPaths[0], manifest: { schema: directionSplitManifestSchema, jobId } }];
  for (const target of targets) {
    const manifestPath = target.manifestPath;
    const manifest = target.manifest ?? { schema: directionSplitManifestSchema, jobId };
    manifest.schema = typeof manifest.schema === "string" ? manifest.schema : directionSplitManifestSchema;
    manifest.jobId = typeof manifest.jobId === "string" ? manifest.jobId : jobId;
    manifest.classification = qualityGate.classification;
    manifest.quality = qualityGate.classification === "quality-failed" || qualityGate.classification === "failed" ? "blocked" : "bronze";
    manifest.serverVerified = false;
    manifest.qualityGate = qualityGate;
    manifest.qualityGateRecordedAt = new Date().toISOString();
    await mkdir2(resolve2(manifestPath, ".."), { recursive: true });
    await writeFile3(manifestPath, `${JSON.stringify(manifest, null, 2)}
`, "utf8");
    written.push(manifestPath);
  }
  return { ok: true, jobId, qualityGate, written };
}
async function writeDirectionSplitAnimationQuality(jobId, animationQuality) {
  const resultDir = await resolveJobOutboxDir(jobId) ?? outboxDir;
  const manifestPaths = [...new Set([
    join3(resultDir, `${jobId}-manifest.json`),
    join3(resultDir, ".staging", jobId, `${jobId}-manifest.json`),
    join3(resultDir, ".staging", jobId, "manifest.json"),
    join3(outboxDir, `${jobId}-manifest.json`)
  ])];
  const written = [];
  const recordedAt = new Date().toISOString();
  const existingManifests = await Promise.all(manifestPaths.map(async (manifestPath) => ({
    manifestPath,
    manifest: await readManifestObject(manifestPath)
  })));
  const targets = existingManifests.some((item) => item.manifest) ? existingManifests.filter((item) => item.manifest) : [{ manifestPath: manifestPaths[0], manifest: { schema: directionSplitManifestSchema, jobId } }];
  for (const target of targets) {
    const manifestPath = target.manifestPath;
    const manifest = target.manifest ?? { schema: directionSplitManifestSchema, jobId };
    manifest.schema = typeof manifest.schema === "string" ? manifest.schema : directionSplitManifestSchema;
    manifest.jobId = typeof manifest.jobId === "string" ? manifest.jobId : jobId;
    manifest.animationQuality = animationQuality;
    manifest.animationQualityRecordedAt = recordedAt;
    await mkdir2(resolve2(manifestPath, ".."), { recursive: true });
    await writeFile3(manifestPath, `${JSON.stringify(manifest, null, 2)}
`, "utf8");
    written.push(manifestPath);
  }
  return { ok: true, jobId, animationQuality, written };
}
async function readManifestObject(path) {
  try {
    await stat2(path);
    const parsed = JSON.parse(await readFile2(path, "utf8"));
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return null;
  }
}
function normalizeQualityClassification(value) {
  if (typeof value !== "string")
    return null;
  const normalized = value.trim().toLowerCase().replace(/[_\s]+/g, "-");
  if (normalized === "usable-final" || normalized === "gold" || normalized === "silver" || normalized === "verified")
    return "usable-final";
  if (normalized === "quality-failed" || normalized === "failed" || normalized === "blocked" || normalized === "rejected")
    return "quality-failed";
  if (normalized === "quarantined-candidate" || normalized === "quarantine" || normalized === "quarantined" || normalized === "bronze")
    return "quarantined-candidate";
  if (normalized === "debug-artifact" || normalized === "debug")
    return "debug-artifact";
  if (normalized === "running" || normalized === "waiting" || normalized === "pending")
    return "running";
  return null;
}
function qualityGateDefaultReason(classification) {
  if (classification === "quality-failed")
    return "Animation result failed the material quality gate.";
  if (classification === "quarantined-candidate")
    return "Candidate was quarantined and is not a final usable result.";
  if (classification === "debug-artifact")
    return "Debug or QA artifact is not a final usable result.";
  if (classification === "failed")
    return "Generation failed before producing a usable final result.";
  if (classification === "running")
    return "Generation is still waiting for final verified artifacts.";
  return "Usable final result.";
}
async function listOutboxResults(resultDir = outboxDir) {
  const artifactStatuses = await inspectAllDirectionSplitArtifacts(resultDir);
  const entries = await readdir(resultDir, { withFileTypes: true });
  const results = await Promise.all(entries.filter((entry) => entry.isFile()).filter((entry) => !shouldIgnoreOutboxResultName(entry.name)).map(async (entry) => {
    const mimeType = mimeTypeForOutboxResult(entry.name);
    if (!mimeType)
      return null;
    const filePath = join3(resultDir, entry.name);
    const fileStat = await stat2(filePath);
    const artifact = artifactStatuses.get(directionSplitJobIdFromFileName(entry.name) ?? "");
    return {
      name: entry.name,
      path: filePath,
      size: fileStat.size,
      modifiedAt: fileStat.mtime.toISOString(),
      mimeType,
      qualityGate: qualityGateForOutboxResultName(entry.name, artifact),
      artifact
    };
  }));
  return results.filter((result) => Boolean(result)).sort((a, b) => Date.parse(b.modifiedAt) - Date.parse(a.modifiedAt));
}
function qualityGateForOutboxResultName(name, artifact) {
  const filterName = normalizeOutboxResultNameForFiltering(name);
  if (filterName.includes("bronze-candidate")) {
    return makeQualityGate("quarantined-candidate", "Bronze candidate is available for diagnostics only and is not a final usable result.", "bronze-candidate", false, false, true);
  }
  if (artifact?.qualityGate)
    return artifact.qualityGate;
  if (directionSplitJobIdFromFileName(name) && !isDirectionSplitManifestFileName(name)) {
    return makeQualityGate("quarantined-candidate", "Raw direction image is a component candidate; import the verified direction-split manifest to create the final sheet.", "raw-direction-component", false, false, true);
  }
  return makeQualityGate("usable-final", "Usable final result.", "usable-final", true, true, false);
}
function resolveTournamentIdForJobRequest(workflowMode, body2) {
  const spriteVariant = body2.spriteVariant ?? "standard";
  return workflowMode === "sprite-generate" && spriteVariant === "standard" && isSafeTournamentId(body2.tournamentId) ? body2.tournamentId : "";
}
function hasTournamentScopedJobFields(body2) {
  return body2.tournamentId !== undefined || body2.tournamentCandidateIndex !== undefined || body2.tournamentCandidateCount !== undefined || body2.repairDirections !== undefined || body2.repairOfJobId !== undefined;
}
function normalizeCandidateIndex(value) {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? Math.floor(value) : 0;
}
function normalizeCandidateCount(value) {
  return typeof value === "number" && Number.isFinite(value) && value > 0 ? Math.floor(value) : 1;
}
function tournamentJobOutboxDir(tournamentId, jobId) {
  return join3(tournamentWorkRootDir, tournamentId, jobId);
}
async function resolveJobOutboxDir(jobId) {
  const liveStatus = runnerStatuses.get(jobId);
  const liveOutbox = resolveSafeOutboxSubdir(liveStatus?.outboxDir);
  if (liveOutbox)
    return liveOutbox;
  try {
    const parsed = parseJsonText(await readFile2(join3(inboxDir, `${jobId}.json`), "utf8"));
    return resolveSafeOutboxSubdir(parsed.returnTo?.outboxDir) ?? outboxDir;
  } catch {
    return null;
  }
}
function resolveSafeOutboxSubdir(value) {
  if (typeof value !== "string" || !value.trim())
    return null;
  const rootDir = resolve2(outboxDir);
  const resolved = resolve2(value);
  const root = rootDir.endsWith(sep2) ? rootDir : `${rootDir}${sep2}`;
  return resolved === rootDir || resolved.startsWith(root) ? resolved : null;
}
async function readRunnerStatusSnapshot(jobId) {
  const liveStatus = runnerStatuses.get(jobId);
  if (liveStatus)
    return liveStatus;
  const statusPath = join3(statusDir, `${jobId}.json`);
  try {
    return parseJsonText(await readFile2(statusPath, "utf8"));
  } catch {
    return {
      jobId,
      state: "unknown",
      message: "No runner status has been recorded for this job.",
      statusPath
    };
  }
}
async function cancelUntrackedRunnerStatus(status, stoppedMessage, pendingMessage) {
  const termination = await terminatePersistedRunnerProcess(status);
  const cancelledStatus = {
    ...status,
    state: "failed",
    message: termination.stopped ? stoppedMessage : `${pendingMessage}, but process termination was not confirmed: ${termination.message}`,
    cancellationPending: termination.stopped ? undefined : true,
    finishedAt: termination.stopped ? new Date().toISOString() : undefined,
    exitCode: null,
    signal: termination.stopped ? "SIGTERM" : undefined
  };
  await writeRunnerStatus(cancelledStatus);
  return {
    jobId: status.jobId,
    ok: termination.stopped,
    status: cancelledStatus,
    message: termination.stopped ? termination.message : `Untracked running status was blocked from resuming; ${termination.message}`
  };
}
async function cancelTournamentLoserRunners(manifest, winnerJobId) {
  const cancellationResults = [];
  for (const candidate of manifest.candidates) {
    if (!candidate.jobId || candidate.jobId === winnerJobId)
      continue;
    const status = await readRunnerStatusSnapshot(candidate.jobId);
    if (status.state !== "running" && !status.cancellationPending)
      continue;
    if (runnerProcesses.has(candidate.jobId)) {
      const result2 = await cancelCodexRunner(candidate.jobId);
      cancellationResults.push({
        jobId: candidate.jobId,
        ok: result2.ok,
        message: result2.message
      });
      continue;
    }
    const result = await cancelUntrackedRunnerStatus(status, "Codex runner cancellation persisted after an animation tournament winner was chosen.", "Codex runner restart was blocked after an animation tournament winner was chosen");
    cancellationResults.push({ jobId: result.jobId, ok: result.ok, message: result.message });
  }
  return cancellationResults;
}
function mergeTournamentCancellationResults(previous = [], current = []) {
  const merged = new Map(previous.map((result) => [result.jobId, result]));
  current.forEach((result) => merged.set(result.jobId, result));
  return [...merged.values()];
}
async function reusedPublishedTournamentWinnerResult(tournamentId, jobId) {
  const publishedResults = (await listOutboxResults()).filter((result) => isJobOutboxFileName(jobId, result.name));
  return {
    ok: true,
    reused: true,
    tournamentId,
    jobId,
    outboxPath: outboxDir,
    manifestName: publishedResults.find((result) => result.name === `${jobId}-manifest.json`)?.name,
    results: publishedResults
  };
}
async function publishAndAcceptTournamentWinner(tournamentId, jobId, decisionRequest) {
  return withAnimationTournamentLock(tournamentId, async () => {
    let manifest = await readAnimationTournamentManifestRequired(tournamentId);
    if (manifest.winnerCandidateId) {
      if (manifest.winnerCandidateId !== jobId) {
        const isManualOverride = !decisionRequest && manifest.humanReview?.manualWinnerJobId === jobId;
        if (!isManualOverride) {
          throw new HttpError(409, "Tournament already has a different accepted winner.");
        }
      } else {
        const currentCancellations2 = await cancelTournamentLoserRunners(manifest, jobId);
        const cancellationResults2 = mergeTournamentCancellationResults(manifest.smartRaceDecision?.cancellationResults, currentCancellations2);
        if (manifest.smartRaceDecision && currentCancellations2.length > 0) {
          manifest.smartRaceDecision.cancellationResults = cancellationResults2;
          await writeAnimationTournamentManifest(manifest);
        }
        return {
          result: await reusedPublishedTournamentWinnerResult(tournamentId, jobId),
          tournament: manifest,
          cancellationResults: cancellationResults2
        };
      }
    }
    const decision = await normalizeAnimationTournamentWinnerDecision(decisionRequest, manifest, jobId);
    const result = await publishTournamentWinnerUnlocked(tournamentId, jobId);
    manifest = await acceptAnimationTournamentWinner(tournamentId, jobId, decision);
    const currentCancellations = await cancelTournamentLoserRunners(manifest, jobId);
    const cancellationResults = mergeTournamentCancellationResults(manifest.smartRaceDecision?.cancellationResults, currentCancellations);
    if (manifest.smartRaceDecision) {
      manifest.smartRaceDecision.cancellationResults = cancellationResults;
      await writeAnimationTournamentManifest(manifest);
    }
    return { result, tournament: manifest, cancellationResults };
  });
}
async function publishTournamentWinnerUnlocked(tournamentId, jobId) {
  const jobOutboxDir = await resolveJobOutboxDir(jobId);
  const expectedOutboxDir = tournamentJobOutboxDir(tournamentId, jobId);
  if (!jobOutboxDir || resolve2(jobOutboxDir) !== resolve2(expectedOutboxDir)) {
    throw new Error("Tournament winner job does not belong to the requested hidden work outbox.");
  }
  const artifact = await inspectDirectionSplitArtifact(jobId, jobOutboxDir);
  if (!artifact.ready || !artifact.verified) {
    throw new Error(`Tournament winner is not ready for root publish: ${artifact.reason}`);
  }
  const expectedSpriteContext = await readJobExpectedSpriteContext(jobId);
  const expectedSlugs = expectedSpriteContext.directionSlugs ?? directionSplitSlugs;
  const candidates = (await Promise.all(expectedSlugs.map((slug) => findDirectionSplitCandidateFile(jobId, slug, jobOutboxDir)))).filter((candidate) => Boolean(candidate));
  if (candidates.length !== expectedSlugs.length) {
    throw new Error("Tournament winner is missing one or more direction files.");
  }
  const sourceManifest = await findDirectionSplitSourceManifest(jobId, jobOutboxDir);
  const expectedChromaKey = expectedSpriteContext.chromaKey;
  const expectedAction = expectedSpriteContext.action ?? normalizeActionValue(sourceManifest?.parsed?.action);
  const manifestName = await publishVerifiedDirectionSplitArtifact(jobId, candidates, sourceManifest, expectedChromaKey, expectedAction, expectedSpriteContext.motionRecipe, expectedSpriteContext.framesPerDirection ?? 8, artifact.warnings, outboxDir, expectedSlugs);
  const publishedResults = (await listOutboxResults()).filter((result) => isJobOutboxFileName(jobId, result.name));
  return {
    ok: true,
    tournamentId,
    jobId,
    outboxPath: outboxDir,
    manifestName,
    files: artifact.files,
    results: publishedResults
  };
}
function resolveOutboxFile(name) {
  return resolveOutboxFileInDir(outboxDir, name);
}
function resolveOutboxFileInDir(resultDir, name) {
  if (!name || name.includes("/") || name.includes("\\") || name.includes(".."))
    return null;
  const rootDir = resolve2(resultDir);
  const filePath = resolve2(rootDir, name);
  const root = rootDir.endsWith(sep2) ? rootDir : `${rootDir}${sep2}`;
  return filePath.startsWith(root) ? filePath : null;
}
function isSafeJobId(jobId) {
  return /^codex-job-[A-Za-z0-9_-]+$/.test(jobId);
}
function isSafeTournamentId(value) {
  return typeof value === "string" && /^[A-Za-z0-9_-]{1,96}$/.test(value);
}
function createCodexJobId(createdAt) {
  const timestamp = createdAt.replace(/[:.]/g, "-");
  const suffix = Math.random().toString(36).slice(2, 8);
  return `codex-job-${timestamp}-${suffix}`;
}
function isRunnerUnavailableError(error) {
  if (!error || typeof error !== "object" || !("code" in error))
    return false;
  const code = error.code;
  return code === "ENOENT" || code === "EACCES" || code === "EPERM";
}
function mimeTypeForImage(name) {
  const extension = extname2(name).toLowerCase();
  if (extension === ".png")
    return "image/png";
  if (extension === ".jpg" || extension === ".jpeg")
    return "image/jpeg";
  if (extension === ".webp")
    return "image/webp";
  if (extension === ".gif")
    return "image/gif";
  return null;
}
function mimeTypeForOutboxResult(name) {
  if (shouldIgnoreOutboxResultName(name))
    return null;
  return mimeTypeForImage(name) ?? (isDirectionSplitManifestFileName(name) ? "application/json" : null);
}
function isDirectionSplitManifestFileName(name) {
  return /-manifest\.json$/i.test(name);
}
function shouldIgnoreOutboxResultName(name) {
  const normalized = name.toLowerCase();
  const filterName = normalizeOutboxResultNameForFiltering(name);
  return normalized.startsWith(".") || filterName.startsWith("local-gen-") || filterName.includes(".staging") || hasTemporaryOutboxResultMarker(normalized) || filterName.includes("-work-") || filterName.endsWith("-qa.json") || filterName.endsWith(".qa.json") || hasQaOutboxResultMarker(filterName) || hasDebugOutboxResultMarker(filterName) || [
    "candidate-contact",
    "contact-sheet",
    "contact.tmp",
    "grid-qa",
    "mechanical-qa",
    "transparent-contact",
    "preview-grid",
    "ab-gallery"
  ].some((marker) => filterName.includes(marker));
}
function normalizeOutboxResultNameForFiltering(name) {
  return name.toLowerCase().replace(/[_\s]+/g, "-");
}
function hasTemporaryOutboxResultMarker(normalizedName) {
  return /(^|[._-])tmp([._-]|$)/.test(normalizedName);
}
function hasQaOutboxResultMarker(filterName) {
  return /(^|[-.])qa([-.]|$)/.test(filterName);
}
function hasDebugOutboxResultMarker(filterName) {
  return /(^|[-.])debug([-.]|$)/.test(filterName);
}
function extensionForMimeType(mimeType) {
  if (mimeType === "image/png")
    return ".png";
  if (mimeType === "image/jpeg")
    return ".jpg";
  if (mimeType === "image/webp")
    return ".webp";
  if (mimeType === "image/gif")
    return ".gif";
  return null;
}
function parseJsonText(text) {
  return JSON.parse(text.replace(/^\uFEFF/, ""));
}
function validateLocalGenerationRequest(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return { ok: false, error: "Request body must be a JSON object." };
  }
  const body2 = value;
  const optionalStringFields = ["workflowMode", "prompt", "negativePrompt", "jobNotes", "seed", "size", "action"];
  for (const field of optionalStringFields) {
    if (body2[field] !== undefined && typeof body2[field] !== "string") {
      return { ok: false, error: `Field "${field}" must be a string when provided.` };
    }
  }
  if (typeof body2.prompt !== "string" || !body2.prompt.trim()) {
    return { ok: false, error: "Prompt is required for local generation" };
  }
  if (body2.size !== undefined && body2.size !== "" && !/^\d{2,4}x\d{2,4}(\s*x\s*\d{1,2})?$/i.test(String(body2.size).trim())) {
    return { ok: false, error: 'Field "size" must look like "512x512" when provided.' };
  }
  if (body2.count !== undefined) {
    const count = body2.count;
    if (typeof count !== "number" || !Number.isFinite(count) || !Number.isInteger(count) || count < 1 || count > 8) {
      return { ok: false, error: 'Field "count" must be an integer between 1 and 8 when provided.' };
    }
  }
  return { ok: true, request: body2 };
}
function readJson(request) {
  return new Promise((resolve3, reject) => {
    const chunks = [];
    request.on("data", (chunk) => chunks.push(chunk));
    request.on("end", () => {
      try {
        const text = Buffer.concat(chunks).toString("utf8");
        resolve3(text ? parseJsonText(text) : {});
      } catch (error) {
        reject(error);
      }
    });
    request.on("error", reject);
  });
}
function sendJson(response, status, body2) {
  response.writeHead(status, { "Content-Type": "application/json; charset=utf-8" });
  response.end(JSON.stringify(body2));
}
function loadDotEnv(path) {
  if (!existsSync(path))
    return;
  const lines = readFileSync(path, "utf8").split(/\r?\n/);
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#"))
      continue;
    const separatorIndex = trimmed.indexOf("=");
    if (separatorIndex <= 0)
      continue;
    const key = trimmed.slice(0, separatorIndex).trim();
    const rawValue = trimmed.slice(separatorIndex + 1).trim();
    if (process.env[key] !== undefined)
      continue;
    process.env[key] = rawValue.replace(/^["']|["']$/g, "");
  }
}
