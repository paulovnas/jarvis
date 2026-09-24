# Native Core

Core includes Context-mode, Ponytail, Beads, Open Design, Context7 and managed
language servers. Installation readiness and actual runtime usage are distinct:
some components perform host-owned work, while optional documentation and
navigation queries run only when needed. Context7 requires configured credentials.

## Installation

`~/.jarvis/core/manifest.json` points to validated, immutable generations of each
package. Updates stage in the same filesystem, run probes, then atomically
replace the manifest. Failed installs preserve the active generation. Old
generations stay available to conversations already running. An OS file lock
and an in-process lock serialize installations.

Versions come from stable GitHub releases. Context-mode's matching npm tarball
is checked with SHA-512; official Node and Beads/Dolt binary archives are checked
with SHA-256. Ponytail uses the release's GitHub source archive and validates its
package identity/version. Archives cannot escape their staging directory.

Node 22.23.2 (including npm and built-in SQLite/FTS5) and Bun are private to Context-mode.
The Bun release archive is checked with the GitHub SHA-256 digest. Shipping Bun
also supports Context-mode's JavaScript/TypeScript execution without depending
on a shell profile or a user's global Bun installation.
Dependency installation disables lifecycle scripts, global installation and
user npm configuration. Jarvis invokes `server.bundle.mjs` directly, avoiding
upstream host configuration/self-repair scripts. Beads includes its own Dolt.
Downloaded packages never depend on the development copies in `docs/`.

Current archive selection supports macOS/Linux x64 and arm64, and Windows x64.
Missing upstream artifacts produce a recoverable error, not a false installed
state. Offline release checks do not invalidate a working installation.

## Hooks and tools

The internal pipeline uses an accepted-result boundary (Codex's tool
registry/orchestrator) and Context-mode's session adapter. It has no
user-configurable hook registry:

1. Session start opens the private session database and restores its snapshot.
2. User prompts enter the session event store.
3. Tool preflight routes raw HTTP shell output toward Context-mode tools.
4. Normal Jarvis authorization still applies before executing a tool.
5. Post-tool hooks extract structured events. Large shell/search/MCP/Beads results
   are indexed; the model receives a short preview and a searchable source.
   The original output remains visible in the conversation history.
6. Pre-compaction writes a resume snapshot; successful compaction records its
   summary and count. Failed compaction preserves the original history.
7. Completed turns record the final response for continuity.

`~/.jarvis/context-mode/<sha256(conversation-id)>/` isolates memory and content
indexes between conversations, including conversations in the same project.
The child process receives explicit project, session and storage environment.
The upstream Claude-shaped event protocol is used with an isolated configuration
directory and an explicit project directory; no existing agent configuration is
read or written. The OMP adapter's treatment of `PI_CODING_AGENT_DIR` as a
workspace variable made it unsuitable for this isolated host.
Deleting a conversation also removes its Context-mode memory after the database
commit. Recovery finishes orphan cleanup after an interrupted deletion.

The agent receives `ctx_execute`, `ctx_execute_file`, `ctx_batch_execute`,
`ctx_index`, `ctx_search`, `ctx_fetch_and_index` and `ctx_stats`. Execution tools
require Manual approval and are absent in Plan. File/cwd arguments stay within
the project; arbitrary executable code has the same scope contract as Jarvis'
shell tool, not an operating-system sandbox. Upgrade/purge/doctor tools are not
exposed: settings owns installation and upgrades.

Hook subprocesses and MCP calls have bounded input, output and timeouts.
Cancellation terminates their process group. The tool result, original output
and bounded provider replay are flushed to the journal **before** auxiliary
capture runs. Failed memory hooks, recall or indexing produce a Core warning
and disable repeated automatic attempts of that operation for the turn. They
do not interrupt an already successful action. Cancellation and journal errors
remain real execution boundaries. Installation probes remain strict.

Automatic hooks, recall and output indexing each have a three-second deadline.
If indexing fails, the model receives a bounded head/tail preview explicitly
stating that the full result is preserved in local history but not searchable.
Structured tool errors retain their original details. The model is told to
request focused, read-only excerpts rather than rerun mutations to recover output.
If the installed Context-mode service cannot start (15-second connection
deadline), the turn uses native reads and bounded output instead. Its unavailable
tools and routing rules are omitted together, so fallback never directs the
agent to a missing tool. Package readiness and installation validation still apply.

## Automatic design preparation

Designer turns (direct and delegated) and custom agents with design resources
receive local reference preparation before inference. No provider call, network
request or project write is needed. The host reads up to three bounded identity
excerpts from DESIGN.md and common stylesheet locations, including explicit
worker scopes and configured repositories for direct agents. It ranks installed
Open Design metadata against the request and saved brief and selects up to two
relevant Markdown excerpts. Existing DESIGN.md excludes automatic selection of a
replacement design system or template. The reference payload is capped at 8,000
characters; canonical path containment and binary checks apply to both sources.

References are marked as untrusted data beneath project/user decisions. They do
not become approvals or replace a saved design brief. The model can use
design_search/design_read for missing details, without a mandatory search/read
ceremony. Content fingerprints avoid reinjecting unchanged references within an
inference loop; local excerpts are revalidated and references are replayed after
compaction. A new turn uses the current installed generation. Open Design
unavailability is reported without stopping the Designer's project work.

## Automatic language diagnostics

Successful native write, edit and transactional patch operations contribute
paths to a shared mutation batch. Results are durable before diagnostics run.
The host checks supported source files once per batch, with a maximum of four
files and five seconds. A structured handoff in the same model response first
consumes pending diagnostics: newly reported compiler errors get one recoverable
tool response so the agent can fix or explain them before handing off. Missing
servers or unfinished checks never become workflow blocks or false successes.

Diagnostic validity is tied to file content and native mutation batches. Changes
to other files, including configuration/deletions, invalidate cached validity;
identical reports are not repeatedly presented as new errors. Content is checked
again after the query, and outdated versioned publishDiagnostics are ignored.
Absent current diagnostics are reported as pending. JSON-RPC frames are read by
a dedicated task, so request deadlines cannot leave half-consumed frames.
Server errors/timeouts disable automatic retries for that language for the turn;
explicit LSP queries remain available. Shell-generated changes are not inferred
as native file mutations. These checks do not substitute for builds or tests.

The concepts follow OpenCode's post-edit feedback and OMP's deferred diagnostics
and diagnostic ledger, adapted to Jarvis's Rust loop and durable tool results.

## Durable Core usage

Agent steps carry optional typed Core receipts: component, action, applied/reused/
unavailable status, summary, sources, content fingerprint and duration where measured.
They cover memory lifecycle/recall/indexing, Ponytail prompt injection, Beads
snapshot restoration, design preparation and automatic LSP work. The journal,
generated IPC contract, incremental renderer deltas and history parser preserve
the same records. Older histories remain compatible without invented activity.

The chat presents one collapsed "Recursos do Core" summary within work history.
It aggregates host receipts separately from model-invoked tools, retaining
warnings and source references. It does not move observations/tools or inflate
the model action count. Context7 is advertised only when configured and remains
demand-driven; installing a component is never displayed as proof of usage.

## Ponytail policy

Ponytail is coding guidance, not a text compressor. The reference Pi/OMP
extension appends `getPonytailInstructions(currentMode)` before agent startup;
the installed OMP package uses version 4.9.0 and defaults to `full`. Metis'
`src/core/system-prompt.ts` supplies the architectural reference for deterministic
instruction assembly and source identity.

Jarvis reads `skills/ponytail/SKILL.md` from the active private installation and
selects Full's intensity row and example in Rust. The coding sections are
preserved. CLI activation/persistence sections are omitted because Core owns
activation; no upstream hooks, JavaScript, global configuration or commands are
executed to load this policy. The package name, version, frontmatter, required
sections, bounded size and canonical paths are validated both for installation
readiness and before an update becomes active. Existing Core installations are
compatible when their downloaded rules are intact.

The internal `before_agent` hook adds a versioned, SHA-256-identified policy to
every normal model request. A turn keeps the same rules through tool follow-ups,
retries and compaction. An update takes effect on the next turn. The policy is
included in context overhead estimates, but never in the compaction summarizer's
own instructions. Both provider backends use this shared assembly.

Precedence is explicit: Jarvis execution constraints, project instructions,
enabled skills and exact user requirements override Ponytail preferences. Full
cannot remove requested scope, relax Plan/Manual restrictions, skip required
checks, replace implementation with a chat snippet, or constrain explicitly
requested explanations. It applies only to coding and technical design. User
messages, tool schemas/arguments, outputs, code and citations remain untouched;
Context-mode retains ownership of output indexing and memory. No automatic
token-saving percentage is claimed: adding a policy increases prompt overhead,
and any reduction in generated code depends on the task and model.

## Beads project tasks

Jarvis invokes the private `bd` binary with structured arguments. Beads 1.2.2
uses embedded Dolt; normal task operations need no persistent SQL server or
listening port. The store is initialized lazily on the first mutation at
`~/.jarvis/beads/projects/<project-id>/store/.beads/embeddeddolt`. Initialization
uses a temporary sibling directory, validates the generated metadata and then
publishes it atomically. Broken or redirected stores fail closed and retain data.

Every child starts with a cleared environment, a private HOME and explicit
BEADS_DIR, embedded mode, noninteractive settings and disabled metrics. User
Git configuration, external tracker routing and credentials are not inherited.
Initialization skips agent instructions and repository hooks. Existing checkout
`.beads` directories are separate: Jarvis neither imports nor modifies them.
Task IDs contain the full project identity. Source conversation and operation
metadata are recorded automatically; repeated creates with the same provider
call ID return the existing task rather than creating a duplicate.

The shared provider loop exposes `beads_list`, `beads_ready`, `beads_show`,
`beads_create`, `beads_update`, `beads_claim`, `beads_close` and
`beads_dependency`. Read tools are available in Plan. Build mutations request
authorization in Manual and run automatically in YOLO. Strict schemas and
project ID checks run before any process starts.
Optional fields accept omission or null, and the Responses wire definition
explicitly disables implicit strict normalization to avoid fabricated filters.
Claims use Beads' atomic
claim operation; close never forces unresolved dependencies or gates. There
are no tracker deletion, import, shell or remote-sync tools.

An OS file lock serializes all conversations accessing a project. Cancellation
terminates and reaps the process group before releasing that lock; CLI output
and execution time are bounded. Active task snapshots are included as untrusted
runtime context at turn start and refreshed after compaction, with their size
included in context estimates. Full results use the existing chat tool history
and Context-mode indexing. Summarization instructions remain independent.
Explicit compaction selects the latest safe history boundary, rather than
retaining the automatic tail budget; the latest user request and complete tool
call pairs remain protected. This also allows short tool histories to shrink.

Conversation deletion retains the shared project tasks. Project deletion removes
only the private tracker after the library transaction commits. Recovery retries
orphan cleanup after interruption or a busy tracker. The process rechecks the
live library after acquiring its project lock, preventing a concurrent deleted
project from being initialized again. Lock files remain outside stores so their
identity does not change during cleanup.

The opt-in `installed_beads_lifecycle_smoke` uses the installed binary with a
temporary home to exercise epic/subtask creation, retry stability, dependency
blocking, ready queries, atomic claim, notes, closing, conversation resume,
project isolation and cleanup. Unit tests cover strict schemas, mode permissions,
invalid paths, lazy reads, cancellation and deletion recovery. A frontend test
verifies that the existing collapsed tool card reveals the saved task on demand.

## Verification

The 2026-09-24 runtime integration passes the complete frontend check (665 tests,
one skipped), formatting, Clippy with warnings denied, and the Rust suite (798
tests, 19 opt-in tests ignored). The installed Context-mode isolation smoke also
passes separately against 1.0.169 using temporary storage and synthetic data.
New regressions cover durable accepted results before auxiliary failure,
structured errors, cancellation, unavailable service routing, design identity/
reference refresh, bounded/stale LSP diagnostics, live receipts and legacy history.
Language-server protocol fixtures run on Unix; this change has not been exercised
through real provider conversations or a manual native UI session. Historical
native smoke observations below belong to earlier Core deliveries.

The opt-in Rust `official_installation_smoke` downloads releases into a temporary
home, checks Node FTS5, discovers Context-mode tools, indexes/searches synthetic
content, executes a synthetic calculation, and verifies output indexing and
session snapshot restoration. Unit tests cover readiness,
versions, unsafe paths, integrity failure, mode policy and conversation
isolation. Frontend tests cover setup gating, partial installation, updates and
recoverable failure. Native UI validation is separate from these checks.

Ponytail tests additionally verify rule/package corruption, path escape,
repair gating, exact host-instruction preservation and per-turn update
isolation. The opt-in `installed_rules_match_upstream_full_coding_guidance`
compares all substantive sections against the private package's official
JavaScript builder. On 4.9.0, the upstream Full instructions measured 5,252
bytes and the native policy plus Jarvis precedence measured 5,969 bytes.

The macOS debug build was also exercised through the native interface: the
initial gate installed all three packages, unlocked the workspace, displayed
their versions in General/Core, and recognized them after quitting and
reopening. A disposable conversation used `ctx_index`, `ctx_search` and
`ctx_stats` against synthetic content, then recovered the same marker with
`ctx_search` after manual compaction. No project source files were accessed or
modified by that test. Native validation does not replace user acceptance or
cross-platform testing.

The Ponytail macOS smoke reopened the existing installation without a repair
step. OpenAI Codex identified the active 4.9.0 Full policy, used native `Set`
for a synthetic coding example, and honored an explicit four-sentence
explanation. After manual compaction, Antigravity/Gemini called `ctx_search`,
recovered the previously indexed marker and identified the same Ponytail
policy. The original model selection was restored; no project files were
read or modified during these requests.

The Beads macOS smoke used the signed debug app and synthetic task data only.
Codex completed ten task operations, including epic/subtask creation, dependency,
claim, notes and closure. Manual denial left the requested title unchanged.
Explicit compaction reduced the measured/estimated history from approximately
14,644 to 1,119 tokens; after quitting and reopening, queries recovered the
original task title, closed predecessor, dependency and exact saved marker.
Antigravity/Gemini then queried the same project in Plan with only the three
read tools available. Synthetic tasks were closed at the end of the smoke.
User acceptance is separate from this technical validation.

## Project Dashboard

Selecting a project without a conversation opens its Dashboard. SQLite retains
that selection and the latest session activity; existing journals receive a
one-time mtime backfill. Actual turn starts and completions update activity,
while selection and renaming do not. The sidebar displays three recent sessions
and expands in batches of ten.

The overview reads a cached, readonly projection of journal checkpoints. It
deduplicates turns and reports provider token usage, models, tool calls, duration,
compactions and daily UTC activity. Unreadable sessions are marked as partial
coverage. Reading metrics never repairs a truncated tail or interrupts a running
turn. Cache keys include journal modification time and length.

The Beads board reads every status with `list --status=all --limit=0` against the
private project database. It does not initialize storage or read checkout Beads.
An explicit output limit returns an error instead of a partial board. Details
include parents, dependencies, dependents and comments. Only `add_bead_comment`
writes, with bounded literal text, explicit project validation and the same OS
lock used by the agent and project deletion. Beads 1.2.2 comment IDs are strings;
legacy numeric IDs are normalized at the Rust boundary.

The installed-binary Dashboard test verifies all seven statuses, epic relations,
comment persistence, unchanged issue fields and isolation between projects.

The signed macOS Dashboard smoke verified real indicators, the three-session
initial list and expansion, type filtering by keyboard, epic/subtask details,
comment submission and comment recovery after restarting. Both maximized and
restored windows were inspected. Dashboard selection and chat panel proportions
survived navigation/restart. Project source files were not changed by the smoke.
Final gates: 175 frontend tests, 182 Rust tests, lint, typecheck, frontend build
and Clippy with warnings denied. The opt-in installed-Beads Dashboard smoke
passed separately; user acceptance and other platforms remain separate.
