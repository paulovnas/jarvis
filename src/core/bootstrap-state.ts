export type BootstrapStepId = "configuration" | "core" | "providers" | "skills" | "workspace";
export type BootstrapStepStatus = "pending" | "running" | "complete" | "warning";

export type BootstrapProgressEvent = {
  id: BootstrapStepId;
  progress: number;
  status: BootstrapStepStatus;
  detail: string;
};

export type BootstrapProgressState = Record<BootstrapStepId, BootstrapProgressEvent>;

export const BOOTSTRAP_STEPS: ReadonlyArray<{
  id: BootstrapStepId;
  label: string;
  weight: number;
}> = [
  { id: "configuration", label: "Configuração local", weight: 10 },
  { id: "core", label: "Ferramentas do Core", weight: 27 },
  { id: "providers", label: "Provedores e limites", weight: 25 },
  { id: "skills", label: "Skills instaladas", weight: 23 },
  { id: "workspace", label: "Workspaces e conversas", weight: 15 },
];

export function initialBootstrapProgress(): BootstrapProgressState {
  return Object.fromEntries(BOOTSTRAP_STEPS.map((step) => [step.id, {
    id: step.id,
    progress: 0,
    status: "pending" as const,
    detail: "Aguardando",
  }])) as BootstrapProgressState;
}

export function bootstrapPercent(state: BootstrapProgressState): number {
  return Math.round(BOOTSTRAP_STEPS.reduce(
    (total, step) => total + state[step.id].progress * step.weight,
    0,
  ));
}
