import { act, render } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { CompanionAnswer } from "./CompanionAnswer";

it.each([true, false])("buffers a %s plain conversation answer and flushes final and replacement content", async global => {
  await import("@/components/chat/ChatMarkdown");
  vi.useFakeTimers();
  const frames = new Set<number>();
  let id = 0;
  vi.spyOn(window, "requestAnimationFrame").mockImplementation(() => { frames.add(++id); return id; });
  vi.spyOn(window, "cancelAnimationFrame").mockImplementation(frame => { frames.delete(frame); });
  const source = "**Resposta** base";
  const view = render(<CompanionAnswer content={source} global={global} isStreaming streamId="chat:turn" />);
  try {
    await act(async () => {});
    expect(view.container).toHaveTextContent(global ? source : "Resposta base");
    expect(Boolean(view.container.querySelector("strong"))).toBe(!global);
    view.rerender(<CompanionAnswer content={`${source} texto em rajada`} global={global} isStreaming streamId="chat:turn" />);
    expect(view.container.textContent).toBe(global ? source : "Resposta base");
    expect(frames.size).toBe(1);
    view.rerender(<CompanionAnswer content={`${source} texto em rajada`} global={global} isStreaming={false} streamId="chat:turn" />);
    expect(view.container).toHaveTextContent("texto em rajada");
    expect(frames.size).toBe(0);
    view.rerender(<CompanionAnswer content={`${source} outra conversa`} global={global} isStreaming streamId="other:turn" />);
    expect(view.container).toHaveTextContent("outra conversa");
  } finally {
    view.unmount();
    vi.useRealTimers();
    vi.restoreAllMocks();
  }
});
