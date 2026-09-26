import { Check, ChevronDown, RefreshCw, TerminalSquare } from "lucide-react";
import { executorOf, type ExecutionSelection } from "@/core/executors";
import type { ProviderAccount, ProviderModel } from "@/core/provider-accounts";
import { ProviderIcon } from "@/components/ProviderIcon";
import { aliasSuffix } from "@/core/provider-usage";
import { reasoningLabel } from "@/core/reasoning";
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuSub, DropdownMenuSubContent, DropdownMenuSubTrigger, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Hint } from "@/components/ui/hint";

export interface ModelOptionDef extends Pick<ProviderModel, "reasoningLevels" | "defaultReasoningLevel"> {
  value: string;
  label: string;
}

export interface ProviderModelGroup {
  provider: string;
  executor?: "claude";
  providerKind?: ProviderAccount["providerKind"];
  models: ModelOptionDef[];
  emptyMessage?: string;
}


export type ModelSelection = ExecutionSelection;
export function ModelPicker({ modelGroups, selection, onSelect, disabled = false, ariaLabel = "Selecionar modelo de IA", showProviderIdentity = false, onRefresh, refreshing = false, emptyMessage = "Conecte um provedor em Configurações." }: { modelGroups: ProviderModelGroup[]; selection?: ModelSelection | null; onSelect: (selection: ModelSelection) => void; disabled?: boolean; ariaLabel?: string; showProviderIdentity?: boolean; onRefresh?: () => void; refreshing?: boolean; emptyMessage?: string }) {
  const currentGroup = modelGroups.find(group => executorOf(group) === executorOf(selection) && group.models.some(model => model.value === selection?.model));
  const currentModelDef = currentGroup?.models.find(model => model.value === selection?.model);
  const reasoning = selection?.reasoning;
  const displayModelLabel = currentModelDef ? `${currentModelDef.label}${reasoning ? ` · ${reasoningLabel(reasoning)}` : ""}` : selection ? `${selection.model.split("/").pop()} · Indisponível` : modelGroups.some(group => group.models.length) ? "Escolher modelo" : "Nenhum modelo conectado";
  const providerLabel = showProviderIdentity && currentGroup ? aliasSuffix(currentGroup.provider) : null;
  const displayLabel = providerLabel ? `${providerLabel} · ${displayModelLabel}` : displayModelLabel;
  return (<DropdownMenu>
              <DropdownMenuTrigger
                aria-label={ariaLabel}
                disabled={disabled}
                className="composer-model flex h-7.5 max-w-full cursor-pointer items-center gap-1 rounded-md border-0 bg-transparent px-2 font-mono text-[10px] font-medium text-foreground shadow-none transition-colors hover:bg-secondary hover:text-foreground focus-visible:ring-1 focus-visible:ring-ring"
              >
                {providerLabel && (currentGroup?.executor === "claude" ? <TerminalSquare className="size-3.5 text-onedark-cyan" aria-hidden="true" /> : <ProviderIcon kind={currentGroup?.providerKind ?? "custom"} className="size-3.5 text-onedark-cyan" />)}
                <Hint content={showProviderIdentity ? displayLabel : selection?.model} whenTruncated><span className="min-w-0 truncate">{displayLabel}</span></Hint>
                <ChevronDown className="size-3 shrink-0 text-muted-foreground" />
              </DropdownMenuTrigger>

              <DropdownMenuContent
                align="end"
                side="top"
                sideOffset={8}
                className="min-w-[220px] border-border bg-card p-1.5 text-foreground"
              >
                {modelGroups.length === 0 ? (
                  <DropdownMenuGroup>
                    <DropdownMenuLabel className="px-2.5 py-2 text-xs font-normal text-muted-foreground">
                      {emptyMessage}
                    </DropdownMenuLabel>
                  </DropdownMenuGroup>
                ) : (
                  modelGroups.map((group) => (
                    <DropdownMenuSub key={group.provider}>
                        <DropdownMenuSubTrigger className="cursor-pointer gap-2 py-2 font-mono text-xs text-onedark-cyan">
                          {group.executor === "claude" ? <TerminalSquare className="size-4" aria-hidden="true" /> : <ProviderIcon kind={group.providerKind ?? "custom"} className="size-4" />}
                          {group.provider}
                        </DropdownMenuSubTrigger>
                      <DropdownMenuSubContent className="max-h-[min(480px,70vh)] min-w-[220px] overflow-y-auto border-border bg-card p-1.5 text-foreground">
                        {!group.models.length && <DropdownMenuGroup><DropdownMenuLabel className="max-w-64 whitespace-normal text-xs font-normal text-muted-foreground">{group.emptyMessage ?? "Nenhum modelo disponível."}</DropdownMenuLabel></DropdownMenuGroup>}
                        {group.models.map((option) => {
                          const isSelected = group === currentGroup && option.value === currentModelDef?.value;

                          if (option.reasoningLevels.length > 0) {
                            return (
                              <DropdownMenuSub key={option.value}>
                                <DropdownMenuSubTrigger
                                  className={`cursor-pointer py-1.5 pl-3 pr-2 text-xs hover:bg-secondary ${
                                    isSelected
                                      ? "font-medium text-[#61afef]"
                                      : "text-foreground"
                                  }`}
                                >
                                  <span className="flex-1 truncate">{option.label}</span>
                                  {isSelected && reasoning && (
                                    <span className="mr-1 text-[10px] text-muted-foreground">
                                      {reasoningLabel(reasoning)}
                                    </span>
                                  )}
                                </DropdownMenuSubTrigger>
                                <DropdownMenuSubContent className="min-w-[130px] border-border bg-card p-1 text-foreground">
                                  <DropdownMenuGroup>
                                    <DropdownMenuLabel className="px-2 py-1 text-[10px] font-medium text-muted-foreground">
                                      Raciocínio
                                    </DropdownMenuLabel>
                                    {option.reasoningLevels.map((level) => (
                                      <DropdownMenuItem
                                        key={level}
                                        onClick={() => {
                                          onSelect({ ...(group.executor ? { executor: group.executor } : {}), model: option.value, reasoning: level });
                                        }}
                                        className={`flex cursor-pointer items-center justify-between px-2.5 py-1.5 text-xs hover:bg-secondary ${
                                          isSelected && reasoning === level
                                            ? "bg-secondary/50 font-medium text-[#61afef]"
                                            : "text-foreground"
                                        }`}
                                      >
                                        <span>{reasoningLabel(level)}</span>
                                        {isSelected && reasoning === level && (
                                          <Check className="size-3 text-[#61afef]" />
                                        )}
                                      </DropdownMenuItem>
                                    ))}
                                  </DropdownMenuGroup>
                                </DropdownMenuSubContent>
                              </DropdownMenuSub>
                            );
                          }

                          return (
                            <DropdownMenuItem
                              key={option.value}
                              onClick={() => {
                                onSelect({ ...(group.executor ? { executor: group.executor } : {}), model: option.value, reasoning: null });
                              }}
                              className={`flex cursor-pointer items-center justify-between py-1.5 pl-3 pr-2 text-xs hover:bg-secondary ${
                                isSelected
                                  ? "font-medium text-[#61afef]"
                                  : "text-foreground"
                              }`}
                            >
                              <span>{option.label}</span>
                              {isSelected && <Check className="size-3 text-[#61afef]" />}
                            </DropdownMenuItem>
                          );
                        })}
                      </DropdownMenuSubContent>
                    </DropdownMenuSub>
                  ))
                )}
                {onRefresh && <>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem className="cursor-pointer gap-2 text-xs" disabled={refreshing} onClick={onRefresh}>
                    <RefreshCw className={`size-3.5 ${refreshing ? "animate-spin motion-reduce:animate-none" : ""}`} />
                    {refreshing ? "Atualizando modelos…" : "Atualizar lista de modelos"}
                  </DropdownMenuItem>
                </>}
              </DropdownMenuContent>
            </DropdownMenu>);
}
