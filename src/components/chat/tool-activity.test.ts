import { describe, expect, it } from "vitest";
import { groupToolActivity, summarizeToolActivity, toolSeverity, toolsSeverity } from "./tool-activity";
import type { ToolCallItem } from "./types";

function tool(name: string, index: number, status: ToolCallItem["status"] = "completed"): ToolCallItem {
  return { id: `${name}-${index}`, name, status, args: {}, output: "" };
}

describe("grouped tool activity", () => {
  it("keeps native computer actions identifiable in collapsed activity", () => {
    expect(summarizeToolActivity([tool("computer_observe", 1), tool("computer_click", 2)]))
      .toBe("Usou recursos do computador");
  });
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
    expect(groups[0]).toMatchObject({ failures: 1, warnings: 0, active: true });
  });

  it("treats the direct-task preflight as an expected warning", () => {
    const reminder = tool("ctx_execute", 1, "error");
    reminder.output = "Atualize a lista com update_tasks e mantenha uma tarefa em andamento antes de executar alterações.";
    const groups = groupToolActivity([reminder, tool("read", 2), tool("read", 3), tool("read", 4), tool("read", 5)]);
    expect(groups[0]).toMatchObject({ failures: 0, warnings: 1, active: false });
  });

  it("classifies one tool set identically at tool and group level", () => {
    const reminder = tool("ctx_execute", 1, "error");
    reminder.output = "Atualize a lista com update_tasks e mantenha uma tarefa em andamento antes de executar";
    const failure = tool("read", 2, "error");

    expect(toolSeverity(reminder)).toBe("warning");
    expect(toolSeverity(failure)).toBe("error");
    expect(toolSeverity(tool("read", 3))).toBe("completed");
    expect(toolSeverity(tool("bash", 4, "running"))).toBe("active");

    expect(toolsSeverity([reminder])).toBe("warning");
    expect(toolsSeverity([reminder, failure])).toBe("error");
    expect(toolsSeverity([failure, tool("bash", 5, "running")])).toBe("active");
    expect(toolsSeverity([])).toBe("completed");

    const groups = groupToolActivity([failure, reminder, tool("read", 7), tool("read", 8), tool("read", 9)]);
    expect(groups[0]).toMatchObject({ severity: "error", failures: 1, warnings: 1 });
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

  it("groups Graft structural discovery with precise reads instead of external integrations", () => {
    expect(summarizeToolActivity([
      tool("graft_repo_map", 1), tool("graft_find_code", 2), tool("graft_trace_calls", 3), tool("read", 4),
    ])).toBe("Leu e pesquisou arquivos");
  });

  it("identifies answered user questions in activity summaries", () => {
    expect(summarizeToolActivity([tool("ask_user", 1)])).toBe("Fez perguntas");
  });

  it("describes audiovisual preparation without claiming a render was produced", () => {
    expect(summarizeToolActivity([tool("video_docs", 1), tool("video_audio", 2), tool("video_presentation", 3)]))
      .toBe("Usou recursos de vídeo e áudio");
  });
});
