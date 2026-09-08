# Project Explorer and read-only file tabs

Tracked in Beads as `jarvis-w7z`. The user explicitly requested viewing files only.

## Behavior

The right chat sidebar has `Inspector` and `Explorer` sections, using the same filled selection style as the bottom terminal panel. The Explorer lists project folders before files, includes dotfiles, loads one directory at a time, and supports arrow keys, Enter, refresh, and collapsing folders.

Clicking a text file opens a central file tab. Opening it again selects the existing tab. `Chat` is always present and has no close button. Closing an inactive file leaves the selected file unchanged; closing the last file returns to Chat. The chat remains mounted while viewing files, preserving its composer and transcript. The bottom terminal panel remains available in both views.

Monaco provides syntax highlighting, line numbers, selection/copy, folding and Ctrl/Cmd+F search. The viewer is read-only at both editor and DOM levels. No save, rename, delete, or other filesystem mutation command is introduced. The file toolbar includes its relative path and refresh action; the footer displays encoding and byte count.

The selected Inspector section and up to 30 file-tab paths per project are stored with existing desktop layout preferences. File contents are not persisted. The content cache is bounded across projects; delayed reads are isolated by project and request version. Refresh explicitly rereads a file that changed on disk.

## Filesystem boundary and limits

Rust resolves a registered project ID to its canonical root, validates relative paths and reads outside the database lock using `spawn_blocking`. Absolute paths, parent traversal, Windows drive/UNC/alternate-stream paths, and links resolving outside the project are rejected. The Explorer does not follow symbolic links.

Text previews accept UTF-8 (with or without BOM) and BOM-marked UTF-16 LE/BE, preserving existing line endings. Reads are limited to 2 MiB, and directory results to 4,000 entries with a visible truncation notice. Binary files, unsupported encodings, unavailable files and permission errors have a clear error state. Images and other binary formats do not have a preview in this version.

The design follows the bounded reads, directory ordering and project-relative access patterns studied in the read-only Metis reference under `docs/metis`; no reference source was changed or copied.

## Bundling and maintenance

Monaco 0.56.0 is pinned and loaded only after a file is opened. Its worker and language definitions ship locally; no CDN or language-service server is required. JSON uses the bundled tokenizer without schema fetching, validation or formatting workers.

This Monaco version ships an empty pt-BR table. `monaco-locale.ts` fills the exposed reader/search labels before editor initialization. Its contract test checks the message indices against the installed library source, so upgrades cannot silently mislabel those controls.

The standalone reader has an explicit 3,000 kB uncompressed lazy-chunk budget. A build plugin retains the 500 kB ceiling for every other JavaScript chunk, including the initial application. This keeps Monaco's service initialization graph intact without weakening the rest of the application's bundle limits.

## Validation

Colocated frontend tests cover duplicate/active/inactive tab behavior, permanent Chat, composer and terminal continuity, restored tabs and sidebar section, directory keyboard navigation, retries and delayed cross-project reads. Native tests cover sorting, dotfiles, accented names, Windows separators, unchanged CRLF, UTF BOMs, binary/oversize rejection and escaping links/junctions. Desktop preference round-trip and validation tests include the new fields.

A temporary production browser fixture exercised the actual Home, Inspector, terminal panel, Monaco, and application CSS with isolated IPC data. It confirmed syntax highlighting for TypeScript and JSON, read-only typing, Portuguese search controls, active/inactive closure, persistence after reload, preserved chat draft and uninterrupted bottom terminal. The browser console contained no errors or warnings. The review browser and its server were closed afterward; its generated files were moved into the disposable build directory.

Native macOS acceptance remains part of `jarvis-wgi`. Windows native test linking emits the previously tracked localized MSVC informational notice (`jarvis-9au`); this is separate from Clippy diagnostics.

On 2026-09-08, the required frontend gates passed in order: lint, TypeScript, all 73 test files (384 passed, 3 skipped), and production build. Clippy passed with `-D warnings`; the sequential native suite passed 379 tests with 16 existing ignored tests. The native linker notice above remains present, so native linking is not described as warning-free.

The Windows release executable and unsigned NSIS installer were rebuilt successfully, and the application was reopened for review. Artifact hashes are recorded in section 31 of [PLAN-MIGRACAO-WINDOWS.md](PLAN-MIGRACAO-WINDOWS.md). No commit, push, installer deployment or publication was performed.
