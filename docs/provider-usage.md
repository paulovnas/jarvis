# Provider quotas

Jarvis reads account quotas through the existing OAuth credential manager. Tokens
are refreshed under the same guard used by inference and disconnection; neither
credentials nor raw provider payloads cross IPC.

- OpenAI Codex: `GET /backend-api/wham/usage`, scoped by `ChatGPT-Account-Id`.
  Available reset credits are enriched with the read-only
  `GET /backend-api/wham/rate-limit-reset-credits` endpoint. There is no reset
  redemption action. Window durations come from the response, not slot positions.
- Antigravity: `POST /v1internal:retrieveUserQuotaSummary`, with
  `fetchAvailableModels` as a compatibility fallback. The summary distinguishes
  Gemini from the shared third-party bucket. The fallback retains model-specific
  buckets and does not guess missing window durations. An optional read through
  `loadCodeAssist` enriches the account plan, preferring `paidTier` to `currentTier`.

Each account has an independent 60-second cache and single-flight request. The
frontend refreshes every 61 seconds while visible and on focus. Failed refreshes
retain the last sample, visibly marked as outdated. Missing percentages remain
unknown; a passed reset timestamp never synthesizes a fresh quota.

`provider_accounts.show_usage` defaults to true. `show_third_party_usage` defaults
to false and controls Antigravity presentation. Preferences are persisted in
Jarvis's local SQLite database. Disabled or hidden accounts are not polled.

The statusbar shows percentages remaining. Hover, keyboard activation, or click opens
the detail card with account identity, plan, windows, and reset-credit expiration
dates when supplied by the provider.

References studied: OMP's `packages/ai/src/usage/openai-codex.ts`,
`openai-codex-reset.ts`, and `google-antigravity.ts`; Metis's desktop inspector;
the official [Codex App Server documentation](https://learn.chatgpt.com/docs/app-server).
The ChatGPT and Antigravity wire endpoints follow OMP's OAuth integrations and
are not represented as stable public API-key endpoints.
