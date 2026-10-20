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

Four read-only tools are available while this local environment is enabled:

- `jarvis_dev_diagnostics`: sanitized runtime and aggregate harness metrics.
- `jarvis_dev_incidents`: summaries of incidents explicitly shared with this
  development project.
- `jarvis_dev_incident`: one shared incident, in pages of at most 50 events.
- `jarvis_dev_logs`: persistent internal diagnostic events, including redacted
  error messages and available stacks. It accepts `level` (`info`, `warning`,
  `error`), a text `query`, `since`/`before` timestamps in milliseconds, and
  `offset`/`limit` (up to 50 events). Reuse the returned `before` and `nextOffset`
  when paging. No filesystem path or source conversation ID is accepted.

Incident events preserve statuses, durations, structural argument/result shapes
and bounded diagnostic metadata. They exclude raw conversation messages,
commands, tool arguments/output, terminal logs and credentials. Optional user
descriptions and selected conversation/project titles are bounded and sanitized.
This is diagnostic evidence, not a reproduction of the full conversation.

## Persistent logs and maintenance

The existing profile-private `diagnostics/events.jsonl` store records startup,
shutdown, panics, storage/provider failures, tool completion/error metadata and
turn failures. Tool events keep the tool name, error code and a hashed call ID,
not its arguments or output. Native panics capture a backtrace. The webview also
reports uncaught errors, unhandled promise rejections, console warnings/errors
and React errors, preserving existing error/component stacks when available.
Arbitrary console objects are not serialized. Credentials, authorization values,
private keys, URL credentials/query strings and email addresses are redacted
before persistence. Do not submit conversation text or request bodies to logging.

Rotation retains at most five 10 MiB diagnostic files and three 2 MiB content-free
harness trace files: **56 MiB total** per profile. Older files are replaced, not
retained indefinitely. Messages are bounded to 8 KiB and stacks to 128 KiB;
exceptionally oversized details explicitly report `truncated: true`. Multiline
stacks below that safety limit are preserved in full. Log responses are paged
and byte-bounded; dropped diagnostic writes are reported, not counted as success.
Logs cannot reconstruct details that were never captured by an older version.

Use **Configurações → Geral → Limpeza → Logs internos → Limpar logs** to review and
confirm removal of diagnostic and harness logs in the current profile. Logging
continues afterwards. Run markers, conversations, projects, settings and shared
incident snapshots remain intact. A failed/partial cleanup requires a fresh
analysis; it is never retried automatically.

While self-development is enabled, the authorized agent may query these
sanitized application logs without preparing a separate incident. Reading logs
in a chat sends those diagnostic details to the selected model provider.
Every read rechecks the exact project authorization before and after accessing
the records. Disabling self-development revokes this access; it does not disable
local logging or retroactively erase evidence already sent to a model.

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
