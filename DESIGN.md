---
name: Jarvis — Native macOS
description: A calm desktop workspace with persistent navigation and inset preferences.
colors:
  background: "#242526"
  foreground: "#ededed"
  card: "#2b2d2e"
  popover: "#303233"
  primary: "#0071e3"
  primary-foreground: "#ffffff"
  secondary: "#343637"
  muted: "#2b2d2e"
  muted-foreground: "#b0b3b7"
  accent: "#393b3d"
  accent-foreground: "#ffffff"
  destructive: "#c6404b"
  border: "rgb(255 255 255 / 9%)"
  input: "rgb(255 255 255 / 12%)"
  ring: "#69b0ff"
  sidebar: "#282a2b"
  sidebar-foreground: "#d6d8da"
  sidebar-accent: "#404345"
  tone-blue: "#69b0ff"
  tone-green: "#84d18a"
  tone-cyan: "#75d5de"
  tone-yellow: "#edc46a"
  tone-purple: "#d5a6f3"
  tone-red: "#ff9096"
  light-background: "#f5f5f5"
  light-foreground: "#242526"
  light-card: "#ffffff"
  light-secondary: "#e9e9e9"
  light-muted: "#ededed"
  light-muted-foreground: "#626569"
  light-accent: "#e3e3e3"
  light-sidebar-foreground: "#343637"
  light-sidebar-accent: "#d7d9db"
  light-border: "rgb(0 0 0 / 12%)"
  light-input: "rgb(0 0 0 / 16%)"
  light-ring: "#0071e3"
  light-tone-blue: "#0066cc"
  light-tone-green: "#287b38"
  light-tone-cyan: "#087b85"
  light-tone-yellow: "#826314"
  light-tone-purple: "#8751b0"
  light-tone-red: "#b82d3a"
typography:
  headline:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', sans-serif"
    fontSize: "18px"
    fontWeight: 600
    lineHeight: "28px"
    letterSpacing: "normal"
  title:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', sans-serif"
    fontSize: "14px"
    fontWeight: 600
    letterSpacing: "normal"
  interface:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: "20px"
  body:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.5
  prose:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: "28px"
  label:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', sans-serif"
    fontSize: "12px"
    fontWeight: 500
    lineHeight: 1.5
    letterSpacing: "normal"
  caption:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.5
  technical:
    fontFamily: "'JetBrains Mono', 'SFMono-Regular', Consolas, monospace"
    fontSize: "11px"
    fontWeight: 400
  badge:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', sans-serif"
    fontSize: "10px"
    fontWeight: 500
rounded:
  sm: "6px"
  md: "8px"
  lg: "10px"
  xl: "12px"
  2xl: "16px"
  navigation: "7px"
  badge: "5px"
spacing:
  xs: "4px"
  sm: "8px"
  control-inline: "10px"
  md: "12px"
  lg: "16px"
  xl: "24px"
  section: "28px"
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.primary-foreground}"
    rounded: "{rounded.lg}"
    height: "32px"
    padding: "0 10px"
  button-secondary:
    backgroundColor: "{colors.secondary}"
    textColor: "{colors.foreground}"
    rounded: "{rounded.lg}"
    height: "32px"
    padding: "0 10px"
  button-outline:
    backgroundColor: "color-mix(in oklab, rgb(255 255 255 / 12%) 30%, transparent)"
    textColor: "{colors.foreground}"
    rounded: "{rounded.lg}"
    height: "32px"
    padding: "0 10px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.foreground}"
    rounded: "{rounded.lg}"
    height: "32px"
    padding: "0 10px"
  button-destructive:
    backgroundColor: "color-mix(in oklab, #c6404b 20%, transparent)"
    textColor: "{colors.tone-red}"
    rounded: "{rounded.lg}"
    height: "32px"
    padding: "0 10px"
  input-field:
    backgroundColor: "color-mix(in oklab, rgb(255 255 255 / 12%) 30%, transparent)"
    textColor: "{colors.foreground}"
    typography: "{typography.body}"
    rounded: "{rounded.lg}"
    height: "32px"
    padding: "4px 10px"
  navigation-settings:
    backgroundColor: "transparent"
    textColor: "{colors.sidebar-foreground}"
    typography: "{typography.body}"
    rounded: "{rounded.navigation}"
    height: "36px"
    padding: "2px 10px"
  navigation-settings-active:
    backgroundColor: "{colors.sidebar-accent}"
    textColor: "{colors.foreground}"
    rounded: "{rounded.navigation}"
  badge-secondary:
    backgroundColor: "{colors.secondary}"
    textColor: "{colors.foreground}"
    typography: "{typography.badge}"
    rounded: "{rounded.badge}"
    height: "20px"
    padding: "2px 8px"
  settings-group:
    backgroundColor: "{colors.card}"
    textColor: "{colors.foreground}"
    rounded: "{rounded.xl}"
    padding: "0"
  settings-row:
    typography: "{typography.body}"
    padding: "14px 16px"
  chat-composer:
    backgroundColor: "{colors.card}"
    textColor: "{colors.foreground}"
    rounded: "{rounded.2xl}"
---

# Design System: Jarvis

## Overview

**Creative North Star: "Native macOS workspace"**

Jarvis is a calm, compact desktop workspace. Persistent navigation, neutral charcoal
layers, inset preference groups and familiar controls keep attention on the user's
project and conversation. The system interface font carries both chrome and prose;
monospace identifies technical details. Color marks an action, identity or meaningful
state.

The first user-supplied native settings reference is the visual authority, preserved
at [.impeccable/references/native-macos-settings.png](.impeccable/references/native-macos-settings.png).
The user explicitly chose a discreet execution indicator on 2026-10-12. This system
supersedes the previous Industrial/Cyber-Deck palette, beveled panels, uppercase
micro-labels and animated rainbow composer perimeter.

**Key Characteristics:**

- Persistent navigation and soft gray selection.
- Compact system typography with technical monospace.
- Inset, divided preferences and responsive independent cards.
- Thin separators, rounded controls and restrained elevation.
- Quiet execution feedback and structural loading states.

This record describes the delivered source. [PRODUCT.md](PRODUCT.md) preserves the
product commitments; the [finish review](.impeccable/review/finish-review.md) and
[verification record](.impeccable/review/verification.md) preserve the final native
evidence and its limits. The reference image is not a shipping asset, and this
redesign introduces no shipping raster artwork. The sidecar's synthesized OKLCH
ramps are preview metadata, not additional application color tokens.

## Colors

Charcoal layers supply the structure; readable semantic accents supply meaning.
The frontmatter records the dark values from `src/index.css`. Reuse the existing
semantic variables and their Tailwind aliases rather than inserting hex values in
consumer components. The legacy `onedark-*` alias names now resolve to this system.

### Primary

- **Action blue** (`primary`): filled actions with `primary-foreground` text.
- **Readable blue** (`tone-blue`): links, highlighted text and execution dots.
  The `text-primary` utility resolves to this foreground tone; a filled primary
  control continues to use the separate action color.
- **Focus blue** (`ring`): visible keyboard focus on controls and composer.

### Secondary

- **Success green** (`tone-green`): successful or enabled state.
- **Service cyan** (`tone-cyan`): relevant workspace and service identity.
- **Attention yellow** (`tone-yellow`): warnings and attention states.
- **Skill purple** (`tone-purple`): skills, voice and related feature identity.
- **Error red** (`tone-red`): readable errors and destructive labels.
  `destructive` supplies the separate destructive fill; `text-destructive` resolves
  to the readable red tone. Do not treat a colored fill and a text accent as
  interchangeable.

### Neutral

- **Canvas** (`background`): the continuous application ground.
- **Inset navigation** (`sidebar`): persistent sidebars and window chrome.
- **Preference surface** (`card`, `muted`): inset groups, cards and composer.
- **Secondary surface** (`secondary`): controls, supporting panels and visible
  Settings skeletons.
- **Popup surface** (`popover`): menus and floating controls.
- **Hover surface** (`accent`): interactive tonal emphasis.
- **Selected navigation** (`sidebar-accent`): soft gray active rows.
- **Primary text** (`foreground`), **supporting text** (`muted-foreground`) and
  **navigation text** (`sidebar-foreground`) preserve the hierarchy without reducing
  the whole control's opacity.
- **Dividers** (`border`) and **field strokes** (`input`) use the source's translucent
  color values. Cards and navigation may use the registry's subtle outline treatment.

The `:root` light fallback is source-defined, while the native redesign evidence is
dark. The `light-*` tokens record its different canvas, text, surfaces, selection,
strokes, focus and semantic text tones. Light cards and popovers share `light-card`;
the light sidebar shares `light-muted`. Light foregrounds also supply card, popover,
secondary and accent text. Primary fills, their white text and the destructive fill
are shared between themes. The existing `--chart-*` visualization palette remains
separate from general interface text tones.

**The Fill and Foreground Rule.** Use the action fill for filled controls and the
readable semantic tone for colored text. Preserve visible keyboard focus.

## Typography

**Interface and prose:** system font first, with installed local Roboto as fallback.
The `font-heading` and `font-sans` stacks are identical. This is desktop interface
typography; there is no separate decorative display face.

**Technical text:** locally installed JetBrains Mono with the source's platform
fallbacks. Paths, model names, tokens, durations, clocks and counters use monospace
and tabular numerals where the source supplies them.

### Hierarchy

- **Page heading:** semibold system type (18px, 600, 28px line height) in Settings.
- **Section and card title:** semibold system type (14px, 600), in normal case.
- **Interface:** ordinary controls and conversation headings (14px); Settings
  labels, inputs and navigation use the compact body size (13px).
- **Supporting copy:** descriptions and status labels (12px, 1.5 line height).
- **Micro-label:** medium system type (12px, 500, normal tracking and case).
- **Assistant prose:** ordinary system type (14px, 28px line height). The composer
  editor uses a slightly larger reading size (14.5px, relaxed line height).
- **Technical metadata:** compact monospace (10–11px); status-bar time uses the
  technical role (11px). Badges default to compact medium text (10px, 500).

**The System and Detail Rule.** Use system type for readable interface and prose,
and monospace for technical metadata. Keep headings and labels in normal case.

## Layout

The native shell keeps project navigation, conversation and inspector in resizable
panels. Existing collapsed-panel flags and expanded proportions remain independent.
The navigation and inspector are detached cards with 12px corners, a thin border,
subtle card shadow and an 8px outer inset. Resize handles occupy the gaps between
cards and the conversation. The conversation keeps its open-tab row without a
second breadcrumb or title row.
The main title bar is compact (40px); the status control row is shallow (28px), with
a thin separator above it. The chat composer and transcript stay inside explicit
scroll viewports; the window root does not scroll.

Settings uses a persistent category sidebar (240px), a fixed page header and a
separately scrolling content pane. The pane fills all remaining width, placing its
scrollbar at the window's right edge. Its inner content is centered (1200px maximum)
with comfortable desktop padding (24px). Below the source's small breakpoint
(640px), the sidebar becomes an icon rail (56px) with accessible names and hints,
and inner padding reduces (16px). This is a defensive web-layout fallback: native
minimum widths are 960px for Settings and 1024px for the main window.

General preferences remain divided, single-column groups. Each row places wrapping
copy to the left and controls to the right (14px vertical, 16px horizontal padding;
12px vertical and 24px horizontal gaps). The copy can wrap from its flexible base
(240px), and controls retain their labels without overflowing.

Independent tools and catalog entries use available-content-width grids: minimum
column width (320px), gap (12px), automatic column fitting. Larger agent and workspace
configurations use the wide grid (480px minimum). These are not fixed viewport
breakpoints. The reviewed independent provider tool panels show three columns in
a 1400px Settings window and two in a 960px window; cards naturally fall to one
column when their own content width requires it. Skills, MCPs, Plugins, Fluxos and
their loading placeholders reuse the same independent-card pattern.

Section headings leave a clear pause before the next group (28px above, 10px below).
Common spacing steps are small (4px, 8px, 12px) inside controls and groups, and larger
(16px, 24px) around content. The empty-chat composer occupies the same persistent
slot as the active composer, with a narrower maximum (48rem empty, 56rem active).
Its entry transition preserves drafts, focus and attachments.

The main window cold-opens maximized in the monitor's usable desktop area, outside
fullscreen. Saved normal bounds and monitor placement remain available for manual
restoration. `desktop.json` continues to preserve layout state; startup maximization
does not replace the normal frame. The defaults and minimums are native window
constraints, not targets for phone layouts.

## Elevation & Depth

Depth comes primarily from tonal layers and thin boundaries. Small offset shadows
separate cards and the composer; larger soft shadows belong to menus and dialogs.
There is no ambient neon, bevel system or rainbow perimeter. The application design
layer applies these treatments through public component slots without modifying
registry files.

### Shadow vocabulary

- **Top edge highlight:** `inset 0 1px 0 rgb(255 255 255 / 3%)` on dark cards and
  selected popup surfaces.
- **Card elevation:** `0 1px 2px rgb(0 0 0 / 8%)`.
- **Preference group elevation:** `0 1px 2px rgb(0 0 0 / 6%)`.
- **Composer elevation:** `0 4px 16px rgb(0 0 0 / 14%)`.
- **Popup elevation:** `0 10px 30px rgb(0 0 0 / 25%)` on dark dialogs and menus.

When a child dialog opens, only the covered parent popup is subdued (3px blur,
0.65 brightness). Child portals stay sharp. This establishes the active modal
level without applying a blur to the underlying desktop layout.

**The Quiet Depth Rule.** Establish hierarchy with neutral layers and thin
separators; keep shadows tied to the actual card, composer or popup role.

## Shapes

The base radius is gently rounded (10px), with source-derived steps (6px, 8px,
10px, 12px, 16px). Preference groups and cards use the larger group radius (12px);
the composer uses its own softer radius (16px). Navigation rows are compact (7px).
Dark badges default to a small radius (5px), while explicit counters, dots and
composer action buttons retain their round silhouettes.

Preference groups clip their contents and divide adjacent rows with thin horizontal
strokes. Connected rows are one group; independent cards stand on their own. Tool
activity stays flat when closed and gains a restrained inset surface when expanded.
Keep native window affordances, resizable separators and code overflow inside their
existing bounds.

## Components

### Buttons

Compact, familiar actions use the installed shadcn button variants. The ordinary
control is short (32px), gently rounded (10px), with modest horizontal padding
(10px); smaller controls retain the existing size variants. Primary uses action
blue and white text. Secondary uses the supporting neutral surface; outline and
ghost actions stay discreet. Destructive buttons use a translucent destructive
fill with readable red labels. Link buttons use the readable blue foreground tone.

Preserve registry hover, pressed, disabled and invalid behavior. Focus uses the
existing visible border and ring (3px, ring at 50% opacity). Main composer action
buttons are circular (30px). Every interactive element shows a pointer cursor;
disabled states remain visibly unavailable. Tooltips and icon-only controls retain
their accessible names.

### Inputs and fields

Settings fields use compact system text (13px), the standard control height (32px)
and the input stroke. Dark fields have a lightly tinted input background. Keep
labels, placeholders, visible focus, invalid state and disabled behavior from the
installed primitives. Long model, shell and sleep-mode values must remain legible;
do not shorten a selected value merely to force a narrower control.

### Cards and preference groups

Inset groups use the card surface, clipped rounded corners, thin row separators and
the small group shadow. Their container has no added internal padding; the rows own
it. General settings preserves this divided-row structure. Independent cards use
the responsive grids described in Layout, without turning every supporting label
into another nested card. Section titles and card titles share the restrained type
hierarchy.

General settings groups application lifecycle, sleep, notifications and Jarvito
separately from conversation language, automatic replies and titles. The Editor
group owns prompt refinement and message predictions. Workspace, storage and
backup controls follow these groups. The close-behavior selector appears only on
Windows and defaults to minimizing to the tray.

### Navigation and window chrome

Persistent sidebars use the navigation surface and soft gray active rows. Settings
tabs are compact (36px minimum height, 13px ordinary text). Every Settings navigation
glyph is white, including muted, yellow, cyan and green categories. The glyph is
small (16px) inside a rounded identity tile (24px); subdued category fills provide
identity while the active row provides selection. Preserve the native-style title
bar controls. The left-sidebar toggle precedes the supplied horizontal Jarvis logo;
four icon toggles for Inspector, GitHub, Explorer and Designer sit before the
Windows/Linux caption controls. macOS retains its traffic-light cluster on the
left and keeps app navigation available in fullscreen. Switching right panels
closes the current card before opening the next; reduced motion skips that wait.

Windows uses a native tray menu: Running and Recent each show up to four chats,
followed by Open Jarvis, Settings, About and Quit, with the section separators.
Chat and project occupy separate native menu columns and truncate independently
to 28 and 18 characters, including three literal dots. Populate the menu from
current runtime activity and library metadata when opened. Hiding the main
window preserves the companion and background work; Quit uses normal shutdown.

The main project sidebar keeps its real project identity accents, unread markers
and execution status. Disclosure controls, truncation hints and resizable panels
remain available. Existing data is the material of dashboard plots and status
summaries; missing values remain explicit rather than becoming synthetic metrics.

### Badges and technical metadata

Badges are compact, medium-weight labels with tabular numerals. Use neutral secondary
or outline variants for counts and metadata, and existing semantic variants for
meaningful state. Model labels and timestamps keep monospace; colored dots have
accessible state text alongside them where the source provides it.

### Composer and execution state

The composer is a card-toned inset field with a soft boundary and modest elevation.
Its focus treatment is a narrow ring (1px, ring at 30% opacity) and focus border
(ring at 60% opacity). Controls wrap within the available width; model selection
retains its bounded width (230px maximum).

Running work shows a quiet blue dot (6px) with `Em execução`; context compaction uses
`Organizando contexto`. Keep the status label, queue action and stop control usable.
The running indicator and stop action disappear when the turn completes, leaving
the composer available. The context meter uses its ordinary primary track during
compaction. Execution does not restore an animated rainbow border or glow.

### Loading, motion and scrolling

Reuse the structural skeletons from `src/components/layout/LoadingSkeletons.tsx`;
they preserve the destination's cards, rows, transcript or composer shape and expose
accessible status labels. In Settings, skeletons use the secondary surface so they
remain distinguishable from card and muted surfaces. Reserve spinners for ongoing
actions without a predictable content layout.

Hover, disclosure and pane transitions remain brief; Settings selection uses a
short color transition (120ms), panel resizing uses a gentle transition (200ms),
and the empty-chat slot uses the existing layout transition (260ms) and opacity
transition (180ms). Respect reduced motion for transitions, skeletons, progress and
spinners. Continuous decorative execution motion is absent.

`src/styles/scrollbars.css` shares one neutral scrollbar vocabulary across native
scrollports, ScrollArea, Monaco and xterm. Tracks are transparent; thumbs brighten
on hover and use muted foreground when active. Native scrollbars reserve a slim
hit area (10px), with transparent thumb edges (2px) and a rounded visible indicator.
Monaco and xterm retain their calculated geometry and draw an inset indicator
(6px). Standard scrollbar properties are the fallback; forced colors preserve
native scrollbars and system colors for custom indicators.

### Identity and feedback

Use the supplied `public/v2` assets through `JarvisLogo`: the transparent SVG mark
for compact UI, the SVG symbol for larger placements, and the tiled
icon for app identity. The README uses `png/jarvis-banner@2x.png`. Assistant message
headers show model metadata and completed-work disclosures without a brand lockup.
Title bars use `png/jarvis-horizontal@2x.png`. Completed responses add a thin
separator between their work header and answer; active work keeps its existing layout.
The optional Jarvito companion
retains its own existing identity and interactions. User actions use the installed
Sonner notifications; errors retain clear labels and recovery actions.

Source provenance: `src/index.css`, `src/styles/scrollbars.css`,
`src/components/settings/SettingsDialog.tsx`, `SystemSettings.tsx`, catalog settings
components, `src/components/layout/{TitleBar,Sidebar,StatusBar,Inspector,LoadingSkeletons}.tsx`,
`src/components/chat/{ChatArea,ChatComposer,SkillInput,AssistantMessageTurn,UserMessageBubble}.tsx`,
`src/components/JarvisLogo.tsx`, the installed `src/components/ui` primitives,
`src-tauri/src/desktop.rs`, and native window configuration. The sidecar expands
the representative components into plain HTML/CSS previews; it does not introduce
another component implementation into the app.

## Do's and Don'ts

### Do:

- **Do** use the shared semantic tokens and existing shadcn components.
- **Do** keep navigation visible and the Settings scrollbar at the window's right edge.
- **Do** use white glyphs in every Settings navigation tile.
- **Do** keep General preferences in divided groups and independent cards in width-driven grids.
- **Do** use system type for interface text and monospace for technical metadata.
- **Do** preserve visible focus, accessible status labels, pointer cursors and reduced motion.
- **Do** show structural loading placeholders with contrast against their containing surface.
- **Do** keep the composer execution indicator discreet and remove its running state on completion.
- **Do** preserve real data, existing identity assets, controls and normal-window restoration.

### Don't:

- **Don't** restore the industrial palette, bevels, neon glow or rainbow composer border.
- **Don't** use action-fill colors as small semantic text on charcoal surfaces.
- **Don't** add uppercase tracked micro-labels or decorative display typography to desktop chrome.
- **Don't** force preference rows into catalog grids or independent cards into one permanent column.
- **Don't** attach the Settings scrollbar to its centered inner content block.
- **Don't** edit registry primitives to apply application-specific styling.
- **Don't** infer runtime behavior on unreviewed platforms or states from these visual rules.

Not canonized or repaired by this record: no material design defects remain in the
finish review. Windows runtime, uncaptured secondary dialogs, the light fallback's
rendered appearance and maintenance completion remain outside the native evidence;
the documented light values are source-derived, and documentation does not widen
those verification claims.
