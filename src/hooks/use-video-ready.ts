import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { z } from "zod";
import { isVideoFile } from "@/core/project-files";
import type { ChatSnapshot } from "@/core/chat";
import type { ProjectFilesController } from "./use-project-files";

const readySchema = z.object({ projectId: z.string(), conversationId: z.string(), path: z.string().min(1) });
const renderSchema = z.object({ sessionId: z.string().min(1), status: z.literal("completed"), exitCode: z.literal(0), action: z.literal("render"), path: z.string().min(1) });
const productionSchema = z.object({
  sessionId: z.string().min(1), resource: z.literal("openmontage"), status: z.literal("completed"), exitCode: z.literal(0), action: z.literal("tool"),
  result: z.object({ success: z.literal(true), videos: z.array(z.object({ path: z.string().min(1), verified: z.boolean() })) }),
});

function firstPresentation(processed: Set<string>, key: string) {
  if (processed.has(key)) return false;
  processed.add(key);
  // ponytail: remember 1,000 outputs per app session; persist IDs for permanent dismissal.
  if (processed.size > 1000) {
    const oldest = processed.values().next().value;
    if (oldest !== undefined) processed.delete(oldest);
  }
  return true;
}

export function useVideoReady(projectId: string, conversationId: string, files?: ProjectFilesController, snapshot?: ChatSnapshot | null) {
  const controller = useRef(files);
  const processed = files?.presentedVideos;
  useEffect(() => { controller.current = files; }, [files]);
  const enabled = files?.projectId === projectId;
  useEffect(() => {
    if (!enabled || !processed || snapshot?.conversationId !== conversationId) return;
    for (const turn of snapshot.turns) for (const step of turn.steps) for (const tool of step.tools) {
      if (tool.status !== "completed" || !["video_run", "video_wait", "bash_wait"].includes(tool.name)) continue;
      try {
        const value: unknown = JSON.parse(tool.output);
        const receipt = renderSchema.safeParse(value);
        const production = productionSchema.safeParse(value);
        const paths = receipt.success ? [receipt.data.path] : production.success ? production.data.result.videos.filter(video => video.verified).map(video => video.path) : [];
        for (const path of paths) {
          if (!isVideoFile(path)) continue;
          const key = JSON.stringify([projectId, conversationId, path]);
          if (!firstPresentation(processed, key)) continue;
          if (!controller.current?.tabs.paths.includes(path)) controller.current?.open(path, true);
        }
      } catch { /* Other video actions and malformed tool output are not render receipts. */ }
    }
  }, [projectId, conversationId, snapshot, enabled, processed]);
  useEffect(() => {
    if (!enabled || !processed) return;
    let alive = true;
    const subscription = listen<unknown>("video:ready", event => {
      const result = readySchema.safeParse(event.payload);
      if (!alive || !result.success) return;
      const ready = result.data;
      if (ready.projectId !== projectId || ready.conversationId !== conversationId || !isVideoFile(ready.path)) return;
      const key = JSON.stringify([projectId, conversationId, ready.path]);
      if (!firstPresentation(processed, key)) return;
      if (!controller.current?.tabs.paths.includes(ready.path)) controller.current?.open(ready.path, true);
    });
    return () => { alive = false; void subscription.then(unlisten => unlisten()).catch(() => {}); };
  }, [projectId, conversationId, enabled, processed]);
}
