# Jarvito notification voice

Place prerecorded WAV clips in this folder before building Jarvis. They are bundled
with the desktop app and played locally, without a microphone, a model download or
a voice service. Missing categories keep the existing local text-to-speech fallback.

Use these file names; each category accepts up to eight optional variants:

| Event | Files | Example script in pt-BR |
| --- | --- | --- |
| Task completed | `completed-1.wav` through `completed-8.wav` | “Pronto! Mais uma tarefa concluída. O resultado está aqui para você conferir.” |
| Task failed | `failed-1.wav` through `failed-8.wav` | “Opa, tivemos um problema em uma tarefa. Melhor você dar uma olhada.” |
| Question waiting | `question-1.wav` through `question-8.wav` | “Preciso da sua ajuda para continuar. Tem uma pergunta esperando por você.” |
| Approval waiting | `approval-1.wav` through `approval-8.wav` | “Uma ação está esperando sua aprovação. Pode conferir comigo?” |

Export standard PCM WAV audio (16-bit, mono or stereo). Use short, natural phrases
without task names: the notification card identifies the actual task. Clips are
selected from the available variants of the matching category. The island opens
only after audio preparation and remains visible during playback.

Recordings placed here are included by the next build without conversion. Only
present variants are selected; missing numbers are optional. Speech mute applies
to both prerecorded clips and local synthesis. Reconnection does not trigger speech.
