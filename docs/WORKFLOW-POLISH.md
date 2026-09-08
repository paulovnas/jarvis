# Workflow canvas and identity

Delivery tracked in Beads `jarvis-ulp` (2026-09-08).

## Changes

- The canvas retains React Flow measurements, selection and drag state through node updates, using `applyNodeChanges`. Previously, each position update rebuilt nodes without measurements and made them temporarily hidden while the library measured them again. Agent content remains memoized while moving. Positions still reach the editable flow immediately and survive subsequent field edits.
- The Inspector's **Subagentes** section now includes the selected custom workflow. Its cards keep every custom step, including repeated executions of the same agent, and open the corresponding read-only transcript. Manual acceptance remains specific to the existing Planned and Complete workflows.
- Custom agents and flows have **Identidade visual** controls: seven semantic colors and sixteen predefined Lucide icons. Choices apply to settings cards, canvas nodes, the composer flow menu and running-agent cards. The four Jarvis workflows and built-in agent definitions remain protected.
- Appearance is optional in existing catalogs and frozen run definitions. Old data retains a compatible default. Rust and TypeScript accept only registered icons and colors; no remote SVG, arbitrary CSS or executable content is accepted. Saving and duplicating definitions preserve appearance.
- Runtime snapshots include a compact identity from the frozen run definition. Editing or deleting a catalog item later does not change the identity of an earlier execution, and the Inspector does not need to load private agent instructions just to draw a card.

The required Metis reference (`docs/metis/docs/agents.md`, `src/core/agent-definition.ts`, session listing and subagent footer) informed the separation between configurable definitions and runtime sessions. Its agent catalog does not provide a comparable graphical canvas. React Flow's documented controlled-node update pattern informed the drag fix: [applyNodeChanges](https://reactflow.dev/api-reference/utils/apply-node-changes).

## Verification

- `CI=1 bun run check`: lint, strict type checks, 421 frontend tests and production build passed; three existing skipped tests remain.
- `cargo clippy --all-targets --features browser-probe -- -D warnings`: passed without diagnostics.
- `cargo test -- --test-threads=1 --quiet`: 404 passed; sixteen existing ignored tests remain.
- The Windows linker still prints the existing localized library-creation informational warning tracked in `jarvis-9au`; this is not a new Clippy warning.
- New and extended tests cover continuous node movement without hiding, retained position after label changes, custom Inspector visibility, icon/color selection and save payloads, composer identity, custom transcript selection, persisted appearance, legacy catalogs, rejected unknown appearance values and frozen runtime identity.
- A disposable page rendered the actual custom-flow editor. During a pointer drag, its frame observer recorded **16 dragging frames, 8 distinct positions and 0 hidden frames**. At 1280 × 720 the dialog and save button remained within the viewport, with all seven color choices aligned on one row. This uses fixture definitions; no user's flow or agent was modified during validation.

## Manual acceptance

1. Open **Configurações → Workflow → Agentes** and edit a custom agent. Choose a color and icon, save, then reopen it. Verify that the selection persists and the card uses it.
2. Edit a custom flow, choose its identity and move a node continuously around the canvas. It should remain visible throughout the drag. Save and reopen to confirm its position.
3. Select the custom flow in the chat composer. Its chosen icon and color should appear in the menu and selection button.
4. Run that flow and open **Inspector → Subagentes**. Each executed step should show the custom agent's name and identity, current status and model. Clicking it should open that step's transcript.
5. Verify that built-in Jarvis cards still offer details rather than definition or appearance editing.

No commit, push or remote Beads sync was performed for this delivery.

## Windows build

`bun run tauri build --ci --bundles nsis --no-sign` completed. The updated executable was reopened and its Jarvis window was confirmed responsive. The installer was generated without running it over the user's installation.

- Executable: `src-tauri/target/release/jarvis.exe`.
- Executable SHA-256: `1E6227FF2CEFA151BCF1486C6862EDC4EBD9019C5AEA1F897EF0383D32BF9DC1`.
- Installer: `src-tauri/target/release/bundle/nsis/Jarvis_0.8.5-beta_x64-setup.exe`.
- Installer SHA-256: `F8B577336D89BB54162CA60DEE931B9A75570C0822557E1B6B093CD255EB7018`.
