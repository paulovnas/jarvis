# Jarvis — Pipeline de Geração de Apresentações Audiovisuais

> Documento de contexto técnico para o Codex, responsável atualmente pela manutenção e evolução do Jarvis.

## 1. Contexto

O Jarvis é uma ferramenta de codificação agêntica. A intenção desta integração é adicionar ao Jarvis a capacidade de gerar **vídeos completos de apresentação de projetos**, incluindo:

- composição visual;
- animações;
- capturas/telas do projeto;
- narração em português brasileiro;
- música de fundo;
- eventualmente efeitos sonoros;
- sincronização entre vídeo, voz e música;
- render final em MP4.

O caso de uso é **exclusivamente apresentação de projetos/produtos**, e não edição de vídeo genérica.

Exemplo de solicitação futura ao Jarvis:

```text
Crie um vídeo de aproximadamente 45 segundos apresentando o projeto Jarvis.

Use uma estética dark baseada em One Dark.
Apresente brevemente o problema, a solução e os principais recursos.
Use uma narração masculina, calma e tecnológica em PT-BR.
Adicione uma trilha instrumental futurista discreta.
Finalize com a logo do Jarvis e uma chamada curta.
```

A expectativa é que o agente seja capaz de transformar isso em um vídeo final praticamente sem intervenção manual.

---

## 2. Ponto de partida: HyperFrames

Repositório:

https://github.com/heygen-com/hyperframes

O HyperFrames é um framework open source voltado para agentes:

```text
HTML/CSS/Media/Animations
          ↓
     HyperFrames
          ↓
         MP4
```

Ele fornece:

- CLI;
- skills para agentes;
- criação de composições;
- preview;
- validação;
- render;
- manipulação de mídia;
- timeline;
- áudio;
- integração com workflows de agentes.

Exemplo básico:

```bash
npx hyperframes init my-video
cd my-video

npx hyperframes preview
npx hyperframes render --output output.mp4
```

As skills podem ser instaladas/atualizadas com:

```bash
npx hyperframes skills update
```

ou:

```bash
npx skills add heygen-com/hyperframes
```

O HyperFrames deve ser tratado como a **engine principal de composição e renderização audiovisual**.

---

# 3. Desenho inicialmente imaginado

A ideia original discutida foi separar cada responsabilidade em uma engine especializada:

```text
                      ┌────────────────────┐
                      │    Agente / LLM    │
                      │ GPT / Claude / etc │
                      └─────────┬──────────┘
                                │
                    roteiro + planejamento
                                │
               ┌────────────────┼────────────────┐
               │                │                │
               ▼                ▼                ▼
          HyperFrames       Chatterbox       ACE-Step
             vídeo             voz             música
               │                │                │
               └────────────────┼────────────────┘
                                ▼
                         FFmpeg / HyperFrames
                                │
                                ▼
                        presentation.mp4
```

Nesse desenho:

### HyperFrames

Responsável por:

- cenas;
- layout;
- animações;
- screenshots;
- elementos gráficos;
- timeline;
- composição;
- render final.

### Chatterbox

Responsável por:

```text
texto → voz
```

Principal candidato:

https://github.com/resemble-ai/chatterbox

O Chatterbox Multilingual V3 suporta português e atualmente também possui um modelo dedicado a português brasileiro:

```text
ResembleAI/Chatterbox-Multilingual-pt-br
```

Pode ser usado para:

- narração;
- identidade vocal fixa;
- voice cloning com áudio de referência;
- controle de expressividade.

### ACE-Step

Responsável por:

```text
prompt → música
```

Principal candidato:

https://github.com/ace-step/ACE-Step-1.5

Possui:

- geração local;
- CLI;
- servidor HTTP;
- API assíncrona;
- geração instrumental;
- controle por prompt.

Para o Jarvis, o uso esperado seria principalmente:

```text
background music / soundtrack
```

e não geração de músicas com vocal.

---

# 4. Importante: não é necessário rodar outro LLM local

O LLM que controla o Jarvis continua sendo responsável apenas por **planejamento e orquestração**.

Por exemplo:

```text
GPT / Claude / Gemini / outro modelo
                  │
                  ▼
       "o que precisa ser criado?"
                  │
          ┌───────┼─────────┐
          ▼       ▼         ▼
        vídeo    voz      música
```

Não é o LLM que sintetiza a voz ou gera a música.

São modelos especializados:

```text
LLM
│
├── escreve roteiro
├── cria storyboard
├── define prompts
├── define timing
└── chama ferramentas
        │
        ├── HyperFrames
        ├── TTS
        └── Music Generator
```

Portanto:

```text
Ollama / LM Studio / LLM local adicional
```

**não são requisitos dessa arquitetura.**

Os modelos de áudio podem rodar localmente, mas eles não são LLMs de conversação.

---

# 5. Atualização importante após verificar o HyperFrames atual

A versão atual do HyperFrames já cobre parte considerável dessa arquitetura.

Ele possui uma skill/camada chamada:

```text
/media-use
```

que funciona como uma espécie de "Media OS".

Atualmente ela consegue resolver:

- voice / TTS;
- BGM;
- SFX;
- imagens;
- ícones;
- transcrição;
- captions;
- outros assets.

Além disso, existe:

```text
/hyperframes-audio
```

para mixagem do áudio já colocado na composição.

Isso significa que o Jarvis **não precisa começar implementando Chatterbox + ACE-Step imediatamente**.

Para o MVP, podemos explorar o pipeline nativo do próprio HyperFrames.

---

# 6. Providers atuais do HyperFrames

Segundo a documentação atual do HyperFrames:

## Voz

Ordem de providers:

```text
HeyGen
  ↓
ElevenLabs
  ↓
Kokoro local
```

Quando utilizado localmente:

```text
Kokoro-82M
```

é o fallback offline.

O CLI também possui:

```bash
npx hyperframes tts <script>
```

Exemplo:

```bash
npx hyperframes tts script.txt \
  --voice <voice-id> \
  --output narration.wav
```

Observação:

O comando `hyperframes tts` é especificamente o caminho local baseado em Kokoro.

Os providers HeyGen/ElevenLabs são utilizados pelos helpers/workflows de mídia.

---

## Música

A resolução atual de BGM segue aproximadamente:

```text
HeyGen music library
        ↓
      Lyria
        ↓
 MusicGen local
```

O fallback local documentado é:

```text
facebook/musicgen-small
```

Portanto, o HyperFrames já consegue trabalhar sem serviço externo depois que os modelos necessários forem baixados.

---

## SFX

A stack também suporta efeitos sonoros usando:

```text
HeyGen library
      ↓
bundled SFX library
```

Para o caso do Jarvis, SFX deve ser considerado **opcional no primeiro momento**.

Vídeos de apresentação de software normalmente precisam prioritariamente de:

1. voz;
2. música;
3. vídeo.

---

# 7. Arquitetura recomendada para o Jarvis — MVP

Antes de criar novas integrações, testar:

```text
                 Jarvis Agent
                      │
                      ▼
               /hyperframes
                      │
             ┌────────┴────────┐
             │                 │
             ▼                 ▼
       HyperFrames          /media-use
          visual               │
                               ├── TTS
                               │    └── Kokoro local
                               │
                               ├── BGM
                               │    └── MusicGen local
                               │
                               └── SFX
                                    └── library
             │
             └─────────┬─────────────┘
                       ▼
              /hyperframes-audio
                       │
                       ▼
                 final render
                       │
                       ▼
             presentation.mp4
```

Isso reduz muito a quantidade de infraestrutura que o Jarvis precisa manter.

---

# 8. Arquitetura recomendada — evolução

Se a qualidade do Kokoro ou MusicGen não for suficiente para apresentações profissionais, trocar apenas os generators.

```text
                 Jarvis Agent
                      │
                      ▼
            Presentation Planner
                      │
          ┌───────────┼───────────┐
          │           │           │
          ▼           ▼           ▼
    HyperFrames   Chatterbox   ACE-Step
       vídeo        PT-BR        música
          │           │           │
          └───────────┼───────────┘
                      ▼
              HyperFrames Audio
                      │
                      ▼
                FFmpeg/render
                      │
                      ▼
             presentation.mp4
```

A camada superior não precisa saber qual engine está sendo utilizada.

Idealmente:

```text
VoiceProvider
├── hyperframes-kokoro
└── chatterbox

MusicProvider
├── hyperframes-musicgen
└── ace-step
```

Assim o Jarvis não fica acoplado a um modelo específico.

---

# 9. Skills sugeridas no Jarvis

Do ponto de vista do agente, a interface deveria ser simples.

Sugestão:

```text
skills/
├── presentation/
├── hyperframes/
├── narration/
└── music/
```

## presentation

Skill principal.

Responsável por:

- entender o projeto;
- coletar informações relevantes;
- criar roteiro;
- criar storyboard;
- definir timing;
- delegar vídeo;
- delegar voz;
- delegar música;
- sincronizar tudo;
- renderizar.

## hyperframes

Pode utilizar diretamente as skills oficiais do HyperFrames.

Responsável por:

- cenas;
- composição;
- animação;
- screenshot;
- layout;
- render.

## narration

Abstração sobre TTS.

Inicialmente:

```text
provider = hyperframes/kokoro
```

Posteriormente:

```text
provider = chatterbox
```

## music

Abstração para música.

Inicialmente:

```text
provider = hyperframes/musicgen
```

Posteriormente:

```text
provider = ace-step
```

---

# 10. Interface conceitual para o agente

O Jarvis deveria trabalhar com comandos/ferramentas de alto nível.

Exemplo:

```bash
jarvis presentation create ./project
```

Internamente:

```text
analyze project
      ↓
create presentation manifest
      ↓
generate narration
      ↓
generate soundtrack
      ↓
create HyperFrames composition
      ↓
mix
      ↓
validate
      ↓
render
```

Caso wrappers próprios sejam criados:

```bash
jarvis-narration generate \
  --script narration.txt \
  --language pt-BR \
  --voice jarvis \
  --output narration.wav
```

```bash
jarvis-music generate \
  --prompt "minimal futuristic software presentation, instrumental" \
  --duration 45 \
  --output soundtrack.wav
```

O agente não deve precisar conhecer detalhes internos dos modelos.

---

# 11. Manifesto intermediário

É recomendado que toda apresentação seja planejada antes da geração.

Exemplo:

```json
{
  "project": {
    "name": "Jarvis",
    "type": "software"
  },

  "presentation": {
    "duration": 45,
    "resolution": "1920x1080",
    "fps": 30
  },

  "voice": {
    "language": "pt-BR",
    "provider": "auto",
    "voice": "jarvis",
    "style": "calm, confident, technological"
  },

  "music": {
    "provider": "auto",
    "prompt": "minimal futuristic software presentation, dark technology, subtle synths, instrumental, no vocals",
    "volume": 0.16
  },

  "scenes": [
    {
      "id": "intro",
      "start": 0,
      "duration": 5,
      "narration": "Conheça o Jarvis.",
      "visual": "logo reveal"
    },

    {
      "id": "problem",
      "start": 5,
      "duration": 10,
      "narration": "Gerenciar agentes, ferramentas e fluxos de inteligência artificial rapidamente se torna complexo.",
      "visual": "show fragmented workflow"
    },

    {
      "id": "solution",
      "start": 15,
      "duration": 12,
      "narration": "O Jarvis centraliza esses recursos em uma única experiência.",
      "visual": "dashboard overview"
    },

    {
      "id": "features",
      "start": 27,
      "duration": 13,
      "narration": "Crie agentes, conecte ferramentas e automatize fluxos de desenvolvimento.",
      "visual": "feature montage"
    },

    {
      "id": "outro",
      "start": 40,
      "duration": 5,
      "narration": "Jarvis. Desenvolvimento assistido por inteligência artificial.",
      "visual": "logo outro"
    }
  ]
}
```

Nome sugerido:

```text
presentation.json
```

ou:

```text
presentation.yaml
```

---

# 12. Por que utilizar um manifesto

Sem manifesto:

```text
LLM
 ↓
faz vídeo
 ↓
faz áudio
 ↓
tenta encaixar
```

Isso tende a produzir sincronização ruim.

Com manifesto:

```text
LLM
 ↓
Timeline planejada
 ↓
┌───────────────────────────────┐
│ Scene 1  00:00 → 00:05       │
│ Scene 2  00:05 → 00:15       │
│ Scene 3  00:15 → 00:27       │
│ Scene 4  00:27 → 00:40       │
│ Scene 5  00:40 → 00:45       │
└───────────────────────────────┘
 ↓
Video + Voice + Music
```

Todos os generators trabalham sobre a mesma timeline.

---

# 13. Sincronização da narração

A narração não deve ser simplesmente um WAV único gerado no final.

Preferencialmente:

```text
Scene 01
"Conheça o Jarvis."
        ↓
scene-01.wav

Scene 02
"Gerenciar agentes..."
        ↓
scene-02.wav

Scene 03
"O Jarvis centraliza..."
        ↓
scene-03.wav
```

Estrutura:

```text
assets/
└── audio/
    ├── voice/
    │   ├── scene-01.wav
    │   ├── scene-02.wav
    │   └── scene-03.wav
    │
    └── music/
        └── soundtrack.wav
```

Vantagens:

- cenas podem mudar de duração;
- uma frase pode ser regenerada isoladamente;
- facilita sincronização;
- facilita retry;
- reduz custo de regeneração;
- permite trocar somente uma parte da narração.

---

# 14. Captions

O HyperFrames também oferece fluxo de:

```text
TTS
 ↓
transcription
 ↓
word timestamps
 ↓
captions
```

No caso do Kokoro:

```bash
npx hyperframes tts script.txt \
  --output narration.wav
```

e depois:

```bash
npx hyperframes transcribe narration.wav \
  --model small \
  --language pt
```

Importante:

Para PT-BR não utilizar modelos Whisper com sufixo `.en`.

Ou seja, evitar:

```bash
--model small.en
```

e utilizar:

```bash
--model small --language pt
```

---

# 15. Mixagem

A música deve funcionar como background.

Exemplo conceitual:

```text
VOICE
████████    █████████    ███████

MUSIC
█████████████████████████████████
```

Quando houver voz:

```text
music = baixo
```

Quando não houver voz:

```text
music = ligeiramente maior
```

Isso é chamado de:

```text
ducking
```

O HyperFrames possui recursos específicos para voiceover carve/ducking e processamento de áudio através da skill:

```text
/hyperframes-audio
```

A preferência deve ser utilizar essa camada em vez de implementar manualmente mixagem com FFmpeg no primeiro momento.

---

# 16. Identidade audiovisual do Jarvis

Como os vídeos serão sempre apresentações de projetos, podemos criar presets consistentes.

Exemplo:

```yaml
presentation_style:
  visual:
    theme: one-dark
    mood: precision-industrial
    transitions: subtle
    typography: technical-modern

  voice:
    language: pt-BR
    gender: male
    tone: calm
    pace: medium
    personality:
      - confident
      - technological
      - restrained

  music:
    style:
      - futuristic
      - minimal
      - technological
      - cinematic
    vocals: false
    intensity: low
    target_volume: 0.16
```

Isso permitirá que diferentes projetos gerem vídeos com uma identidade consistente.

---

# 17. Provider abstraction

Não acoplar o Jarvis diretamente a:

```text
Kokoro
MusicGen
Chatterbox
ACE-Step
```

Criar interfaces.

Exemplo conceitual em TypeScript:

```ts
interface NarrationProvider {
  generate(input: {
    text: string;
    language: string;
    voice?: string;
    output: string;
  }): Promise<NarrationResult>;
}

interface MusicProvider {
  generate(input: {
    prompt: string;
    duration: number;
    output: string;
  }): Promise<MusicResult>;
}
```

Implementações futuras:

```text
NarrationProvider
├── HyperFramesKokoroProvider
├── ChatterboxProvider
├── ElevenLabsProvider
└── HeyGenProvider
```

```text
MusicProvider
├── HyperFramesMusicGenProvider
├── AceStepProvider
├── LyriaProvider
└── HeyGenMusicProvider
```

O planner utiliza somente:

```text
NarrationProvider
MusicProvider
```

---

# 18. Estratégia de provider

Sugestão:

```text
provider: auto
```

Resolver algo como:

## Narration

```text
1. provider explicitamente configurado
2. Chatterbox local, se instalado
3. HyperFrames/Kokoro local
4. serviço remoto configurado
```

## Music

```text
1. provider explicitamente configurado
2. ACE-Step local, se instalado
3. HyperFrames/MusicGen local
4. serviço remoto configurado
```

Para o MVP, isso pode começar muito mais simples:

```text
voice = hyperframes
music = hyperframes
```

---

# 19. Fases propostas

## Fase 1 — HyperFrames puro

Objetivo:

Validar o conceito com o menor número possível de componentes.

Implementar:

```text
Jarvis
 ↓
HyperFrames
 ↓
video + Kokoro + MusicGen
 ↓
MP4
```

Validar:

- PT-BR;
- qualidade da voz;
- música instrumental;
- sincronização;
- tempo de geração;
- consumo de VRAM/RAM;
- estabilidade;
- workflow do agente.

---

## Fase 2 — Presentation Skill

Criar:

```text
/presentation
```

Responsável por gerar:

```text
presentation.json
```

e coordenar:

```text
/hyperframes
/media-use
/hyperframes-audio
```

---

## Fase 3 — Chatterbox

Caso Kokoro não tenha qualidade suficiente:

```text
NarrationProvider
      ↓
Chatterbox PT-BR
```

Manter Kokoro como fallback.

---

## Fase 4 — ACE-Step

Caso MusicGen não entregue a qualidade desejada:

```text
MusicProvider
      ↓
ACE-Step
```

Manter MusicGen como fallback.

---

## Fase 5 — SFX

Somente depois do pipeline principal estar estável:

```text
SFX
```

Pode ser adicionado para:

- whoosh;
- clicks;
- transitions;
- logo reveal;
- UI feedback.

MMAudio/Foley generation não é prioridade para apresentações de software.

---

# 20. Estrutura possível dentro do Jarvis

Uma estrutura conceitual poderia ser:

```text
src/
└── presentation/
    ├── planner/
    │   ├── presentation-planner.ts
    │   └── schema.ts
    │
    ├── providers/
    │   ├── narration/
    │   │   ├── provider.ts
    │   │   ├── hyperframes-kokoro.ts
    │   │   └── chatterbox.ts
    │   │
    │   └── music/
    │       ├── provider.ts
    │       ├── hyperframes-musicgen.ts
    │       └── ace-step.ts
    │
    ├── hyperframes/
    │   ├── composition.ts
    │   └── renderer.ts
    │
    ├── audio/
    │   └── mixer.ts
    │
    └── presentation-service.ts
```

Isso é apenas uma referência arquitetural.

O Codex deve adaptar ao desenho atual do Jarvis, evitando introduzir uma arquitetura paralela desnecessária.

---

# 21. Workflow final esperado

A UX ideal:

```text
Usuário
  │
  ▼
"Crie uma apresentação deste projeto."
  │
  ▼
Jarvis
  │
  ├── analisa projeto
  ├── identifica features
  ├── identifica screenshots/assets
  ├── escreve roteiro
  ├── gera storyboard
  ├── gera presentation.json
  │
  ├── gera voz
  ├── gera música
  ├── gera composição HyperFrames
  ├── sincroniza
  ├── valida
  └── renderiza
       │
       ▼
presentation.mp4
```

---

# 22. Resultado conceitual

```text
00:00 ───────────────────────────────────────── 00:45

VIDEO
│ Intro │ Problem │ Solution │ Features │ Outro │

VOICE
│██████ │█████████│██████████│██████████│█████│

MUSIC
│██████████████████████████████████████████████│

SFX
│   ●       ●          ●      ●       ●        │
```

A música deve permanecer discreta.

A voz deve ser o elemento principal.

O vídeo deve acompanhar o ritmo da narração, e não o contrário.

---

# 23. Decisão técnica recomendada

Não começar construindo três integrações independentes.

Primeiro:

```text
HyperFrames
+
/media-use
+
/hyperframes-audio
```

O HyperFrames atual já possui infraestrutura de:

```text
vídeo
+
voz
+
música
+
SFX
+
mixagem
+
render
```

Testar essa solução integrada primeiro.

Somente substituir os generators quando houver um motivo mensurável:

```text
Kokoro     → Chatterbox
MusicGen   → ACE-Step
```

Isso mantém o Jarvis simples enquanto preserva uma rota clara para maior qualidade.

---

# 24. Decisão sobre execução local

O objetivo ideal é permitir:

```text
100% local
```

quando os modelos estiverem instalados.

MVP local:

```text
HyperFrames
├── Kokoro-82M
├── MusicGen
├── bundled SFX
└── FFmpeg
```

Evolução local:

```text
HyperFrames
├── Chatterbox PT-BR
├── ACE-Step
└── FFmpeg
```

O LLM que controla o Jarvis pode continuar sendo remoto ou local independentemente dessa decisão.

---

# 25. Pontos que o Codex deve investigar

Antes de implementar, verificar no ambiente atual do Jarvis:

1. Como as skills são registradas e executadas atualmente.
2. Se HyperFrames já está instalado ou deve ser uma dependência opcional.
3. Como o Jarvis executa subprocessos/CLI.
4. Como dependencies externas são detectadas.
5. Como downloads de modelos devem ser gerenciados.
6. Onde armazenar cache de modelos.
7. Como reportar progresso de geração.
8. Como cancelar uma geração.
9. Como armazenar projetos de vídeo.
10. Como abrir o preview do HyperFrames dentro do fluxo atual.
11. Se a GPU disponível deve ser detectada automaticamente.
12. Como implementar provider fallback.
13. Como persistir presets de voz/música.
14. Como manter `presentation.json` como fonte de verdade.
15. Como permitir regeneração isolada de uma cena.

---

# 26. Requisitos arquiteturais

A implementação deve favorecer:

- providers substituíveis;
- execução local;
- CLI/process isolation;
- manifesto determinístico;
- regeneração por cena;
- cache de assets;
- retries;
- observabilidade;
- cancelamento;
- progresso;
- fallback;
- baixo acoplamento entre Jarvis e engines específicas.

Evitar:

- amarrar a feature diretamente ao ACE-Step;
- amarrar a feature diretamente ao Chatterbox;
- gerar toda a narração como um único áudio indivisível;
- deixar cada modelo decidir sua própria timeline;
- montar sincronização somente depois do vídeo final;
- duplicar funcionalidades que HyperFrames já oferece.

---


# 27. Repositórios locais de referência

Para facilitar o estudo da integração pelo Codex, manter clones rasos dos projetos relevantes dentro de:

```text
docs/multimedia/video-agent/
```

Estrutura esperada:

```text
docs/
└── multimedia/
    └── video-agent/
        ├── hyperframes/
        ├── chatterbox/
        ├── ace-step/
        ├── kokoro/
        └── audiocraft/
```

## HyperFrames

Repositório:

https://github.com/heygen-com/hyperframes

Caminho local esperado:

```text
docs/multimedia/video-agent/hyperframes/
```

Papel:

- engine principal do Video Agent;
- composição visual;
- timeline;
- áudio;
- preview;
- render;
- skills para agentes.

Clone:

```bash
git clone --depth 1 https://github.com/heygen-com/hyperframes.git \
  docs/multimedia/video-agent/hyperframes
```

## Chatterbox

Repositório:

https://github.com/resemble-ai/chatterbox

Caminho local esperado:

```text
docs/multimedia/video-agent/chatterbox/
```

Papel:

- candidato para narração de maior qualidade;
- TTS multilíngue;
- PT-BR;
- voice cloning/referência vocal;
- substituto futuro do Kokoro caso a qualidade justifique.

Clone:

```bash
git clone --depth 1 https://github.com/resemble-ai/chatterbox.git \
  docs/multimedia/video-agent/chatterbox
```

## ACE-Step

Repositório:

https://github.com/ace-step/ACE-Step-1.5

Caminho local esperado:

```text
docs/multimedia/video-agent/ace-step/
```

Papel:

- geração de música;
- soundtrack/BGM instrumental;
- candidato para substituir MusicGen quando for necessário maior controle ou qualidade.

Clone:

```bash
git clone --depth 1 https://github.com/ace-step/ACE-Step-1.5.git \
  docs/multimedia/video-agent/ace-step
```

## Kokoro

Repositório:

https://github.com/hexgrad/kokoro

Caminho local esperado:

```text
docs/multimedia/video-agent/kokoro/
```

Papel:

- estudar o TTS local leve utilizado como referência/fallback;
- entender carregamento do modelo, geração e opções de execução local;
- referência para o MVP de voz.

Clone:

```bash
git clone --depth 1 https://github.com/hexgrad/kokoro.git \
  docs/multimedia/video-agent/kokoro
```

## AudioCraft / MusicGen

Repositório:

https://github.com/facebookresearch/audiocraft

Caminho local esperado:

```text
docs/multimedia/video-agent/audiocraft/
```

Papel:

- estudar o ecossistema que contém o MusicGen;
- referência para geração local de música;
- entender opções de inferência e integração;
- útil para avaliar o fallback musical antes de adotar ACE-Step como provider principal.

Clone:

```bash
git clone --depth 1 https://github.com/facebookresearch/audiocraft.git \
  docs/multimedia/video-agent/audiocraft
```

## Comando completo

```bash
mkdir -p docs/multimedia/video-agent

git clone --depth 1 https://github.com/heygen-com/hyperframes.git \
  docs/multimedia/video-agent/hyperframes

git clone --depth 1 https://github.com/resemble-ai/chatterbox.git \
  docs/multimedia/video-agent/chatterbox

git clone --depth 1 https://github.com/ace-step/ACE-Step-1.5.git \
  docs/multimedia/video-agent/ace-step

git clone --depth 1 https://github.com/hexgrad/kokoro.git \
  docs/multimedia/video-agent/kokoro

git clone --depth 1 https://github.com/facebookresearch/audiocraft.git \
  docs/multimedia/video-agent/audiocraft
```

Esses diretórios devem ser tratados como **material de referência para o Codex**, e não necessariamente como dependências vendorizadas da aplicação.

---

# 28. Referências principais


HyperFrames:

https://github.com/heygen-com/hyperframes

HyperFrames CLI:

https://github.com/heygen-com/hyperframes/blob/main/docs/packages/cli.mdx

HyperFrames Skills:

https://github.com/heygen-com/hyperframes/blob/main/docs/guides/skills.mdx

HyperFrames Authentication / Media Providers:

https://github.com/heygen-com/hyperframes/blob/main/docs/guides/authentication.mdx

HyperFrames Media OS:

https://github.com/heygen-com/hyperframes/blob/main/skills/media-use

Chatterbox:

https://github.com/resemble-ai/chatterbox

ACE-Step:

https://github.com/ace-step/ACE-Step-1.5

ACE-Step CLI:

https://github.com/ace-step/ACE-Step-1.5/blob/main/docs/en/CLI.md

ACE-Step API:

https://github.com/ace-step/ACE-Step-1.5/blob/main/docs/en/API.md

---

# 29. Resumo para implementação

Em uma frase:

> Adicionar ao Jarvis uma skill de geração de apresentações que utiliza um manifesto comum para coordenar vídeo, narração e música, começando com as capacidades nativas do HyperFrames e permitindo substituir posteriormente Kokoro por Chatterbox PT-BR e MusicGen por ACE-Step sem alterar o restante do pipeline.

MVP:

```text
Jarvis
  ↓
Presentation Skill
  ↓
presentation.json
  ↓
HyperFrames
├── Video
├── Kokoro Voice
├── MusicGen BGM
└── Audio Mix
  ↓
presentation.mp4
```

Evolução:

```text
Jarvis
  ↓
Presentation Skill
  ↓
presentation.json
  ↓
┌─────────────────────────────────┐
│ HyperFrames Video               │
│ Chatterbox PT-BR Narration      │
│ ACE-Step Music                  │
│ HyperFrames Audio/Mix/Render    │
└─────────────────────────────────┘
  ↓
presentation.mp4
```

Essa é a direção proposta para acoplar a geração audiovisual ao Jarvis.
