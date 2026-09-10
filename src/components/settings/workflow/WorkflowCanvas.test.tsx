import { useState } from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkflowCanvas from "./WorkflowCanvas";
import { builtinAgent, builtinFlow, customAgent, customFlow } from "@/test/workflow-fixtures";
const originalResizeObserver = window.ResizeObserver;

beforeEach(() => {
  // jsdom has no layout. Report a real measurement once, as the browser does
  // when a node is mounted; moving it does not trigger a new ResizeObserver entry.
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(224);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(160);
  vi.stubGlobal("DOMMatrixReadOnly", class { m22 = 1; });
  window.ResizeObserver = class {
    constructor(private callback: ResizeObserverCallback) {}
    observe(target: Element) { queueMicrotask(() => this.callback([{ target, contentRect: new DOMRect(0, 0, 224, 160), borderBoxSize: [], contentBoxSize: [], devicePixelContentBoxSize: [] }], this)); }
    unobserve() {}
    disconnect() {}
  };
});

it("renders native flow definitions as a locked delegation canvas", async () => {
  const onChange = vi.fn();
  const view = render(<WorkflowCanvas flow={builtinFlow} agents={[builtinAgent]} selected={builtinFlow.entry} onChange={onChange} onSelect={vi.fn()} disabled />);
  await waitFor(() => expect(view.container.querySelector(".react-flow__node")).toBeVisible());
  expect(screen.getByText("Jarvis")).toBeVisible();
  expect(screen.getByText("Delega conforme o escopo →")).toBeVisible();
  fireEvent.keyDown(view.container.querySelector<HTMLElement>(".react-flow__node")!, { key: "ArrowRight" });
  expect(onChange).not.toHaveBeenCalled();
});
afterEach(() => { window.ResizeObserver = originalResizeObserver; vi.restoreAllMocks(); vi.unstubAllGlobals(); });

it("keeps a measured node visible while moving and preserves its position after editing its label", async () => {
  function Editor() {
    const [flow, setFlow] = useState(customFlow);
    const [selected, setSelected] = useState<string | null>(customFlow.entry);
    const [agents, setAgents] = useState([customAgent]);
    return <><button onClick={() => setAgents([{ ...customAgent, name: "Nome atualizado" }])}>Renomear</button><output aria-label="Posição">{flow.steps[0].position.x}</output><WorkflowCanvas flow={flow} agents={agents} selected={selected} onChange={setFlow} onSelect={setSelected} disabled={false} /></>;
  }
  const view = render(<Editor />);
  const node = view.container.querySelector<HTMLElement>(".react-flow__node")!;
  await waitFor(() => expect(node).toBeVisible());
  const original = node.style.transform;
  for (let i = 0; i < 3; i++) {
    fireEvent.keyDown(node, { key: "ArrowRight" });
    await waitFor(() => expect(Number(screen.getByLabelText("Posição").textContent)).toBeGreaterThan(40));
    expect(node).toBeVisible();
    expect(node.style.visibility).not.toBe("hidden");
  }
  const moved = node.style.transform;
  expect(moved).not.toBe(original);
  fireEvent.click(screen.getByRole("button", { name: "Renomear" }));
  expect(await screen.findByText("Nome atualizado")).toBeVisible();
  expect(node.style.transform).toBe(moved);
});
