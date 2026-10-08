import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import ChatMarkdown from "./ChatMarkdown";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(), revealItemInDir: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));

beforeEach(() => {
  vi.mocked(openUrl).mockReset().mockResolvedValue();
  vi.mocked(revealItemInDir).mockReset().mockResolvedValue();
  vi.mocked(toast.error).mockClear();
});

it.each([
  ["/Users/paulo/Downloads/Video.mp4", "/Users/paulo/Downloads/Video.mp4"],
  ["/home/paulo/Meu projeto/Vídeo (final).mp4", "/home/paulo/Meu projeto/Vídeo (final).mp4"],
  ["/tmp/Meu%20projeto/build.zip", "/tmp/Meu projeto/build.zip"],
  ["/tmp/progress%2520.pdf", "/tmp/progress%20.pdf"],
  ["/tmp/progress%25done.pdf", "/tmp/progress%done.pdf"],
  ["C:/Users/Paulo/Downloads/Jarvis.exe", "C:/Users/Paulo/Downloads/Jarvis.exe"],
  ["C:/project/.env", "C:/project/.env"],
  [String.raw`C:\Users\Paulo\Downloads\Jarvis.exe`, String.raw`C:\Users\Paulo\Downloads\Jarvis.exe`],
  ["%5C%5Cserver%5Cshare%5Cbuild.zip", String.raw`\\server\share\build.zip`],
  ["file:///Users/paulo/Downloads/Video.mp4", "/Users/paulo/Downloads/Video.mp4"],
  ["file:///C:/Users/Paulo/Meu%20projeto/build.zip", "C:/Users/Paulo/Meu projeto/build.zip"],
])("reveals a descriptive file link in the OS file manager: %s", async (target, path) => {
  const user = userEvent.setup();
  render(<ChatMarkdown content={`[Build pronto](<${target}>)`} />);
  const link = screen.getByRole("button", { name: "Build pronto" });
  expect(link).toHaveAttribute("title", `Mostrar na pasta: ${path}`);
  await user.click(link);
  expect(revealItemInDir).toHaveBeenCalledExactlyOnceWith(path);
  expect(openUrl).not.toHaveBeenCalled();
});

it.each([
  ["unchanged content", "Veja [Arquivo pronto](/tmp/portal-ita.pdf)."],
  ["updated prose", "Pronto: [Arquivo pronto](/tmp/portal-ita.pdf)."],
])("keeps a pressed file link active and focused after rerendering with %s", async (_label, content) => {
  const user = userEvent.setup();
  const { rerender } = render(<ChatMarkdown content="Veja [Arquivo pronto](/tmp/portal-ita.pdf)." />);
  await user.pointer({ target: screen.getByRole("button", { name: "Arquivo pronto" }), keys: "[MouseLeft>]" });
  rerender(<ChatMarkdown content={content} />);
  const link = screen.getByRole("button", { name: "Arquivo pronto" });
  await user.pointer({ target: link, keys: "[/MouseLeft]" });
  expect(revealItemInDir).toHaveBeenCalledExactlyOnceWith("/tmp/portal-ita.pdf");
  expect(link).toHaveFocus();
});

it("opens web links normally and keeps unsafe or relative destinations inert", async () => {
  const user = userEvent.setup();
  render(<ChatMarkdown content={"[Documentação](https://example.com/docs) [Script](javascript:alert%281%29) [Dados](data:text/html,test) [Relativo](build/app.zip) [Remoto](//example.com/file) [Arquivo remoto](file://example.com/file) [Controle](/tmp/file%00.txt)"} />);
  expect(screen.getAllByRole("button")).toHaveLength(1);
  await user.click(screen.getByRole("button", { name: "Documentação" }));
  expect(openUrl).toHaveBeenCalledExactlyOnceWith("https://example.com/docs");
  expect(revealItemInDir).not.toHaveBeenCalled();
  expect(screen.getByText("Script")).toBeInTheDocument();
});

it("reports a file manager failure without opening the artifact as a URL", async () => {
  vi.mocked(revealItemInDir).mockRejectedValue(new Error("file not found"));
  const user = userEvent.setup();
  render(<ChatMarkdown content="[Arquivo pronto](/tmp/missing.zip)" />);
  await user.click(screen.getByRole("button", { name: "Arquivo pronto" }));
  expect(toast.error).toHaveBeenCalledWith("Não foi possível mostrar o arquivo na pasta. Ele pode ter sido movido ou removido.");
  expect(openUrl).not.toHaveBeenCalled();
});

it("highlights Go and copies only code, preserving indentation", async () => {
  const user = userEvent.setup();
  const copy = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
  const code = 'package main\n\nfunc main() {\n  println("Olá")\n}';
  render(<ChatMarkdown content={`Exemplo:\n\n\`\`\`go\n${code}\n\`\`\``} />);
  const block = screen.getByRole("region", { name: "Código go" });
  expect(block.querySelector(".hljs-keyword")).toHaveTextContent("package");
  await user.click(screen.getByRole("button", { name: "Copiar código" }));
  expect(copy).toHaveBeenCalledWith(code);
  expect(screen.getByText("Copiado")).toBeInTheDocument();
});
it("keeps unknown languages and HTML inert, with no copy control on inline code", () => {
  render(<ChatMarkdown content={'Use `go run`.\n\n```unknown\n<script>alert(1)</script>\n```'} />);
  expect(screen.getAllByRole("button", { name: "Copiar código" })).toHaveLength(1);
  expect(screen.getByText("<script>alert(1)</script>")).toBeInTheDocument();
  expect(document.querySelector("script")).toBeNull();
});
