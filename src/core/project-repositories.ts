import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";

export const PROJECT_REPOSITORIES_CHANGED = "project:repositories-changed";

export const projectRepositorySchema = z.object({
  id: z.string(),
  projectId: z.string(),
  path: z.string(),
  directory: z.string(),
  name: z.string(),
  description: z.string(),
  branch: z.string().nullable(),
  upstream: z.string().nullable(),
  ahead: z.number().int().nonnegative(),
  behind: z.number().int().nonnegative(),
  staged: z.number().int().nonnegative(),
  unstaged: z.number().int().nonnegative(),
  untracked: z.number().int().nonnegative(),
  remoteUrl: z.string().nullable(),
  available: z.boolean(),
  error: z.string().nullable(),
  createdAt: z.number().int(),
  updatedAt: z.number().int(),
});

export type ProjectRepository = z.infer<typeof projectRepositorySchema>;
export type ProjectRepositoryInput = Pick<ProjectRepository, "name" | "description" | "directory"> & { id?: string };

export async function getProjectRepositories(projectId: string, includeDefault = false) {
  return projectRepositorySchema.array().parse(await invoke("get_project_repositories", { projectId, includeDefault }));
}

export async function saveProjectRepository(projectId: string, repository: ProjectRepositoryInput) {
  return projectRepositorySchema.parse(await invoke("save_project_repository", { projectId, repository }));
}

export async function deleteProjectRepository(projectId: string, repositoryId: string) {
  await invoke("delete_project_repository", { projectId, repositoryId, confirmed: true });
}
