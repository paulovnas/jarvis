const WINDOWS_ABSOLUTE_PATH = /^(?:[a-zA-Z]:[\\/]|\\\\)/;

/**
 * Joins a project root with a backend-provided relative file path without
 * allowing the resulting path to escape the project directory.
 */
export function resolveProjectFilePath(projectPath: string, relativePath: string): string | null {
  if (!projectPath || !relativePath || (!projectPath.startsWith("/") && !WINDOWS_ABSOLUTE_PATH.test(projectPath))) return null;
  if (relativePath.startsWith("/") || relativePath.startsWith("\\") || /^[a-zA-Z]:/.test(relativePath)) return null;

  const parts = relativePath.split(/[\\/]/);
  if (parts.some(part => !part || part === "." || part === ".." || part.includes("\0"))) return null;

  const separator = projectPath.includes("\\") && !projectPath.includes("/") ? "\\" : "/";
  const root = projectPath.replace(/[\\/]+$/, "") || (projectPath.startsWith("/") ? "/" : projectPath);
  return `${root}${root.endsWith(separator) ? "" : separator}${parts.join(separator)}`;
}
