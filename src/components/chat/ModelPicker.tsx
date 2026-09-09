import { Check, ChevronDown } from "lucide-react";
import type { ProviderAccount, ProviderModel } from "@/core/provider-accounts";
import { ProviderIcon } from "@/components/ProviderIcon";
import { reasoningLabel } from "@/core/reasoning";
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSub, DropdownMenuSubContent, DropdownMenuSubTrigger, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";

export interface ModelOptionDef extends Pick<ProviderModel, "reasoningLevels" | "defaultReasoningLevel"> {
  value: string;
  label: string;
}

export interface ProviderModelGroup {
  provider: string;
  providerKind?: ProviderAccount["providerKind"];
  models: ModelOptionDef[];
}


export type ModelSelection = { model: string; reasoning: string | null };
export function ModelPicker({ modelGroups, selection, onSelect, disabled = false, ariaLabel = "Selecionar modelo de IA" }: { modelGroups: ProviderModelGroup[]; selection?: ModelSelection | null; onSelect: (selection: ModelSelection) => void; disabled?: boolean; ariaLabel?: string }) {
  const currentModelDef = modelGroups.flatMap(group => group.models).find(model => model.value === selection?.model);
  const reasoning = selection?.reasoning;
  const displayModelLabel = currentModelDef ? `${currentModelDef.label}${reasoning ? ` · ${reasoningLabel(reasoning)}` : ""}` : selection ? `${selection.model.split("/").pop()} · Indisponível` : modelGroups.length ? "Escolher modelo" : "Nenhum modelo conectado";
  return (<DropdownMenu>
              <DropdownMenuTrigger
                aria-label={ariaLabel}
                title={selection?.model}
                disabled={disabled}
                className="composer-model flex h-7.5 max-w-full cursor-pointer items-center gap-1 rounded-md border-0 bg-transparent px-2 font-mono text-[10px] font-medium text-foreground shadow-none transition-colors hover:bg-secondary hover:text-foreground focus-visible:ring-1 focus-visible:ring-ring"
              >
                <span className="truncate">{displayModelLabel}</span>
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
                      Conecte um provedor em Configurações.
                    </DropdownMenuLabel>
                  </DropdownMenuGroup>
                ) : (
                  modelGroups.map((group) => (
                    <DropdownMenuSub key={group.provider}>
                        <DropdownMenuSubTrigger className="cursor-pointer gap-2 py-2 font-mono text-xs text-onedark-cyan">
                          <ProviderIcon kind={group.providerKind ?? "custom"} className="size-4" />
                          {group.provider}
                        </DropdownMenuSubTrigger>
                      <DropdownMenuSubContent className="max-h-[min(480px,70vh)] min-w-[220px] overflow-y-auto border-border bg-card p-1.5 text-foreground">
                        {group.models.map((option) => {
                          const isSelected = option.value === currentModelDef?.value;

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
                                          onSelect({ model: option.value, reasoning: level });
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
                                onSelect({ model: option.value, reasoning: null });
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
              </DropdownMenuContent>
            </DropdownMenu>);
}
