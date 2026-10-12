import { lazy, Suspense } from "react";
import { Skeleton } from "@/components/ui/skeleton";
import type { ChatMarkdownProps } from "./ChatMarkdown";

const ChatMarkdown = lazy(() => import("./ChatMarkdown"));

export function LazyChatMarkdown(props: ChatMarkdownProps) {
  return <Suspense fallback={<Skeleton aria-label="Carregando conteúdo" className="h-12 w-full" />}><ChatMarkdown {...props} /></Suspense>;
}
