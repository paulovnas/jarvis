# On-demand project knowledge: research and recommendation

Research date: 2026-09-28. Baseline: Jarvis 1.7.2 (`d317055`).

Research tracking: `jarvis-ckr8` (closed). The user approved implementation under `jarvis-jns1`; the delivered contract is recorded below. Research findings are distinct from runtime validation.

## Recommendation

Add a small, editable project knowledge library with four categories: product, technical architecture, rules, and design. Combine a compact project index and essential applicable rules with section-level retrieval on demand. Keep documents readable and portable; do not inject all four documents into every model request.

The expected benefit is less repeated discovery and fewer unsupported project decisions. Lower token use or shorter task duration are hypotheses to measure, not guaranteed consequences of adding documentation. This feature does not repair provider failures, stalled tool calls, or agent orchestration by itself.

The exact `prd.md` / `trd.md` / `rules.md` / `design.md` quartet is a useful organizational choice, not a universal agent interoperability standard.

## Research evidence

| Source | Observed behavior or finding | Implication for Jarvis |
| --- | --- | --- |
| [Kiro steering](https://kiro.dev/docs/steering/) | Foundation documents cover product purpose, technology, and project structure. Steering supports always-included, file-matched, and manual inclusion. | The proposed product experience has a close existing analogue. Different information needs different inclusion policies. |
| [OpenAI skills documentation](https://learn.chatgpt.com/docs/build-skills) | Skill metadata is available for discovery; full instructions load when selected. References are separate resources. | Reuse progressive disclosure as a context-loading principle; project facts need not become executable skills. |
| [Anthropic context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) | Recommends small, relevant context and just-in-time retrieval. Explains that runtime exploration has latency costs and that hybrid approaches can be preferable. | A compact initial index plus targeted retrieval is more appropriate than either loading everything or making every fact require another model round trip. |
| [Claude Code memory documentation](https://code.claude.com/docs/en/memory) | Distinguishes maintained project instructions from accumulated memory and supports rules scoped to file types. Instructions are context, not deterministic enforcement. | Separate durable project decisions from conversational recollections. Existing runtime controls still enforce constraints. |
| [Evaluating AGENTS.md](https://arxiv.org/abs/2602.11988) | The abstract reports no general improvement in task success and over 20% higher average inference cost in its evaluated settings. It finds value in non-standard practices and warns that repository overviews are not automatically helpful. | Do not equate more generated documentation with better agents. Measure this proposal against a baseline. These results are not a measurement of Jarvis or section-based retrieval. |

The primary pages above were retrieved during this research. The paper's claims here are limited to its abstract; this is not a reproduction of its experiments.

## Relevant local implementations

The reference repositories were inspected read-only. Concepts should be adapted rather than copied.

- Codex: `docs/codex/codex-rs/core/src/agents_md.rs:55` loads instructions across environments with a shared byte budget. `agents_md_tests.rs:663` and `:679` cover per-document and combined-size limits. `agents_md_manager.rs:65` manages refresh and cached loaded instructions. This is bounded instruction loading, not evidence of a native four-document knowledge library.
- OpenCode: `docs/opencode/packages/opencode/src/session/instruction.ts:179` attaches applicable nearby instruction files and avoids reattaching documents already present. `test/session/instruction.test.ts:160` exercises duplicate prevention.
- OMP: `docs/omp/packages/coding-agent/src/system-prompt.ts:412` deduplicates exactly contained context, preserving scope precedence. `:447` loads context by scope. Approximate summaries should not silently erase distinct instructions.

Jarvis already has useful building blocks:

- `src-tauri/src/agent/tools.rs:126` includes root project instructions.
- `src-tauri/src/agent/instructions.rs:44` discovers nested `AGENTS.md` for tool targets; `:106` adds scoped instructions. Mandatory rules must not be demoted to optional knowledge search.
- `src-tauri/src/core/context.rs:37` builds a bounded recall query. `:41` places Context-mode storage under a hash of the session. This is useful execution memory, but it is not a shared, user-maintained project knowledge catalog.
- `src-tauri/src/core/design/preparation.rs:20` prepares bounded local design references. `:124` discovers `DESIGN.md` / `design.md` and related files by scope. The proposal should enrich this path rather than duplicate its prompt content.
- `src/components/dashboard/ProjectOptions.tsx:30` is the existing options surface. Repository metadata already exists through `src/core/project-repositories.ts` and `src-tauri/src/library/repositories.rs`.

No unified PRD/TRD/rules/design knowledge API was found in the inspected application source.

## Content boundaries

| Category | Maintain | Avoid |
| --- | --- | --- |
| Product | Purpose, target users, existing capabilities, business constraints, explicit scope and non-goals. Distinguish current behavior from planned behavior. | Inferring stakeholder intent from code as if confirmed; copying the complete backlog into the PRD. |
| Technical | Repository responsibilities, entry points, stack, integrations, data flows, established patterns, commands, and links to decisions. | Exhaustive generated file inventories, copied implementation details, unsupported explanations of why a decision was made. |
| Rules | Explicit conventions, required libraries, error handling, restricted operations, test expectations, and their scope. | A second conflicting copy of `AGENTS.md`; treating current code conventions as immutable user policy. |
| Design | Semantic tokens, typography, layout principles, interaction/accessibility rules, reusable component locations, and representative examples. | Copying every stylesheet or treating an accidental inconsistency as an approved design principle. |

Beads remains the task and progress system. Skills remain reusable procedures. This library records project-specific facts and decisions; it does not replace either system.

## Retrieval contract

The storage format does not determine model token cost. A Markdown document can be searched by Rust without entering model context. Conversely, text returned by a native tool still consumes model context and may remain in subsequent requests. Prompt caching can reduce some processing cost, but does not make irrelevant context free.

Use three levels:

1. A compact project/repository index and essential applicable rules available at task start.
2. Relevant sections selected by explicit repository/path scope and the current task, with bounded output.
3. A native retrieval tool for additional searches or a complete selected section when the agent needs it.

Start with one small tool contract supporting search and section reading; avoid a separate overlapping tool for every document type. A result should identify category, repository scope, section, source, revision, and whether source changes may have made it outdated. Return real excerpts, not an untraceable synthesized answer.

Required rules are loaded through the harness according to scope, before the affected action. Product and architecture details remain selectively retrievable. A retrieval miss must be explicit so the agent can inspect source or ask a genuinely necessary product question; it must not invent an answer.

Native, direct, custom-canvas, and delegated execution need the same capability. Role may influence default relevance, but must not prevent a Builder from reading design guidance or a Designer from reading technical constraints. External executors such as Claude require an equivalent exposed tool or bounded context adapter; otherwise the UI would advertise a capability that some providers cannot use.

Reuse a loaded section while its revision and visibility in the active context remain valid. Do not treat filesystem caching as proof that a compacted conversation or a fresh subagent already has the content. Preserve section identifiers and reload when needed after compaction, branching, or handoff.

## Storage and scope

Prefer one canonical, readable document source with a rebuildable local retrieval index. A reasonable default for newly generated content is a project-owned directory such as `.jarvis/knowledge/`, with Markdown edited through Jarvis. Existing documents should be linked and indexed rather than silently copied into competing authoritative versions. A local database may hold index and revision metadata; it should not become a second independently edited document copy.

This is a proposed storage choice, not a requirement that every project root be a Git repository. A workspace containing separate frontend/backend repositories still needs shared product knowledge plus repository-specific technical and design sections. Reuse configured repository IDs and paths; use the project root when no repositories are configured.

Source identity, repository scope, content fingerprint, and section identifiers prevent cross-repository confusion and support invalidation. A changed source indicates that a section may need refreshing; it does not prove that the section is wrong. Track working-tree content, not only the last Git commit.

Preserve existing instruction precedence. A generated product or technical description is evidence, not an instruction to override the current user request or scoped project rules. Conflicts between sources should be visible; inferred content must not silently supersede explicit user-maintained decisions.

For a handful of documents, begin with heading-aware sections and local lexical search. A vector database, embedding service, extra provider, and per-query LLM reranking are unnecessary prerequisites. Reuse Context-mode where useful for retrieval and execution evidence, while keeping authoritative documents accessible if that component is unavailable. Add more complex ranking only if measured retrieval failures justify it.

## Generation and update experience

Project Details > Options can contain a Knowledge section with Product, Technical, Rules, and Design editors. Show source links, last generation/update, and potentially outdated sections. Offer Analyze and generate, Update, and Markdown import/export. The user can edit or link existing documents without generating anything.

Generation should be a bounded, cancelable job with progress and useful partial results:

1. Inventory configured repositories and existing README, AGENTS, design and architecture documents.
2. Read manifests, scripts, top-level structure, integrations, tokens, component registries, and representative source/tests. Skip dependency/build directories, binaries, and secret files.
3. Extract deterministic facts locally, then use the selected/configured model for a bounded synthesis. Do not impose an exhaustive source crawl or an always-required chain of agents.
4. Separate observed facts, explicit documented decisions, inferred candidates, and unanswered product questions. Reference the source paths for factual claims. Code cannot establish business intent or design rationale by itself.
5. Present an editable draft. Regeneration shows changes and preserves user-authored decisions rather than replacing them wholesale. Partial or uncertain knowledge does not block normal project work.

Analyze initially on demand and refresh affected sections when requested. Fingerprint changes can mark material as potentially outdated without automatically launching model calls after every edit. Do not regenerate the entire knowledge library on every chat or every commit.

Generation must respect the non-chat network resilience work: timeout, cancellation, recoverable provider errors, and no indefinite loading screen. UI save and file writes must preserve user edits if generation fails or completes against an older revision.

Export/import needs to carry authoritative content and source metadata when project knowledge is included. The rebuildable index need not be exported. Existing settings-only backup semantics must be made explicit rather than silently omitting knowledge or implying that a settings backup contains all project files.

## Evaluation before claiming an efficiency gain

Compare the existing harness with the proposed hybrid retrieval on the same representative tasks, models, and settings. Include a UI correction that must reuse existing components, a backend task with local error conventions, a cross-repository change, a product question absent from code, conflicting/outdated documentation, and a long task resumed after compaction.

Measure correct task completion, adherence to relevant conventions, unsupported assumptions, user corrections, wall-clock time, input/output tokens, cached input where available, redundant reads, and retrieval usefulness. Report generation and indexing cost separately and include their amortized cost across repeated tasks. Lower token use alone is not success if tasks become less correct.

No live model comparison or native UI validation was performed during the research. Efficiency improvements still require the comparative evaluation above.

## Implemented contract

Project Details → Options contains Product, Technical, Rules, and Design editors, scoped to the shared project or its configured repositories. Unsaved edits survive category/scope changes within the editor. Generation and Markdown import open an editable preview; accepting the preview updates the editor, and Save is a separate action. Reload preserves drafts. A conflicting external revision requires an explicit choice before overwriting it.

New documents live at `.jarvis/knowledge/<scope-hash>/{prd,trd,rules,design}.md`. Existing files with these names in the scope or its `docs/` directory are discovered case-insensitively. A user can explicitly link another project Markdown; saving then edits that canonical file. Linking never deletes the previous file. `.jarvis/knowledge/index.json` stores scope/path bindings, essential rules and source fingerprints, not a duplicate copy of document bodies. Copy/version the whole knowledge directory together with linked documents. Global settings backup excludes project files. Markdown import/export transfers the selected body only; it does not transfer bindings, essential-rule settings or source fingerprints.

The read-only `project_knowledge` tool is available to native/direct/custom agents and through the existing Claude MCP bridge. It searches heading-based sections locally and returns citations, revision hashes and freshness hints. Shared facts remain available when a repository path is specified; sibling repository facts are excluded. Output is bounded to six results, with `start`/`nextStart` for result pagination and `sectionId`/`offset`/`nextOffset` for reading section text. Search excerpts start near a matching term. Missing or unreadable knowledge does not block ordinary project operations.

The initial prompt contains a compact document catalog and shared essential rules. Repository essential rules are loaded before applicable native file/command operations through the existing instruction resolver; changes are deduplicated. The current user request and scoped AGENTS.md retain precedence. Full documents are not injected automatically. Linked design documents also feed the existing Open Design preparation; this introduces no additional model call.

Generation analyzes one selected category and scope at a time. It samples standard documentation, manifests and representative source with an approximately 24,000-character evidence budget, plus up to 8,000 characters of the current saved document. Shared generation considers up to eight configured repositories. Dependency/generated directories, hidden files, symlinks and unreadable/binary/oversized sources are excluded from the sample. This is bounded discovery, not an exhaustive audit or a guarantee that arbitrary source files contain no sensitive data. The UI explains that sampled local content is sent to the selected provider.

Synthesis uses the selected existing native provider or installed Claude Code. It has no tools, no execution-worker chain, a three-minute deadline and explicit cancellation. Provider errors, cancellation, empty results and invalid output leave saved Markdown unchanged. The generation contract separates facts, documented decisions, hypotheses and unknowns; it does not automatically promote inferred rules to essential instructions.

Documents are limited to 64 KiB, essential instructions to 2,000 characters, metadata to 128 documents / 32 scopes, and fingerprints to 32 source references per document. External edits invalidate optimistic revisions. Changed/missing source files indicate potential staleness rather than proving a claim wrong. Retrieval uses local lexical matching without embeddings, a new service or a vector database.

Regression coverage includes scoped retrieval, bounded search/section pagination, file reuse/linking, source changes, optimistic saves, symlink boundaries, automatic rules, native tool dispatch, model result handling, generation cancellation/deadlines, multi-repository sampling, Open Design preparation, draft preservation, import/export and publication-independent UI loading. Live provider synthesis and native desktop interaction remain separate acceptance checks; unit tests are not evidence of either.

Validation on 2026-09-28: `bun run check` passed lint, TypeScript, 780 frontend tests (one skipped), production build and generated IPC-contract verification. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` passed; Rust reported 1,050 passed and 24 ignored, with no failures. No commit, push, release, live model comparison or installed-app restart was performed.
