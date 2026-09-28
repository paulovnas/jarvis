# Learning from project feedback

Research date: 2026-09-28. Tracking: `jarvis-tfvx`.

Status: implemented locally under `jarvis-8kzi`; final validation is recorded in the delivery section below. The preceding project-knowledge feature was committed separately as `c4c88c1` before this work started. This learning implementation has not been committed or pushed.

## Implemented contract

- Project Details → Options now includes **Aprendizados do projeto**. Future-feedback capture and recall start enabled. The project switch disables both while preserving editable records. The UI explains the additional model usage.
- Real root user messages and live corrections are captured independently of successful task completion. A small Portuguese/English cue filter excludes ordinary tasks, explicit one-off requests, code blocks and quoted instructions before scheduling extraction. There is no historical backfill. Other languages or corrections without these cues can be missed.
- A durable queue in the existing app SQLite database holds up to 20 feedback messages per project. One extraction runs globally at a time, with a 20-second inference deadline and at most two attempts separated by 30 seconds. Pending work resumes on the next chat execution after restart. Learning never becomes a startup requirement, approval request or implementation failure.
- Extraction uses the selected chat provider/model with low supported reasoning, a bounded feedback excerpt and nearby evidence, relevant existing lessons and authored rules. It makes no tool calls and returns at most three records. Secrets matching common credential formats are redacted before storage or extraction; this is a best-effort filter, not a guarantee that arbitrary sensitive prose is detected.
- `learn_project` gives agents a second capture path. Both paths validate literal evidence against an actual eligible user correction, the directory scope and record budgets. A subagent uses its root conversation's sources, not its generated task prompt as supposed user evidence.
- Exact normalized duplicates share a record. A later explicit correction can disable a prior automatic lesson in the same scope. Conflicts targeting user-edited, disabled or different-scope lessons remain inactive suggestions. Semantic extraction/deduplication still depends on the model; the local validator does not prove two differently worded lessons mean the same thing.
- Users can inspect source excerpts, edit, disable, delete, search, export and import lessons. Edits/deletions invalidate in-flight extractions; hashed source/fingerprint exclusions prevent older evidence from recreating deleted or replaced text. Original chat messages are preserved. Import previews support scope remapping and imports start as suggestions. Export omits conversation evidence and provider configuration; the general settings backup excludes project lessons.
- **Incorporar às regras** prepares an editable before/after preview, preserving the existing document and its essential rules. Saving uses the maintained document's revision. Automatic capture never rewrites `AGENTS.md` or project Markdown.
- Rust selects lessons locally by task terms, topics, explicitly mentioned scope and observed tool paths. Native providers and Claude share this recall path, as do workflow children and model fallbacks. IDs/revisions prevent repeated unchanged injection; lost context is restored after compaction. Selection stays subordinate to current user intent and authored instructions, never grants permissions and never claims suggested checks have passed.
- `project_knowledge(kind="learning")` provides additional scoped read/search access. Automatic recall does not rely on agents calling this tool. A lesson first discovered from a tool path is delivered as subsequent runtime guidance; it is not a pre-execution enforcement gate.

Hard bounds: 200 records per project, 600 characters per lesson, 300 per verification hint, eight short topics, eight evidence references, 12,000 characters of extraction input, 12,000 bytes of structured output and up to six lessons/2,400 characters of lesson payload per automatic selection. Saturation and extraction failures are shown in project options without stopping the task. Local recall has a 500 ms best-effort deadline and does not require internet or Context-mode.

Deferred: a separately configurable extraction model, embeddings, historical bulk learning, global cross-project profiles, autonomous validation mining, retrieval analytics, automatic age-based eviction and a dedicated chat activity card. The existing tool history labels `learn_project`; the settings panel is the source of truth for saved/suggested records. No measured efficiency or compliance improvement is claimed without live comparisons.

## Recommendation

Add native, project-scoped learning from user corrections. Store short, evidenced lessons, retrieve the relevant ones automatically, and use applicable lessons to guide regression checks. This directly addresses repeated reminders about spacing, interactive affordances, component reuse, and Select labels.

This is persistent operational memory, not model-weight training. Jarvis would remember what was corrected and provide that knowledge to whichever agent/provider handles the next relevant task. A different model, a new chat, or a compacted conversation should not erase the lesson. Memory improves the information available to the model; it does not guarantee that the model will obey it. Observable checks remain necessary.

The smallest useful implementation reuses Jarvis's existing SQLite database, project/repository scopes, instruction resolver, provider adapters, and project-knowledge UI/tool. It needs neither a vector database nor a new core installation, recursive reflection workflow, or autonomous agent that rewrites project rules.

## Evidence from other systems

Local references were inspected read-only. These are repository snapshots, not claims about the latest upstream release:

| Reference | Observed behavior | Relevant decision for Jarvis |
| --- | --- | --- |
| Codex, `a592c38c16cd` | Separate extraction and consolidation. Bounded eligible sessions, worker leases, retry backoff, secret redaction, and retention informed by use. Current extraction prompts distinguish actual user corrections from assistant claims and single-task requests from reusable preferences. Consolidation preserves provenance and respects deleted/corrected claims. | Separate evidence capture from lesson selection. Keep updates incremental, scoped, reversible, and outside the foreground task. Do not generalize authorization or preferences beyond their evidence. |
| OMP, `5964a0f76492` | Local `learn` persists explicit lessons with deduplication, redaction and bounds; its local summary pipeline operates separately. The local lesson prompt prefix changes on the next session. Optional richer backends offer retain/recall/reflect and memory editing. | A small local lesson store is sufficient initially. Learn from the bounded local implementation rather than installing its remote memory stack. Preserve prompt caching and distinguish queued from persisted writes. |
| OpenCode, `830d5eb53548` | The inspected instruction resolver loads applicable nearby instructions and avoids attaching them repeatedly. Session compaction retains continuation context. These inspected paths are not a cross-chat correction-learning mechanism. | Reuse scope-aware loading and deduplication. Conversation compaction and project learning serve different purposes. |
| [Claude Code memory documentation](https://code.claude.com/docs/en/memory) | Auto memory captures corrections/preferences across sessions, separately from authored instructions. It uses repository scope, a concise startup index, on-demand topic files, and user controls. The documentation explicitly says memory is context, not enforced configuration. | Separate authored knowledge from learned lessons; provide inspection/edit/delete controls and relevant automatic recall. Do not promise deterministic compliance from prompting. |
| [Anthropic: effective context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) | Describes progressive disclosure, structured notes outside the context window, and context as a finite resource. Warns that exploration has runtime cost and excessive context can reduce effectiveness. | Retrieve a small relevant set locally; avoid rereading complete histories or adding all lessons to every prompt. |
| [Reflexion](https://arxiv.org/abs/2303.11366) | Studies verbal feedback and episodic memory to improve subsequent attempts without updating model weights. | User corrections and verified outcomes can inform later tasks. Its benchmark results do not establish a Jarvis improvement. |
| [Agentic Context Engineering — ACE](https://arxiv.org/abs/2510.04618) | Studies incremental context adaptation and the loss of useful detail through repeated monolithic rewriting. | Update individual lessons and retain evidence instead of repeatedly summarizing the entire knowledge base into a shrinking paragraph. Do not import the complete research architecture. |

Claude's documentation currently describes a startup index limit of 200 lines or 25 KB, whichever comes first. OMP's local lessons are capped at 100 entries, with per-entry limits and a shared injection budget. These demonstrate bounded designs, not appropriate defaults to copy unchanged into Jarvis.

### Inspected contracts and tests

- Codex: [memory pipeline overview](codex/codex-rs/memories/README.md), [extraction policy](codex/codex-rs/memories/write/templates/memories/stage_one_system_v2.md), [consolidation policy](codex/codex-rs/memories/write/templates/memories/consolidation_v2.md), [output schema/redaction](codex/codex-rs/memories/write/src/phase1_output.rs).
- Codex tests: [bounded Unicode evidence and excluded contextual fragments](codex/codex-rs/memories/write/src/rollout_input_tests.rs), [quota-aware background work](codex/codex-rs/memories/write/src/guard_tests.rs). The latter avoids starting background memory work when usage limits are too low.
- OMP: [autonomous memory](omp/docs/memory.md), [learn tool](omp/packages/coding-agent/src/tools/learn.ts), [retain tool](omp/packages/coding-agent/src/tools/memory-retain.ts), [Mnemopi scopes and recall](omp/docs/mnemosyne-memory-backend.md).
- OMP tests: [local lesson behavior](omp/packages/coding-agent/test/autolearn-learn-local.test.ts), including redaction, exact deduplication, size limits, empty records, and concurrent saves without lost lessons. These tests were inspected, not executed.
- OpenCode: [instruction resolver](opencode/packages/opencode/src/session/instruction.ts), [instruction tests](opencode/packages/opencode/test/session/instruction.test.ts), [compaction](opencode/packages/opencode/src/session/compaction.ts). Tests cover applicable subdirectory rules and avoiding duplicate attachment within a message.

## Baseline at the time of research

This table describes the baseline before the learning implementation above.

| Existing mechanism | Evidence | What it does not yet provide |
| --- | --- | --- |
| Session memory and automatic recall | `src-tauri/src/core/context.rs:37–44` constructs a bounded recall query and stores Context-mode data under a session hash. `:265–335` bounds recall output and handles timeout/failure. | A shared project registry of curated corrections with lifecycle and provenance. |
| Native memory hooks | `src-tauri/src/core/hooks.rs:11–28` covers session start, user prompt, tool results, compaction and turn end. `:80–124` makes auxiliary hook failures nonfatal. | Classification of reusable lessons versus temporary requests. |
| Historical-data boundary | `src-tauri/src/agent.rs:2740–2780` recalls history and labels injected references as historical data subordinate to current user instructions. | The cross-chat lesson capture/retrieval contract proposed here. |
| Maintained project knowledge | `src-tauri/src/agent/knowledge.rs` provides scoped PRD/TRD/rules/design documents, bounded search and essential rules; `ProjectKnowledgeSettings.tsx` exposes them in project options. | Automatic feedback learning. Its native tool is currently read-only and documents remain explicitly edited/saved. |
| Scope-aware instructions | `src-tauri/src/agent/instructions.rs` resolves affected paths and adds applicable knowledge rules and `AGENTS.md` content without making optional metadata a task blocker. | Selecting and tracking learned lessons by topic and affected scope. |
| Existing local database | `src-tauri/src/persistence.rs` already uses SQLite, migrations and a local `jarvis.db`. | Lesson records, evidence references and processing cursors. No new database engine is needed. |

The existing knowledge feature and learning are complementary:

- **Authored knowledge:** what the product is, how it is built, and which rules/design decisions the user maintains.
- **Learned lessons:** which mistakes recurred, what the user corrected, when that correction applies, and how to check it.
- **Execution history:** what happened during a particular attempt; retain it in existing session/task mechanisms.

Do not duplicate an existing `AGENTS.md` rule as a growing collection of paraphrases. For example, Jarvis already requires `cursor-pointer`: recurring violations indicate that retrieval or verification is failing. A useful lesson can reference that rule and the missed check instead of adding another copy of the same instruction.

## Proposed behavior

### Capture useful feedback without slowing the task

Persist the real user message first. A background collector examines only new candidate feedback and the small amount of neighboring evidence needed to interpret it. It must also handle corrections during a running turn and before an interrupted task finishes; learning cannot depend on a successful final response.

The active agent can submit a lesson candidate through one native operation, referencing actual message IDs. A bounded extractor supplies a second capture path when the agent does not do so. Both paths share the same validation, deduplication and storage. A tool alone is insufficient: the agent can forget to call it just as it can forget an existing instruction.

| Input | Treatment |
| --- | --- |
| Explicit reusable correction, such as “always use the project Select” | Save a scoped active lesson once its source and meaning are validated. No approval popup per lesson. |
| Specific correction of a reproducible defect, such as displaying a raw option value | Save the narrow defect/prevention lesson; do not infer unrelated design preferences. |
| Pattern across distinct user corrections | Create a bounded candidate; activate only when its reusable meaning and scope are supported. Ambiguous candidates remain visible suggestions without blocking the chat. Repeated assistant statements do not count as independent evidence. |
| “Only this time”, a temporary branch decision, or a one-off deployment request | Preserve in the current task, not as a permanent project preference. |
| A test result or implementation outcome | May support a technical lesson with the actual result attached. “The agent says it worked” is not verification. |
| Generic praise, isolated frustration without a concrete correction, ordinary logs | No lesson unless nearby real evidence establishes a specific useful correction. |
| Quoted instructions, tool output, web pages or subagent suggestions | Evidence only; cannot impersonate a user preference or authorization. |

Automatic capture should target future feedback when enabled. A historical backfill is a separate user-triggered operation with a range and cost limit; never scan every past chat during bootstrap.

### Store a small, inspectable record

Each record needs a stable ID, project ID, optional repository/path scope, short lesson, applicability tags, optional verification guidance, source message/event IDs, minimal redacted supporting excerpt, dates, state, and revision. Link observed verification separately from a suggested future check.

Use explicit evidence classes such as `user_correction`, `recurring_feedback`, and `verified_outcome`, rather than a model-invented numerical confidence percentage. Preserve the distinction between suggested, active, superseded and disabled lessons.

Keep automatic lessons in the existing app-local database. This supports atomic concurrent updates from several chats and prevents conversational feedback from unexpectedly appearing in a Git commit. It also survives repository moves through a stable project ID. Repository/path scopes must be revalidated when the project location or configured repositories change.

The maintained Markdown knowledge stays in its current project files. Offer explicit export/import of lessons and deliberate promotion into Rules/Design with a visible diff; do not silently rewrite `AGENTS.md`, PRD, TRD, rules or design documents. Extend the existing backup/import contract when implementing this store, including scope remapping and a clear distinction between local app memory and project Markdown.

### Retrieve automatically, where it matters

Selection belongs to the Rust runtime and must not depend solely on the model deciding to search:

1. Before the first model request, retrieve relevant active lessons using the project, current task, and known repository scope.
2. When the affected files or delegated task become known, add applicable lessons not already supplied. Reuse the instruction resolver and handoff preparation rather than asking the agent to perform another research cycle.
3. Before completion, surface the relevant prevention/check guidance for touched areas. Reuse checks already performed; do not require a second build or test run just because memory was recalled.

Share the same project memory across direct agents, native/custom workflows, subagents, model fallbacks and Claude Code integration. A role is a ranking hint, not a private memory silo: a Builder modifying UI needs the relevant design lesson too.

Start with local lexical/tag/path matching over a small collection and extend `project_knowledge` with an optional learned-lesson read/search category. Keep authored documents and automatically learned evidence clearly distinguished in tool results. The save operation is separate and narrowly scoped to lesson candidates, not arbitrary file writes.

Use a stable prompt prefix and append bounded runtime context with lesson IDs/revisions. Do not rewrite the base instructions on every capture. Compaction/resume must preserve the applied IDs or restore the relevant current records once; they must not replay completed actions or duplicate the same memory every turn.

Current user intent and applicable authored instructions take precedence over inferred memories. Memories never create tool permissions, publication authorization or permission to ignore a later request. If a current instruction intentionally differs, honor it for that task without silently turning it into a new global preference.

### Keep corrections reversible

Normalize exact duplicates within scope and append new evidence instead of another entry. Update individual records instead of regenerating the whole store.

A later explicit correction can supersede an earlier lesson for the same scope. An inferred contradiction should remain inactive rather than silently overwriting a user-edited rule. If ambiguity actually prevents the requested task, use the normal clarification path; memory maintenance itself is not a reason to stop work.

Let users edit, disable and delete lessons. An exclusion marker and processing cursor must prevent old transcripts from immediately recreating a deleted lesson. A delete removes lesson content/excerpts; exclusion metadata should contain no retained plaintext. Deleting a lesson is distinct from deleting its source conversation. Fresh explicit feedback can intentionally establish a new lesson.

Age alone must not expire an explicit user preference. Technical claims tied to a component/version need freshness checks when that source changes. Retrieval counts help diagnose relevance, but being frequently retrieved is not proof that a lesson is correct or useful.

### Keep resource use bounded

Starting budgets to validate, not measured guarantees:

| Resource | Initial proposal |
| --- | --- |
| Capture input | A new feedback delta and minimal evidence, up to 12,000 characters; no project scan or full-history replay. |
| Model work | At most one short, tool-free extraction per candidate turn; coalesce repeated triggers. Reuse the selected provider through existing adapters; allow a configured cheaper model without silently changing providers. |
| Output | Up to three candidates, each with at most 600 characters of lesson text and bounded metadata. Empty output is valid. |
| Foreground recall | Up to six relevant lessons and 2,400 characters total. Fetch additional detail only on demand. |
| Execution | One background extraction at a time, a 20-second deadline, cancellation, bounded queue and persisted processing cursor. No recursive agents or unbounded retries. |
| Retention | Start with a bounded active collection, for example 200 lessons per project. Deduplicate/retire superseded automatic entries before accepting more; never silently evict user-maintained lessons. Surface saturation in the settings panel, not as a task failure. |

These bounds add cost, including extra model input for recalled lessons. The intended saving is fewer corrections and repeated investigations, not “free tokens.” Record actual latency/tokens and adjust from evidence.

When the provider is offline, rate-limited, cancelled or returns invalid data, preserve the feedback source and continue the user's task. Schedule only bounded later retry with backoff. Do not turn learning into a failed implementation, task blocker, startup dependency or publication approval. Local recall should work without internet or Context-mode; unavailable memory must not block the agent.

Validate records and source ownership at the boundary. Exclude credentials and sensitive payloads before extraction where possible and redact before persistence. Structured output and secret filtering are useful controls, not proof that arbitrary model text is trusted. Candidate processing must distinguish real user messages from Jarvis runtime messages and imported/quoted text.

## How the user's examples should work

| Correction | Useful lesson | Relevant prevention/check |
| --- | --- | --- |
| Buttons and badges repeatedly have poor spacing | “When changing compact controls, check text/icon gaps, padding and alignment against neighboring project components.” | Reuse shared variants. Verify visually at the relevant sizes; a passing lint run cannot establish good spacing. |
| Missing pointer on actionable UI | Reference the existing interactive-affordance rule and the affected component class. | Prefer correcting a shared component once. Use targeted style/component checks where reliable; avoid a broad text regex that flags unrelated elements. |
| Native Select used instead of the project component | “Use the project's approved Select/Combobox for this frontend scope.” | Locate the existing component before adding one. Add a narrow import/component policy check only if appropriate to the project's tooling. |
| A selected option displays its raw value | “Keep the option value as the stored identifier and show its user-facing label, including after options load or selection is restored.” | An observable regression test selects or restores a value and checks the visible label; cover asynchronous options if that was the defect. |

The lesson should reach the next relevant implementation automatically, not wait until the user says “that same problem again.” Where a deterministic check fits, add it as part of the authorized bugfix. Do not auto-generate speculative tests or modify unrelated projects merely because a lesson was stored.

## Product experience

Add **Aprendizados** alongside maintained knowledge in Project Details → Options, using existing shadcn components and the current visual system.

Provide one clear project switch, **Aprender com minhas correções**, with a short explanation of persistence and occasional model use. Recommend enabling capture for future feedback by default when the feature is introduced, with a clear off switch; keep historical backfill opt-in. Turning it off stops automatic capture and recall while retaining editable records.

Show concise lessons with scope, state, evidence origin and last update. Offer edit, disable/delete, and deliberate promotion to maintained knowledge. A compact expandable activity entry can say **Aprendizado registrado** or **3 aprendizados aplicados**, linking to the records. Use accurate states: proposed, queued, saved and applied are different. Avoid a toast or approval dialog for every lesson.

The active task remains the main experience. Learning must not create another mandatory review phase or permanently display a panel that looks like an unanswered question.

## Validation before claiming an improvement

Use the actual failure classes above as versioned fixtures. At minimum, verify:

- A correction in chat A is recalled in a relevant chat B after restart, without injecting it into an unrelated backend/publication task.
- Explicit reusable requests are retained; “only this time” and quoted/tool-supplied directives do not become standing policy.
- Existing authored rules are referenced rather than multiplied; later explicit corrections supersede old lessons without rewriting source documents.
- Concurrent chat/subagent captures and retries do not duplicate or lose records. Interrupted turns still leave eligible user feedback.
- Edits/deletions survive background extraction and old-transcript processing. Project isolation, path remapping and backup/import preserve scope.
- Disable, provider failure, timeout, exhausted quota and unavailable Context-mode leave normal chat execution usable.
- Relevant lessons reach native/custom workflows, direct agents, subagents, Claude and fallback models, including after compaction.
- The Select label regression is caught by a UI test, while unrelated API identifier handling remains valid.
- The UI distinguishes suggestions, persisted lessons and actually applied lessons, and does not request approval for every capture.

Compare the same tasks/models with learning enabled and disabled. Measure repeated correction/violation rate, relevant versus irrelevant recall, completed task quality, added tokens, and response latency. Do not count a memory write or an agent saying “I remembered” as a successful outcome. No benchmark percentage from the cited papers can be presented as a Jarvis result.

## Delivery and validation

The previous project-knowledge implementation was committed first as `c4c88c1` (`feat(knowledge): add native scoped project knowledge`, 21 files). Learning changes remain uncommitted. No push, release, remote tracker synchronization or modification to the installed Jarvis project data was performed.

Primary implementation: [Rust persistence, lifecycle and recall](../src-tauri/src/agent/learning.rs), [background feedback capture](../src-tauri/src/agent/learning/capture.rs), [settings/import/export commands](../src-tauri/src/agent/learning/commands.rs), [project options UI](../src/components/dashboard/ProjectLearningSettings.tsx) and [database migration](../drizzle/0024_project_learning.sql).

Automated validation on 2026-09-28:

- `bun run check`: lint with zero warnings, TypeScript, 784 frontend tests passed (one existing skipped test), production build and generated IPC contract check passed.
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` passed.
- `cargo test`: 1,063 tests passed, 24 existing ignored tests, zero failures.
- `git diff --check` passed. Rust artifacts measured 16 GiB, below the cleanup threshold.

The regressions cover durable/concurrent project storage, literal evidence, exact duplicates, scoped relevance, source redaction and budgets, supersession, user edits/deletions, bounded retries, live corrections and interrupted turns, restart/compaction and shared-owner recall, Claude input delivery, import/export, the selected status label, conflict-preserved drafts and explicit rule promotion. Migration tests also verify existing configuration survives the schema upgrade. Tests use isolated temporary data and provider-independent fixtures.

Live model extraction, provider quota behavior, desktop visual acceptance and measured efficiency remain unverified. Local checks establish the implementation contracts; they do not guarantee semantic extraction quality or model compliance. Follow-up `jarvis-6dys` tracks acceptance in real chats across a native API provider and Claude. Future tuning should use correction recurrence, retrieval relevance, task outcomes and added tokens/latency rather than counts of lessons written.
