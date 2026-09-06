import type { LibrarySnapshot } from "@/core/library";

export function emptyLibrary(): LibrarySnapshot {
  return {
    workspaces: [],
    projects: [],
    conversations: [],
    selection: { workspaceId: null, projectId: null, conversationId: null },
  };
}

export function populatedLibrary(): LibrarySnapshot {
  return {
    workspaces: [
      { id: "w1", name: "Pessoal", createdAt: 1 },
      { id: "w2", name: "Trabalho", createdAt: 2 },
    ],
    projects: [
      {
        id: "p1",
        workspaceId: "w1",
        name: "Jarvis",
        path: "/projects/jarvis",
        createdAt: 1,
      },
      {
        id: "p2",
        workspaceId: "w2",
        name: "Outro projeto",
        path: "/projects/other",
        createdAt: 2,
      },
    ],
    conversations: [
      { id: "c1", projectId: "p1", title: "Primeira conversa", createdAt: 1, lastActivityAt: 1 },
      {
        id: "c2",
        projectId: "p2",
        title: "Conversa do trabalho",
        createdAt: 2,
        lastActivityAt: 2,
      },
    ],
    selection: { workspaceId: "w1", projectId: "p1", conversationId: "c1" },
  };
}

export function conversationDetails() {
  const library = populatedLibrary();
  return {
    workspace: library.workspaces[0],
    project: library.projects[0],
    conversation: library.conversations[0],
  };
}
