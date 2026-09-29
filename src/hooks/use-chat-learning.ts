import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ChatSnapshot } from "@/core/chat";
import { learningSnapshotSchema, type ProjectLesson } from "@/core/project-learning";

const empty: ProjectLesson[] = [];

export function useChatLearning(projectId: string | undefined, snapshot: ChatSnapshot): ProjectLesson[] {
  const conversationId = snapshot.conversationId;
  const [result, setResult] = useState<{ projectId: string; conversationId: string; lessons: ProjectLesson[] }>();
  // A tool can save a lesson before the background extractor emits its project event.
  const savedCalls = snapshot.turns.flatMap(turn => turn.steps.flatMap(step => step.tools
    .filter(tool => tool.name === "learn_project" && tool.status === "completed")
    .map(tool => tool.id))).join("|");
  const activeTurnId = snapshot.activeTurnId;

  useEffect(() => {
    if (!projectId) return;
    let active = true;
    let request = 0;
    const reload = async () => {
      const id = ++request;
      try {
        const next = learningSnapshotSchema.parse(await invoke("get_project_learning", { projectId }));
        if (active && id === request) setResult({
          projectId, conversationId,
          lessons: next.lessons.filter(lesson => lesson.evidence.some(source => source.conversationId === conversationId)),
        });
      } catch {
        // Optional context must not interrupt the chat or erase previously loaded lessons.
      }
    };
    const events = listen<string>("project:learning-changed", ({ payload }) => {
      if (active && payload === projectId) void reload();
    });
    // Subscribe before reading so a completed background extraction cannot fall in a gap.
    void events.then(() => { if (active) void reload(); }).catch(() => { if (active) void reload(); });
    return () => { active = false; void events.then(unlisten => unlisten()).catch(() => {}); };
  }, [projectId, conversationId, savedCalls, activeTurnId]);

  return result?.projectId === projectId && result?.conversationId === conversationId ? result.lessons : empty;
}
