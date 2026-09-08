import { useState } from "react";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Button } from "@/components/ui/button";
import { SortableItem, SortableList } from "./SortableList";

function Harness({ changed }: { changed: (ids: string[]) => void }) {
  const [ids, setIds] = useState(["A", "B", "C"]);
  return <SortableList ids={ids} onReorder={next => { setIds(next); changed(next); }}><ul aria-label="Itens">{ids.map(id => <SortableItem id={id} key={id}>{sort => <li data-reorder-item ref={sort.setNodeRef} style={sort.style}><Button ref={sort.setActivatorNodeRef} {...sort.listeners} aria-describedby={sort.attributes["aria-describedby"]}>{id}</Button></li>}</SortableItem>)}</ul></SortableList>;
}

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const item = this.closest("[data-reorder-item]");
    const index = item ? [...document.querySelectorAll("[data-reorder-item]")].indexOf(item) : 0;
    return { x: 0, y: index * 48, top: index * 48, left: 0, right: 160, bottom: index * 48 + 40, width: 160, height: 40, toJSON: () => ({}) };
  });
});
afterEach(() => vi.restoreAllMocks());

it("reorders with a pointer drag without requiring a context menu", async () => {
  const changed = vi.fn(); render(<Harness changed={changed} />);
  fireEvent.pointerDown(screen.getByRole("button", { name: "A" }), { pointerId: 1, isPrimary: true, button: 0, clientX: 20, clientY: 20 });
  fireEvent.pointerMove(document, { pointerId: 1, clientX: 20, clientY: 30 });
  await act(async () => {});
  fireEvent.pointerMove(document, { pointerId: 1, clientX: 20, clientY: 116 });
  fireEvent.pointerUp(document, { pointerId: 1, button: 0 });
  await waitFor(() => expect(changed).toHaveBeenCalledWith(["B", "C", "A"]));
});

it("supports keyboard reordering and Escape cancellation", async () => {
  const user = userEvent.setup(); const changed = vi.fn(); render(<Harness changed={changed} />);
  screen.getByRole("button", { name: "A" }).focus();
  await user.keyboard("[Space][ArrowDown][Space]");
  expect(changed).toHaveBeenCalledWith(["B", "A", "C"]);
  changed.mockClear();
  screen.getByRole("button", { name: "B" }).focus();
  await user.keyboard("[Space][ArrowDown][Escape]");
  expect(changed).not.toHaveBeenCalled();
});
