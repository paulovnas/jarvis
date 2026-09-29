export const httpRunLabels = { running: "Em execução", completed: "Concluída", failed: "Falha de transporte", cancelled: "Cancelada", interrupted: "Interrompida" } as const;
export function httpSize(bytes: number) { return bytes < 1024 ? `${bytes} B` : bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(1)} KB` : `${(bytes / (1024 * 1024)).toFixed(1)} MB`; }
