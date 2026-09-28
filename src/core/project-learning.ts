import { z } from "zod";

export const lessonSchema = z.object({
  id: z.string(), scope: z.string(), content: z.string(), topics: z.array(z.string()), check: z.string(),
  status: z.enum(["active", "suggested", "disabled"]), origin: z.enum(["feedback", "user", "imported"]),
  revision: z.number(), updatedAt: z.number(),
  evidence: z.array(z.object({ conversationId: z.string(), messageId: z.string(), excerpt: z.string(), createdAt: z.number() })),
});
export const learningSnapshotSchema = z.object({ enabled: z.boolean(), revision: z.number(), lessons: z.array(lessonSchema), pending: z.number(), notice: z.string().nullable() });
export type ProjectLesson = z.infer<typeof lessonSchema>;
export type LearningSnapshot = z.infer<typeof learningSnapshotSchema>;
export const LESSON_STATUS = { active: "Ativo", suggested: "Sugestão", disabled: "Desativado" } as const;
export const lessonEdit = (lesson: ProjectLesson) => ({
  id: lesson.id, revision: lesson.revision, scope: lesson.scope, content: lesson.content,
  topics: lesson.topics, check: lesson.check, status: lesson.status,
});
