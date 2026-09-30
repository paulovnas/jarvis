# Antigravity efficiency review

Research date: 2026-09-29. This review combines a read-only production incident audit, the local Codex/OpenCode/OMP references and current public provider documentation. The research preceded the changes documented under Implementation status; incident observations describe the original running application.

## Incident evidence

The Salesforce planning conversation `281640bd8e3e8045386ac5de410cb54e` delegated Designer job `09e0ec509c7fe8832346bf1f03eec2b9`, using `gemini-3.8-flash` with the explicitly selected `high` reasoning level. At the 22:59:45 snapshot, the same Designer turn had run for 58m48s. Workflow dispatch attempts remained one and worker recovery attempts remained zero.

| Observation | Measured result |
| --- | --- |
| First four Antigravity inference steps, including 13 executed tool calls | Approximately 48 seconds |
| Fifth inference step, including five reconnects | 40m50.344s |
| Six unsuccessful provider requests within that step | 2,390.267 seconds |
| Backoff between those requests | 60.077 seconds |
| Final lack of parsed output, reported as 300 seconds on each request | 30 minutes combined |
| Remaining request time, including headers and partial reasoning | 9m50.267s; finer attribution unavailable |
| All recorded tools through the snapshot, including later Codex work | Approximately 40 seconds |
| Recorded Core execution time | 0.011 seconds |

All six failed requests returned HTTP 200 before timing out while awaiting continuation. None produced a completed tool or file change during the 40m50s interval. The 13 calls above are executed tools, not the size of the advertised catalog; that size was not captured.

The configured secondary model, `gpt-6.1-sol / max`, took over at 22:42:38 without restarting the worker. Its first major `apply_patch` inference took 614.720 seconds and reported 19,059 output tokens. The resulting HTML was 47,999 bytes and 397 lines, with seven sections, 31 code elements and nine SVGs. Smaller patches and verification followed.

The user requested a simple, professional `index.html` presentation. The planner produced a 6,216-character dispatch and a 5,159-character Beads specification with multiple suggested sections, documentation excerpts, diagrams, copy actions and checks. This elaboration is a workload amplifier, although it does not independently explain six failed Antigravity requests.

The latest successful request reported 39,537 input tokens in a 1,048,576-token window, including roughly 33,000 cached tokens. Before the failed step, the replay contained 38 wire items and 67,860 serialized bytes. The first request already had 30,148 input tokens. There is meaningful fixed overhead, but no evidence of context saturation or a lost cache explaining this incident.

The running application was `/Applications/Jarvis.app`, version 1.8.0, started before the local recovery patches. This incident is not a live retest of those patches. Request-level transport telemetry was unavailable. The journals preserve partial summaries at failure, without individual chunk timestamps. Their timeout descriptions demonstrate absence of parsed progress; they do not prove that the backend had stopped generating or that no raw transport bytes arrived.

## Primary investigation: long tool arguments versus stream progress

Jarvis's SSE parser produces events after a complete JSON frame. The Antigravity deadline advances after a parsed event contains reasoning, text or complete tool arguments. Raw bytes belonging to an unfinished frame do not advance that deadline. See `src-tauri/src/agent/provider.rs` (`Sse::push_bounded`) and `src-tauri/src/agent/provider/antigravity.rs` (`receive`).

A large function-call argument can therefore be important in two different ways:

- A fragmented JSON event could be actively arriving while the semantic deadline expires. This is a concrete condition in the current implementation, requiring a regression fixture.
- The service could buffer a large argument before transmitting it. This would create genuine transport silence while generation continues. Public evidence does not establish that this happened in the failed Antigravity requests.

The secondary model's ten-minute generation makes a substantial output workload plausible. It does not prove that Gemini was generating the same result or would eventually finish. Raising every timeout would also permit genuine stalls to last longer.

The next implementation should reuse the existing request telemetry to record aggregate bytes, complete events, buffered frame size, reasoning character count and last transport/parsed-progress timings on failures. Do not store raw prompts, arguments, signatures or credentials. Add a controlled SSE fixture that delivers one valid `functionCall` over more than 300 seconds while bytes continue arriving; confirmed tools must still execute only after terminal validation. Compare with OpenCode's separate header and body-read deadlines.

For large artifacts, evaluate building a useful initial structure and applying focused patches. Reuse templates already present in the project. This reduces individual buffered generations and preserves completed work instead of repeatedly asking for one complete, large file. Measure total requests and delivery time: smaller calls are not automatically faster if they repeatedly replan the entire artifact. Do not impose an arbitrary output cap or silently reduce requested quality.

## Compatibility improvements with independent evidence

### Signed replay through Cloud Code Assist

Jarvis reconstructs some function calls with `skip_thought_signature_validator`. OMP removed this sentinel from optional unsigned secondary calls in CCA/Antigravity parallel groups after reproducing HTTP 400 `INVALID_ARGUMENT`. CCA still requires the bypass when the first call itself is unsigned; this differs from both the public Gemini API and Vertex. Normal Jarvis replay preserves original parts, so the compatibility concern is primarily reconstructed history after interruption, model changes or compaction.

Preserve actual signatures where received and test unsigned/parallel calls with the CCA-specific first-call rule, including later function-call groups. This is not an explanation of the present HTTP-200 timeout incident. Source: [OMP PR 10368](https://github.com/can1357/oh-my-pi/pull/10368), local `google-shared.ts`.

### Completion containing only reasoning

Jarvis currently classifies a `STOP` with only thought content as `provider_protocol`, which enters generic inference retry. OMP distinguishes this terminal state and routes it to final-output recovery. An explicit request to deliver the result is more useful than replaying an identical generation without accounting for why it ended.

Add fixtures for thought-only `STOP`, empty streams and complete function calls so recovery preserves intent and completed receipts. The six requests in the audited incident ended in timeout, not `STOP`; this is a separate integration improvement. References: `antigravity.rs` (`Output::finish`) and OMP `google-gemini-cli.ts` (thought-only finish handling).

### Model-specific thinking configuration

Gemini 3.8 Flash supports `low`, `medium` and `high`; `minimal` is unsupported. The model-specific documentation supersedes generic Gemini 3 guidance. Jarvis's normalized tiers carry thinking-mode metadata, but a bare 3.8 model without that metadata falls through older model-name heuristics to `thinkingBudget`. Cover catalog and metadata-missing routes with fixtures, ensuring the wire route and thinking configuration agree.

The effective route metadata of the live request could not be retrieved, so this fallback cannot be attributed to the incident. Keep the user's selected `high`. A separately selected `low` may reduce latency for simple work, but is a user-visible tradeoff, not a repair to silently apply. Sources: [Gemini 3.8 Flash](https://ai.google.dev/gemini-api/docs/models/gemini-3.8-flash), [OMP PR 12992](https://github.com/can1357/oh-my-pi/pull/12992).

### Tool schema constraints

Jarvis's Gemini schema projection removes some unsupported constraints without retaining their meaning in descriptions. OMP preserves the removed constraints as descriptive guidance. This can reduce avoidable invalid-argument round trips while leaving local validation authoritative. It is a lower-priority improvement here: the initial 13 calls completed quickly and there were no confirmed argument-validation failures explaining the stalled step. Compare `antigravity.rs` (`schema`) with OMP `utils/schema/normalize.ts` (`spillToDescription`).

## Changes not supported by this incident

- Do not introduce a thinking-loop detector as this incident's fix. The six partial summaries contain 16–23 paragraphs per attempt, no exact paragraph duplicates and no headings. They do repeat the task between failed requests, as expected. No sufficient evidence demonstrates a degenerate loop within a single request. OMP's detector is calibrated; its open structured-output false-positive fix also shows how an aggressive guard can increase retries. Preserve the incident as a negative fixture if loop protection is added later. Sources: [OMP PR 11132](https://github.com/can1357/oh-my-pi/pull/11132), local `thinking-loop.ts`.
- Do not invent an incremental function-argument flag for Antigravity. The official SDK's `streamFunctionCallArguments`, `partialArgs` and `willContinue` fields are not supported by the Gemini Developer API; incremental Interactions or Vertex examples do not establish CCA support. OMP's CCA integration does not enable them. Sources: [official SDK types](https://github.com/googleapis/js-genai/blob/main/src/types.ts#L3012), [function-calling guide](https://ai.google.dev/gemini-api/docs/function-calling).
- Do not reduce `maxOutputTokens` to force faster thinking. Thinking and visible output share the budget; reducing it can truncate the useful result. The current 65,536 output setting and `VALIDATED` tool mode match the OMP/Antigravity request profile. Source: [Google thinking token limits](https://ai.google.dev/gemini-api/docs/thinking#token-limits-and-max_output_tokens).
- Do not add a new cache subsystem for this case. Prefix stability is already implemented and the measured cache read was substantial. Public Gemini explicit-cache APIs do not establish equivalent support on CCA. Source: [Google context caching](https://ai.google.dev/gemini-api/docs/caching).

## Priority and verification

First verify the long-event condition and record transport-versus-parsed progress using the existing telemetry. Next evaluate incremental artifact delivery against this task, measuring completion time, retries and successful retained patches. Fix the replay and terminal-response compatibility gaps independently with fixtures. Cover missing model metadata and schema descriptions without claiming that either caused this incident.

Codex remains the primary harness reference for completed-result preservation and bounded recovery. OpenCode contributes the transport deadline distinction. OMP contributes the provider-specific envelope, thinking and replay contracts. No live provider benchmark, credential change, interruption of the user's task, commit or push was performed for this review.

## Implementation status

The subsequent implementation renews the Antigravity read deadline while unfinished SSE event data grows. Comments and completed empty events do not extend the semantic deadline, and tools execute only after complete output validation. The shared parser scans newly received bytes instead of repeatedly scanning an existing incomplete line. Existing startup/silence deadlines, cancellation and size bounds remain enforced.

Aggregate stream telemetry records bytes, completed events, buffered bytes, reasoning character count and transport/parsed-progress idle durations. Model names are hashed; prompt, argument and signature content is excluded. Diagnostic counters do not add another provider request to reports.

Replay now applies CCA's first-unsigned-call signature rule to reconstructed groups and preserves real signatures. Unsigned Gemini thought text becomes clearly identified prior-summary prose; unsigned Claude thought content is omitted. Gemini 3.8 fallback metadata uses thinking levels, preserves explicitly disabled thinking and retains the user's selected reasoning level. Removed schema constraints remain descriptive guidance while local validation remains authoritative.

A completed Antigravity thought-only STOP is journaled with its signature and usage before one final-output continuation. Another thought-only completion in the same running turn invocation exhausts this recovery and activates the configured secondary model. The continuation guard resets when the invocation restarts; journal contents and completed tool receipts survive reload. Empty or truncated streams remain protocol errors. Shared workflow guidance keeps the requested scope proportional and encourages useful initial artifacts followed by focused edits for substantial output.

Controlled HTTP/SSE fixtures cover a single function-call event delivered over 600 virtual seconds, silence, empty keepalives and cancellation during incomplete tool arguments. Recovery tests cover signed continuation and secondary-model reload preserving an initial write before a focused edit. Replay tests cover reconstructed parallel groups and model changes. Telemetry tests cover serialization, content exclusion, large counters and report accounting; receive fixtures do not directly assert emitted diagnostic counters.

Validation passed: `CI=true bun run check` (916 frontend tests passed, one skipped, plus lint, typecheck, extension/app builds and IPC verification), `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, and `RUST_TEST_THREADS=2 cargo test` (1,139 passed, 24 ignored). The 25 focused Antigravity tests also passed. A preceding parallel Rust run failed the existing Beads cleanup-lock test after releasing its lock; that test passed in isolation and in the final full run. Its intermittent cause was not established, and no Beads runtime change was made.

No live Antigravity retest was performed. These changes address reproducible integration failures and workload amplification; they do not establish how much upstream generation latency will improve. The installed 1.8.0 application must be rebuilt or updated before it can exercise this implementation.

## Completed planned-flow audit (2026-09-30)

The completed Salesforce conversation has a recorded root duration of 6,700,375 ms: 1h51m40s. Both final journals and workflow state confirm completion. This is the original 1.8.0 execution, with Claude Opus/max coordinating, Antigravity followed by the configured Codex secondary for Designer, and Claude Sonnet/max for Builder. The subsequent local fixes were not running in that conversation.

| Phase or measurement | Recorded duration |
| --- | --- |
| Designer initial delivery | 59m44.898s |
| Designer correction round | 16m56.485s |
| Total Designer active time across those rounds | 76m41.383s |
| Builder initial validation | 14m58.779s |
| Builder revalidation | 14m26.731s |
| Parent inference/CLI steps | Approximately 6 minutes |

These measurements occupy the same coordinated wall-clock timeline. Parent `hub_wait` time already contains child execution and must not be added to child durations. Some request, tool and coordination intervals overlap; this table is not an additive cost model.

Designer made four successful Antigravity requests, then six unsuccessful requests consuming 39m50s plus approximately 60s of backoff. After fallback, all sixteen recorded secondary requests completed, totaling approximately 34m10s before handlers. No further Designer timeout was recorded. The initial patch request took 10m15s; the correction patch request took 8m28s. The first patch added approximately 47 KB of HTML; the later correction contained 23 hunks and approximately 31 KB of patch text. Designer's 56 tools consumed 43.163s in total, including 34.216s requesting guidance.

Builder's two rounds ran 23 shell commands in 2.790s combined. The lengthy intervals were model/CLI requests producing scripts, reports and handoffs, rather than execution of the checks themselves. The second round recovered a Python formatting error and an overlong evidence array; both handlers ran quickly. Generating and correcting the rejected handoff consumed about two minutes. Claude journal usage totals were not treated as independently verified generated-token counts.

Scope expansion materially increased the work: the 6,216-character dispatch required seven sections, fourteen commands with copy buttons, fixed navigation, a timeline, a diagram and extensive static checks. The user did explicitly request Builder validation during execution. Eleven correction items were then assigned and all seven original checks were repeated alongside those corrections. Two fidelity corrections repaired inaccuracies introduced by the original brief. Those requested corrections should be completed, but the original suggestions should not have been promoted into extra product requirements. The proportionality and incremental-delivery guidance now addresses this across all providers; it does not change user-selected reasoning or remove required checks.

An additional integration flaw was reproduced against the installed Open Design index: generic `html`, `index`, `preview` and `real` overlap selected WebGL and worker-visualization templates. The initial journal records 3,761 injected reference characters, despite no request for either subject. Preparation took only milliseconds; there is no evidence that these references caused the provider timeout. Automatic template/system qualification now requires the topic of the resource name or slug to be present, while craft guidance, project identity and explicit search/read access remain available. This deliberately conservative lexical qualification uses explicit discovery for aliases and translated template names. Regressions reproduce the Salesforce selection, an explicit WebGL request and an email restriction that must not select email marketing.

Reference decisions: Codex `ext/skills/src/catalog_prompt.rs` and `core/src/skills/render.rs` limit catalog context and load selected skill content progressively; OpenCode `tool/skill.ts` and `tool/skill.txt` load the named skill; OMP `extensibility/skills.ts` preserves the origin of selected/autoloaded skill content. Jarvis keeps native preparation while requiring meaningful relevance for specialized examples. No live benchmark or provider request was performed during this completed-flow audit.

The additional qualification fix passed all eight design-preparation tests, `CI=true bun run check` (916 passed, one skipped), Rust formatting and Clippy with warnings denied, and the complete Rust suite with two test threads (1,142 passed, 24 ignored). Native-provider and Claude execution paths recompute preparation through the same helper. This verification establishes reference selection and existing behavior; live latency improvement remains unmeasured.
