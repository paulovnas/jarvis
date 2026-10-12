import { render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import type { Options } from "react-markdown";
import ChatMarkdown from "./ChatMarkdown";

const parsedSources = vi.hoisted(() => [] as string[]);
vi.mock("react-markdown", async importOriginal => {
  const actual = await importOriginal<typeof import("react-markdown")>();
  return { ...actual, default: (props: Options) => {
    parsedSources.push(props.children ?? "");
    return actual.default(props);
  } };
});
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(), revealItemInDir: vi.fn() }));

beforeEach(() => {
  parsedSources.length = 0;
  vi.spyOn(window, "matchMedia").mockReturnValue({ matches: true, media: "(prefers-reduced-motion: reduce)", onchange: null, addListener: () => {}, removeListener: () => {}, addEventListener: () => {}, removeEventListener: () => {}, dispatchEvent: () => true });
});

it("processes only the changing tail once a preceding block is stable", () => {
  const prefix = `Stable **Markdown** ${"content ".repeat(100)}\n\n`;
  const { container, rerender } = render(<ChatMarkdown content={`${prefix}Growing\n`} isStreaming streamId="a" />);
  const stableElement = container.querySelector("strong");
  for (let index = 1; index <= 20; index++) rerender(<ChatMarkdown content={`${prefix}Growing\n${"tail ".repeat(index)}`} isStreaming streamId="a" />);

  expect(parsedSources[parsedSources.length - 1]).toBe(`Growing\n${"tail ".repeat(20)}`);
  expect(container.querySelector("strong")).toBe(stableElement);
  const parsedCharacters = parsedSources.reduce((count, source) => count + source.length, 0);
  expect(parsedCharacters).toBeLessThan(prefix.length * 21 / 3);
});

it.each([
  ["fences and nested lists", "# Plano\n\nTexto **forte** e ~~riscado~~.\n\n```ts\nconst x = 1;\n\nconst y = x;\n```\n\n- primeiro\n  - filho\n\n  parágrafo do item\n- segundo\n\nConclusão\nmais texto"],
  ["GFM tables and task lists", "Cabeçalho\n\n| Nome | Valor |\n| --- | ---: |\n| Um | 1 |\n| Dois | 2 |\n\n- [x] Feito\n- [ ] Aberto\n\nVeja https://example.com.\nTexto final"],
  ["references defined after their use", "[Arquivo][doc] e [Site].\n\nCorpo da resposta\nmais texto\n\n[doc]: /tmp/report.md\n[Site]:\n  https://example.com\n\nFinal\ntexto"],
  ["partial ordered and unordered list markers", "1. Primeiro\n\n1. Segundo\n\nParágrafo\nmais texto\n\n- item\n\n- outro\n\nFinal\ntexto"],
  ["quotes, HTML and setext headings", "> Um parágrafo\n>\n> Outro\n\nContinuação\nmais texto\n\nTítulo\n---\n\n<div>HTML inerte</div>\n\nFinal\ntexto"],
])("matches a cold renderer at every growth step for %s", (_label, source) => {
  const streamed = render(<ChatMarkdown content="" isStreaming streamId="a" />);
  for (let length = 1; length <= source.length; length++) {
    const content = source.slice(0, length);
    streamed.rerender(<ChatMarkdown content={content} isStreaming streamId="a" />);
    const cold = render(<ChatMarkdown content={content} />);
    expect(streamed.container.innerHTML, `source length ${length}`).toBe(cold.container.innerHTML);
    cold.unmount();
  }
  streamed.rerender(<ChatMarkdown content={source} streamId="a" />);
  const cold = render(<ChatMarkdown content={source} />);
  expect(streamed.container.innerHTML).toBe(cold.container.innerHTML);
  expect(parsedSources[parsedSources.length - 1]).toBe(source);
});

it("reparses the complete document on edits, identity changes and completion", () => {
  const initial = "Primeiro bloco\n\nCauda\ntexto";
  const { rerender } = render(<ChatMarkdown content={initial} isStreaming streamId="a" />);
  rerender(<ChatMarkdown content={`${initial} maior`} isStreaming streamId="a" />);
  expect(parsedSources[parsedSources.length - 1]).not.toContain("Primeiro bloco");

  const edited = "Bloco editado\n\nCauda\ntexto maior";
  rerender(<ChatMarkdown content={edited} isStreaming streamId="a" />);
  expect(parsedSources[parsedSources.length - 1]).toBe(edited);
  rerender(<ChatMarkdown content={`${edited} nova`} isStreaming streamId="b" />);
  expect(parsedSources[parsedSources.length - 1]).toBe(`${edited} nova`);
  rerender(<ChatMarkdown content={`${edited} nova`} streamId="b" />);
  expect(parsedSources[parsedSources.length - 1]).toBe(`${edited} nova`);
  rerender(<ChatMarkdown content="" isStreaming streamId="b" />);
  expect(screen.queryByText("Bloco editado")).not.toBeInTheDocument();
});

it("limits retained prefix blocks and falls back for oversized text", () => {
  const blocks = Array.from({ length: 130 }, (_value, index) => `# Bloco ${index}\n\n`).join("");
  const { container, rerender } = render(<ChatMarkdown content={`${blocks}Cauda\n`} isStreaming streamId="a" />);
  rerender(<ChatMarkdown content={`${blocks}Cauda\nmais`} isStreaming streamId="a" />);
  expect(container.querySelectorAll("h1")).toHaveLength(130);
  expect(parsedSources[parsedSources.length - 1]).toContain("# Bloco 100");

  const oversized = `Primeiro\n\n${"a".repeat(140_000)}\n`;
  rerender(<ChatMarkdown content={oversized} isStreaming streamId="b" />);
  rerender(<ChatMarkdown content={`${oversized}mais`} isStreaming streamId="b" />);
  expect(parsedSources[parsedSources.length - 1]).toBe(`${oversized}mais`);
});
