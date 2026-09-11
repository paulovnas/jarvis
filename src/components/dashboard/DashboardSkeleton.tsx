import { Skeleton } from "@/components/ui/skeleton";

export function DashboardSkeleton({ board = false }: { board?: boolean }) {
  return <div role="status" aria-label="Carregando detalhes" className="space-y-5 p-6">
    <div className={`grid gap-3 ${board ? "grid-cols-4" : "grid-cols-2 @3xl:grid-cols-4"}`}>
      {Array.from({ length: 4 }, (_, index) => <div key={index} className="space-y-4 rounded-lg border border-border bg-card/50 p-4"><Skeleton className="h-3 w-20" /><Skeleton className={board ? "h-24 w-full" : "h-9 w-24"} />{board && <Skeleton className="h-28 w-full" />}</div>)}
    </div>
    {!board && <><Skeleton className="h-56 w-full rounded-lg" /><div className="grid grid-cols-2 gap-4"><Skeleton className="h-52" /><Skeleton className="h-52" /></div></>}
  </div>;
}
