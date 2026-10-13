# Native macOS redesign

## Direction contract

THESIS: A calm desktop workspace with persistent navigation and inset, divided preference rows. Replace the previous two-column industrial settings arrangement.

OWN-WORLD: Neutral charcoal surfaces, system typography, quiet gray selection and native window affordances. Reserve color for actions, identities and meaningful state.

STORY: People move between projects and conversations, inspect active work, and configure connected providers without losing context or familiar controls.

FIRST VIEWPORT: Settings opens with its sidebar, a restrained page title and single-column Sistema rows. The main window keeps project navigation, chat and inspector visible at their existing resizable widths.

FORM: Operate, code-first. The user pinned the first supplied native macOS screenshot as primary; the third is supplementary. Use 13–14px interface text, 10–12px groups, flat separators, and minimal offset shadows. Running work has a quiet status indicator; remove the rainbow border and ambient glow.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance

## Implementation contract

Shared semantic tokens own appearance. Dark background #242526, sidebar #282a2b, card #2b2d2e, secondary #343637, popover #303233, foreground #ededed, muted foreground #b0b3b7, active surface #404345, borders white at 9%. Filled primary #0071e3 with white text; blue foreground accents use a separate readable semantic token. System font first, installed Roboto fallback, JetBrains Mono for code and metadata. The light fallback remains readable and neutral.

Root owns src/index.css, scrollbars, composer/chat styling, shared secondary windows and cross-section loading consistency. Settings changes live under src/components/settings; main chrome changes live under src/components/layout. Registry components remain untouched. Preserve the existing event wiring and all real options.

## References

- Primary: /var/folders/zg/_kysl8ss5hsghcc2myr8tf9w0000gn/T/codex-clipboard-0cee077a-0ca3-4d2d-91bd-ead2241ce978.png
- Supplementary: /var/folders/zg/_kysl8ss5hsghcc2myr8tf9w0000gn/T/codex-clipboard-aaf5d6b3-16b0-45ac-b4b7-4a8d0a60dba2.png
- Apple navigation and settings guidance: https://developer.apple.com/design/human-interface-guidelines/sidebars and https://developer.apple.com/design/human-interface-guidelines/settings
- OpenCode settings-v2 row organization; OMP settings-list; Codex settings_popups. Presentation stays separate from existing behavior and persistence.
