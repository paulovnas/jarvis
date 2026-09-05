# Jarvis — Precision Industrial

Jarvis is a precise developer instrument: quiet graphite surfaces, machined edges,
compact controls and a generous central reading area. Its signature is the contrast
between restrained chrome and the live rainbow perimeter of the composer.

## References and translation

- User-supplied Cyber-Deck reference: layered graphite, dense navigation, technical
  tool slots and a segmented inspector. Keep real Jarvis functionality and Portuguese labels.
- `docs/metis/desktop`: persistent project selection, project tree, center transcript
  and resizable inspector. Translate its hierarchy; do not copy its implementation.
- Existing Jarvis shadcn/Base UI components: preserve accessible keyboard interaction,
  native dialogs, skills, queues and the compact question workflow.
- frontend-art-direction and ui-ux-pro-max: intentional silhouette, strong hierarchy,
  readable contrast and reduced motion. The supplied UI/UX skill has no local search dataset.

## Material, type and shape

- Canvas `#181b20`; inset navigation `#14171b`; raised surfaces `#20242b`;
  secondary controls `#292e37`. Text `#d7dce5`, secondary text `#969eac`.
- Neutral borders use white at 9%; raised surfaces carry a 1px inset top highlight.
  Accent colors remain semantic: blue for actions, cyan for workspace, green for success,
  amber for attention, red for destructive actions, violet for skills/reasoning.
- Roboto remains the interface and prose face. Locally bundled JetBrains Mono is used
  for paths, models, tokens, durations and numeric counters, with tabular figures.
- Micro-labels: 10px, semibold, uppercase, tracking 0.12em. Body: 13–14px; prose: 14px,
  comfortable line height. Uppercase is for navigation labels, never long text.
- Panels/cards: 6–8px corners. Menus and dialogs: 8px. Composer: 22px capsule.
  Tool rows resemble slots: fine perimeter, inset edge, small semantic icon and mono detail.
- Use the geometric cut-corner J mark at small sizes. Avoid decorative gradients elsewhere.

## Interaction and persistence

The composer keeps its 5s conic rainbow animation while running. Context compaction
keeps its matching rainbow meter. Other motion is limited to short hover/focus and
disclosure transitions; reduced-motion preferences stop continuous animation.

Skeletons mirror the actual panel geometry. Active selection uses a restrained blue
wash and a fine leading indicator. Hover and keyboard focus remain distinct.

`~/.jarvis/desktop.json` stores versioned native normal bounds, maximization/fullscreen,
panel proportions, project expansion, inspector disclosure and selected settings/inspector
tabs. Native geometry is debounced and flushed on close/exit; layout writes are ordered.
Writes use a same-directory atomic rename. Removed monitors are handled by fitting the
window into an available work area. Unreadable/newer files are preserved. Transient dialogs,
authorization and execution state are not desktop preferences.

## Verification

Review the native application at default, narrow and wide sizes: sidebar truncation,
composer controls, expanded tool rows, questions, settings cards and diff viewer. Verify
real resize/restart restoration and maximization separately from the user's acceptance test.
