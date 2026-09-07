import { useMemo, useState } from "react";
import hljs from "highlight.js/lib/common";
import { Check, Copy } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import "highlight.js/styles/atom-one-dark.css";

export function CodeBlock({ code, language = "" }: { code: string; language?: string }) {
  const [copied, setCopied] = useState(false);
  const highlighted = useMemo(() => {
    if (!hljs.getLanguage(language) || code.length > 50_000) return null;
    try { return hljs.highlight(code, { language, ignoreIllegals: true }).value; }
    catch { return null; }
  }, [code, language]);
  async function copy() {
    try { await navigator.clipboard.writeText(code); setCopied(true); }
    catch { toast.error("Não foi possível copiar o código"); }
  }
  return <section className="chat-code-block my-3 min-w-0 overflow-hidden rounded-lg border border-white/10 bg-card shadow-[inset_0_1px_0_#ffffff0a]" aria-label={`Código ${language || "texto"}`}>
    <header className="flex items-center justify-between gap-3 border-b border-white/5 px-3 py-1.5">
      <span className="font-mono text-[10px] text-muted-foreground">{language || "texto"}</span>
      <Button variant="ghost" size="sm" className="h-6 cursor-pointer gap-1.5 px-2 text-[10px] text-muted-foreground" onClick={() => void copy()} onBlur={() => setCopied(false)} aria-label="Copiar código">{copied ? <Check className="size-3 text-onedark-green" /> : <Copy className="size-3" />}{copied ? "Copiado" : "Copiar"}</Button>
    </header>
    <pre className="code-content"><code className="hljs font-mono" {...(highlighted === null ? { children: code } : { dangerouslySetInnerHTML: { __html: highlighted } })} /></pre>
  </section>;
}
