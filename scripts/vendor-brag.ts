// Refresh the reviewed, self-contained Brag snapshot. Runtime never reads docs/.
import { createHash } from "node:crypto";
import { readFileSync, readdirSync, mkdirSync, writeFileSync, lstatSync } from "node:fs";
import { resolve, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import { execFileSync } from "node:child_process";

const root = fileURLToPath(new URL("../", import.meta.url));
const source = resolve(root, "docs/brag");
const destination = resolve(root, "src-tauri/src/core/brag-docs");
const revision = "cb89b9f44309b0bf4e3cb89e685fadf80c7999ed";
const hyperframesRevision = "bc57e282fdde4afdccec1d1a2bacd94a9c3c5383";
const hyperframesDestination = join(destination, "hyperframes");
if (process.argv.includes("--refresh-hyperframes")) {
  const response = await fetch(`https://api.github.com/repos/heygen-com/hyperframes/git/trees/${hyperframesRevision}?recursive=1`);
  if (!response.ok) throw new Error("Could not read the pinned HyperFrames tree.");
  const tree = await response.json() as { tree: Array<{ type: string; path: string; sha: string }> };
  const selection = tree.tree.filter(item => item.type === "blob" && (item.path === "LICENSE" || /^skills\/hyperframes-(animation|creative|keyframes)\/.*\.md$/.test(item.path)));
  const files: Array<{ path: string; sourcePath: string; gitBlob: string; sha256: string; bytes: number }> = [];
  const contents = new Map<string, Buffer>();
  for (let offset = 0; offset < selection.length; offset += 8) {
    await Promise.all(selection.slice(offset, offset + 8).map(async item => {
      const response = await fetch(`https://raw.githubusercontent.com/heygen-com/hyperframes/${hyperframesRevision}/${item.path}`, { signal: AbortSignal.timeout(30000) });
      if (!response.ok) throw new Error(`Could not fetch pinned document ${item.path}`);
      const data = Buffer.from(await response.arrayBuffer());
      const gitBlob = createHash("sha1").update(`blob ${data.length}\0`).update(data).digest("hex");
      if (gitBlob !== item.sha) throw new Error(`Pinned document integrity failed: ${item.path}`);
      const path = item.path.replace(/^skills\//, "");
      contents.set(path, data);
      files.push({ path, sourcePath: item.path, gitBlob, sha256: createHash("sha256").update(data).digest("hex"), bytes: data.length });
    }));
  }
  files.sort((a, b) => a.path.localeCompare(b.path));
  for (const [path, data] of contents) {
    mkdirSync(join(hyperframesDestination, path, ".."), { recursive: true });
    writeFileSync(join(hyperframesDestination, path), data);
  }
  writeFileSync(join(hyperframesDestination, "provenance.json"), JSON.stringify({
    source: "https://github.com/heygen-com/hyperframes", revision: hyperframesRevision, license: "Apache-2.0",
    scope: "Unmodified Markdown for animation, creative and keyframes domains only. No helper scripts or external runtime libraries installed.", files,
  }, null, 2) + "\n");
  console.log(`Reviewed HyperFrames supplemental documentation: ${files.length} pinned files.`);
}
if (execFileSync("git", ["rev-parse", "HEAD"], { cwd: source, encoding: "utf8" }).trim() !== revision) {
  throw new Error("Brag checkout does not match the reviewed revision.");
}
execFileSync("git", ["diff", "--exit-code", "HEAD", "--", "LICENSE", "skills/brag"], { cwd: source, stdio: "pipe" });
const adapter = readFileSync(join(destination, "JARVIS_ADAPTER.md"));
const records = new Map<string, Buffer>([["JARVIS_ADAPTER.md", adapter], ["MUSIC_NOTICE.md", readFileSync(join(destination, "MUSIC_NOTICE.md"))]]);
const hyperframesProvenance = readFileSync(join(hyperframesDestination, "provenance.json"));
const supplemental = JSON.parse(hyperframesProvenance.toString()) as { revision: string; files: Array<{ path: string; bytes: number; sha256: string }> };
if (supplemental.revision !== hyperframesRevision) throw new Error("HyperFrames docs do not match the reviewed revision.");
for (const file of supplemental.files) {
  if (!/^(LICENSE|hyperframes-(animation|creative|keyframes)\/[a-zA-Z0-9_./-]+\.md)$/.test(file.path) || file.path.split("/").includes("..")) throw new Error("Unexpected supplemental path.");
  const data = readFileSync(join(hyperframesDestination, file.path));
  if (data.length !== file.bytes || createHash("sha256").update(data).digest("hex") !== file.sha256) throw new Error(`Supplemental documentation changed: ${file.path}`);
  records.set(`hyperframes/${file.path}`, data);
}
records.set("hyperframes/provenance.json", hyperframesProvenance);
const copy = (input: string, output: string) => {
  const path = join(source, input);
  if (!lstatSync(path).isFile()) throw new Error(`Unsupported source: ${input}`);
  records.set(output, readFileSync(path));
};
copy("LICENSE", "LICENSE");
copy("skills/brag/SKILL.md", "SKILL.md");
records.set("SKILL.md", Buffer.from(records.get("SKILL.md")!.toString().replace(
  /## Invocation dispatch \(must happen first\)[\s\S]*?Before inspecting the project,/,
  "## Jarvis managed workflow (takes precedence)\n\nRead [JARVIS_ADAPTER.md](JARVIS_ADAPTER.md) before applying this document or any reference. Always use the full HyperFrames workflow through Jarvis tools. Upstream external CLI and provider examples in the references are creative context and do not apply as execution instructions inside Jarvis.\n\nBefore inspecting the project,",
)));
for (const entry of readdirSync(join(source, "skills/brag/references")).sort()) {
  if (entry.endsWith(".md")) copy(`skills/brag/references/${entry}`, `references/${entry}`);
}
copy("skills/brag/assets/sfx/sfx-analysis.md", "assets/sfx/sfx-analysis.md");
copy("skills/brag/assets/sfx/sfx-analysis.json", "assets/sfx/sfx-analysis.json");
const assets: Array<{ path: string; family: string; format: string; license: string; bytes: number; sha256: string; source: string; licenseUrl?: string; attribution?: string; modifications?: string }> = [];
const walk = (directory: string) => {
  for (const item of readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    const path = join(directory, item.name);
    if (item.isSymbolicLink()) throw new Error("Source symlinks are not allowed.");
    if (item.isDirectory()) walk(path);
    else if (/\.(ogg|wav)$/.test(item.name)) {
      const name = relative(join(source, "skills/brag/assets"), path).replaceAll("\\", "/");
      const data = readFileSync(path);
      records.set(`assets/${name}`, data);
      const family = name.split("/")[1];
      assets.push({ path: name, family, format: item.name.split(".").at(-1)!, license: "CC0-1.0", bytes: data.length,
        sha256: createHash("sha256").update(data).digest("hex"),
        source: family === "keyboard" ? "https://opengameart.org/content/keyboard-soundpack-1-typing-and-single-keystrokes" : "https://kenney.nl/assets",
        licenseUrl: "https://creativecommons.org/publicdomain/zero/1.0/",
      });
    }
  }
};
walk(join(source, "skills/brag/assets/sfx"));
if (assets.length !== 260) throw new Error("The reviewed SFX snapshot must have exactly 260 assets.");
const songs = new Map([[1, 12866], [9, 12874], [10, 12875], [11, 12876], [12, 12881]]);
for (const [volume, id] of songs) {
  const name = `happy-beats-business-moves-vol-${volume}-by-ende-dot-app`;
  const songSource = `https://ende.app/en/song/${id}-happy-beats-business-moves-vol-${volume}`;
  const path = `music/${name}.mp3`;
  const data = readFileSync(join(source, "skills/brag/assets", path));
  records.set(`assets/${path}`, data);
  for (const extension of ["json", "md"]) copy(`skills/brag/assets/music/cues/${name}.music-cues.${extension}`, `assets/music/cues/${name}.music-cues.${extension}`);
  assets.push({ path, family: "music", format: "mp3", license: "CC-BY-4.0", bytes: data.length,
    sha256: createHash("sha256").update(data).digest("hex"), source: songSource,
    licenseUrl: "https://creativecommons.org/licenses/by/4.0/",
    attribution: `Happy Beats & Business Moves Vol. ${volume} by Sascha Ende — ende.app — ${songSource} — CC BY 4.0`,
    modifications: "Unmodified upstream MP3; disclose editing/conversion in delivered credits.",
  });
}
records.set("asset-catalog.json", Buffer.from(JSON.stringify({ schemaVersion: 1, assets }, null, 2) + "\n"));
records.set("provenance.json", Buffer.from(JSON.stringify({
  schemaVersion: 1, version: "0.4.0", repository: "https://github.com/latent-spaces/brag", revision,
  adapterVersion: "1.0.0", softwareLicense: "MIT", sfxLicense: "CC0-1.0", sfxCount: 260, musicCount: 5,
  musicLicense: "CC-BY-4.0", musicTerms: "https://ende.app/en/standard-license",
  omitted: ["brag-slim", "Python helpers and dependencies"],
}, null, 2) + "\n"));
const files = [...records.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([path, data]) => ({
  path, bytes: data.length, sha256: createHash("sha256").update(data).digest("hex"),
  license: path.startsWith("hyperframes/") ? "Apache-2.0" : /\.mp3$/.test(path) || path === "MUSIC_NOTICE.md" ? "CC-BY-4.0" : /\.(ogg|wav)$/.test(path) ? "CC0-1.0" : "MIT",
}));
const archive: Buffer[] = [];
for (const file of files) {
  const data = records.get(file.path)!;
  const header = Buffer.alloc(512);
  let name = file.path;
  if (Buffer.byteLength(name) > 100) {
    const separator = name.lastIndexOf("/");
    const prefix = name.slice(0, separator);
    name = name.slice(separator + 1);
    if (separator < 0 || Buffer.byteLength(name) > 100 || Buffer.byteLength(prefix) > 155) throw new Error(`Tar path exceeds portable bound: ${file.path}`);
    header.write(prefix, 345, 155);
  }
  header.write(name, 0, 100);
  const octal = (offset: number, length: number, value: number) => header.write(value.toString(8).padStart(length - 1, "0") + "\0", offset, length);
  octal(100, 8, 0o644); octal(108, 8, 0); octal(116, 8, 0); octal(124, 12, data.length); octal(136, 12, 0);
  header.fill(32, 148, 156); header.write("0", 156); header.write("ustar\0", 257); header.write("00", 263);
  const checksum = header.reduce((sum, byte) => sum + byte, 0);
  header.write(checksum.toString(8).padStart(6, "0") + "\0 ", 148, 8);
  archive.push(header, data, Buffer.alloc((512 - data.length % 512) % 512));
  if (!file.path.startsWith("assets/")) {
    mkdirSync(join(destination, file.path, ".."), { recursive: true });
    writeFileSync(join(destination, file.path), data);
  }
}
archive.push(Buffer.alloc(1024));
const packed = gzipSync(Buffer.concat(archive), { level: 9 });
writeFileSync(join(destination, "package.tar.gz"), packed);
writeFileSync(join(destination, "manifest.json"), JSON.stringify({ schemaVersion: 1, version: "0.4.0", archiveSha256: createHash("sha256").update(packed).digest("hex"), files }, null, 2) + "\n");
console.log(`Brag ${revision.slice(0, 8)}: ${files.length} reviewed files, 260 CC0 effects, 5 attributed music tracks, ${packed.length} archive bytes.`);
