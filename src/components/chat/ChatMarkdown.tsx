import ReactMarkdown, { defaultUrlTransform, type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { File } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Children, isValidElement, useMemo, useState } from "react";
import { CodeBlock } from "./CodeBlock";
import { MarkdownPrefixCache } from "./markdown-prefix";
import { useStreamedText } from "./use-streamed-text";

function localFilePath(href: string): string | null {
  try {
    let path = href;
    if (/^file:/i.test(path)) {
      const url = new URL(path);
      if (url.hostname && url.hostname !== "localhost") return null;
      path = url.pathname.replace(/^\/([a-z]:\/)/i, "$1");
    }
    path = decodeURIComponent(path);
    if (/\p{Cc}/u.test(path)) return null;
    return /^(?:\/(?!\/)|[a-z]:[\\/]|\\\\[^\\]+\\[^\\]+)/i.test(path) ? path : null;
  } catch {
    return null;
  }
}

const markdownComponents: Components = {
  pre: ({ children }) => {
    const child = Children.toArray(children)[0];
    if (isValidElement<{ children?: string; className?: string }>(child) && typeof child.props.children === "string") {
      return <CodeBlock code={child.props.children.replace(/\n$/, "")} language={child.props.className?.match(/language-([^\s]+)/)?.[1]} />;
    }
    return <pre>{children}</pre>;
  },
  a: ({ href, children }) => {
    const path = localFilePath(href ?? "");
    if (path) return <Button variant="link" className="h-auto max-w-full cursor-pointer whitespace-normal px-0 text-left text-sm" title={`Mostrar na pasta: ${path}`} onClick={() => { void revealItemInDir(path).catch(() => toast.error("Não foi possível mostrar o arquivo na pasta. Ele pode ter sido movido ou removido.")); }}><File aria-hidden="true" />{children}</Button>;
    return /^https?:\/\//i.test(href ?? "")
      ? <Button variant="link" className="h-auto cursor-pointer px-0 text-sm" onClick={() => { if (href) void openUrl(href).catch(() => toast.error("Não foi possível abrir o link")); }}>{children}</Button>
      : <span>{children}</span>;
  },
  img: ({ alt }) => <span className="text-muted-foreground">[Imagem: {alt || "sem descrição"}]</span>,
};

export type ChatMarkdownProps = { content: string; isStreaming?: boolean; streamId?: string };

export default function ChatMarkdown({ content, isStreaming = false, streamId = "" }: ChatMarkdownProps) {
  const [cache] = useState(() => new MarkdownPrefixCache());
  const displayed = useStreamedText(content, isStreaming, streamId);
  return useMemo(() => {
    const projection = cache.prepare(displayed, isStreaming, streamId);
    return <ReactMarkdown remarkPlugins={[remarkGfm, projection.plugin]} urlTransform={(url, key) => key === "href" && localFilePath(url) ? url : defaultUrlTransform(url)} components={markdownComponents}>{projection.content}</ReactMarkdown>;
  }, [cache, displayed, isStreaming, streamId]);
}
