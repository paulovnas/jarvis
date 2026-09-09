import { describe, expect, it } from "vitest";
import { groupToolActivity, summarizeToolActivity } from "./tool-activity";
import type { ToolCallItem } from "./types";

function tool(name: string, index: number, status: ToolCallItem["status"] = "completed"): ToolCallItem {
  return { id: `${name}-${index}`, name, status, args: {}, output: "" };
}

describe("grouped tool activity", () => {
  it("describes mixed batches in natural pt-BR", () => {
    expect(summarizeToolActivity([tool("ctx_search", 1), tool("read", 2), tool("bash", 3)]))
      .toBe("Usou Context Mode, leu e pesquisou arquivos e executou comandos");
  });

  it("reduces more than one hundred actions to a bounded number of groups", () => {
    const groups = groupToolActivity(Array.from({ length: 101 }, (_, index) => tool(index % 2 ? "read" : "ctx_search", index)));
    expect(groups).toHaveLength(7);
    expect(groups.flatMap(group => group.tools)).toHaveLength(101);
  });

  it("keeps active and failed states visible at group level", () => {
    const groups = groupToolActivity([
      tool("read", 1), tool("read", 2, "error"), tool("bash", 3, "running"), tool("edit", 4), tool("ctx_search", 5),
    ]);
    expect(groups[0]).toMatchObject({ failures: 1, active: true });
  });

  it("leaves short histories ungrouped", () => {
    expect(groupToolActivity([tool("read", 1), tool("edit", 2)])).toEqual([]);
  });

  it("groups LSP navigation with reads and transactional patches with writes", () => {
    expect(summarizeToolActivity([
      tool("lsp_definition", 1),
      tool("apply_patch", 2),
    ])).toBe("Leu e pesquisou arquivos e alterou arquivos");
  });
});
