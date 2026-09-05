export type MessageRole = "user" | "assistant";

export type ToolStatus = "pending" | "completed" | "running" | "error";

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
  steps: {
    thinking: string;
    commentary: string;
    tools: ToolCallItem[];
  }[];
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
  parts?: import("@/core/chat").MessagePart[];
  id: string;
  role: MessageRole;
  content: string;
  timestamp: string;
  attachments?: MessageAttachment[];
  work?: AssistantWorkData;
  error?: ChatErrorData;
  streaming?: boolean;
  model?: string;
}

export type CollaborationMode = "build" | "plan";
