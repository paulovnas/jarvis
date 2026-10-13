<!-- impeccable:product-schema 1 -->
# Jarvis

## Platform
web

## Product
Jarvis is a desktop application for developing software with AI agents. Its React interface runs inside Tauri v2 with a Rust backend. It supports macOS on Apple Silicon and Windows x64.

## Audience and jobs
People working on software projects connect their own AI providers, choose models and agents, send instructions, review results, and manage several persistent projects and conversations. The interface is in Brazilian Portuguese.

## Existing capabilities
Persistent workspaces, projects and conversations; model and workflow selection; streaming reasoning and tool activity; attachments and message queues; agents and plans; files, diffs, terminals, browser and HTTP panels; provider connections; skills, MCP servers, hooks and plugins; notifications, usage information, backups and updates. Jarvito is an optional floating desktop companion.

## Constraints
Preserve existing data, preferences, provider authentication, execution controls, keyboard shortcuts and native window behavior. All interface controls use the installed shadcn/Radix components. Business logic belongs in Rust. This redesign does not add a new provider or change agent execution.

## Brand commitments
The user requested a radical replacement of the previous industrial appearance with a native macOS inspired dark interface. Their first supplied screenshot is the primary visual reference; the third screenshot is supplementary. Persistent navigation, neutral charcoal surfaces, inset settings groups, subtle elevation, rounded panels, thin separators, muted typography, compact spacing and minimal controls are binding. On 2026-10-12 the user explicitly chose a discreet macOS style composer execution indicator.

## Assets
Keep the existing Jarvis identity and application icon. The supplied screenshots are design references, not shipping assets. Use the system interface font on macOS and the installed local fonts as fallbacks; retain monospace for technical metadata and code.

## Evidence
Product capabilities and platform support are documented in README.md and implemented in the current source. Visual commitments come directly from the user's screenshots, design prompt and execution-indicator answer.
