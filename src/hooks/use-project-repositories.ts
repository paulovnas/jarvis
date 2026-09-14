import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getProjectRepositories, PROJECT_REPOSITORIES_CHANGED, type ProjectRepository } from "@/core/project-repositories";
import { libraryError } from "@/core/library";

type Result = { projectId: string; repositories: ProjectRepository[]; loading: boolean; error?: string };

export function useProjectRepositories(projectId: string | null, enabled = true, includeDefault = false) {
  const [result, setResult] = useState<Result | null>(null);
  const generation = useRef(0);
  const refresh = useCallback(async () => {
    if (!projectId || !enabled) return;
    const request = ++generation.current;
    setResult(current => current?.projectId === projectId ? { ...current, loading: true, error: undefined } : { projectId, repositories: [], loading: true });
    try {
      const repositories = await getProjectRepositories(projectId, includeDefault);
      if (generation.current === request) setResult({ projectId, repositories, loading: false });
    } catch (cause) {
      if (generation.current === request) setResult({ projectId, repositories: [], loading: false, error: libraryError(cause, "Não foi possível consultar os repositórios do projeto.") });
    }
  }, [enabled, includeDefault, projectId]);

  useEffect(() => {
    if (!projectId || !enabled) { generation.current += 1; return; }
    const request = ++generation.current;
    void getProjectRepositories(projectId, includeDefault).then(repositories => {
      if (generation.current === request) setResult({ projectId, repositories, loading: false });
    }).catch(cause => {
      if (generation.current === request) setResult({ projectId, repositories: [], loading: false, error: libraryError(cause, "Não foi possível consultar os repositórios do projeto.") });
    });
    let active = true;
    let unlisten: (() => void) | undefined;
    void listen<string>(PROJECT_REPOSITORIES_CHANGED, event => {
      if (active && event.payload === projectId) void refresh();
    }).then(stop => { if (active) unlisten = stop; else stop(); }).catch(() => {});
    return () => { active = false; generation.current += 1; unlisten?.(); };
  }, [enabled, includeDefault, projectId, refresh]);

  const selected = result?.projectId === projectId ? result : null;
  return { repositories: selected?.repositories ?? [], loading: Boolean(projectId && enabled && (!selected || selected.loading)), error: selected?.error, refresh };
}
