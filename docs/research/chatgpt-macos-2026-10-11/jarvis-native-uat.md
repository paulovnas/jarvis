# Jarvis native UAT after the REA implementation

Started on 2026-10-11; continued on 2026-10-12 (America/Sao_Paulo).
Tracking: `jarvis-6kdm`. Implementation: [REA changes and automated validation](jarvis-implementation.md).

## Build identity

The optimized production app was built from the current working tree with
`bun run tauri build --bundles app`. The build finished successfully, including
the main renderer, Chrome/Firefox extensions, Rust release binary, CEF helpers
and the signed native bundle. Production storage was used so the already
configured provider account could be tested through the real application.

The intact bundle was moved to:

`/Users/enesolucoes/Downloads/Jarvis-REA-UAT-2026-10-11/Jarvis.app`

- App version: `1.9.9`; bundle identifier: `com.foxtag.jarvis`; architecture: Apple Silicon.
- Bundle size: 447 MiB.
- `codesign --verify --deep --strict --verbose=2` passed after the move.
- Main binary SHA-256: `92029b17f08f115b48865cc958310ea6acd16451db7ed67514f44e835f7801e5`.
- Application-source fingerprint: `edf9fbaad084f3647c047dd69c114637dc943c7bf8e1617931b320c923a36f71` over 886 non-test source/configuration files, unchanged before and after the build.
- Launched process PID `21408` matched the executable inside this exact Downloads bundle.

Release build artifacts were cleaned after moving and verifying the bundle,
before launching Jarvis, because the volume had less than 15% free space.
The clean removed 3.7 GiB of rebuildable artifacts; the Downloads app remained intact.

## Native conversation

A new conversation was created in `Pessoal / jarvis` through the native UI.
Its automatic title is **Organização de biblioteca fictícia**.

- Project ID: `0c2c9ab33978baf02bc46fd23b10774d`.
- Conversation ID: `783f02fe4ea3f8e380acdb0125f6f7ec`.
- Journal: `/Users/enesolucoes/.jarvis/sessions/0c2c9ab33978baf02bc46fd23b10774d/783f02fe4ea3f8e380acdb0125f6f7ec.jsonl`.
- Initial account/model: `ene / gpt-6.1-sol`, maximum reasoning, default workflow.
- Test messages requested fictitious output and explicitly prohibited tools, file reads and project changes.

### Observed results

| Case | Native observation | Durable evidence |
| --- | --- | --- |
| A: Long structured response | Connecting/running state, live reasoning summaries, expandable processing details, completed response and copy/export controls. The accessibility tree contains twelve numbered headings, a three-column table with five data rows, a Rust code block and a nested list. | Turn `ba0332dca6d650995615e5b66e69dc6f` is `completed`, duration 190389 ms; final response contains 7314 UTF-16 units, begins with `UAT-REA-A-INICIO` and ends exactly with `UAT-REA-A-FIM 🧪 ação concluída.`. No tool calls or turn error. |
| B: Continuation with unchanged account/model/reasoning | The response correctly recalls **Biblioteca das Sete Luas** and the table columns **Categoria fictícia, Etiqueta, Estante**; `UAT-REA-B-FIM` appears in the UI. | Turn `d519b18c9db6a340b70fe2382fd05dcf` is `completed`, duration 20808 ms, with the exact recalled facts and marker; no tool calls or turn error. |
| Reading older content during B | Scrolling up showed the prior response's sections/list and then its complete table. B completed without forcing the viewport to the latest turn; an unread indicator appeared. | Native screenshots and accessibility observations in this task; the journal retains A and B. |
| C: Cancel a response after text arrives | The live response contained the opening marker and a growing numbered list. Clicking **Interromper execução** removed the running controls and retained forty full paragraphs plus a partial paragraph, with an interrupted status. | Turn `e79e8e16aa8c65e97651c6c73b528331` is `cancelled`, duration 134401 ms, with 10240 UTF-16 units retained. Its only error is the expected `cancelled` / `Execução interrompida.`; the requested final marker is absent. |
| D: Send after cancellation | The composer accepted another message immediately; the next reply contains only `UAT-REA-D-RECUPERADO 🧪.` and does not continue the cancelled list. | Turn `72bf5ba798d6c3337553aac3cd8da4a8` is `completed`, duration 9109 ms, with no tool calls or turn error. |
| E: Navigate away while running and return after completion | A second blank test conversation was created while E ran. The original conversation retained its running indicator, then an unread indicator. Returning showed the eight-paragraph response and its final marker, without losing the cancelled C or completed D. | Turn `f63be36f19ec00909e0d96ee565b9805` is `completed`, duration 36475 ms, with 2610 UTF-16 units and intact `UAT-REA-E-INICIO` / `UAT-REA-E-FIM`; no tool calls or turn error. |
| Export the canonical text | **Salvar resposta em Markdown** opened the native save dialog and reported success after saving E beside the Downloads app. | `/Users/enesolucoes/Downloads/Jarvis-REA-UAT-2026-10-11/UAT-REA-E.md` is byte-for-byte equal to E's canonical response. SHA-256: `1bbe318a693de829c179e95cfd1d154e5d4f22f12ceccc0502133f14be132352`. |
| Native virtualization | At the latest response, the accessibility tree contains C, D and E, while A and B are unmounted. Scrolling to the beginning restores A, B and C, including A's complete Markdown and final marker; D and E leave the mounted window. | Native accessibility observations demonstrate a three-turn mounted window over five durable turns. The test establishes reconstruction and omission of offscreen rows; it does not measure an FPS improvement. |
| Preserve an offscreen selection | Dragging over **A Biblioteca das Sete Luas,** selected the text visibly. Scrolling to the latest response retained both that selected text and A's mounted row alongside C, D and E; B remained unmounted. Focusing the empty composer cleared the selection. | Accessibility selection text remained exactly `A Biblioteca das Sete Luas,` after the scroll; the protected row was retained outside the ordinary window. |
| Clean restart and history recovery | The app's normal exit confirmation was accepted after all five turns were terminal. The same Downloads app was opened again; `Pessoal / jarvis`, the selected conversation, E's complete response, D and C's interrupted result were restored. The original maximum-reasoning setting had been restored before exit and was present after restart. | PID `21408` stopped. Diagnostics recorded `clean_shutdown` for run `3403fd12209aa7a0bcd5e9d7aa951f98`. New PID `55618` again matched the exact Downloads binary, with startup run `1f25d051cb481b464de30e78d1be5fc8`. |
| F: Real provider request after restart | The restarted app accepted another message and correctly recalled **Biblioteca das Sete Luas**, followed by `UAT-REA-F-REINICIO-OK`. | Turn `3c03bdf08b371a0f3db78fdf7ff2c4fd` is `completed`, duration 14596 ms, with 46 UTF-16 units, no tool calls and no turn error. |

For C, D and E, the same account/model used **low** reasoning through the normal
per-conversation model selector. The original **maximum** setting was restored
before the restart. The auxiliary blank conversation has no provider turn;
it was retained as a navigation-test artifact.

The journal has six unique turns: five completed and one deliberately cancelled.
There are zero unexpected turn errors and zero tool calls across the six turns.
The diagnostic events for both identified native runs contain startup and a
normal shutdown, with zero error-level records at the final audit. This count
only describes the emitted diagnostic records; it is not proof that every
possible console warning was captured.

### Automation window selection

After the first response, the automation selected the auxiliary **Jarvito**
panel instead of the main **Jarvis** window. Finder reopening, menu actions,
foregrounding the main window and resetting the automation did not change
the selected window. The user temporarily disabled Jarvito; the next app
selection immediately exposed the main window and the full response.
This is an observed automation selection limitation. It does not establish
that the main chat failed, and the completed journal and subsequently visible
response confirm that A was retained. A native restart was not confirmed by
these unsuccessful reopening attempts.

After all six turn checks, **Assistente na área de trabalho** was re-enabled
in Settings. The native screenshot showed its switch changing from off to on.
Settings was closed and the main window again exposed the tested conversation,
including the completed post-restart response. This restores the original
Jarvito preference. The Settings content rendered visually, but its accessibility
tree exposed only the native window frame; the switch interaction was grounded
in screenshots.

## Evidence boundaries

This report records real provider calls and native UI behavior. It does not
derive a CPU/FPS/latency improvement percentage from a functional smoke test.
The first turn's 190389 ms includes maximum provider reasoning and generation;
it is not a renderer benchmark. Same-configuration continuation alone does not
prove a reused physical WebSocket, particularly after the automation delay.
The transport-reuse assertions and fault cases remain separately documented
in the implementation's automated tests.

The implementation's frontend and Rust quality gates were already green before
this UAT. No application source changed between those gates and the fresh
native release build. The native build and the real scenarios above add
functional runtime evidence; no additional implementation repair was needed.

No live CPU/FPS benchmark, authenticated network packet/socket instrumentation,
provider-token revocation/401 injection, IPC overflow injection or very large
history stress run was performed. Direct companion-chat streaming was not part
of these main-chat scenarios. The independent native ChatGPT-versus-Jarvis
runtime comparison remains open as `jarvis-bg4r`.

No implementation source was changed during these observations. No commit,
push, publication or Dolt remote sync was performed.
