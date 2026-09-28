import { z } from "zod";

export const KNOWLEDGE_KINDS = {
  product: { label: "Produto", file: "prd.md", description: "Propósito, público, funcionalidades e limites do produto." },
  technical: { label: "Técnico", file: "trd.md", description: "Stack, arquitetura, integrações e decisões técnicas." },
  rules: { label: "Regras", file: "rules.md", description: "Convenções e restrições que orientam o trabalho dos agentes." },
  design: { label: "Design", file: "design.md", description: "Identidade visual, componentes, tipografia e padrões de interação." },
} as const;
export type KnowledgeKind = keyof typeof KNOWLEDGE_KINDS;
const sourceSchema = z.object({ path: z.string(), fingerprint: z.string() });
export const knowledgeDocumentSchema = z.object({
  kind: z.enum(["product", "technical", "rules", "design"]), scope: z.string(), path: z.string(),
  content: z.string(), essential: z.string(), revision: z.string(),
  sources: z.array(sourceSchema), staleSources: z.array(z.string()), error: z.string().nullable(),
});
export const knowledgeSnapshotSchema = z.object({ documents: z.array(knowledgeDocumentSchema), scopes: z.array(z.string()) });
export const knowledgeDraftSchema = z.object({ content: z.string(), revision: z.string(), sources: z.array(sourceSchema) });
export type KnowledgeDocument = z.infer<typeof knowledgeDocumentSchema>;
export type KnowledgeSnapshot = z.infer<typeof knowledgeSnapshotSchema>;
export type KnowledgeDraft = z.infer<typeof knowledgeDraftSchema>;
export const knowledgeKey = (document: Pick<KnowledgeDocument, "scope" | "kind">) => `${document.scope}:${document.kind}`;
export const knowledgeChanged = (draft: KnowledgeDocument, saved: KnowledgeDocument) =>
  draft.content !== saved.content || draft.essential !== saved.essential || JSON.stringify(draft.sources) !== JSON.stringify(saved.sources);
