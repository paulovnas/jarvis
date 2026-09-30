# Antigravity CLI integration review

## Decision

The official `agy` CLI is a viable candidate for an optional external executor,
alongside the existing Jarvis and Claude executors. It can own authentication,
provider transport, native model context and provider-specific recovery while
Jarvis keeps workflows, tools, Core resources, history and the user interface.
Keep the direct Antigravity provider available while the new executor is validated.
No CLI-backed executor was implemented or benchmarked during this review.

## Verified interfaces

The locally installed `agy` 1.2.13 advertises noninteractive `--print`,
`--input-format stream-json`, `--output-format stream-json`, `--conversation`,
`--continue`, `--model`, `--effort low|medium|high|max`, `--agent`, and
`--dangerously-skip-permissions`. Help/version
were inspected without running inference or modifying account configuration.

Official installation documentation states that the CLI reuses an authenticated
account through the operating system credential store. Jarvis can launch the
installed CLI without copying tokens or introducing a Python dependency.
Official MCP documentation supports local stdio and remote MCP servers, including
workspace configuration. A Jarvis bridge must implement standard MCP: Claude's
proprietary `sdk` MCP transport and control messages cannot be reused unchanged.

The linked Python SDK is a separate integration path: its documented authentication
uses Gemini API keys or Vertex credentials, and its Python client launches a bundled
Go harness. Subscription/account-login reuse through that SDK was not established.
Current SDK metadata reviewed: `google-antigravity` 0.1.20; source commit
`12f9a4c3becf487302dc799b0f59054f01f3ddb9`.

## Implementation boundaries

Reuse Jarvis's executor selection, role-specific tool catalog, native tool handlers,
MCP discovery, event projection, session identity and executor-switch handoffs.
Adapt the tool bridge and CLI event parser; avoid reproducing provider OAuth or
the SDK's internal harness protocol. The CLI must continue its own loop after
Jarvis supplies each MCP result, rather than restart for every tool call.

Community integrations demonstrate structured `agy` transport. One generates a
workspace agent with `excludeDefaultComponents: true`, `inheritMcp: false`,
`mainAgent: true` and an embedded `mcpServers` list. This is promising for isolating
Jarvis tools without changing the user's global MCP configuration; the contract
needs validation against the supported CLI version and official agent specification.
Reject unexpected fallback to the CLI's default agent.

Validate before replacing the direct provider:

- Native tool exclusion and a real MCP bridge that returns Jarvis tool results.
- Text, progress, tools, usage and terminal/error event projection without duplicates.
- Explicit user autonomy, cancellation of child processes, and continuation after tools.
- Resume and executor/model fallback preserving confirmed and uncertain outcomes.
- Model catalog, effort mapping and authentication errors on macOS, Linux and Windows.
- Steering support, account selection and machine-readable quotas, whose compatibility
  with Jarvis has not yet been established.

Delegating transport can reduce compatibility work. It does not prove faster model
generation or repair malformed history produced by Jarvis itself.

## Current Portal ITA incident

The conversation “Filtro por intervalo de pontuação” had two Designer follow-ups
fail after the planned implementation. The preceding Claude turn had four tool
previews without persisted calls. `journal::interrupt_tools` then persisted unknown
outputs for those previews without the corresponding `function_call` entries.
Automatic compaction retained all four orphan outputs in the history tail.

Antigravity's request conversion could not resolve their tool names and failed
locally before HTTP dispatch. Recovery repeated the same malformed input. The GPT
secondary received those orphan outputs and rejected `input` with HTTP 400.
This explains both errors in the screenshot without attributing the first failure
to an Antigravity connection interruption.

The correction preserves observed call identity when closing interrupted previews.
Historical orphan results are retained as untrusted reference messages after the
tool group, preserving full payloads without inventing actions or splitting parallel
results. The shared projection also applies to compaction and all supported provider
protocols. The original installed-app journal remains unchanged by this investigation.

Validation completed: `bun run check` (949 tests passed, one existing skip),
`cargo clippy --all-targets -- -D warnings`, and `cargo test` (1,149 passed,
24 existing ignored tests). An additional offline replay of a private copy of
the actual conversation preserved all four orphan receipts and passed compaction
and cold reload checks. No live provider request or installed-app smoke test was run.

## Optional executor integration

The optional `agy` executor is a separate singleton provider, disabled by default.
It uses the installed official CLI and its existing Google login; execute `agy`
in a terminal to complete login. The CLI does not expose `agy auth login` in 1.2.13.
The direct Antigravity provider remains an independent selection.

Jarvis discovers the model catalog through `agy models`. Usage comes from the
structured native `/quota` command: five-hour and weekly buckets retain their
actual remaining fractions and reset timestamps. These metadata commands do not
run inference. No subscription tier or account email is inferred.

Effort variants are grouped under their base model. Only levels actually listed
by the CLI are offered; `--effort=max` in generic help is not a model capability.
Legacy variant selections resolve to the base plus their selected effort, avoiding
the CLI's conflicting `--model ...-high --effort ...` combinations. Models without
advertised effort variants do not expose an adjustable reasoning selector.

A private durable workspace contains the generated custom agent. The real project
is an additional directory; HOME, global MCP configuration and login credentials
are unchanged. The custom agent excludes default components and inherited MCPs.
Its only MCP server is the authenticated loopback Jarvis bridge, which reuses
the existing role, project, autonomy, tool, Core and workflow contracts.
Native execution outside this bridge stops the process. The CLI's internal
`manage_task(Action="list")` query is bookkeeping and can continue without a
Jarvis tool effect. Mutating task actions still require the Jarvis execution path.

CLI 1.2.13 has two important discovery details: `agy agents` lists global agents,
while `agy -p=/agents --output-format json --agent jarvis-runtime` also discovers
workspace agents without inference. The stream `init.tools` field advertises the
global catalog before filtering. The MCP dispatcher is installed by `mcpServers`;
putting `call_mcp_tool` in the custom agent's `tools` list is invalid because it is
not a component registry entry. Tests therefore verify actual MCP dispatch and
native agent identity rather than treating the advertised catalog as availability.

Native conversation IDs are persisted for resume; executor changes and secondary
model fallback begin a new native conversation with a bounded historical handoff.
AGY also snapshots custom-agent instructions and MCP endpoints in a conversation.
Jarvis therefore persists a private loopback port, bearer token and static execution
profile in the executor workspace, outside journals and exported backups. Restart
rebinds that port and retains the token. If the port is occupied or the execution
profile changes, a new native conversation receives the bounded handoff, including
confirmed receipts from an interrupted current turn. Live workflow, task and memory
context is sent with the current request, rather than frozen in the custom agent.
Tool identity uses the native conversation and step index. Confirmed receipts are
reused; a started operation without a durable result is never blindly repeated.
Streamed messages, reasoning, tasks and tools use the existing public chat journal.
Tool-free knowledge generation and feedback extraction use the same optional CLI.

The installed-CLI smoke tests are opt-in because the bridge test performs two small
inference rounds, with a process restart and actual tool dispatch in the same native
conversation. Other discovery commands do not consume inference.
They operate in temporary projects and do not read or modify the user's projects:

```sh
JARVIS_AGY_SMOKE_MODEL=<available-cli-model> cargo test --manifest-path src-tauri/Cargo.toml native_cli_ -- --ignored --nocapture
```

Final implementation validation on macOS: `bun run check` passed (980 tests,
one existing skip), Clippy passed with warnings denied, and `cargo test` passed
(1,181 tests, 26 ignored). Both opt-in tests passed with installed AGY 1.2.13:
custom-agent discovery and two real MCP dispatches across a native conversation
restart. The smoke project remained empty and both processes were cleaned up.
The installed Jarvis application and native Windows/Linux executors were not
exercised by this validation.

## References

- [Official CLI introduction](https://antigravity.google/blog/introducing-google-antigravity-cli)
- [CLI installation and authentication](https://antigravity.google/docs/cli/install/)
- [CLI reference](https://antigravity.google/docs/cli/reference/)
- [MCP configuration](https://antigravity.google/docs/mcp/)
- [Custom agents](https://antigravity.google/docs/subagents/)
- [Python SDK](https://github.com/google-antigravity/antigravity-sdk-python/tree/12f9a4c3becf487302dc799b0f59054f01f3ddb9)
- [AWS CLI Agent Orchestrator](https://github.com/awslabs/cli-agent-orchestrator/blob/main/docs/antigravity-cli.md)
- [Hermes AGY adapter](https://github.com/Realtyxxx/hermes-plugin-antigravity-agy/tree/f85003d3ce0de3844e2d5c23174d630ff3586e5d)
- Codex: `docs/codex/codex-rs/core/src/context_manager/{history,normalize}.rs`.
- OpenCode: `docs/opencode/packages/opencode/src/session/message-v2.ts` and provider transforms.
- OMP: `docs/omp/packages/ai/src/providers/openai-codex/request-transformer.ts`
  and `transform-messages.ts`.
