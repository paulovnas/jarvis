import { expect, it } from "vitest";
import { moveItem, orderedItems } from "./item-order";

it("restores custom order while tolerating additions and deletions", () => {
  expect(orderedItems(["b", "c", "d"], ["c", "a", "b"], id => id)).toEqual(["c", "b", "d"]);
  expect(orderedItems(["b", "a"], undefined, id => id)).toEqual(["b", "a"]);
  expect(moveItem(["a", "b", "c"], "c", "a")).toEqual(["c", "a", "b"]);
  expect(moveItem(["a", "b"], "unknown", "a")).toEqual(["a", "b"]);
});
