import { createHash } from "node:crypto";
import { readFile, stat } from "node:fs/promises";
import { posix, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import JSZip from "jszip";

export interface ArtifactEvidence {
  path: string;
  bytes: number;
  sha256: string;
  format: "png" | "apng" | "gif" | "webp" | "zip" | "json";
  width?: number;
  height?: number;
  frames?: number;
  frameDurationsMs?: number[];
  alphaChannel?: boolean;
  schema?: string;
  entries?: string[];
  manifestReferences?: string[];
}

function requireBytes(buffer: Buffer, offset: number, count: number) {
  if (offset < 0 || count < 0 || offset + count > buffer.length) {
    throw new Error(`Truncated artifact at byte ${offset}.`);
  }
}

function inspectPng(buffer: Buffer): Partial<ArtifactEvidence> {
  let cursor = 8;
  let ended = false;
  let width = 0;
  let height = 0;
  let alphaChannel = false;
  let frames = 1;
  const frameDurationsMs: number[] = [];
  while (cursor < buffer.length) {
    requireBytes(buffer, cursor, 12);
    const length = buffer.readUInt32BE(cursor);
    requireBytes(buffer, cursor + 8, length + 4);
    const type = buffer.toString("ascii", cursor + 4, cursor + 8);
    const start = cursor + 8;
    if (type === "IHDR") {
      if (cursor !== 8 || length !== 13) throw new Error("Invalid PNG header.");
      width = buffer.readUInt32BE(start);
      height = buffer.readUInt32BE(start + 4);
      alphaChannel = [4, 6].includes(buffer[start + 9]);
    } else if (type === "tRNS") {
      alphaChannel = true;
    } else if (type === "acTL") {
      if (length !== 8) throw new Error("Invalid APNG animation control.");
      frames = buffer.readUInt32BE(start);
      if (!frames) throw new Error("APNG declares no frames.");
    } else if (type === "fcTL") {
      if (length !== 26) throw new Error("Invalid APNG frame control.");
      frameDurationsMs.push(1000 * buffer.readUInt16BE(start + 20) / (buffer.readUInt16BE(start + 22) || 100));
    } else if (type === "IEND") {
      ended = true;
      break;
    }
    cursor += 12 + length;
  }
  if (!ended || !width || !height) throw new Error("PNG lacks a complete header or end chunk.");
  if (frameDurationsMs.length && frameDurationsMs.length !== frames) throw new Error("APNG frame count does not match frame controls.");
  return { format: frameDurationsMs.length ? "apng" : "png", width, height, frames, alphaChannel, frameDurationsMs };
}

function inspectGif(buffer: Buffer): Partial<ArtifactEvidence> {
  requireBytes(buffer, 0, 13);
  const width = buffer.readUInt16LE(6);
  const height = buffer.readUInt16LE(8);
  let cursor = 13 + ((buffer[10] & 0x80) ? 3 * 2 ** ((buffer[10] & 7) + 1) : 0);
  let ended = false;
  let delay = 0;
  let alphaChannel = false;
  const frameDurationsMs: number[] = [];
  const skipBlocks = () => {
    for (;;) {
      requireBytes(buffer, cursor, 1);
      const size = buffer[cursor++];
      if (!size) return;
      requireBytes(buffer, cursor, size);
      cursor += size;
    }
  };
  while (cursor < buffer.length) {
    const marker = buffer[cursor++];
    if (marker === 0x3b) {
      ended = true;
      break;
    }
    if (marker === 0x21) {
      requireBytes(buffer, cursor, 1);
      const label = buffer[cursor++];
      if (label === 0xf9) {
        requireBytes(buffer, cursor, 6);
        if (buffer[cursor] !== 4) throw new Error("Invalid GIF graphic control.");
        delay = buffer.readUInt16LE(cursor + 2) * 10;
        alphaChannel ||= Boolean(buffer[cursor + 1] & 1);
      }
      skipBlocks();
    } else if (marker === 0x2c) {
      requireBytes(buffer, cursor, 9);
      const flags = buffer[cursor + 8];
      cursor += 9 + ((flags & 0x80) ? 3 * 2 ** ((flags & 7) + 1) : 0);
      requireBytes(buffer, cursor, 1);
      cursor++;
      skipBlocks();
      frameDurationsMs.push(delay);
      delay = 0;
    } else {
      throw new Error(`Invalid GIF block 0x${marker.toString(16)}.`);
    }
  }
  if (!ended || !width || !height || !frameDurationsMs.length) throw new Error("GIF lacks dimensions, frames or trailer.");
  return { format: "gif", width, height, frames: frameDurationsMs.length, frameDurationsMs, alphaChannel };
}

function inspectWebp(buffer: Buffer): Partial<ArtifactEvidence> {
  requireBytes(buffer, 0, 12);
  if (buffer.readUInt32LE(4) + 8 !== buffer.length) throw new Error("WebP RIFF length mismatch.");
  let cursor = 12;
  let width: number | undefined;
  let height: number | undefined;
  let alphaChannel = false;
  let animated = false;
  const frameDurationsMs: number[] = [];
  while (cursor < buffer.length) {
    requireBytes(buffer, cursor, 8);
    const type = buffer.toString("ascii", cursor, cursor + 4);
    const length = buffer.readUInt32LE(cursor + 4);
    const start = cursor + 8;
    requireBytes(buffer, start, length);
    if (type === "VP8X") {
      if (length !== 10) throw new Error("Invalid WebP extended header.");
      width = buffer.readUIntLE(start + 4, 3) + 1;
      height = buffer.readUIntLE(start + 7, 3) + 1;
      alphaChannel = Boolean(buffer[start] & 0x10);
      animated = Boolean(buffer[start] & 2);
    } else if (type === "ANMF") {
      if (length < 16) throw new Error("Invalid WebP frame.");
      frameDurationsMs.push(buffer.readUIntLE(start + 12, 3));
    } else if (type === "VP8L") {
      requireBytes(buffer, start, 5);
      if (buffer[start] !== 0x2f) throw new Error("Invalid WebP lossless header.");
      const bits = buffer.readUInt32LE(start + 1);
      width ??= (bits & 0x3fff) + 1;
      height ??= ((bits >>> 14) & 0x3fff) + 1;
      alphaChannel ||= Boolean((bits >>> 28) & 1);
    }
    cursor = start + length + (length & 1);
  }
  if (animated && !frameDurationsMs.length) throw new Error("Animated WebP contains no frames.");
  return { format: "webp", width, height, frames: frameDurationsMs.length || 1, frameDurationsMs, alphaChannel };
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function fileReferences(value: unknown): string[] {
  if (typeof value === "string") return [value];
  if (Array.isArray(value)) return value.flatMap(fileReferences);
  // V1 directionPreviews includes a direction identifier alongside file paths.
  return record(value)
    ? Object.entries(value).flatMap(([key, entry]) => key === "direction" ? [] : fileReferences(entry))
    : [];
}

function safeArchivePath(name: string) {
  if (name.includes("\\") || name.includes("\0") || /^[a-z]:|^\//i.test(name) || name.split("/").includes("..")) {
    throw new Error(`Unsafe archive path: ${name}`);
  }
}

export async function inspectArtifactBytes(path: string, buffer: Buffer): Promise<ArtifactEvidence> {
  const base = { path, bytes: buffer.length, sha256: createHash("sha256").update(buffer).digest("hex") };
  let details: Partial<ArtifactEvidence>;
  if (buffer.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) {
    details = inspectPng(buffer);
  } else if (/^GIF8[79]a$/.test(buffer.toString("ascii", 0, 6))) {
    details = inspectGif(buffer);
  } else if (buffer.toString("ascii", 0, 4) === "RIFF" && buffer.toString("ascii", 8, 12) === "WEBP") {
    details = inspectWebp(buffer);
  } else if (buffer.readUInt16LE(0) === 0x4b50) {
    const zip = await JSZip.loadAsync(buffer, { checkCRC32: true });
    const entries = Object.keys(zip.files).filter((name) => !zip.files[name].dir).sort();
    for (const name of entries) {
      safeArchivePath(zip.files[name].unsafeOriginalName ?? name);
      safeArchivePath(name);
    }
    const manifestFile = zip.file("manifest.json") ?? zip.file("composite.json");
    const manifest: unknown = manifestFile ? JSON.parse(await manifestFile.async("string")) : null;
    const references = record(manifest) ? fileReferences(manifest.files) : [];
    for (const reference of references) {
      safeArchivePath(reference);
      if (!zip.file(posix.normalize(reference))) throw new Error(`Archive manifest references missing file: ${reference}`);
    }
    details = {
      format: "zip", entries, manifestReferences: references,
      schema: record(manifest) && typeof manifest.schema === "string" ? manifest.schema : undefined,
    };
  } else {
    const data: unknown = JSON.parse(buffer.toString("utf8"));
    if (!record(data) && !Array.isArray(data)) throw new Error("JSON artifact must be an object or array.");
    details = { format: "json", schema: record(data) && typeof data.schema === "string" ? data.schema : undefined };
  }
  if (!details.format) throw new Error("Unrecognized artifact format.");
  return { ...base, ...details, format: details.format };
}

export async function inspectArtifact(path: string): Promise<ArtifactEvidence> {
  const absolute = resolve(path);
  const metadata = await stat(absolute);
  if (!metadata.isFile() || metadata.size > 128 * 1024 * 1024) throw new Error("Expected an artifact file up to 128 MiB.");
  return inspectArtifactBytes(absolute, await readFile(absolute));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.length < 3) throw new Error("Usage: bun scripts/pixel-cockpit/verify-artifacts.ts <artifact> [...]");
  for (const path of process.argv.slice(2)) {
    try {
      console.log(JSON.stringify({ ok: true, artifact: await inspectArtifact(path) }));
    } catch (error) {
      process.exitCode = 1;
      console.log(JSON.stringify({ ok: false, path, error: error instanceof Error ? error.message : String(error) }));
    }
  }
}
