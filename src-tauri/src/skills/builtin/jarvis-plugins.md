---
name: jarvis-plugins
description: Manage Codex-compatible Jarvis plugins and marketplaces, or create a plugin package, using the supervised native tools.
---

Read `jarvis_catalog` with `view=plugins` (or `jarvito_catalog` in the global assistant) for the separate plugins revision, sources, installed components and requirements. Use `jarvis_propose_plugin` with that exact `pluginsRevision`. The native review approves the materialized package and its fingerprint once. Never write the plugin registry or other agents' configuration files through shell or filesystem tools.

Choose the requested operation from the tool schema. Marketplaces accept a GitHub repository, Git URL or local folder; Git references and sparse paths apply only to Git sources. After adding or refreshing a marketplace, inspect the catalog again before installing a plugin by its exact ID. Local folders and archives can be imported; creating a package accepts the draft fields supported by the schema. Use real product requirements to decide which skills, MCPs, hooks and resources belong in the package.

Preserve unrelated installations and the user's component choices. Explain material dependencies, local commands, external service requirements and conflicts with native Jarvis components before proposing activation. In particular, a second Context-mode, Beads, Ponytail or other native integration can duplicate behavior. Do not disable the native integration automatically or present a duplicate package as ready merely because its manifest parsed.

Installation and trusting hooks are separate changes. Hook trust applies to the reviewed package and commands; updates can require fresh trust. An empty explicit hooks definition must remain empty. Portable and legacy manifests have different host capabilities; use the catalog's supported flags and warnings. Never promise unsupported components will run.

Never put credentials in a draft, command, URL, source or summary, and never request existing stored credentials. The user supplies private server values and completes OAuth in plugin settings. Apps refer to connector IDs and require an enabled ChatGPT account plus actual gateway availability. An installed package is not proof of successful authorization or connectivity.

Newly approved plugin skills and MCP servers become discoverable on the next model step. Package versions already pinned in the running turn update on the next turn; disabling a plugin or component, or revoking its authorization, takes effect immediately. Hook configuration and trust changes retain their existing subsequent-turn contract. After approval, report the actual returned status and inspect the current catalog when further work depends on it. On rejection, honor the user's note. On a stale revision, re-read and reconsider the change. On an uncertain result, inspect state before retrying; do not repeat the effect blindly. Uninstall removes owned contributions while preserving user-authored settings and the plugin's persistent data.
