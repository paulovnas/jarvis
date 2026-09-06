import { expect, it } from "vitest";
import { notesFromTag, parseReleaseArguments, releaseManifest, releaseVersion, replaceCargoVersion, targetPlatforms, validateCIRequest } from "./release-plan";

it("keeps publication restricted to the official main and explicit version tags", () => {
  expect(() => validateCIRequest("paulovnas/jarvis", "refs/heads/main", "", false)).not.toThrow();
  expect(() => validateCIRequest("paulovnas/jarvis", "refs/heads/main", "v0.8.3-beta.1", true)).not.toThrow();
  for (const ref of ["refs/heads/feature", "refs/pull/1/merge", "refs/tags/v0.8.3", undefined]) expect(() => validateCIRequest("paulovnas/jarvis", ref, "v0.8.3", true)).toThrow();
  expect(() => validateCIRequest("fork/jarvis", "refs/heads/main", "v0.8.3", true)).toThrow();
  for (const tag of ["", "main", "v../main", "v0.8.3\ninjected", "v0.8.3+build", "--help"]) expect(() => validateCIRequest("paulovnas/jarvis", "refs/heads/main", tag, true)).toThrow();
});

it("parses launcher options without allowing ambiguous or unsupported release targets", () => {
  expect(parseReleaseArguments(["0.8.3-beta.1", "--notes-file", "/a path/notes.md", "--dry-run"])).toEqual({ version: "0.8.3-beta.1", notesFile: "/a path/notes.md", dryRun: true });
  for (const options of [["--notes-file"], ["--notes-file", "--dry-run"], ["--target", "universal-apple-darwin"], ["--dry-run", "--dry-run"], ["--force"]]) expect(() => parseReleaseArguments(["0.8.3", ...options])).toThrow();
  expect(notesFromTag("Jarvis 0.8.3\n\nNotas em português.\n", "0.8.3")).toBe("Notas em português.");
  expect(notesFromTag("Jarvis 0.8.3\n", "0.8.3")).toBe("");
  expect(() => notesFromTag("Jarvis 0.8.2\nNotas", "0.8.3")).toThrow();
});

it("aceita a primeira publicação e versões posteriores sem permitir downgrade ou tags ambíguas", () => {
  expect(releaseVersion("0.8.0-beta.1", "0.8.0-beta.1")).toBe("0.8.0-beta.1");
  expect(releaseVersion("0.8.0-beta.10", "0.8.0-beta.2")).toBe("0.8.0-beta.10");
  expect(releaseVersion("0.8.0", "0.8.0-beta.10")).toBe("0.8.0");
  for (const invalid of ["0.7.0", "v0.8.1", "0.8", "0.8.1+build", "../../tag"]) expect(() => releaseVersion(invalid, "0.8.0")).toThrow();
});

it("gera manifestos assinados para cada arquitetura de um app universal", () => {
  const result = releaseManifest("0.8.1-beta.1", "universal-apple-darwin", "Jarvis.app.tar.gz", " signature\n", "Notas", new Date("2026-09-06T12:00:00Z"));
  expect(result.platforms["darwin-aarch64"]).toEqual({ signature: "signature", url: "https://github.com/paulovnas/jarvis/releases/download/v0.8.1-beta.1/Jarvis.app.tar.gz" });
  expect(result.platforms["darwin-x86_64"]).toEqual(result.platforms["darwin-aarch64"]);
  expect(result.notes).toBe("Notas");
  expect(() => releaseManifest("0.8.0", "aarch64-apple-darwin", "file", "", "")).toThrow();
  expect(() => targetPlatforms("unknown")).toThrow();
});

it("altera somente a versão do pacote Jarvis no Cargo e mantém dependências intactas", () => {
  const contents = '[package]\nname = "jarvis"\nversion = "0.1.0"\n\n[dependencies]\nserde = "1"\n';
  expect(replaceCargoVersion(contents, "0.8.0-beta.1")).toContain('version = "0.8.0-beta.1"');
  expect(replaceCargoVersion(contents, "0.8.0-beta.1")).toContain('serde = "1"');
  const windowsContents = contents.replaceAll("\n", "\r\n");
  expect(replaceCargoVersion(windowsContents, "0.8.0-beta.1")).toBe(windowsContents.replace('version = "0.1.0"', 'version = "0.8.0-beta.1"'));
  const lock = '[[package]]\nname = "other"\nversion = "0.1.0"\n\n[[package]]\nname = "jarvis"\nversion = "0.1.0"\n';
  expect(replaceCargoVersion(lock, "0.8.0-beta.1", true)).toBe('[[package]]\nname = "other"\nversion = "0.1.0"\n\n[[package]]\nname = "jarvis"\nversion = "0.8.0-beta.1"\n');
  const windowsLock = lock.replaceAll("\n", "\r\n");
  expect(replaceCargoVersion(windowsLock, "0.8.0-beta.1", true)).toBe(windowsLock.replace('name = "jarvis"\r\nversion = "0.1.0"', 'name = "jarvis"\r\nversion = "0.8.0-beta.1"'));
});
