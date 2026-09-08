import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import {
  libraryError,
  readLibrarySnapshot,
  type LibrarySnapshot,
  type LibraryTarget,
  type LibraryDeleteTarget,
} from "@/core/library";

export function useLibrary() {
  const [snapshot, setSnapshot] = useState<LibrarySnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const generation = useRef(0);
  const mutation = useRef(false);
  const refreshQueued = useRef(false);

  const load = useCallback(() => {
    if (mutation.current) return Promise.resolve();
    const request = ++generation.current;
    return invoke<unknown>("get_library_snapshot")
      .then((result) => {
        const loaded = readLibrarySnapshot(result);
        if (generation.current === request) setSnapshot(loaded);
      })
      .catch((cause: unknown) => {
        if (generation.current === request)
          setError(
            libraryError(cause, "Não foi possível carregar seus projetos."),
          );
      })
      .finally(() => {
        if (generation.current === request) setLoading(false);
      });
  }, []);

  const refresh = async () => {
    if (mutation.current) return;
    setLoading(true);
    setError(null);
    await load();
  };

  useEffect(() => {
    void load();
    let active = true;
    let dispose: (() => void) | undefined;
    void listen("library:changed", () => {
      if (mutation.current) refreshQueued.current = true;
      else void load();
    }).then(unlisten => {
      if (active) dispose = unlisten;
      else unlisten();
    }).catch(() => {});
    return () => {
      active = false;
      dispose?.();
      generation.current += 1;
    };
  }, [load]);

  const perform = async (
    command: string,
    args: Record<string, unknown>,
    success?: string,
  ): Promise<boolean> => {
    if (mutation.current) return false;
    mutation.current = true;
    const request = ++generation.current;
    setPending(true);
    setError(null);
    try {
      const result = await invoke<unknown>(command, args);
      if (generation.current !== request) return false;
      if (command === "add_project" && result === null) return false;
      setSnapshot(readLibrarySnapshot(result));
      if (success) toast.success(success);
      return true;
    } catch (cause) {
      if (generation.current === request)
        setError(
          libraryError(
            cause,
            "Não foi possível concluir a operação. Tente novamente.",
          ),
        );
      return false;
    } finally {
      mutation.current = false;
      if (generation.current === request) {
        setPending(false);
        setLoading(false);
      }
      if (refreshQueued.current) {
        refreshQueued.current = false;
        void load();
      }
    }
  };

  return {
    snapshot,
    loading,
    pending,
    error,
    refresh,
    clearError: () => setError(null),
    createWorkspace: (name: string) =>
      perform("create_workspace", { name }, "Workspace criado"),
    addProject: (workspaceId: string) =>
      perform("add_project", { workspaceId }, "Projeto adicionado"),
    createConversation: (projectId: string) =>
      perform("create_conversation", { projectId }, "Conversa criada"),
    renameProject: (id: string, name: string) =>
      perform("rename_project", { id, name }, "Projeto atualizado"),
    moveProject: (id: string, workspaceId: string) =>
      perform("move_project_workspace", { id, workspaceId }, "Projeto movido"),
    renameConversation: (id: string, title: string) =>
      perform("rename_conversation", { id, title }, "Conversa atualizada"),
    deleteItem: (target: LibraryDeleteTarget) =>
      perform("delete_library_item", { target, confirmed: true }, target.kind === "workspace" ? "Workspace e históricos excluídos" : target.kind === "project" ? "Projeto e históricos excluídos" : "Conversa excluída"),
    select: (target: LibraryTarget) =>
      perform("select_library_item", { target }),
  };
}

export type LibraryController = ReturnType<typeof useLibrary>;
