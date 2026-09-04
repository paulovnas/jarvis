export type MessageRole = "user" | "assistant";

export type ToolStatus = "completed" | "running" | "error";

export interface ToolCallItem {
  id: string;
  name: string;
  status: ToolStatus;
  durationMs?: number;
  args?: Record<string, unknown>;
  output?: string;
  error?: string;
  diffStats?: { added: number; removed: number };
}

export interface AssistantWorkData {
  durationSeconds: number;
  thinking: string;
  tools: ToolCallItem[];
}

export interface MessageAttachment {
  id: string;
  name: string;
  size?: string;
  type: "file" | "image" | "code";
}

export interface ChatErrorData {
  title: string;
  message: string;
  command?: string;
}

export interface ChatMessage {
  id: string;
  role: MessageRole;
  content: string;
  timestamp: string;
  attachments?: MessageAttachment[];
  work?: AssistantWorkData;
  error?: ChatErrorData;
  streaming?: boolean;
}

export type CollaborationMode = "build" | "plan";

export type AiModel =
  | "gemini-2.5-pro"
  | "gemini-2.5-flash"
  | "gemini-2.0-flash-thinking"
  | "gpt-4o"
  | "gpt-4o-mini"
  | "o3-mini";
