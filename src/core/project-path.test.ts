import { describe, expect, it } from "vitest";
import { resolveProjectFilePath } from "./project-path";

describe("resolveProjectFilePath", () => {
  it("joins safe relative paths using the project's platform separator", () => {
    expect(resolveProjectFilePath("/projects/jarvis", "src/components/App.tsx")).toBe("/projects/jarvis/src/components/App.tsx");
    expect(resolveProjectFilePath("C:\\Projects\\Jarvis", "src/components/App.tsx")).toBe("C:\\Projects\\Jarvis\\src\\components\\App.tsx");
  });

  it("rejects paths that could escape the selected project", () => {
    expect(resolveProjectFilePath("/projects/jarvis", "../outside.ts")).toBeNull();
    expect(resolveProjectFilePath("/projects/jarvis", "src/../../outside.ts")).toBeNull();
    expect(resolveProjectFilePath("/projects/jarvis", "/tmp/outside.ts")).toBeNull();
    expect(resolveProjectFilePath("C:\\Projects\\Jarvis", "C:\\outside.ts")).toBeNull();
    expect(resolveProjectFilePath("/projects/jarvis", "src//App.tsx")).toBeNull();
  });
});
