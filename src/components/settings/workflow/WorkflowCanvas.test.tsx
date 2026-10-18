import { useState } from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkflowCanvas from "./WorkflowCanvas";
import { builtinAgent, builtinFlow, customAgent, customFlow } from "@/test/workflow-fixtures";
const originalResizeObserver = window.ResizeObserver;
const originalBBox = Object.getOwnPropertyDescriptor(SVGElement.prototype, "getBBox");

beforeEach(() => {
  // jsdom has no layout. Report a real measurement once, as the browser does
  // when a node is mounted; moving it does not trigger a new ResizeObserver entry.
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(224);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(160);
  vi.stubGlobal("DOMMatrixReadOnly", class { m22 = 1; });
  Object.defineProperty(SVGElement.prototype, "getBBox", { configurable: true, value: () => ({ x: 0, y: 0, width: 100, height: 16 }) });
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
afterEach(() => { window.ResizeObserver = originalResizeObserver; if (originalBBox) Object.defineProperty(SVGElement.prototype, "getBBox", originalBBox); else Reflect.deleteProperty(SVGElement.prototype, "getBBox"); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

it("shows named conditional connections without turning them into a fixed sequence", async () => {
  const specialist = { ...customFlow.steps[0], id: "d".repeat(32), position: { x: 360, y: 40 }, next: customFlow.entry };
  const flow = { ...customFlow, steps: [{ ...customFlow.steps[0], routes: [{ id: "lore", label: "Consultar narrativa", target: specialist.id }, { id: "finish", label: "Entregar", target: null }] }, specialist] };
  const props = { agents: [customAgent], selected: flow.entry, onChange: vi.fn(), onSelect: vi.fn(), disabled: false };
  const view = render(<WorkflowCanvas flow={customFlow} {...props} />);
  await waitFor(() => expect(view.container.querySelector(".react-flow__node")).toBeVisible());
  view.rerender(<WorkflowCanvas flow={flow} {...props} />);
  await waitFor(() => expect(view.container.querySelectorAll(".react-flow__edge")).toHaveLength(2));
  expect(screen.getByText("Consultar narrativa")).toBeVisible();
  expect(screen.getByText("Escolhe uma rota →")).toBeVisible();
  expect(screen.getByLabelText("Saída condicional")).toBeInTheDocument();
  view.rerender(<WorkflowCanvas flow={customFlow} {...props} />);
  await waitFor(() => expect(view.container.querySelectorAll(".react-flow__edge")).toHaveLength(0));
  expect(screen.getByLabelText("Saída de conclusão")).toBeInTheDocument();
});

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
