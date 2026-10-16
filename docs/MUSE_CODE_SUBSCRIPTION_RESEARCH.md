# Muse Code subscription integration assessment

Research date: 2026-10-15. Research only; no provider implementation, installation, login, credential access, inference, commit, push, or release was performed.

## Result

Muse Code here means Meta's coding agent, not Muse models sold through OpenCode Go. A Muse Code subscription must not be treated as a general-purpose Meta API subscription. Meta documents a credential attached during CLI onboarding, restricted to that coding harness; separately created API keys use pay-as-you-go billing. The current Model API terms, updated October 9, repeat the harness restriction in section 3.2. [Subscription documentation](https://dev.meta.ai/docs/muse-code/subscriptions), [Model API terms](https://dev.meta.ai/legal/terms-of-service).

There is a documented application integration surface: an application can drive the official `muse serve` process through the Muse Session Protocol (MSP). This is the recommended architecture to investigate for subscription use. It keeps Muse Code as the executing harness. It does **not** establish that an arbitrary Jarvis model request may reuse the subscription credential. Subscription charging through a real MSP session still needs account-level verification. [Official integration overview](https://meta-models.github.io/muse-code-sdk/next/).

## Reference update and scope

`docs/omp` is the submodule configured for `https://github.com/can1357/oh-my-pi.git`, branch `main`. It was clean before updating.

- Before: `fa786c0d790a6f8408960386530efe95ef45f8c6` (2026-10-03).
- After: `55c065ed0354f680c94751e55da59bbec1f66161` (2026-10-15).
- Update: fast-forward only, no stash or rebase; 4,039 commits advanced. The final checkout is clean and has zero ahead/behind versus `origin/main`.
- The containing Jarvis checkout now has an uncommitted `docs/omp` gitlink change. Existing changes in other references and local verification artifacts were preserved.

Relevant additions since the previous checkout include subscription-only Muse Wonder catalog/pricing estimates, recovery of stored Muse Responses after disconnect, and making remote response storage opt-in. Those estimates are not evidence of subscription invoice amounts. [Update comparison](https://github.com/can1357/oh-my-pi/compare/fa786c0d790a6f8408960386530efe95ef45f8c6...55c065ed0354f680c94751e55da59bbec1f66161).

Jarvis source inspection covered `main` at `02339ec` and the existing attached worktree `/Users/enesolucoes/.codex/worktrees/opencode-go-recovery/jarvis`, which also contains the recent uncommitted provider display-name work. Nothing in either product source tree was changed by this research.

## How Jarvis integrates providers today

| Boundary | Existing implementation | Implication for Muse |
| --- | --- | --- |
| Account registration and catalog | `src-tauri/src/openai_codex.rs`: `ProviderAccount`, `ProviderModel`, `OpenAiCodexState`; `src-tauri/src/persistence.rs`: provider metadata in SQLite | A native subscription needs explicit identity, provider kind, catalog discovery and account-state handling. Adding a label alone is insufficient. |
| Credentials | `SecretStore` in `openai_codex.rs`; macOS Keychain, shared native vault on Windows/Linux | Metadata can remain in SQLite. Model/account credentials must remain outside renderer state and logs. An official-host adapter should leave Muse credentials under Muse's control. |
| Compatible API configuration | `openai_codex/custom.rs`: `Protocol`, `AuthMode`, `Config`, `save_custom_provider`; `custom/discovery.rs` | Supports OpenAI Completions, OpenAI Responses and Anthropic Messages with Bearer or x-api-key authentication. This does not add subscription entitlement or device login. |
| Native model execution | `agent/provider.rs`: `TurnSession`; `agent/provider/custom/`; `agent/provider/capabilities.rs`: `ModelCapabilities::resolve` | Streaming, tools, replay, images, reasoning and context capabilities are explicit contracts. Some family selection still depends on custom/project credential fields; Muse cannot safely inherit the default Codex behavior. |
| Credential renewal | `openai_codex/inference_auth.rs`: `renew_inference_credential`; `agent/provider/auth.rs`: `Authentication` | The current native path assumes existing Codex/Antigravity renewal semantics. Muse's lifecycle would need a separate branch, not an OpenAI refresh request. |
| External coding-agent execution | `claude.rs`: `Executor`; `claude/transport.rs`: `ClaudeProcess`; `agent/claude_executor/`: process bridge, approvals, session references and event projection | Closest existing architecture for official Muse Code subscription integration. Reuse concepts and tested lifecycle behavior; MSP is a different wire protocol. |
| UI and usage | `SettingsDialog.tsx`, provider forms/cards, `core/provider-accounts.ts`, backend `openai_codex/usage.rs`, shared usage components | Add subscription-specific setup/status, model groups, usage and reconnect errors. Preserve aliases as internal IDs and reuse the local display-name work. Usage dispatch/refresh currently contains provider-kind allowlists. |

The examined references make the same boundaries visible: Codex separates provider authentication and device login (`docs/codex/codex-rs/model-provider/src/auth.rs`, `login/src/device_code_auth.rs`); OpenCode exposes provider-specific OAuth/API methods and pending authorization (`docs/opencode/packages/opencode/src/provider/auth.ts`). Neither implies that another vendor's subscription can use OpenAI's auth contract.

## What OMP actually implements

These are **confirmed source observations**, not successful requests performed during this research. Paths below are relative to the updated `docs/omp` checkout.

1. `packages/catalog/src/compat/rules/auth/muse-code.kdl` defines a device-code login using Meta authorization/token endpoints, a configured public client ID, an after-exchange hook and no refresh grant.
2. `packages/ai/src/registry/oauth/muse-code.ts` exchanges the account token at `https://api.meta.ai/muse-code/key`, checks subscription/payment state and stable identity, and retains separate account-token and subscription-key fields. It reuses an existing minted key instead of repeatedly minting one.
3. `packages/ai/src/registry/muse-code.ts` unwraps only the subscription key for inference/discovery and adds `x-api-version: 1.0.0`.
4. `packages/ai/src/usage/muse-code.ts` uses the account token to retrieve quota, maps window/weekly usage, and removes `api_key` from diagnostic payloads. Tests distinguish subscription quota from PAYG credentials and cover inactive subscriptions/rate limiting.

[Pinned OAuth implementation](https://github.com/can1357/oh-my-pi/blob/55c065ed0354f680c94751e55da59bbec1f66161/packages/ai/src/registry/oauth/muse-code.ts), [transport](https://github.com/can1357/oh-my-pi/blob/55c065ed0354f680c94751e55da59bbec1f66161/packages/ai/src/registry/muse-code.ts), [quota](https://github.com/can1357/oh-my-pi/blob/55c065ed0354f680c94751e55da59bbec1f66161/packages/ai/src/usage/muse-code.ts).

The provider rules select `https://api.meta.ai/v1` with OpenAI Responses. OMP disables `tool_choice`, keeps edits as function tools rather than custom/freeform tools, constrains output limits, and handles encrypted reasoning. Its catalog includes plan-dependent Wonder variants. These adaptations are evidence against assuming universal compatibility merely from the endpoint shape. [Pinned provider rules](https://github.com/can1357/oh-my-pi/blob/55c065ed0354f680c94751e55da59bbec1f66161/packages/catalog/src/compat/rules/providers/muse-code.kdl), [provider tests](https://github.com/can1357/oh-my-pi/blob/55c065ed0354f680c94751e55da59bbec1f66161/packages/ai/test/muse-code-provider.test.ts).

Stored-response recovery is optional: `providers.muse-code.storeResponses` defaults to false. Enabling it retains prompts/results remotely; OMP tests recovery of the existing result instead of replaying the turn. Jarvis's generic Responses request also currently sends `store: false`. Storage, reconnect recovery and retrying a mutation must remain separate decisions. [OMP storage setting](https://github.com/can1357/oh-my-pi/blob/55c065ed0354f680c94751e55da59bbec1f66161/packages/coding-agent/src/session/settings.ts), [recovery tests](https://github.com/can1357/oh-my-pi/blob/55c065ed0354f680c94751e55da59bbec1f66161/packages/ai/test/openai-responses-resume.test.ts).

OMP's direct implementation proves a technical route exists in that project. It does not establish Meta authorization for Jarvis to copy its OAuth client identity or use a subscription key outside Muse Code.

## Access, authentication and compatibility requirements

- Confirm an eligible Meta account, region, active plan and available models. Models/allowances must come from the account's current catalog rather than an unconditional Spark/Wonder list. [Subscriptions](https://dev.meta.ai/docs/muse-code/subscriptions).
- For the official-host route, install a supported `muse` binary and authenticate it. An inherited `META_API_KEY` or stored API key takes precedence over browser login and can change which credential pays. Detect the effective authentication mode and never silently fall back to PAYG. [Authentication and billing](https://dev.meta.ai/docs/muse-code/auth).
- Current official documentation supports macOS, Linux and native Windows. Windows uses PowerShell and may request UAC for sandbox initialization; voice input and inter-session messaging have platform limitations. This is documentation evidence, not a Windows runtime test. [Platform support](https://dev.meta.ai/docs/muse-code).
- Contributor behavior must be explicit. Under the current terms, subscription content mode is controlled by the account setting, not solely by a model's `-contributor` suffix. Inspect the effective mode before presenting a privacy claim. [Terms sections 3.3, 5 and 6](https://dev.meta.ai/legal/terms-of-service).

## Proposed integration boundaries

| Option | What changes in Jarvis | Assessment |
| --- | --- | --- |
| Official Muse Code host over MSP | Add an executor adapter, host lifecycle, session mapping, approvals, event projection, model/usage discovery and login status | Recommended investigation path for subscription use. Muse retains its agent loop. |
| Direct subscription model provider as in OMP | Add native Meta auth/key lifecycle, provider identity, catalog, quotas, transport policy and replay/recovery tests | Technically evidenced in OMP; outside-harness subscription use conflicts with the published restriction. Obtain a supported integration arrangement before treating this as a product path. |
| Meta PAYG API via compatible provider | Reuse existing compatible protocols and validate Meta-specific capabilities | Separate billing/product option. Does not satisfy the subscription requirement. [Meta API overview](https://dev.meta.ai/docs/overview). |

For MSP, a native Rust client fits the current architecture: the backend owns a child process and exchanges line-delimited JSON-RPC 2.0 over stdio. Existing Tokio/Serde/process code can be reused, but typed MSP contracts and conformance fixtures would be new work. The protocol is pre-1.0; inspect `initialize` and pin/test the schema fingerprint rather than assuming indefinite compatibility. [Official wire guide](https://meta-models.github.io/muse-code-sdk/next/guides/msp-wire/).

Alternatively, the official MIT-licensed `@muse-code/sdk` requires Node 20+, a Muse binary and no package runtime dependencies. A Node sidecar adds distribution/supervision work to the Tauri application; it cannot simply run inside the browser renderer. The SDK is a developer preview. Its MIT license does not establish permission to redistribute the separate Muse binary; prefer detecting a user-installed official binary until distribution terms are confirmed. [Official SDK repository](https://github.com/meta-models/muse-code-sdk).

The external-executor change also reaches `src/core/executors.ts`, chat options and generated IPC contracts, workflow/model selection, backup validation, Companion and remote projections. Existing unknown executors become unavailable rather than falling back to Jarvis; that safety property must remain intact. Account display names already implemented locally must stay separate from aliases and Muse session/account identity. These are implementation surfaces, not changes performed here.

An MSP adapter must map session/turn IDs to Jarvis conversations, handle approvals and input requests, project streamed items, cancel without accepting stale results, and preserve confirmed receipts across disconnects. Resume must respect the host's advertised durability. Tools/skills/MCP and native browser/computer actions need an explicit bridge and permission contract; merely rendering model text would not provide feature parity. [Sessions and turns](https://meta-models.github.io/muse-code-sdk/next/guides/msp-concepts/sessions-and-turns/), [durability](https://meta-models.github.io/muse-code-sdk/next/guides/msp-concepts/durability-profiles/).

The account methods include host-owned device-code login and projected account status, but they are marked experimental. A minimum compatible host/schema and a fallback to the official CLI login must be established. Quota can be exposed through `usage/read` and `usage/changed`; preserve observation timestamps and use only reported windows. [Login](https://meta-models.github.io/muse-code-sdk/next/generated/msp/methods/account-loginstart/), [account status](https://meta-models.github.io/muse-code-sdk/next/generated/msp/methods/account-read/), [usage](https://meta-models.github.io/muse-code-sdk/next/generated/msp/notifications/usage-changed/).

## Open questions and recommended validation

The next decision is whether Muse must be only an LLM under Jarvis's existing tool loop, or may execute as an official external coding agent. Subscription support is not equivalent across those designs.

For the external-host design, the next authorized experiment should use an isolated profile and a subscribed account: verify effective browser-account authentication and subscription meter consumption; list models; run a bounded read-only turn; exercise approval, cancellation, process death and resume; and verify there is no PAYG fallback. Do not infer billing from a successful login or a returned model list.

Still unconfirmed: the user's plan/region/entitlements; subscription billing for a real MSP turn; minimum host version and experimental-account API availability; supported multi-account/profile isolation; mapping of Jarvis workflows, refinement/prediction side requests and native tools; actual model capabilities; and packaging, sandbox and cancellation behavior on each OS. A direct-provider design additionally needs an officially supported subscription integration basis and its own OAuth client/access arrangement.

No integration tests, inference calls, credential reads or platform builds were run. Verification in this stage consisted of Git safety/synchronization checks, static source/test inspection and primary documentation review. Prior feature-test results are not evidence of Muse compatibility. The only authored repository file is this assessment; `docs/omp` was advanced as requested.
