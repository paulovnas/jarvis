import { expect, it } from "vitest";
import { browserAddress, browserOccluded } from "./browser";

it("accepts development servers and HTTPS without allowing local files or credentials", () => {
  expect(browserAddress("localhost:5173/teste")).toBe("http://localhost:5173/teste");
  expect(browserAddress(" https://example.com/ ")).toBe("https://example.com/");
  for (const value of ["", "file:///C:/secrets", "javascript:alert(1)", "https://user:password@example.com", "tauri://localhost"]) expect(() => browserAddress(value)).toThrow();
});

it("hides native content for application dialogs and restores it when they close", () => {
  const dialog = document.createElement("div");
  dialog.setAttribute("role", "dialog"); document.body.append(dialog);
  expect(browserOccluded(document)).toBe(true);
  dialog.setAttribute("data-closed", "");
  expect(browserOccluded(document)).toBe(false);
  dialog.remove();
});
