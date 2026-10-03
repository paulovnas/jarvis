# Agent Harness Efficiency Audit

Date: 2026-09-10

## Scope

This audit evaluates Jarvis against the installed tool-use, agent architecture, context engineering, prompt engineering, grounding, evaluation, production hardening, and provider SDK skills. It uses two real Movarte conversations as regression evidence:

- the successful Planned flow `Seleção livre de status`;
- the direct request to query the Movarte Gemini Notebook MCP.

The implementation references `docs/metis`, `docs/opencode`, and `docs/omp` for architecture and runtime behavior. Those trees remain read-only.

## Production evidence

### Explicit MCP request drifted to an unrelated database

The user explicitly requested the Gemini Notebook MCP. Jarvis initially called `notebook_list`, found `Movarte Express`, and then called `notebook_query`. The MCP's own argument requested a 180-second timeout, but the Jarvis transport terminated the request after about 5 seconds. The model then called the unrelated Database MCP repeatedly and even attempted a disconnect.

The failure had three independent causes:

1. A single `timeout` setting controlled MCP startup, tool discovery, and normal tool execution. The configured 5-second startup limit therefore also killed a legitimate long-running query.
2. Every enabled MCP and every tool schema was exposed simultaneously. Gemini Notebook contributed 48 tools and Database contributed another 6, even though the user had named one integration.
3. Jarvis did not preserve MCP server ownership as an execution scope after tool discovery. A provider could select any exposed MCP tool after the requested one failed.

The direct-task preflight also treated every `mcp_*` call as mutating. This incorrectly required a task before read-only calls such as `notebook_list` and `notebook_query`.

### Planned flow completed, but spent calls on avoidable recovery

The main Planned turn made 149 tool calls and recorded 9 failed calls. Its two workers made 109 and 149 calls. The implementation was correct and passed user validation, but the trace exposed avoidable harness friction:

- `search` against a file path attempted to resolve `<file>/AGENTS.md`, converted `ENOTDIR` into a misleading invalid-instructions error, and forced a fallback.
- `hub_spawn` advertised every built-in role even when the current flow only allowed Builder and Designer. The Planner therefore selected Reviewer, which runtime policy later rejected.
- workers retried edits and patches after their source excerpts became stale;
- workers requested `bun_typecheck` in packages that had no `typecheck` script;
- an empty Context-mode search produced little recovery guidance;
- a patch executor failure surfaced as a generic agent error.

## Applied skill guidance

The changes follow the shared recommendations across the installed skills:

- classify intent before routing tools;
- expose the smallest useful tool catalog;
- preserve explicit tool and integration selection;
- validate tool names, arguments, schemas, permissions, and role contracts before execution;
- return structured, actionable errors;
- retry only bounded, idempotent operations and never replay an uncertain side effect automatically;
- keep static prompt content stable and place mutable state late;
- apply least privilege by role and tool semantics;
- turn real failures into deterministic regression tests.

## Implemented hardening

### MCP routing and request lifecycle

- `timeout` now means startup/discovery timeout and defaults to 30 seconds.
- `requestTimeout` (with `request_timeout` accepted as an input alias) controls tool calls independently and defaults to 300 seconds.
- Existing configurations with only `timeout: 5000` retain a 5-second startup limit while receiving the safer 300-second request limit.
- An explicit phrase such as `use o MCP do Gemini Notebook` is normalized across case, accents, punctuation, hyphens, and a redundant `-mcp` name suffix.
- Explicit requests connect and expose only the named MCP. A disabled, incomplete, failed, or unauthorized requested MCP returns a focused error; Jarvis does not expose an alternative integration.
- Generic turns expose one lightweight `mcp_activate` selector instead of every MCP schema. The selected server's tools appear in the next model step. At most three servers can be activated implicitly in one turn; an explicit multi-server request remains possible.
- Generic discovery keeps only non-secret server metadata in memory. Jarvis loads the selected MCP configuration from secure storage only when the model activates that server; an explicit request loads only the named integration.
- MCP ownership uses the persisted server ID and generated wire name rather than textual tool-name prefixes.
- Explicit MCP turns require at least one attempted call before a final answer.

### MCP safety and recovery

- Errors now preserve specific codes for invalid arguments, request timeout, server/tool failure, unavailable requested servers, and scope violations.
- Error messages identify the server and tool without exposing configuration secrets.
- Timeout recovery distinguishes read-only calls, which may be retried once, from effectful calls with uncertain outcomes, which must be verified before repetition.
- Read-only classification honors MCP annotations first and safely recognizes conventional verbs such as `list`, `get`, `read`, `search`, `query`, `lookup`, `fetch`, `describe`, `status`, and `resolve`. Any mutation verb or explicit non-read-only annotation wins.
- Read-only MCP calls no longer require a direct-flow task or manual effect approval. Ambiguous and mutating MCP calls remain protected.

### Tool-loop and workflow efficiency

- A failed `edit` or `apply_patch` marks its target files stale. Further mutation of those files is rejected as a recoverable tool result until each file is read again.
- The existing five-identical-call loop guard remains in place. No arbitrary total-step limit was introduced.
- Repeated local `read` calls reuse the earlier result only after reopening the protected file and matching a SHA-256 fingerprint of its complete bytes. The cache stores no file content, is limited to 64 range identities per turn, skips results compacted by Context Mode, and is cleared whenever provider history is compacted or a later read fails.
- File-scoped searches resolve hierarchical `AGENTS.md` files from the containing directory.
- `hub_spawn` publishes only roles allowed by the current `(flow, parent role)` topology.
- Bun workflow checks inspect `package.json` first. Missing scripts return the actual available script names instead of starting a guaranteed-failing process.
- Context-mode search failures tell the agent how to seed or process new content before retrying retrieval.
- Unexpected patch executor termination reports an uncertain mutation outcome and requires inspection before another write.

## Prompt-cache audit

Jarvis already handles the provider-specific cache mechanisms that are available without inventing unsupported behavior:

- OpenAI Responses sends a stable conversation `prompt_cache_key` and reports cached input as a breakdown of total input tokens.
- Compatible Anthropic Messages and OpenRouter request paths add supported ephemeral `cache_control` markers to stable system/tool/history boundaries. Unknown compatible endpoints do not receive fields that their protocol alone does not guarantee.
- Provider tool definitions are sorted deterministically, so reconnect order and operating-system enumeration do not invalidate an otherwise identical prefix.
- Shared instructions are assembled before mutable runtime checkpoints. Scoped project instructions and newly activated tools necessarily change the suffix when the work scope changes.
- Gemini/Antigravity caching remains provider-managed; Jarvis preserves stable request prefixes and records cache usage when the provider reports it.

The largest provider-prefix improvement is MCP deferral. A normal turn no longer pays for dozens of unused external schemas. Activating a server changes the catalog once, at the point where those schemas become relevant. Local read reuse is a separate harness reduction: it omits a duplicate file payload only while the referenced result is still present in the active replay and the complete file remains byte-for-byte identical.

### External-result reuse assessment

External results remain uncached. A read-only MCP annotation says that a call should not mutate state; it does not provide a freshness validator for the returned data. Web Search is intentionally time-sensitive, and Context7 can publish updated documentation without changing the model's arguments. A time-to-live would reduce calls but could not guarantee validity, while reissuing the request to validate it would usually erase the intended latency and cost benefit.

The safe future boundary is opt-in per integration: reuse only when the upstream protocol returns an immutable version identifier or supports a conditional validator such as an ETag, and include every scope-changing argument and authenticated tenant in the identity. Effectful calls, uncertain outcomes, generated media, Vision, generic MCP tools, Web Search and unversioned documentation queries must remain excluded.

## Regression coverage

The Rust suite now covers:

- legacy startup timeout versus the independent request timeout;
- a real MCP call that runs longer than its one-second startup timeout and completes within its request timeout;
- camelCase and snake_case request-timeout input;
- explicit Gemini Notebook intent with only that server exposed;
- rejection of an unrelated MCP wire tool within explicit scope;
- failure of a disabled explicit MCP before any fallback connection;
- deferred MCP activation and post-activation tool visibility;
- deferred secure-configuration loading, including a one-server bound for explicit requests;
- read-only Notebook-style names versus effectful Database-style names;
- structured invalid-argument errors;
- file-path `AGENTS.md` discovery;
- topology-specific `hub_spawn` schemas;
- missing Bun scripts with real alternatives;
- stale edit and multi-file patch recovery;
- equivalent local read ranges, same-size changes, changes outside the requested range, file removal, symbolic-link replacement, direct and shell mutations, bounded capacity, and compaction invalidation;
- read-only MCP approval behavior.

## Follow-up measurements

The next useful optimization should be driven by telemetry rather than another broad prompt change. Capture, per turn and per provider:

- tool schemas sent before and after activation;
- input, output, cache-read, and cache-write tokens;
- invalid-argument, timeout, repeated-call, and scope-violation counts;
- time to first useful tool call and time spent in recovery;
- MCP activations by server and unused activated schemas.
- validated local-read reuse count and original versus retained bytes.

These measurements can become a small versioned evaluation corpus. The two Movarte traces should remain the first golden cases: one for successful multi-agent completion with bounded recovery, and one for explicit integration fidelity.
