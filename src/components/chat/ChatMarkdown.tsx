import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Children, isValidElement } from "react";
import { CodeBlock } from "./CodeBlock";

export default function ChatMarkdown({ content }: { content: string }) {
  return <ReactMarkdown remarkPlugins={[remarkGfm]} components={{
    pre: ({ children }) => {
      const child = Children.toArray(children)[0];
      if (isValidElement<{ children?: string; className?: string }>(child) && typeof child.props.children === "string") {
        return <CodeBlock code={child.props.children.replace(/\n$/, "")} language={child.props.className?.match(/language-([^\s]+)/)?.[1]} />;
      }
      return <pre>{children}</pre>;
    },
    a: ({ href, children }) => /^https?:\/\//i.test(href ?? "")
      ? <Button variant="link" className="h-auto cursor-pointer px-0 text-sm" onClick={() => { if (href) void openUrl(href).catch(() => toast.error("Não foi possível abrir o link")); }}>{children}</Button>
      : <span>{children}</span>,
    img: ({ alt }) => <span className="text-muted-foreground">[Imagem: {alt || "sem descrição"}]</span>,
  }}>{content}</ReactMarkdown>;
}
