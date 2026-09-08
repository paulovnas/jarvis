# Context efficiency and cache telemetry

Jarvis keeps provider replay smaller without imposing a maximum number of agent
steps. The full visible conversation and original tool results remain in the
session journal; model replay can use indexed previews and continuation summaries.

## Investigation baseline

A read-only analysis of the reported direct Designer session on September 8, 2026
found 10 turns and 296 measured inference steps with Gemini 3.8 Flash:

| Measurement | Observed value |
| --- | ---: |
| Cumulative main-agent input | 52,896,326 tokens |
| Cumulative main-agent output | 135,101 tokens |
| Median input per request | 179,516 tokens |
| Largest input request | 377,834 tokens |
| Model context window | 1,048,576 tokens |
| Compactions | 0 |
| Explicit Context-mode calls / automatic indexes | 0 / 0 |
| Browser snapshots / screenshots / Vision calls | 15 / 14 / 16 |
| Snapshot result content | 192,288 characters |
| Process-output result content | 146,848 characters |
| Read result content | 158,963 characters |
| Additional Vision + Web Search input / output | 33,685 / 23,784 tokens |

Dashboard checkpoint deduplication was correct. The large figure was cumulative
input resent over many requests, not a single context window, and does not
establish a billable token count or monetary cost. Browser screenshots were saved
as attachments, not embedded as base64 in the main replay. Vision made separate
requests. Repeated growing textual history was the principal amplification path.

The previous automatic Context-mode whitelist excluded browser and process output.
Compaction waited until close to the model's large context limit. Mutable workflow
state, design briefs, terminal tails, and Beads snapshots also changed request
prefixes. Cache breakdowns returned by providers were discarded.

## Runtime behavior

- Context-mode is a runtime invariant, independent of model, role or workflow.
  Every turn performs a local `ctx_search` memory lookup before inference (one
  bounded query, including a fresh/empty knowledge base). The model receives at
  most 1,200 characters of recalled context. Automatic queries are journaled with
  the inference step and displayed separately in Dashboard. The runtime rejects
  a toolset that removed `ctx_search` or `ctx_index`. Role permissions still apply;
  read-only agents cannot gain execution through Context-mode.
- Every tool passes through Context-mode hooks. Results over 8,000 bytes are
  indexed by default, including native reads, newly introduced tools and verbose
  `ctx_*` results. There is no provider/flow opt-out or whitelist of tool families
  that can accidentally omit a future tool. The original result is journaled while
  the provider receives a smaller preview plus a `ctx_search` source reference.
- Browser previews retain the first 20 current element IDs unchanged. The complete
  snapshot, including omitted element IDs, stays searchable under its source.
  Retrieval does not create a new snapshot or invalidate IDs. Navigation or a new
  browser snapshot still invalidates them normally.
- Small exact edit excerpts remain verbatim. Large native `read` results become
  indexed previews; agents can use `offset`/`limit` to obtain the exact smaller
  range needed for an edit. Only `read_skill` is exempt from index substitution:
  applicable instructions must be read in full through its native bounded reader.
  All hooks still run. Executable Context-mode tools get an automatic indexing
  `intent` when the model omitted it and the installed tool schema supports it;
  neither the code being executed nor its authorization is changed.
- Automatic compaction also triggers when estimated conversation replay reaches
  128,000 tokens, even for models with larger windows. The actual catalog limit
  and output reserve remain the safety boundary. The existing safe call/result
  grouping, latest user request, recent tail, summary and visible compaction event
  are preserved. This is a soft context budget, not a step limit or cancellation.
- Mutable runtime checkpoints are appended only when changed, outside the stable
  system prefix. Session memory and Beads snapshots are also appended as reference
  data instead of replacing the first input on every turn. Runtime records are
  explicitly marked as data and cannot replace the latest actual user request
  during compaction. Private markers are removed before provider requests.
- Terminal context advertises metadata; logs are retrieved with `terminal_output`
  when needed instead of attaching changing terminal tails to every system prompt.
- Browser instructions require a concrete unresolved question, favor DOM/console
  for nonvisual evidence, reuse captures when appropriate, batch related Vision
  questions, and stop collecting evidence once relevant criteria are satisfied.
  Browser access remains available. There is no screenshot or agent-step hard cap.

## Provider cache versus Context-mode

Provider prompt caching reuses inference work for matching prompt prefixes.
Context-mode reduces the content sent to inference. They are complementary:
cached input still occupies context and can still be charged at a provider-specific
rate; compacting context changes the prefix and may initially reduce cache hits.

Jarvis preserves the Codex session `prompt_cache_key` and the Antigravity session
identity. Custom OpenAI requests also receive a stable `prompt_cache_key` on the
verified official HTTPS host; OpenRouter gets the documented `x-session-id` header
for session affinity/grouping. Unknown gateways do not receive these extra fields.
Vision uses a stable conversation-specific identity, with unchanged image content
before the variable question to improve prefix reuse across related questions.
Equivalent tool catalogs are sorted by name before provider dispatch so arbitrary
MCP discovery order cannot invalidate an otherwise identical tool prefix.
For Claude models on the documented HTTPS Anthropic and OpenRouter hosts, Jarvis
adds ephemeral cache breakpoints in supported Messages/Completions requests.
Unknown gateways and non-Claude models retain their existing request contracts.
OpenAI and supported Gemini models offer implicit caching; actual Antigravity
gateway behavior and cache hits depend on the service. Jarvis does not create
paid Gemini explicit-cache resources or assume that API is exposed by Antigravity.

### Durable fields

`Usage` retains `inputTokens` and `outputTokens`, adding optional `cacheReadTokens`
and `cacheWriteTokens`. Absent fields remain unknown; reported zero remains zero.

| Protocol | Cache read | Cache write | Total input semantics |
| --- | --- | --- | --- |
| OpenAI Responses / Codex | `input_tokens_details.cached_tokens` | `input_tokens_details.cache_write_tokens` | `input_tokens` includes the breakdown |
| OpenAI-compatible Completions | `prompt_tokens_details.cached_tokens`, or DeepSeek `prompt_cache_hit_tokens` | `prompt_tokens_details.cache_write_tokens` | `prompt_tokens` includes the breakdown |
| Anthropic Messages | `cache_read_input_tokens` | `cache_creation_input_tokens` | Uncached `input_tokens` + cache read + cache write |
| Antigravity / Gemini | `cachedContentTokenCount` | Not inferred | `promptTokenCount` includes cached input |

Cumulative streaming usage snapshots replace previous values; they are not summed
as separate inference requests. Dashboard sums only latest durable turn checkpoints
and merges root/worker sessions. Native Vision/Web Search usage is counted once,
separately identifiable and included in the displayed accumulated totals.

The cache-hit percentage is `reported cache-read tokens / input tokens of requests
with valid cache-read reporting`. Coverage is displayed as reporting requests over
measured requests. Missing historical reporting never becomes a fictional cache
miss. Cache writes have their own reporting availability. No currency or cost
discount is inferred from token percentages.

Automatic indexing records original and retained bytes alongside the completed
step. Dashboard displays this separately from explicit Context-mode tool calls.
Byte reduction is neither tokenizer measurement nor repeated-request cost savings.

### Limits of the metrics

Old journals load without migration. Discarded historical cache details and old
automatic-index telemetry cannot be reconstructed reliably and are not invented.
Existing native Vision/Web Search outputs with usage can be projected immediately.
Totals reflect recorded successful inference responses, not a provider invoice:
failed requests without usage and background operations such as title generation,
summary inference and image generation are not included in these turn metrics.
Manual acceptance and live cache-hit/cost comparison require subsequent real
provider sessions; unit fixtures cannot establish a particular provider's savings.

## Sources and implementation references

Validation commands: `bun run check`, Rust Clippy with all targets/features and
warnings denied, and serial Rust tests. The optional
`installed_core_enforces_budget_and_recalls_in_isolation` test exercises the
installed Context-mode package with disposable data and no provider inference.
It verifies empty-memory lookup, mandatory large-read indexing, retrieval and
default indexing for a previously unknown tool. No real-provider optimization
run, Windows-native UAT, commit or release was performed as part of this change.

- [OpenAI prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching)
- [Google generateContent caching](https://ai.google.dev/gemini-api/docs/generate-content/caching)
- [Anthropic prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching)
- [OpenRouter prompt caching](https://openrouter.ai/docs/guides/best-practices/prompt-caching)
- Local design references: `docs/metis/docs/custom-provider.md` and
  `docs/metis/src/core/model-registry.ts` (read-only).
- Runtime: `src-tauri/src/core/context.rs`, `src-tauri/src/agent.rs`,
  `src-tauri/src/agent/compaction.rs`, `src-tauri/src/agent/workflow.rs`.
- Accounting: `src-tauri/src/agent/provider/`, `src-tauri/src/agent/dashboard.rs`.
- UI: `src/components/dashboard/UsageEfficiency.tsx` and `src/core/dashboard.ts`.
