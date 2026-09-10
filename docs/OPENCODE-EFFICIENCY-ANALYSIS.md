# OpenCode efficiency analysis for Jarvis

Date: 2026-09-09

Reference snapshots:

- OpenCode: `830d5eb53548`
- OMP: `5964a0f76492`
- Metis: `f099a8c60ddc`

## Purpose

This document compares the execution, context, persistence, and tool patterns in OpenCode with Jarvis, OMP, and Metis. The goal is to identify changes that measurably reduce token use, repeated reads, provider calls, and local storage without weakening recovery or making model behavior provider-dependent.

The recommendations are architectural ideas. Reference repositories under `docs/` remain read-only and no source should be copied verbatim.

## Executive recommendation

Jarvis already covers most of the high-value prompt-cache and large-output behavior. It should not add a second generic truncation layer or rely on a stronger system sentence to make models behave efficiently. The best next investments are:

1. Replace cumulative turn checkpoints with incremental persistence. This is the most urgent item because `jarvis-ncf` records a real 202,574,473-byte journal where 519 cumulative snapshots account for 201,106,548 bytes. The latest state of each turn totals only 1,497,190 bytes.
2. Load directory-scoped `AGENTS.md` files lazily as tools enter a subtree. Jarvis already loads the root file correctly, but monorepo instructions are not yet scoped hierarchically.
3. Add native LSP navigation and diagnostics tools. Symbol-aware lookup can replace broad text searches and repeated file reads.
4. Add a transactional multi-file patch tool that validates the entire patch before mutation and reports diagnostics after applying it.
5. Detect consecutive identical tool calls and steer the model once before stopping a proven loop. This prevents unbounded waste without reintroducing an arbitrary global step limit.
6. Evaluate small model-family prompt overlays with telemetry. This can improve Gemini/Antigravity tool use, but it has a higher maintenance cost and should follow the deterministic runtime improvements above.

## Current Jarvis baseline

Jarvis already implements several mechanisms that should remain the foundation:

- `src-tauri/src/core/context.rs` makes Context-mode a runtime capability, requires retrieval/index definitions, performs automatic bounded recall, and substitutes indexed previews for large results. The visible output budget is 8,000 characters.
- `src-tauri/src/agent/tools.rs` excludes common dependency and generated trees from ordinary discovery and tells tools to page focused reads.
- `src-tauri/src/agent/compaction.rs` performs explicit continuation summarization and the application targets automatic compaction at 80% of the usable context window.
- `src-tauri/src/agent/provider.rs` keeps a stable session cache key and sorts tool definitions by name so equivalent catalogs preserve the same cacheable prefix.
- `src-tauri/src/agent/provider/custom/request.rs` emits explicit cache markers only for compatible gateways/protocols. OpenAI-compatible implicit caching is not polluted with unsupported markers.
- Provider usage parsing preserves cache-read and cache-write counters as a breakdown of input tokens, allowing dashboard telemetry without double counting.
- Provider requests have bounded retry behavior, while the chat records live retry state for the user.
- Visible conversation history is paged. Opening a chat does not require materializing the complete transcript in React.

These mechanisms mean OpenCode's generic output truncation should not be ported as another layer. Context-mode keeps the full result searchable while returning a bounded preview, which is more useful than discarding old output.

## Who owns `AGENTS.md`

Reading `AGENTS.md` belongs to the Jarvis runtime/tool layer. It must not depend on a model remembering an instruction such as "open AGENTS.md before working."

Jarvis already follows that rule for the project root. `src-tauri/src/agent/tools.rs` resolves the root `AGENTS.md`, reads it with the same confined file path rules used by tools, and appends up to 24,000 characters to the system instructions. This happens before provider execution, so it applies to every model, agent, and flow.

The current limitation is scope rather than enforcement:

- only the root file is loaded;
- content after 24,000 characters is intentionally omitted;
- a nested package's instructions are not discovered when a file inside that package is read or edited;
- loading every nested file eagerly would waste tokens and destabilize the provider cache prefix.

The recommended extension is hierarchical and lazy. The runtime should attach the nearest applicable instruction files only when a tool first enters their directory scope, once per agent turn. System and user instructions must continue to outrank repository files. Symlinks and paths outside the project must remain rejected.

## OpenCode findings

### Hierarchical instructions

OpenCode's `packages/opencode/src/session/instruction.ts` separates initial and lazy instruction discovery:

- global and root project instruction files are collected at session setup;
- `AGENTS.md`, `CLAUDE.md`, and the deprecated `CONTEXT.md` are searched upward within the workspace boundary;
- completed read-tool metadata records which instruction paths were already loaded;
- claims are tracked per assistant message so the same nested file is not attached repeatedly;
- reading a file can resolve additional instructions relevant to that file's directory.

The useful idea is the scope-aware runtime hook, not support for every OpenCode filename. Jarvis should start with `AGENTS.md`, because that is its documented project contract.

### Normalized messages and cursor paging

OpenCode's `packages/opencode/src/session/message-v2.ts` stores message metadata and parts separately, hydrates parts only for selected messages, and pages backward with a stable `(created time, id)` cursor. It requests `limit + 1` rows to determine whether another page exists. Part updates and text deltas are independent events.

Jarvis already pages visible history and streams live deltas, so a storage rewrite only makes sense as part of the journal fix. The transferable principle is to persist each changed part or event once instead of appending the entire accumulated turn after every tool step.

### Compaction and old tool outputs

OpenCode's `packages/opencode/src/session/compaction.ts` uses two stages:

- old tool outputs can be marked compacted before full conversation summarization;
- full compaction preserves a recent tail selected by a token budget and can replay the overflowing request after compaction.

Relevant constants in this snapshot include a 20,000-token minimum before pruning, 40,000 protected tokens, 2,000-character tool-output serialization for the summary, and a recent budget bounded from 2,000 to 15,000 tokens unless configured.

Jarvis should not clear stored Context-mode results. It may, however, replace old provider-wire tool payloads with stable indexed references after their full content is durably searchable. This would shrink provider replay while preserving audit/history data elsewhere.

OMP provides the stronger compaction baseline for Jarvis. `packages/agent/src/compaction/compaction.ts` reserves 16,384 tokens by default, keeps 20,000 recent tokens, adjusts for model/context size, and cuts at valid message boundaries. Jarvis should continue using an 80% trigger while separately measuring reserved response space; a percentage alone is unsafe for small or unusually large model windows.

### Bounded reads

OpenCode's `packages/opencode/src/tool/read.ts` combines:

- line paging;
- a default 2,000-line limit;
- 2,000-character per-line truncation;
- a 50 KiB byte ceiling;
- binary/media handling;
- an LSP warm-up after source reads;
- metadata listing instruction files loaded for that path.

Jarvis already caps files at 1 MiB and result output at 32,000 characters, with Context-mode substitution above its lower output budget. The missing efficiency gain is semantic navigation. Reducing the generic read limit alone would create more round trips.

### LSP navigation

OpenCode's `packages/opencode/src/tool/lsp.ts` exposes:

- go to definition;
- find references;
- hover;
- document and workspace symbols;
- go to implementation;
- incoming and outgoing call hierarchy.

These operations provide bounded, code-aware answers and are especially valuable in large TypeScript, Rust, Go, and Python projects. They should be optional per language server availability and return a clear unavailable result rather than causing the model to retry blindly.

### Transactional patches and diagnostics

OpenCode's `packages/opencode/src/tool/apply_patch.ts` parses and validates the requested multi-file changes before applying them. After mutation, it notifies watchers, touches edited documents in the LSP, collects diagnostics, and returns a compact file summary plus errors for affected files.

Jarvis currently provides exact single-occurrence `edit` and full-file `write`. They are safe but can require many inference/tool round trips for a coherent multi-file change. A native patch operation should:

1. validate every path and hunk before writing anything;
2. reject symlinks and paths outside the project;
3. preserve line endings and file permissions where applicable;
4. stage all temporary files, then atomically replace targets where the platform permits;
5. roll back or leave every target untouched if validation fails;
6. return changed paths and bounded LSP diagnostics;
7. honor Plan/Build permissions and the existing approval model.

### Cache policy

OpenCode's `packages/llm/src/cache-policy.ts` places automatic cache breakpoints at the last tool definition, last system part, and latest user message for protocols with inline cache markers. It skips that pass for OpenAI and Gemini routes, where caching is implicit or uses a different mechanism.

Jarvis already implements the essential equivalents:

- a stable conversation/session identifier for provider affinity;
- deterministically sorted tool definitions;
- explicit markers for supported Anthropic/OpenRouter shapes;
- no invalid inline markers for OpenAI Responses;
- cache-read/write telemetry from provider responses.

No generic provider or external-result cache layer is recommended. Jarvis separately reuses repeated local file reads within one active turn, but only after hashing the complete protected file again; it clears those references when history is compacted and does not retain file contents in the cache. Provider cache work should remain measurement-focused: cache-hit ratio per provider/model/flow, stable-prefix size, and invalidation reasons such as changing tools, system instructions, or model selection.

### Model-family prompt overlays

OpenCode's `packages/opencode/src/session/system.ts` selects a small base prompt by model family. This recognizes that identical wording does not produce identical tool behavior across GPT, Gemini, Claude, Kimi, and other families.

Jarvis can use this as a controlled experiment for Antigravity/Gemini, where weaker Context-mode adherence has been observed. Overlays should stay small, preserve the same policy, and change only operational phrasing. Each overlay needs telemetry and an escape hatch; large divergent prompts would fragment behavior and reduce cache reuse.

## OMP and Metis comparison

OMP remains the better stability reference for the agent loop and compaction:

- `crates/pi-natives/src/workspace.rs` discovers directory-scoped `AGENTS.md` files while pruning ignored directories;
- `packages/agent/src/compaction/compaction.ts` has explicit response reserve, recent-context retention, valid cut points, and auto-continue behavior;
- `packages/ai/src/utils/tool-call-loop-guard.ts` hashes consecutive canonical tool calls;
- `packages/coding-agent/src/config/settings-schema.ts` enables the loop guard by default at five identical calls;
- `packages/coding-agent/src/session/tool-call-loop-redirect.ts` injects a corrective steer containing bounded argument/result summaries;
- checkpoint entries and the checkpoint tool preserve explicit long-running investigation state.

Metis confirms several compatible design choices:

- `src/core/session-manager.ts` uses append-only JSONL entries for messages, model changes, compactions, and branch summaries;
- `src/core/compaction/` records file operations and continuation state in structured summaries;
- `docs/models.md` treats cache read/write as provider capabilities and usage fields rather than pretending every provider supports the same cache-control protocol;
- project instruction loading remains a runtime trust decision rather than a voluntary model action.

OpenCode adds the strongest examples for LSP tooling, multi-file patches, normalized part storage, and lazy instruction attachment. OMP adds the more mature loop guard and context-budget behavior. Metis supports the append-one-entry persistence direction. Jarvis's Context-mode remains stronger than all three for retaining searchable large outputs.

## Proposed implementation options

### Option A: Incremental journal persistence

Priority: immediate.

Replace cumulative `turn_checkpoint` payloads with append-once deltas or periodic bounded snapshots plus deltas. Add a Windows-safe vacuum that writes a new file, flushes it, and atomically swaps it only after validation.

Acceptance criteria:

- a 500-step synthetic turn grows approximately with new event bytes rather than `O(steps × accumulated turn size)`;
- the Portal ITA reproduction is at least 90% smaller;
- recovery after termination at every persisted event boundary reconstructs the same tool/result state;
- compaction markers, file checkpoints, retry state, and paged history remain intact;
- interrupted vacuum leaves either the previous valid journal or the complete replacement on macOS and Windows.

Tracked separately by `jarvis-ncf`.

### Option B: Lazy hierarchical `AGENTS.md`

Priority: high.

Extend file tools with an instruction resolver. On the first read/edit/write/search result inside a directory during an agent turn, attach applicable instruction files between the project root and target directory. Cache canonical paths and content hashes for that turn.

Acceptance criteria:

- root instructions still reach every model before its first call;
- a nested instruction is attached before the model acts on a file in its scope;
- each file is attached once per turn;
- sibling package instructions do not leak into each other;
- symlinks and parent traversal cannot load external files;
- tests cover nested monorepos, missing files, oversized files, and changed instructions;
- provider cache telemetry shows the stable base prompt remains reusable.

### Option C: Native LSP tool

Priority: high.

Create a Rust-managed language-server registry and expose a bounded read-only tool. Start servers lazily per project/language and stop them with the project/application lifecycle.

Acceptance criteria:

- definitions/references/symbol lookup works for the initial supported languages;
- unavailable or broken servers return one actionable error without automatic retry loops;
- results are path-confined, capped, and indexed by Context-mode when large;
- a benchmark corpus shows fewer read/search calls and fewer input tokens than text-only navigation;
- idle servers have a bounded memory and process lifetime.

### Option D: Transactional `apply_patch`

Priority: high after LSP foundations.

Add one Build-only multi-file patch tool with preflight validation and post-write diagnostics.

Acceptance criteria:

- an invalid hunk changes zero files;
- mixed add/update/delete/move patches are covered on macOS and Windows;
- path and symlink confinement matches existing tools;
- changed files preserve expected permissions and line endings;
- returned diagnostics are limited to affected files and bounded for Context-mode;
- representative multi-file tasks use fewer provider/tool round trips than repeated `edit` calls.

### Option E: Repeated-tool loop guard

Priority: medium-high.

Track canonical `(tool name, arguments, normalized result class)` sequences across consecutive model iterations. At five identical calls, inject one synthetic corrective explaining the repeated call and last bounded result. If the same loop repeats after correction, end with a specific recoverable error and notify the user.

Acceptance criteria:

- legitimate polling and explicitly exempt tools are not blocked;
- equivalent JSON argument ordering hashes identically;
- the first threshold produces a steer, not a hard stop;
- ignoring the steer produces a clear error and system notification;
- no global step limit is introduced;
- telemetry records avoided calls and provider/model distribution.

### Option F: Model-family operational overlays

Priority: experimental.

Add concise GPT, Gemini, and Claude variants for tool-use phrasing while sharing one canonical policy. Start with a Gemini overlay emphasizing Context-mode retrieval before repeated reads and early tool execution after planning.

Acceptance criteria:

- policy and permissions remain identical across models;
- an A/B fixture set measures tool calls, repeated reads, completion rate, latency, and tokens;
- adoption requires a statistically meaningful efficiency gain without reduced task success;
- prompt hashes and overlay version are recorded with telemetry;
- overlays can be disabled independently.

## Suggested sequence

1. Implement `jarvis-ncf` and add disk-amplification telemetry.
2. Add the repeated-tool loop guard because it is small, deterministic, and immediately limits waste.
3. Implement lazy hierarchical `AGENTS.md` loading.
4. Build the LSP registry and read-only navigation tool.
5. Add transactional patching on top of the LSP lifecycle.
6. Run model-family prompt experiments only after the deterministic measurements exist.

This order addresses the proven storage defect first, then prevents obvious provider waste, then improves instruction correctness and semantic code navigation. It also creates the telemetry needed to decide whether prompt specialization is actually useful.
