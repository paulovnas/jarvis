import { lazy, Suspense } from "react";
import { Skeleton } from "@/components/ui/skeleton";

const ChatMarkdown = lazy(() => import("./ChatMarkdown"));

export function LazyChatMarkdown({ content }: { content: string }) {
  return <Suspense fallback={<Skeleton aria-label="Carregando conteúdo" className="h-12 w-full" />}><ChatMarkdown content={content} /></Suspense>;
}
