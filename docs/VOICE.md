# Jarvis Voice

Voice is a local input/output adapter around existing conversations. The selected provider, model, agent/flow, project scope, tools, permissions and history continue through the normal chat pipeline.

## Using voice

1. Open **Configurações → Voz** (or click a microphone/phone control before setup).
2. Enable Jarvis Voice, choose the input/output devices, and download a transcription model. **Small** favors Portuguese accuracy; **Tiny** favors latency on slower CPUs.
3. Prepare the existing **Audiovisual** component in **Ferramentas** for spoken replies. It contains the private Kokoro runtime; no system Python installation or cloud speech key is required.
4. Click the microphone in a chat to dictate, then click it again to finish. The transcript joins the existing draft and its attachments. `Ctrl/Cmd + Shift + Space` toggles dictation while the conversation is focused.
5. Click the phone to start a call. The Jarvito island also has a phone control. Speech is sent after a configurable pause; final visible replies are read aloud. The call view shows subtitles, duration, audio level and the character's listening/speaking expressions. **Mostrar conversa** reveals the same chat.
6. Use the microphone control to pause, resume or interrupt a spoken reply; the hang-up control (or Escape in the conversation) closes voice. Ending voice does not cancel an already submitted agent request.

Calls can answer agent questions by option number/label or free text. Project handoff uses the existing explicit scope confirmation, including a spoken confirmation, and retargets the same call to the confirmed conversation. Tool, Git and authoring approvals retain their original decision controls.

## Audio and lifecycle

- CPAL 0.17 captures the selected device in a bounded queue. Stereo input is mixed to mono and resampled continuously to 16 kHz. Driver disconnects/overruns fail visibly without sending incomplete speech.
- whisper-rs 0.16 wraps bundled whisper.cpp. PT-BR transcription uses quantized multilingual Small/Tiny; macOS uses Metal with CPU fallback. Silero VAD uses a bounded rolling window, pre-roll and a calibrated silence threshold.
- Kokoro ONNX reuses the existing managed Audiovisual installation in its private Python process. The worker stays warm for the call, synthesizes bounded phrases and uses PT-BR voices Alex, Dora or Santa. Lead/tail padding preserves speech edges.
- Rodio 0.22 plays decoded speech. Output RMS drives the Rive mouth; microphone RMS drives the listening meter. Reduced motion is honored.
- A single native owner controls the microphone across desktop/island webviews. Session IDs, targets, monotonically increasing revisions and transcript sequences reject stale or duplicate events. Cancellation stops synthesis/playback and releases capture resources; it never rolls back completed agent actions.
- Microphone activation is explicit. Reopening Jarvis does not restore an open mic. Voice continues while the island is collapsed, but unmounting its owning desktop conversation ends that call. Destroying the owning window (including disabling Jarvito), app exit and updating also shut voice down.
- Raw microphone audio is not written to history or logs. Only recognized text enters the provider's normal conversation. Synthesized WAVs live in a private temporary directory removed when the worker exits. Models are downloaded only on request, pinned to revisions and verified by size/SHA-256 before publication; interrupted transfers do not replace installed models.

## Deliberate current boundaries

Calls use half-duplex audio to avoid feeding speaker output back into the recognizer: tap the microphone to interrupt. Wake words, acoustic echo cancellation and automatic speech barge-in are not enabled. Speech synthesis starts after the final visible reply, excluding fenced code, hidden reasoning and URL targets. Voice is a desktop feature; the mobile remote UI remains text-based.

## Packaging and validation

Linux builds require `cmake` and `libasound2-dev` in addition to existing Tauri prerequisites; both native CI workflows install them. Windows uses WASAPI, macOS uses Core Audio. macOS requires 10.15 or later for Whisper's C++ filesystem APIs; packages merge the microphone usage description and sign with the audio-input hardened-runtime entitlement.

Behavioral frontend tests cover explicit activation, setup, model download/cancellation, stale events, dictation recovery, question answers, handoff and spoken reply deduplication. Native tests cover calibration, resampling, endpointing and prose segmentation. The Rive test loads the real WASM animator and checks the audio-driven mouth.

For an opt-in real engine smoke without opening a microphone:

```sh
<managed-python> scripts/validate-voice.py <audiovisual-runtime-root> <temporary-smoke-directory>
# Place the pinned Tiny and Silero model files from voice/models.rs in that directory.
cd src-tauri
JARVIS_VOICE_SMOKE_DIR=<temporary-smoke-directory> cargo test voice::worker::tests::real_portuguese_transcription_and_voice_activity -- --ignored --nocapture
```

Physical microphone/output permissions, Bluetooth hotplug and native Windows/Linux audio require hardware UAT; passing the synthetic fixture does not certify those paths.
