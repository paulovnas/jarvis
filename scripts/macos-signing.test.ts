import { execFileSync, spawnSync } from "node:child_process";
import path from "node:path";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { describe, expect, it } from "vitest";
import {
  developmentArguments,
  localSigningIdentity,
  projectIdentifier,
  selectSigningIdentity,
  signedDevArguments,
  signingCommand,
  usesDevelopmentProfile,
} from "./macos-signing";

const firstHash = "A".repeat(40);
const secondHash = "B".repeat(40);
const identities = `  1) ${firstHash} "Apple Development: Local Developer (TEAM)"\n     1 valid identities found`;

describe("macOS signing setup", () => {
  it("automatically selects the single valid application certificate", () => {
    expect(selectSigningIdentity(identities).hash).toBe(firstHash);
  });

  it("refuses to silently switch between multiple certificates", () => {
    const multiple = `${identities}\n  2) ${secondHash} "Developer ID Application: Local Developer (TEAM)"`;
    expect(() => selectSigningIdentity(multiple)).toThrow("mais de um certificado");
    expect(selectSigningIdentity(multiple, secondHash.toLowerCase()).hash).toBe(secondHash);
  });

  it("accepts an explicit certificate name and rejects ad hoc or unavailable identities", () => {
    expect(selectSigningIdentity(identities, "Apple Development: Local Developer (TEAM)").hash).toBe(firstHash);
    for (const requested of ["-", "Missing", secondHash]) {
      expect(() => selectSigningIdentity(identities, requested)).toThrow("exatamente um certificado");
    }
  });

  it("does not select installer certificates or start unsigned when no certificate exists", () => {
    expect(() => selectSigningIdentity(`1) ${firstHash} "Developer ID Installer: Local Developer (TEAM)"`)).toThrow("Nenhum certificado");
    expect(() => selectSigningIdentity("0 valid identities found")).toThrow("Nenhum certificado");
  });

  it("requires an explicit hash when certificate names are ambiguous", () => {
    const sameName = `${identities}\n  2) ${secondHash} "Apple Development: Local Developer (TEAM)"`;
    expect(() => selectSigningIdentity(sameName, "Apple Development: Local Developer (TEAM)")).toThrow("exatamente um certificado");
  });

  it("keeps help and unrelated Tauri commands independent of certificates", () => {
    for (const args of [["info"], ["dev", "--help"], ["help", "build"], ["--version"], ["icon"]]) {
      expect(signingCommand(args)).toBeUndefined();
    }
    expect(signingCommand(["dev", "--", "--", "--help"])).toBe("dev");
    expect(signingCommand(["build", "--debug"])).toBe("build");
    expect(signingCommand(["bundle", "--bundles", "app"])).toBe("bundle");
    expect(usesDevelopmentProfile(["dev"])).toBe(true);
    expect(usesDevelopmentProfile(["build", "--debug"])).toBe(true);
    expect(usesDevelopmentProfile(["bundle", "--debug"])).toBe(true);
    expect(usesDevelopmentProfile(["build", "--", "--debug"])).toBe(false);
    expect(usesDevelopmentProfile(["build"])).toBe(false);
  });

  it("preserves Cargo and app arguments and checkout paths containing spaces", () => {
    const original = ["dev", "--no-watch", "--", "--locked", "--", "hello world"];
    const prepared = signedDevArguments(original, "/tmp/Jarvis Project");
    expect(prepared.slice(0, 2)).toEqual(original.slice(0, 2));
    expect(prepared.slice(4)).toEqual(original.slice(2));
    const config: { build: { runner: { args: string[] } } } = JSON.parse(prepared[3]);
    for (const argument of [config.build.runner.args[1], config.build.runner.args[3]]) {
      const runner: string[] = JSON.parse(argument.slice(argument.indexOf("[")));
      expect(runner).toEqual(["/bin/sh", "/tmp/Jarvis Project/scripts/run-signed-macos.sh"]);
    }
    expect(() => signedDevArguments(["dev", "--runner", "custom"], "/tmp")).toThrow("remova a opção");
  });

  it("applies the isolated development config before Cargo and app arguments", () => {
    const projectRoot = "/tmp/Jarvis Project";
    const developmentConfig = path.join(
      projectRoot,
      "src-tauri/tauri.dev.conf.json",
    );
    const original = ["dev", "--no-watch", "--", "--locked", "--", "hello world"];
    const prepared = developmentArguments(original, projectRoot);
    expect(prepared).toEqual([
      "dev",
      "--no-watch",
      "--config",
      developmentConfig,
      "--",
      "--locked",
      "--",
      "hello world",
    ]);
    expect(developmentArguments(["build"], projectRoot)).toEqual(["build"]);
    expect(developmentArguments(["build", "--debug"], projectRoot)).toEqual([
      "build",
      "--debug",
      "--config",
      developmentConfig,
    ]);
  });

  it.skipIf(process.platform === "win32")("refuses to launch an executable without signing configuration", () => {
    const result = spawnSync("/bin/sh", [path.resolve("scripts/run-signed-macos.sh"), "/untrusted/executable"], {
      env: { PATH: process.env.PATH }, encoding: "utf8",
    });
    expect(result.status).toBe(1);
    expect(result.stderr).toContain("bun run tauri dev");
  });

  it("uses the application's configured identifier", () => {
    expect(projectIdentifier(process.cwd())).toBe("com.foxtag.jarvis");
    expect(projectIdentifier(process.cwd(), true)).toBe("com.foxtag.jarvis.dev");
  });

  it.skipIf(process.platform !== "darwin" || process.env.JARVIS_TEST_MACOS_BUNDLE !== "1")("runs signed development inside a native notification-capable bundle", () => {
    const temporary = mkdtempSync(path.join(tmpdir(), "Jarvis bundle probe "));
    try {
      const binary = path.join(temporary, "probe");
      execFileSync("/usr/bin/clang", ["-Wall", "-Wextra", "-Werror", "-fobjc-arc", "scripts/fixtures/notification-bundle.m", "-framework", "Foundation", "-framework", "UserNotifications", "-o", binary]);
      const result = spawnSync("/bin/sh", [path.resolve("scripts/run-signed-macos.sh"), binary, "argument with spaces"], {
        env: { ...process.env, APPLE_SIGNING_IDENTITY: localSigningIdentity().hash, JARVIS_SIGNING_IDENTIFIER: projectIdentifier(process.cwd(), true), JARVIS_DEV_APP_BUNDLE: "1" }, encoding: "utf8",
      });
      expect(result.stderr, "native runner failed").not.toMatch(/error:/i);
      expect(result.status, result.stderr).toBe(0);
      expect(result.stdout.trim().split("\n")).toEqual(["com.foxtag.jarvis.dev", "Jarvis", "argument with spaces"]);
    } finally { rmSync(temporary, { recursive: true, force: true }); }
  });

  it.skipIf(process.platform === "win32")("uses a valid shell runner", () => {
    expect(() => execFileSync("/bin/sh", ["-n", "scripts/run-signed-macos.sh"])).not.toThrow();
  });
});
