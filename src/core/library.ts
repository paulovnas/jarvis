export interface Workspace {
  id: string;
  name: string;
  createdAt: number;
}

export interface Project extends Workspace {
  workspaceId: string;
  path: string;
}

export interface Conversation {
  id: string;
  projectId: string;
  title: string;
  createdAt: number;
  lastActivityAt?: number;
}

export interface LibrarySelection {
  workspaceId: string | null;
  projectId: string | null;
  conversationId: string | null;
}

export interface LibrarySnapshot {
  workspaces: Workspace[];
  projects: Project[];
  conversations: Conversation[];
  selection: LibrarySelection;
}

export interface LibraryTarget {
  kind: "workspace" | "project" | "conversation";
  id: string;
}

export interface LibraryDeleteTarget {
  kind: "project" | "conversation";
  id: string;
}

export interface ConversationDetails {
  conversation: Conversation;
  project: Project;
  workspace: Workspace;
}

function invalid(): never {
  throw new Error(
    "Os dados recebidos do Jarvis são inválidos. Tente atualizar a lista.",
  );
}

function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) invalid();
  return value as Record<string, unknown>;
}

function text(value: unknown): string {
  if (typeof value !== "string" || !value) invalid();
  return value;
}

function timestamp(value: unknown): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0)
    invalid();
  return value;
}

function workspace(value: unknown): Workspace {
  const item = record(value);
  return {
    id: text(item.id),
    name: text(item.name),
    createdAt: timestamp(item.createdAt),
  };
}

function project(value: unknown): Project {
  const item = record(value);
  return {
    ...workspace(item),
    workspaceId: text(item.workspaceId),
    path: text(item.path),
  };
}

function conversation(value: unknown): Conversation {
  const item = record(value);
  return {
    id: text(item.id),
    projectId: text(item.projectId),
    title: text(item.title),
    createdAt: timestamp(item.createdAt),
    lastActivityAt: timestamp(item.lastActivityAt ?? item.createdAt),
  };
}

function list<T extends { id: string }>(
  value: unknown,
  parse: (item: unknown) => T,
): T[] {
  if (!Array.isArray(value)) invalid();
  const items = value.map(parse);
  if (new Set(items.map((item) => item.id)).size !== items.length) invalid();
  return items;
}

export function readLibrarySnapshot(value: unknown): LibrarySnapshot {
  const item = record(value);
  const workspaces = list(item.workspaces, workspace);
  const projects = list(item.projects, project);
  const conversations = list(item.conversations, conversation);
  const selected = record(item.selection);
  const optionalId = (id: unknown) => (id === null ? null : text(id));
  const selection = {
    workspaceId: optionalId(selected.workspaceId),
    projectId: optionalId(selected.projectId),
    conversationId: optionalId(selected.conversationId),
  };
  const workspaceIds = new Set(workspaces.map((entry) => entry.id));
  const projectMap = new Map(projects.map((entry) => [entry.id, entry]));
  const conversationMap = new Map(
    conversations.map((entry) => [entry.id, entry]),
  );
  if (
    projects.some((entry) => !workspaceIds.has(entry.workspaceId)) ||
    conversations.some((entry) => !projectMap.has(entry.projectId)) ||
    (selection.workspaceId !== null &&
      !workspaceIds.has(selection.workspaceId)) ||
    (selection.projectId !== null &&
      projectMap.get(selection.projectId)?.workspaceId !==
        selection.workspaceId) ||
    (selection.conversationId !== null &&
      conversationMap.get(selection.conversationId)?.projectId !==
        selection.projectId)
  )
    invalid();
  return { workspaces, projects, conversations, selection };
}

export function readConversationDetails(value: unknown): ConversationDetails {
  const item = record(value);
  const details = {
    conversation: conversation(item.conversation),
    project: project(item.project),
    workspace: workspace(item.workspace),
  };
  if (
    details.conversation.projectId !== details.project.id ||
    details.project.workspaceId !== details.workspace.id
  )
    invalid();
  return details;
}

export function libraryError(error: unknown, fallback: string): string {
  if (
    error &&
    typeof error === "object" &&
    "message" in error &&
    typeof error.message === "string"
  )
    return error.message;
  return fallback;
}
