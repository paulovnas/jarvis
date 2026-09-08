import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { DEFAULT_FILE_TABS, type FileTabsLayout } from "@/core/desktop-layout";
import { fileError, type FilePreview, type PreviewState } from "@/core/project-files";
import { useDesktopLayout } from "./use-desktop-layout";

function cachePreview(current: Record<string, PreviewState>, key: string, preview: PreviewState) {
  const next = { ...current };
  delete next[key];
  next[key] = preview;
  // Bound file contents across projects as well as the number of visible tabs.
  for (const oldest of Object.keys(next).slice(0, -30)) delete next[oldest];
  return next;
}

export function useProjectFiles(projectId: string | null) {
  const { layout, updateLayout } = useDesktopLayout();
  const tabs = projectId ? layout.fileTabs[projectId] ?? DEFAULT_FILE_TABS : DEFAULT_FILE_TABS;
  const [previews, setPreviews] = useState<Record<string, PreviewState>>({});
  const versions = useRef(new Map<string, number>());
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  const remember = useCallback((update: (current: FileTabsLayout) => FileTabsLayout) => {
    if (!projectId) return;
    updateLayout(current => ({ fileTabs: { ...current.fileTabs, [projectId]: update(current.fileTabs[projectId] ?? DEFAULT_FILE_TABS) } }));
  }, [projectId, updateLayout]);

  const load = useCallback(async (path: string) => {
    if (!projectId) return;
    const key = JSON.stringify([projectId, path]);
    const version = (versions.current.get(key) ?? 0) + 1;
    versions.current.set(key, version);
    setPreviews(current => cachePreview(current, key, { loading: true }));
    try {
      const data = await invoke<FilePreview>("read_project_file", { projectId, path });
      if (mounted.current && versions.current.get(key) === version) setPreviews(current => cachePreview(current, key, { loading: false, data }));
    } catch (error) {
      if (mounted.current && versions.current.get(key) === version) setPreviews(current => cachePreview(current, key, { loading: false, error: fileError(error) }));
    }
  }, [projectId]);

  const activeKey = projectId && tabs.activePath ? JSON.stringify([projectId, tabs.activePath]) : null;
  const active = activeKey ? previews[activeKey] : undefined;
  useEffect(() => {
    if (tabs.activePath && !active) void load(tabs.activePath);
  }, [tabs.activePath, active, load]);

  const open = useCallback((path: string) => {
    if (tabs.paths.length >= 30 && !tabs.paths.includes(path)) { toast.error("Feche uma aba antes de abrir mais arquivos (limite de 30)."); return; }
    remember(current => ({ paths: current.paths.includes(path) ? current.paths : [...current.paths, path], activePath: path }));
  }, [remember, tabs.paths]);
  const select = useCallback((path: string | null) => remember(current => ({ ...current, activePath: path === null || current.paths.includes(path) ? path : null })), [remember]);
  const close = useCallback((path: string) => {
    if (projectId) {
      const key = JSON.stringify([projectId, path]);
      versions.current.set(key, (versions.current.get(key) ?? 0) + 1);
      setPreviews(current => { const next = { ...current }; delete next[key]; return next; });
    }
    remember(current => {
      const index = current.paths.indexOf(path);
      const paths = current.paths.filter(item => item !== path);
      return { paths, activePath: current.activePath === path ? paths[Math.max(0, index - 1)] ?? null : current.activePath };
    });
  }, [projectId, remember]);
  const refresh = useCallback(() => { if (tabs.activePath) void load(tabs.activePath); }, [tabs.activePath, load]);
  return { projectId, tabs, active: active ?? { loading: true }, open, select, close, refresh };
}

export type ProjectFilesController = ReturnType<typeof useProjectFiles>;
