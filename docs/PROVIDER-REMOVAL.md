# Provider removal and model references

Tracked by Beads `jarvis-2rj`.

## User behavior

Settings > Provedores > provider details > Desconectar now opens a dependency
review. Each card shows its current provider/model and an optional replacement.
The dialog lists tool settings, built-in agent model assignments, custom agents
with their consuming flows, and model choices reused by conversations and their
queued messages. Source and destination stay separate, with a visible warning
and a count of items that still have no replacement.

Only enabled, compatible destination providers and models are offered. The
provider being removed is excluded. Users can replace all, some or none of the
references. Canceling does not change the provider or any model assignment.

Unmapped references remain identifiable after removal. Sonner reports the
affected configurations, and their settings show an inline error. Chats retain
the unavailable choice and the draft; they do not silently switch to another
account. Invalid models in the selected workflow prevent sending. Provider
failures reported by execution also produce a Sonner error.

An explicit later model selection supersedes an old replacement, including
when a provider alias has been recreated. Replacement bindings apply to future
use. Existing execution snapshots are not rewritten.

## Persistence and failure handling

- Native inventory reads model metadata from the settings and session journals.
  It does not rewrite historical messages or built-in agent/flow definitions.
- Migration 17 preserves tool references as soft references and adds guarded
  provider/model bindings. Invalid references can therefore be explained instead
  of being silently erased by a foreign-key action.
- A review revision covers the current provider and its dependencies. Removal
  checks the revision again under the database transaction. Changed dependencies
  require a new review; destination identity and custom model configuration are
  checked again before committing.
- Selected replacements and provider removal share one SQLite transaction.
  Credential cleanup happens only after preparing the database changes. A SQL
  failure preserves the credential; a credential-removal failure rolls back the
  database changes. A commit failure attempts to restore the credential backup.
- Missing credentials are distinguished from inaccessible credentials on both
  Windows and macOS. Cleanup errors identify the failed stage and retain the
  review so the user can retry.
- Configuration and binding events refresh the open chat and settings. A late
  inventory response is not combined with a newer provider list.

The exact original macOS error was unavailable. An isolated replica of the
Windows schema allowed removal with linked tool settings, so this change does
not claim that a foreign-key restriction caused that original failure. macOS
native acceptance remains pending on macOS hardware.
That follow-up is tracked separately as `jarvis-6i5`.

## Verification

The frontend quality gates passed in order: lint (zero warnings), typecheck,
Vitest and production build. Vitest: 410 passed, 3 pre-existing skipped, 81 files.
Clippy passed with `-D warnings`.

The final native run used `cargo test -- --test-threads=1 --quiet`: 397 passed,
16 pre-existing ignored, zero failures. Concurrency exposed time-sensitive MCP
and terminal fixtures, so the final suite ran without competing subprocesses.
The Windows linker still reports its previously tracked localized informational
library-creation notice (`jarvis-9au`); this is not a Clippy diagnostic.

Regression coverage includes dependency discovery, optional and partial
replacements, stale reviews, duplicate submissions, compatible destinations,
credential/database rollback, repeated provider removal, preservation of
history and immutable definitions, missing-reference Sonner errors, and manual
selection after recreating an alias.

The real dialog was visually inspected at 1280x720 and 380x600 using simulated
IPC data. Provider selection worked; the warning, source/destination cards and
footer remained accessible without horizontal overflow. Native persistence was
tested separately using isolated databases and synthetic session journals.
No real provider was deleted and no paid inference was used during validation.

## Windows build (2026-09-08)

This records the provider-removal delivery. The later [browser delivery](BROWSER.md) supersedes these local artifacts and records the latest validation.

`bun run tauri build --ci --bundles nsis --no-sign` completed. The unsigned
installer is `src-tauri/target/release/bundle/nsis/Jarvis_0.8.5-beta_x64-setup.exe`.
The executable is `src-tauri/target/release/jarvis.exe`.
The updated Jarvis window was opened and confirmed responsive. A read-only
database check confirmed schema version 17, the same two provider records and
zero foreign-key violations; no replacement bindings were created in user data.

- Executable SHA-256: `97C09E8FE317755564B01B1097A0EA1D2C403436EF62A17FAD6AC0B03FB6EE7E`.
- Installer SHA-256: `1574477D5149AF3191039E6B6D5551ED0EBC746BE31A3D9AA229D26D665AB20F`.

## Manual acceptance on macOS

1. Configure a disposable provider on a tool, an agent and a chat. Open its
   removal dialog and confirm that the linked items and flow context appear.
2. Cancel once and verify that all choices remain. Reopen and assign another
   provider/model to some items; leave another without a destination.
3. Remove the disposable provider. Verify the replacements, the Sonner warning
   for the unresolved item, and the preserved chat history. Select a valid model
   for the unresolved item and confirm that it can be used again.
4. If Keychain access fails, confirm that the error names credential removal and
   that the provider and its dependencies remain. Retry after restoring access.

Changes remain uncommitted in the working tree. The earlier authorized baseline
commit and push (`776650e`) are preserved.
