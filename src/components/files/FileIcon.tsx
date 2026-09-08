import { FileCode2, FileJson2, FileText, Image, FileCog, type LucideIcon } from "lucide-react";
import { fileLanguage } from "@/core/project-files";

export function FileIcon({ path, className = "size-3.5" }: { path: string; className?: string }) {
  const language = fileLanguage(path);
  let Icon: LucideIcon;
  let color: string;
  if (["typescript", "javascript"].includes(language)) { Icon = FileCode2; color = language === "typescript" ? "text-primary" : "text-onedark-yellow"; }
  else if (language === "json") { Icon = FileJson2; color = "text-onedark-yellow"; }
  else if (["markdown", "plaintext"].includes(language)) { Icon = FileText; color = "text-onedark-cyan"; }
  else if (["ini", "yaml", "dockerfile"].includes(language)) { Icon = FileCog; color = "text-onedark-purple"; }
  else { Icon = FileCode2; color = "text-onedark-green"; }
  if (/\.(png|jpe?g|webp|gif|ico|svg)$/i.test(path)) { Icon = Image; color = "text-onedark-purple"; }
  return <Icon aria-hidden="true" className={`${className} shrink-0 ${color}`} />;
}
