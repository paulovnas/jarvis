# Agent harness evaluations

This directory documents the small, versioned evaluation set used to measure Jarvis harness changes. The executable fixtures live beside the Rust agent tests in `src-tauri/src/agent/fixtures/evaluations`.

Run the focused baseline with:

```bash
bun run eval:harness
```

The command does not contact an AI provider or open a private project. It validates sanitized observations captured from real Movarte sessions, prints one `HARNESS_EVAL` record per case, and runs the behavioral regression tests connected to those cases.

## Initial cases

| Case | Behavioral signal | Recorded metrics |
| --- | --- | --- |
| `movarte-explicit-mcp-fidelity` | An explicit Gemini Notebook request must use that MCP and must not drift to Database after a recoverable failure. | Provider steps, measured steps, tool calls, error classes, input/output/cache tokens, wall time and summed agent time. |
| `movarte-planned-status-selection` | The planned flow must complete, expose only topology-valid spawn roles, and keep failures measurable. | The same totals, split between the planner and both builders. |
| `mcp-individual-tool-demand` | A large MCP must expose only bounded search/load controls, then the single schema selected from the latest search. | Full catalog tools/bytes, initial schema bytes and reduction in basis points. |
| `local-read-reuse-validity` | An identical local file range may omit its duplicate only after the complete protected file is byte-for-byte unchanged. | Original bytes, retained bytes, reuse count and reduction in basis points. |

The first fixture intentionally preserves the original failure: one cancelled agent and eight calls to an unrequested MCP. Its linked runtime tests verify the corrected routing behavior, durable MCP affinity across continuations, compaction and worker handoffs, and that a timeout reconnects only the requested MCP without replaying the failed call. A later user directive may replace the selected MCP, return to on-demand selection, exclude one integration or disable MCPs; a mere mention of another MCP does not change the affinity. The second fixture preserves a successful result with high recovery overhead, so future changes can be compared against an honest starting point.

MCP regression tests also require machine-readable error envelopes (`code`, retry safety, outcome certainty, recovery status, and bounded schema violations). The transcript keeps the concise human message while every provider receives the structured JSON as the tool result.

Large MCP catalogs are evaluated with a deterministic 50-tool fixture. The initial provider step receives only `mcp_search_tools` and `mcp_load_tool`; the current baseline reduces the serialized schema payload from 19,949 to 1,147 bytes (94.26%). Search returns at most eight short results, loading is limited to the latest search, and at most eight deferred schemas remain active. The regression suite also covers read-only Plan filtering, agent permission filtering, activation boundaries, reconnection, catalog refresh and structured control-argument errors. Small MCPs remain eager when they publish no more than three tools and no more than 8 KiB of schemas.

Local read reuse is scoped to one active turn and at most 64 file-range identities. Jarvis hashes the complete file bytes from the same protected open used for the read, so a same-size change, a change outside the requested range, removal or replacement by a symbolic link cannot return stale content. The cache is cleared after compaction because the referenced result may no longer be in the provider replay. Results compacted by Context Mode are not retained as reusable reads, and external queries, MCPs, Web Search, Context7, directory listings and searches remain outside this cache.

## Comparison discipline

- Keep the task, model, reasoning level, provider and tool configuration fixed when comparing a candidate run.
- Compare task completion and routing correctness before token or latency reductions.
- Treat this first production observation as descriptive, not statistically significant. For live comparisons, collect at least three runs and report their median alongside individual regressions.
- `wallTimeMs` represents elapsed user-visible time. `agentTimeMs` is the sum of all agent durations and can be greater when subagents overlap.
- Input tokens already include provider-reported cache reads and writes. Cache breakdowns are not added to the input total again.
- Add every newly confirmed production failure as a sanitized case rather than embedding project content, credentials, paths, account aliases or raw model output.
