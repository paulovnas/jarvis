import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ArrowDownToLine,
  Check,
  ChevronLeft,
  ChevronRight,
  ChevronsUpDown,
  ExternalLink,
  Flame,
  FolderGit2,
  Search,
  Store,
  TrendingUp,
  Trophy,
  X,
} from "lucide-react";
import { toast } from "sonner";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { CommandInput, Input } from "@/components/TextInput";
import { Pagination, PaginationContent, PaginationItem } from "@/components/ui/pagination";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { cn } from "@/lib/utils";
import {
  marketplaceSchema,
  skillsSnapshotSchema,
  skillError,
  type MarketplaceSkill,
  type Skill,
} from "@/core/skills";
import { SkillDetailsDialog, type SkillSelection } from "./SkillDetailsDialog";
import { Hint } from "@/components/ui/hint";

const PAGE_SIZE = 24;
const number = new Intl.NumberFormat("pt-BR", { notation: "compact", maximumFractionDigits: 1 });

export function SkillsMarketplace({
  open,
  onOpenChange,
  installed,
  onInstalled,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  installed: Skill[];
  onInstalled: (value: unknown) => unknown;
}) {
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [ranking, setRanking] = useState("alltime");
  const [source, setSource] = useState("all");
  const [page, setPage] = useState(1);
  const [limit, setLimit] = useState(60);
  const [reload, setReload] = useState(0);
  const [response, setResponse] = useState<{
    key: string;
    skills: MarketplaceSkill[];
    error: string | null;
  } | null>(null);
  const [installing, setInstalling] = useState<string | null>(null);
  const [selection, setSelection] = useState<SkillSelection | null>(null);
  const [repoOpen, setRepoOpen] = useState(false);
  const [repoSearch, setRepoSearch] = useState("");

  const pending = useRef(false);
  const mounted = useRef(false);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(() => {
    const timer = setTimeout(() => setDebounced(query.trim()), 400);
    return () => clearTimeout(timer);
  }, [query]);

  const key = `${debounced}:${ranking}:${limit}:${reload}`;

  useEffect(() => {
    if (!open) return;
    let active = true;
    void invoke("browse_skill_marketplace", { query: debounced, ranking, limit })
      .then((value) => {
        if (active) setResponse({ key, skills: marketplaceSchema.parse(value), error: null });
      })
      .catch((cause) => {
        if (active) setResponse({ key, skills: [], error: skillError(cause) });
      });
    return () => {
      active = false;
    };
  }, [open, debounced, ranking, limit, reload, key]);

  const current = response?.key === key ? response : null;
  const skills = useMemo(() => current?.skills ?? [], [current]);

  const sourceCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const skill of skills) {
      counts.set(skill.source, (counts.get(skill.source) ?? 0) + 1);
    }
    return counts;
  }, [skills]);

  const sources = useMemo(() => {
    return [...sourceCounts.keys()].sort((a, b) => {
      const diff = (sourceCounts.get(b) ?? 0) - (sourceCounts.get(a) ?? 0);
      if (diff !== 0) return diff;
      return a.localeCompare(b, undefined, { numeric: true });
    });
  }, [sourceCounts]);

  const repoQuery = repoSearch.trim().toLowerCase();
  const displayedSources = useMemo(() => {
    if (!repoQuery) {
      const top10 = sources.slice(0, 10);
      if (source !== "all" && !top10.includes(source) && sources.includes(source)) {
        return [...top10, source];
      }
      return top10;
    }
    return sources.filter((s) => s.toLowerCase().includes(repoQuery));
  }, [sources, repoQuery, source]);

  const showsAllOption =
    !repoQuery ||
    "todos os repositórios".includes(repoQuery) ||
    "all".includes(repoQuery) ||
    "todos".includes(repoQuery);

  const filtered = skills.filter((skill) => source === "all" || skill.source === source);
  const pages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const currentPage = Math.min(page, pages);
  const visible = filtered.slice((currentPage - 1) * PAGE_SIZE, currentPage * PAGE_SIZE);
  const installedIds = new Set(installed.map((skill) => skill.marketplaceId).filter(Boolean));

  async function install(skill: MarketplaceSkill) {
    if (pending.current) return;
    pending.current = true;
    setInstalling(skill.id);
    try {
      const value = skillsSnapshotSchema.parse(
        await invoke("install_marketplace_skill", {
          source: skill.source,
          skillId: skill.skillId,
        })
      );
      onInstalled(value);
      toast.success(`${skill.name} instalada`);
    } catch (cause) {
      toast.error(skillError(cause));
    } finally {
      pending.current = false;
      if (mounted.current) setInstalling(null);
    }
  }

  function resetFilters() {
    setPage(1);
    setSource("all");
    setLimit(60);
  }

  function selectSource(nextSource: string) {
    setSource(nextSource);
    setPage(1);
    setRepoOpen(false);
    setRepoSearch("");
  }

  return (
    <>
      <Dialog
        open={open}
        onOpenChange={(next) => {
          if (!pending.current) {
            onOpenChange(next);
            if (!next) setSelection(null);
          }
        }}
      >
        <DialogContent className="dark flex h-[88vh] max-h-[920px] flex-col gap-4 p-6 sm:max-w-[min(1160px,94vw)]">
          <DialogHeader className="pr-8">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <div className="flex size-9 items-center justify-center rounded-lg border border-primary/25 bg-primary/10 text-primary shadow-xs">
                  <Store aria-hidden="true" className="size-4.5" />
                </div>
                <div>
                  <DialogTitle className="flex items-center gap-2 text-base font-semibold tracking-tight text-foreground">
                    Marketplace de Skills
                    <Badge
                      variant="outline"
                      className="border-primary/20 bg-primary/5 text-[10px] font-normal text-primary"
                    >
                      skills.sh
                    </Badge>
                  </DialogTitle>
                  <DialogDescription className="text-xs text-muted-foreground mt-0.5">
                    Explore, descubra e instale pacotes de skills para expandir a capacidade do Jarvis.
                  </DialogDescription>
                </div>
              </div>
            </div>
          </DialogHeader>

          <div className="relative shrink-0">
            <Search
              aria-hidden="true"
              className="pointer-events-none absolute top-2.5 left-3 size-4 text-muted-foreground"
            />
            <Input
              aria-label="Pesquisar no Marketplace"
              value={query}
              onChange={(event) => {
                setQuery(event.target.value);
                resetFilters();
              }}
              maxLength={160}
              placeholder="Pesquisar skills por nome ou palavra-chave..."
              className="pl-9 pr-9 text-xs"
            />
            {query && (
              <button
                type="button"
                aria-label="Limpar pesquisa"
                onClick={() => {
                  setQuery("");
                  resetFilters();
                }}
                className="absolute top-2.5 right-3 cursor-pointer rounded-xs p-0.5 text-muted-foreground hover:text-foreground transition-colors"
              >
                <X className="size-3.5" />
              </button>
            )}
          </div>

          <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border/40 pb-3">
            <Tabs
              value={ranking}
              onValueChange={(value) => {
                setRanking(String(value));
                resetFilters();
              }}
            >
              <TabsList className="bg-muted/50 p-0.5">
                <TabsTrigger
                  value="alltime"
                  className="cursor-pointer gap-1.5 px-3 text-xs"
                  disabled={!!debounced}
                >
                  <Trophy aria-hidden="true" className="size-3.5" />
                  Populares
                </TabsTrigger>
                <TabsTrigger
                  value="trending"
                  className="cursor-pointer gap-1.5 px-3 text-xs"
                  disabled={!!debounced}
                >
                  <TrendingUp aria-hidden="true" className="size-3.5" />
                  Em alta
                </TabsTrigger>
                <TabsTrigger
                  value="hot"
                  className="cursor-pointer gap-1.5 px-3 text-xs"
                  disabled={!!debounced}
                >
                  <Flame aria-hidden="true" className="size-3.5" />
                  Destaques
                </TabsTrigger>
              </TabsList>
            </Tabs>

            <div className="flex items-center gap-2">
              {source !== "all" && (
                <Hint content="Limpar filtro de repositório"><Button
                  variant="secondary"
                  size="sm"
                  aria-label="Limpar filtro de repositório"
                  className="cursor-pointer gap-1 px-2 py-1 text-xs font-normal text-foreground hover:bg-destructive/15 hover:text-destructive transition-colors"
                  onClick={() => {
                    setSource("all");
                    setPage(1);
                  }}
                >
                  <span>@{source}</span>
                  <X className="size-3" />
                </Button></Hint>
              )}

              <Popover open={repoOpen} onOpenChange={setRepoOpen}>
                <PopoverTrigger
                  render={
                    <Button
                      variant="outline"
                      role="combobox"
                      aria-expanded={repoOpen}
                      aria-label="Filtrar por repositório"
                      className="w-64 max-w-full justify-between cursor-pointer text-xs font-normal"
                    />
                  }
                >
                  <div className="flex items-center gap-2 truncate">
                    <FolderGit2 className="size-3.5 shrink-0 text-muted-foreground" />
                    <span className="truncate">
                      {source === "all" ? "Todos os repositórios" : source}
                    </span>
                  </div>
                  <ChevronsUpDown className="ml-2 size-3.5 shrink-0 opacity-50" />
                </PopoverTrigger>
                <PopoverContent className="w-72 p-0" align="end">
                  <Command shouldFilter={false}>
                    <CommandInput
                      placeholder="Pesquisar repositório..."
                      value={repoSearch}
                      onValueChange={setRepoSearch}
                      aria-label="Pesquisar repositório"
                    />
                    <CommandList className="max-h-60">
                      {displayedSources.length === 0 && !showsAllOption ? (
                        <CommandEmpty>Nenhum repositório encontrado.</CommandEmpty>
                      ) : (
                        <CommandGroup
                          heading={
                            repoQuery
                              ? `Resultados (${displayedSources.length})`
                              : "Top 10 repositórios"
                          }
                        >
                          {showsAllOption && (
                            <CommandItem
                              value="all"
                              role="option"
                              aria-label="Todos os repositórios"
                              onSelect={() => selectSource("all")}
                              onClick={() => selectSource("all")}
                              className="cursor-pointer"
                            >
                              <Check
                                className={cn(
                                  "mr-2 size-3.5",
                                  source === "all" ? "opacity-100 text-primary" : "opacity-0"
                                )}
                              />
                              <span className="font-medium">Todos os repositórios</span>
                            </CommandItem>
                          )}
                          {displayedSources.map((s) => (
                            <CommandItem
                              key={s}
                              value={s}
                              role="option"
                              aria-label={s}
                              onSelect={() => selectSource(s)}
                              onClick={() => selectSource(s)}
                              className="cursor-pointer"
                            >
                              <Check
                                className={cn(
                                  "mr-2 size-3.5",
                                  source === s ? "opacity-100 text-primary" : "opacity-0"
                                )}
                              />
                              <span className="truncate flex-1">{s}</span>
                              <span className="text-[10px] text-muted-foreground tabular-nums">
                                {sourceCounts.get(s)}
                              </span>
                            </CommandItem>
                          ))}
                        </CommandGroup>
                      )}
                    </CommandList>
                  </Command>
                </PopoverContent>
              </Popover>
            </div>
          </div>

          {installing && (
            <div
              role="status"
              className="flex items-center gap-2 rounded-lg border border-primary/20 bg-primary/5 px-3 py-1.5 text-xs text-primary animate-in fade-in duration-200"
            >
              <Spinner className="size-3.5" />
              <span>Instalando skill no ambiente...</span>
            </div>
          )}

          <ScrollArea className="min-h-0 flex-1 -mx-2 px-2" key={`${currentPage}:${key}:${source}`}>
            {!current ? (
              <div
                role="status"
                aria-label="Carregando Marketplace"
                className="grid grid-cols-1 gap-3.5 p-2 pb-6 sm:grid-cols-2 lg:grid-cols-3"
              >
                {Array.from({ length: 9 }, (_, i) => (
                  <Card
                    key={i}
                    className="flex flex-col justify-between gap-3.5 rounded-lg border border-border/60 bg-background p-3.5"
                  >
                    <div className="flex items-start gap-3">
                      <Skeleton className="size-9 shrink-0 rounded-lg bg-card" />
                      <div className="flex-1 space-y-2">
                        <Skeleton className="h-4 w-3/4 rounded bg-card" />
                        <Skeleton className="h-3 w-1/2 rounded bg-card" />
                      </div>
                    </div>
                    <div className="flex items-center justify-between border-t border-border/40 pt-2.5">
                      <Skeleton className="h-4 w-20 rounded bg-card" />
                      <Skeleton className="h-7 w-20 rounded-lg bg-card" />
                    </div>
                  </Card>
                ))}
              </div>
            ) : current.error ? (
              <div className="flex flex-col items-center justify-center gap-3 py-16 text-center">
                <p role="alert" className="text-sm font-medium text-destructive">
                  {current.error}
                </p>
                <Button
                  variant="outline"
                  className="cursor-pointer text-xs"
                  onClick={() => setReload((value) => value + 1)}
                >
                  Tentar novamente
                </Button>
              </div>
            ) : visible.length === 0 ? (
              <div className="flex flex-col items-center justify-center gap-3 py-16 text-center">
                <div className="flex size-12 items-center justify-center rounded-lg border border-border/60 bg-muted/30 text-muted-foreground">
                  <Search className="size-6 opacity-60" />
                </div>
                <div className="space-y-1">
                  <p className="text-sm font-medium text-foreground">Nenhuma skill encontrada</p>
                  <p className="text-xs text-muted-foreground">
                    {query || source !== "all"
                      ? "Tente buscar com outros termos ou remover os filtros aplicados."
                      : "Nenhuma skill disponível no momento."}
                  </p>
                </div>
                {(query || source !== "all") && (
                  <Button
                    variant="outline"
                    size="sm"
                    className="cursor-pointer text-xs mt-1"
                    onClick={() => {
                      setQuery("");
                      resetFilters();
                    }}
                  >
                    Limpar filtros
                  </Button>
                )}
              </div>
            ) : (
              <div className="grid grid-cols-1 gap-3.5 p-2 pb-6 sm:grid-cols-2 lg:grid-cols-3">
                {visible.map((skill) => {
                  const isInstalled = installedIds.has(skill.id);
                  const owner = skill.source.split("/")[0] || "";
                  const avatarUrl = owner ? `https://github.com/${owner}.png?size=64` : undefined;
                  const initials = (owner || skill.name).slice(0, 2).toUpperCase();

                  return (
                    <Card
                      key={skill.id}
                      className="group relative flex min-w-0 flex-col justify-between gap-3.5 rounded-lg border border-border bg-background p-3.5 shadow-sm transition-all duration-150 hover:bg-secondary hover:border-[#61afef]/60 hover:shadow-md"
                    >
                      <div className="flex items-start justify-between gap-2.5">
                        <Button
                          variant="ghost"
                          className="h-auto min-w-0 flex-1 cursor-pointer items-start justify-start gap-3 p-0 text-left hover:bg-transparent"
                          aria-label={`Ver ${skill.name}`}
                          onClick={() =>
                            setSelection({
                              name: skill.name,
                              source: skill.source,
                              skillId: skill.skillId,
                            })
                          }
                        >
                          <Avatar size="sm" className="size-9 shrink-0 rounded-lg border border-border bg-card">
                            <AvatarImage src={avatarUrl} alt={owner} loading="lazy" />
                            <AvatarFallback className="rounded-lg bg-card text-[10px] font-semibold text-foreground">
                              {initials}
                            </AvatarFallback>
                          </Avatar>
                          <div className="min-w-0 flex-1">
                            <div className="flex items-center gap-1.5">
                              <Hint content={skill.name}><span
                                className="truncate text-sm font-semibold text-[#e5e5e6] group-hover:text-[#61afef] transition-colors"
                              >
                                {skill.name}
                              </span></Hint>
                            </div>
                            {skill.skillId !== skill.name ? (
                              <Hint content={skill.skillId}><span
                                className="block truncate font-mono text-[11px] text-muted-foreground"
                              >
                                {skill.skillId}
                              </span></Hint>
                            ) : (
                              <Hint content={skill.source}><span
                                className="block truncate text-xs text-muted-foreground"
                              >
                                {skill.source}
                              </span></Hint>
                            )}
                          </div>
                        </Button>

                        <div className="flex shrink-0 items-center gap-1">
                          <Hint content="Ver no skills.sh"><Button
                            variant="ghost"
                            size="icon-sm"
                            className="size-7 cursor-pointer text-muted-foreground hover:text-[#e5e5e6] hover:bg-card"
                            aria-label={`Ver ${skill.name} no skills.sh`}
                            onClick={() => {
                              void openUrl(
                                `https://skills.sh/${skill.source}/${skill.skillId}`
                              ).catch(() => {
                                toast.error("Não foi possível abrir o link");
                              });
                            }}
                          >
                            <ExternalLink className="size-3.5" />
                          </Button></Hint>
                        </div>
                      </div>

                      <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-2.5">
                        <div className="flex min-w-0 items-center gap-2">
                          <Hint content={`Filtrar por @${skill.source}`}><button
                            type="button"
                            onClick={() => {
                              setSource(skill.source);
                              setPage(1);
                            }}
                            aria-label={`Filtrar por @${skill.source}`}
                            className="cursor-pointer max-w-[140px] truncate rounded-md bg-card border border-border/60 hover:border-[#61afef]/40 px-2 py-0.5 text-[11px] font-medium text-foreground hover:text-white transition-colors"
                          >
                            @{skill.source}
                          </button></Hint>
                          <Hint content={`${skill.installs.toLocaleString("pt-BR")} instalações`}><span
                            className="flex items-center gap-1 text-[11px] tabular-nums text-muted-foreground shrink-0"
                          >
                            <ArrowDownToLine aria-hidden="true" className="size-3" />
                            {number.format(skill.installs)}
                          </span></Hint>
                        </div>

                        <div>
                          {isInstalled ? (
                            <Badge
                              variant="outline"
                              className="h-7 border-emerald-500/30 bg-emerald-500/10 px-2.5 text-xs font-medium text-[#98c379] gap-1 rounded-lg"
                            >
                              <Check className="size-3" aria-hidden="true" />
                              Instalada
                            </Badge>
                          ) : (
                            <Button
                              variant="secondary"
                              size="sm"
                              className="h-7 cursor-pointer text-xs gap-1.5 font-medium bg-card border border-border text-foreground hover:bg-[#61afef] hover:text-primary-foreground hover:border-transparent rounded-lg transition-colors"
                              disabled={installing !== null}
                              aria-label={`Instalar ${skill.name}`}
                              onClick={() => {
                                void install(skill);
                              }}
                            >
                              {installing === skill.id ? (
                                <>
                                  <Spinner className="size-3" />
                                  <span>Instalando…</span>
                                </>
                              ) : (
                                <>
                                  <ArrowDownToLine className="size-3" aria-hidden="true" />
                                  <span>Instalar</span>
                                </>
                              )}
                            </Button>
                          )}
                        </div>
                      </div>
                    </Card>
                  );
                })}
              </div>
            )}
          </ScrollArea>


          <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-t border-border/40 pt-3 text-xs text-muted-foreground">
            <span className="tabular-nums">
              {filtered.length} {filtered.length === 1 ? "skill encontrada" : "skills encontradas"}
            </span>
            {debounced && skills.length >= limit && limit < 600 && (
              <Button
                variant="ghost"
                size="sm"
                className="cursor-pointer text-xs"
                onClick={() => setLimit((value) => value + 60)}
              >
                Carregar mais resultados
              </Button>
            )}
            <Pagination className="mx-0 w-auto" aria-label="Paginação do Marketplace">
              <PaginationContent>
                <PaginationItem>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    className="cursor-pointer"
                    aria-label="Página anterior"
                    disabled={currentPage === 1 || !current}
                    onClick={() => setPage(currentPage - 1)}
                  >
                    <ChevronLeft />
                  </Button>
                </PaginationItem>
                <PaginationItem>
                  <span aria-live="polite" className="px-2 tabular-nums">
                    {currentPage} / {pages}
                  </span>
                </PaginationItem>
                <PaginationItem>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    className="cursor-pointer"
                    aria-label="Próxima página"
                    disabled={currentPage === pages || !current}
                    onClick={() => setPage(currentPage + 1)}
                  >
                    <ChevronRight />
                  </Button>
                </PaginationItem>
              </PaginationContent>
            </Pagination>
          </div>
        </DialogContent>
      </Dialog>
      <SkillDetailsDialog selection={selection} onClose={() => setSelection(null)} />
    </>
  );
}
