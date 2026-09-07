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
- Use the supplied PNG identity: `logo_icon.png` for compact marks, `logo_horizontal.png` in the title bar, and `logo_vertical.png` in About. The wordmark variants already include the Jarvis name. Generate native application icons from `public/logo_icon_background.png` with `bunx tauri icon public/logo_icon_background.png`. Avoid decorative gradients elsewhere.

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

### Project Dashboard

The Dashboard extends the industrial instrument language into a project control
surface. Its primary material is real data, not imagery: a compact metric rail,
30-day activity plot, model ranking, Core usage and recent conversations. No hero
copy, decorative gradients, synthetic savings or sample metrics.

Reference read: Linear Board Layout and Insights (linear.app/docs/board-layout,
linear.app/docs/insights), plus shadcn Area Charts (ui.shadcn.com/charts/area).
Adopt status lanes, sparse card metadata and progressive detail disclosure. Keep
Jarvis typography, inset highlights, semantic One Dark accents and 6–8px corners.
The existing Metis desktop Sidebar/Inspector informed selection and detail
navigation, without copying its light theme or implementation.

Density is high, expressiveness restrained, motion low. The board uses horizontal
lanes with independently scrollable task lists; keyboard-accessible cards open a
right Sheet. Only comments are editable. Search, type filters and optional empty
lanes support large trackers. Loading mirrors the destination geometry. Missing
data is shown explicitly instead of converted into a misleading zero.

Dashboard replaces the chat and its inspector for the selected project, preserving
the chat's saved panel proportions. Project selection already persists in SQLite.

Chat header edge controls collapse each sidebar, with short sliding motion and
reduced-motion support. Desktop preferences store collapsed flags separately from
expanded proportions. Settings use a centered medium dialog, sized to content up
to 85dvh. Its tab navigation stays fixed above the scrolling content, with a blue
surface, border and label marking the active tab. The status bar owns the settings
action. Inspector plans summarize unfinished project Beads epics and their child
tasks, with a compact dialog linking directly to the project Kanban. Its file list
shows only session-owned changes that remain uncommitted, refreshed from the
working tree after edits and external commits.
The global status bar uses a quiet local HH:mm clock. Completed compactions appear
as compact timeline separators with time and estimated token reduction, persisted
atomically with their replay checkpoint. The model menu follows provider alias,
model, then only the reasoning levels reported by that model.

Review the native application at default, narrow and wide sizes: sidebar truncation,
composer controls, expanded tool rows, questions, settings cards and diff viewer. Verify
real resize/restart restoration and maximization separately from the user's acceptance test.
