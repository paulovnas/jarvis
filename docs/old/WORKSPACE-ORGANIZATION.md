# Workspace organization and agent permissions

Workspaces are groups of registered projects. Moving a project updates its workspace
and current selection in one SQLite transaction. Project IDs, source paths, conversation
IDs and immutable journal headers do not change. No source directory is moved.

The Workspaces settings page measures local journals, recovery journals, attachments,
workflow artifacts and Context-mode stores, including worker memory. Source checkout
files are excluded. Files are measured using metadata in the Rust backend, without
reading histories into React. Symbolic links are not traversed. Each project history
directory is enumerated once per measurement.

Deleting a workspace removes its Jarvis registrations and conversation histories.
Project directories remain intact and may be registered again. Active chats and
compactions block deletion. The existing registry lock also protects against new
turns starting during removal. All affected journals are staged before a single
metadata commit; interrupted deletion uses SQLite as the recovery authority.
Conversation-owned terminals, browser tabs and memory are cleaned using the same
path as individual project deletion. Other workspaces retain their data.

## Ordering

Projects and chats support pointer dragging and keyboard sorting (Space, arrows,
Space to commit, Escape to cancel). Auxiliary file and browser tabs share one sortable
sequence; Chat remains outside that sequence. Native file drag/drop is unaffected:
sorting uses pointer events rather than the platform file-drop API.

Explicit orders live in `~/.jarvis/desktop.json` under `itemOrder`, scoped by workspace,
project or conversation. Missing IDs are ignored, and new items follow the saved
items in their default order. Until manually sorted, chats retain activity order.
Empty workspaces retain navigation and access to settings.

## Custom-agent permissions

Agent definitions persist an optional `deniedTools` list. Existing catalogs default
to an empty list. The available tools still intersect with the agent's project access,
configured services and runtime mode. An individual permission cannot elevate a
read-only agent into a writer or command executor.

The Rust catalog is derived from native tool definitions plus the MCPs' last discovered
tools; it does not launch MCPs or call inference providers while opening settings.
MCP entries use the exact deterministic wire name used during execution. `mcp_*`
disables all MCP calls; individual entries can also be disabled.

Both provider tool catalogs and native dispatch check the same policy. Core memory
retrieval/indexing/statistics and the workflow completion handoff are mandatory and
cannot be disabled. Open Design resources are available to custom agents when allowed.
Tool permissions are routing controls, not an OS sandbox: a permitted command still
has the existing shell capabilities. The configured access level remains authoritative.

## Terminal completion

Ctrl+C is sent to the PTY. An interactive shell remains usable when its foreground
command exits. A service terminal ends with its command; intentional interrupts are
reported as ended rather than failed. Completion is published before draining the
PTY so inherited output pipes cannot leave the tab indefinitely marked as running.
Trailing output still streams. Ended terminals keep their logs and offer a new shell;
the previous service command is never replayed automatically.
