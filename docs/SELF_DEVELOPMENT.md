# Developing Jarvis with Jarvis

The installed application can work on its own source repository. This does not
require running a debug binary or replacing the stable app that hosts the chat.
Keep test builds separate from the running application and its user data.

## Enable the local development environment

1. Add the root of the official Jarvis checkout as a project.
2. Open that project's settings and select **Autodesenvolvimento**.
3. Enable **Ambiente de desenvolvimento do Jarvis**.
4. Select one source conversation and optionally describe the problem.
5. Prepare an incident, copy its reference, and paste it into a chat in the
   authorized Jarvis project with the investigation request.

The section is absent for ordinary projects. Native code checks the Git root,
Git common directory, official origin, package identity and Tauri identifier.
Consent is local to this installation/profile and bound to the exact project
registration and canonical checkout. A similarly named folder, subdirectory,
another checkout or another project registration does not inherit consent.

## What the agent can read

Three read-only tools are available while this local environment is enabled:

- `jarvis_dev_diagnostics`: sanitized runtime and aggregate harness metrics.
- `jarvis_dev_incidents`: summaries of incidents explicitly shared with this
  development project.
- `jarvis_dev_incident`: one shared incident, in pages of at most 50 events.

Incident events preserve statuses, durations, structural argument/result shapes
and bounded diagnostic metadata. They exclude raw conversation messages,
commands, tool arguments/output, terminal logs and credentials. Optional user
descriptions and selected conversation/project titles are bounded and sanitized.
This is diagnostic evidence, not a reproduction of the full conversation.

Snapshots are stored outside the source checkout, in the current runtime
profile's private data directory. At most 20 incidents are retained for each
authorization; incidents expire after 30 days. Nothing is committed to Git or
automatically sent to an agent when an incident is prepared. Reading it in an
agent chat sends that sanitized evidence to the selected model provider, just
like other tool results.

## Revoke access

Remove an individual incident to prevent further reads, or disable the local
environment to revoke access and remove its stored incidents. Native and Claude
executors revalidate access when reading; private Claude receipts are read
afresh rather than replayed from a cache. Subagents retain the same destination
project boundary and their configured tool permissions.

Revocation cannot erase information previously delivered to a model, chat
history or the clipboard. Start a new development chat if that previous context
should no longer be used. The incident tools never provide a free-form source
conversation ID lookup or permission to repeat the original operations.
