# Workspace, project and conversation foundation

Durable tracking: `jarvis-549` and its child tasks. This document records design and scope; execution status belongs in Beads.

## Outcome

Replace the sidebar's demo objects with real Workspace > Project > Conversation records. A workspace is only a named group. Adding a project opens the native directory picker and stores the canonical directory path. Creating a conversation creates a durable session immediately. Selected context survives restart and drives the center panel.

## Reference decisions

- Metis `src/core/session-manager.ts`, `docs/sessions.md` and `docs/session-format.md`: a session has a stable ID, versioned JSONL header and working directory; histories are kept in a managed application directory grouped by project/cwd.
- Metis desktop `src/components/sidebar/Sidebar.tsx`: selection and creation callbacks belong to the parent application state; the sidebar renders project/session identity.
- OMP `packages/coding-agent/src/session/session-manager.ts`: persist the new session header before returning so an empty new conversation remains distinct after restart.
- Port concepts only. Reference trees remain read-only.

## Storage and invariants

- SQLite remains in `~/.jarvis/jarvis.db`. A new immutable Drizzle migration adds workspaces, projects, conversations and one navigation-selection row. Existing onboarding/provider data is preserved.
- Workspaces contain only ID, name and creation time; they have no working directory, provider settings or configuration inheritance.
- Each project belongs to one workspace and references an existing canonical directory. Canonical-path uniqueness prevents registering the same folder (including symlink aliases) twice. The folder name supplies the initial project name. Adding a project never modifies its directory or requires Git.
- Each conversation belongs to one project. Its session ID is generated in Rust and also identifies `~/.jarvis/sessions/<project-id>/<conversation-id>.jsonl`. Names never become filesystem paths.
- The first line is a versioned session header with ID, project ID, initial title, creation time and canonical cwd. SQLite indexes sidebar metadata; JSONL is the session-history boundary for subsequent agent work.
- Create the file exclusively and sync it before committing the conversation index/navigation transaction. Normal failures remove only the newly created file and roll back SQLite. A process crash before DB commit may leave an unindexed header; never delete such files automatically or overwrite them.
- Reads validate session identity/version against the requested project. Missing or corrupt files produce a recoverable error without resetting or replacing history. Validate generated IDs and reject symlinked session paths.
- Navigation is selected by typed IDs, validated against parent relationships. Creating an item selects it; selecting a workspace/project clears descendant selection. All mutations and snapshots use the existing Rust database mutex and parameterized SQL.

## UI and scope

- Preserve the three-panel shell, One Dark theme, Roboto and existing shadcn primitives. Workspaces/projects/conversations start empty, with actionable empty states.
- Named workspace and conversation dialogs validate input; project creation uses the native folder picker. Cancel creates nothing. Duplicate/invalid input and storage failures remain visible and retryable. Pending operations prevent duplicate submission.
- The sidebar shows projects in the selected workspace and conversations in the selected project. The Conversations tab remains scoped to that project.
- The selected conversation controls the center panel's title, project path and empty session state. Remove sample messages, fake completion toasts and simulated tool execution from this real flow. Retain model selection; message execution remains unavailable until the agent loop is implemented.
- Inspector context reflects the selected workspace/project/conversation; sample file/plan/subagent results must not be presented as real session data.
- No agent execution, message append API, search, rename, move, delete, archive, worktrees or workspace-level configuration in this slice.

## Naming and scoped-menu extension (`jarvis-tyx`)

Creation now assigns `Nova Conversa` in Rust without a dialog. Projects and conversations offer an explicit shadcn context menu with **Editar**; the native WebView context menu is suppressed, including in portals. The edit target is independent of the current navigation selection. Project names are editable while paths/IDs remain unchanged. The current conversation header and inspector reflect returned metadata immediately.

SQLite stores mutable `display_title` and `title_source` separately from the immutable initial `title` validated against JSONL. Editing metadata does not rewrite history or require file/index compensation. Existing titles migrate as manual; new titles start as default. This adapts Metis's distinction between initial session identity and current display name, and OMP's title-source distinction. The first real agent exchange will supply automatic concise naming (`jarvis-u3i`), preserving manual overrides. Vite scans `index.html` only and excludes reference directories from watching.

## Validation

Rust tests cover migration preservation, empty bootstrap, name/path validation, canonical-path duplicates, parent isolation, durable creation/reopen, file/index rollback, concurrent creation and missing/corrupt session failures. React tests exercise creation, cancellation, empty/error/loading states, selection across workspaces/projects, restored state and center-panel context without fabricated results. Run lint, typecheck, tests and build in order, then Clippy with warnings denied and Rust tests. Record native picker/restart proof separately from automated checks. No commit or push without user authorization.
