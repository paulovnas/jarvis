import { describe, expect, it } from "vitest";
import {
  conversationDetails,
  emptyLibrary,
  populatedLibrary,
} from "@/test/library-fixtures";
import { readConversationDetails, readLibrarySnapshot } from "./library";

describe("Library command boundary", () => {
  it("accepts empty and persisted hierarchies", () => {
    expect(readLibrarySnapshot(emptyLibrary())).toEqual(emptyLibrary());
    expect(readLibrarySnapshot(populatedLibrary())).toEqual(populatedLibrary());
    expect(readConversationDetails(conversationDetails())).toEqual(
      conversationDetails(),
    );
  });
  it("rejects malformed, orphaned, duplicate and inconsistent selections", () => {
    const original = populatedLibrary();
    for (const value of [
      null,
      [],
      {},
      { ...original, workspaces: [] },
      { ...original, projects: [] },
      {
        ...original,
        conversations: [...original.conversations, original.conversations[0]],
      },
      {
        ...original,
        selection: { workspaceId: "w2", projectId: "p1", conversationId: "c1" },
      },
      {
        ...original,
        selection: { workspaceId: "w1", projectId: null, conversationId: "c1" },
      },
      {
        ...original,
        selection: {
          workspaceId: "w1",
          projectId: "p1",
          conversationId: "missing",
        },
      },
      {
        ...original,
        projects: [{ ...original.projects[0], createdAt: "yesterday" }],
      },
    ])
      expect(() => readLibrarySnapshot(value)).toThrow("inválidos");
  });
  it("rejects conversation context from a different project", () => {
    expect(() =>
      readConversationDetails({
        ...conversationDetails(),
        project: populatedLibrary().projects[1],
      }),
    ).toThrow("inválidos");
  });
});
