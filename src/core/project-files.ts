import { z } from "zod";

export type FileEntry = { name: string; path: string; kind: "directory" | "file" | "link" };
export type DirectoryListing = { path: string; entries: FileEntry[]; truncated: boolean };
export type FilePreview = { path: string; content: string; size: number; encoding: string };
export const videoPreviewSchema = z.object({ path: z.string().min(1), absolutePath: z.string().min(1), size: z.number().positive(), mime: z.enum(["video/mp4", "video/webm", "video/quicktime", "video/ogg", "audio/wav", "audio/mpeg", "audio/ogg"]) });
export type VideoPreview = z.infer<typeof videoPreviewSchema> & { url: string };
export type PreviewState = { loading: boolean; data?: FilePreview; video?: VideoPreview; error?: string };

export function isVideoFile(path: string): boolean {
  return /\.(mp4|webm|mov|m4v|ogv)$/i.test(path);
}

export function isAudioFile(path: string): boolean {
  return /\.(wav|mp3|ogg)$/i.test(path);
}

export function fileName(path: string): string {
  return path.split("/").pop() || path;
}

export function fileError(error: unknown): string {
  return error && typeof error === "object" && "message" in error && typeof error.message === "string"
    ? error.message : "Não foi possível abrir este arquivo. Tente atualizar a visualização.";
}

export function fileLanguage(path: string): string {
  const name = fileName(path).toLowerCase();
  if (name === "dockerfile" || name.startsWith("dockerfile.")) return "dockerfile";
  const extension = name.split(".").pop() || "";
  const languages: Record<string, string> = {
    ts: "typescript", tsx: "typescript", js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript",
    json: "json", jsonc: "json", css: "css", scss: "scss", less: "less", html: "html", htm: "html", vue: "html", svelte: "html",
    md: "markdown", mdx: "markdown", rs: "rust", py: "python", go: "go", php: "php", rb: "ruby", java: "java", cs: "csharp",
    c: "c", h: "c", cpp: "cpp", hpp: "cpp", sh: "shell", bash: "shell", zsh: "shell", ps1: "powershell", psm1: "powershell",
    yml: "yaml", yaml: "yaml", toml: "ini", ini: "ini", env: "ini", sql: "sql", xml: "xml", svg: "xml", bat: "bat", cmd: "bat",
    lock: "plaintext", txt: "plaintext", gitignore: "plaintext", dart: "dart", kt: "kotlin", swift: "swift",
  };
  if (name === ".env" || name.startsWith(".env.")) return "ini";
  return languages[extension] || "plaintext";
}
