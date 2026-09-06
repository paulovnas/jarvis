import { expect, it } from "vitest";
import { releaseManifest, releaseVersion, replaceCargoVersion, targetPlatforms } from "./release-plan";

it("aceita a primeira publicação e versões posteriores sem permitir downgrade ou tags ambíguas", () => {
  expect(releaseVersion("0.8.0-beta.1", "0.8.0-beta.1")).toBe("0.8.0-beta.1");
  expect(releaseVersion("0.8.0-beta.10", "0.8.0-beta.2")).toBe("0.8.0-beta.10");
  expect(releaseVersion("0.8.0", "0.8.0-beta.10")).toBe("0.8.0");
  for (const invalid of ["0.7.0", "v0.8.1", "0.8", "0.8.1+build", "../../tag"]) expect(() => releaseVersion(invalid, "0.8.0")).toThrow();
});

it("gera manifestos assinados para cada arquitetura de um app universal", () => {
  const result = releaseManifest("0.8.0-beta.2", "universal-apple-darwin", "jarvis.app.tar.gz", " signature\n", "Notas", new Date("2026-09-06T12:00:00Z"));
  expect(result.platforms["darwin-aarch64"]).toEqual({ signature: "signature", url: "https://github.com/paulovnas/jarvis/releases/download/v0.8.0-beta.2/jarvis.app.tar.gz" });
  expect(result.platforms["darwin-x86_64"]).toEqual(result.platforms["darwin-aarch64"]);
  expect(result.notes).toBe("Notas");
  expect(() => releaseManifest("0.8.0", "aarch64-apple-darwin", "file", "", "")).toThrow();
  expect(() => targetPlatforms("unknown")).toThrow();
});

it("altera somente a versão do pacote Jarvis no Cargo e mantém dependências intactas", () => {
  const contents = '[package]\nname = "jarvis"\nversion = "0.1.0"\n\n[dependencies]\nserde = "1"\n';
  expect(replaceCargoVersion(contents, "0.8.0-beta.1")).toContain('version = "0.8.0-beta.1"');
  expect(replaceCargoVersion(contents, "0.8.0-beta.1")).toContain('serde = "1"');
  const lock = '[[package]]\nname = "other"\nversion = "0.1.0"\n\n[[package]]\nname = "jarvis"\nversion = "0.1.0"\n';
  expect(replaceCargoVersion(lock, "0.8.0-beta.1", true)).toBe('[[package]]\nname = "other"\nversion = "0.1.0"\n\n[[package]]\nname = "jarvis"\nversion = "0.8.0-beta.1"\n');
});
