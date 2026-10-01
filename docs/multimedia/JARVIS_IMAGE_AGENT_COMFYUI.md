# Jarvis — Image Agent com ComfyUI

> Documento de contexto técnico para o Codex, responsável pela manutenção e evolução do Jarvis.
>
> Este documento descreve o **agente gerador de imagem** do Jarvis como um escopo **separado** do agente criador de vídeo.

---

# 1. Contexto

O Jarvis já possui um **agente criador de vídeo** que utiliza o **HyperFrames** por baixo dos panos para geração de apresentações audiovisuais.

Tudo o que foi discutido anteriormente sobre:

- vídeo;
- narração;
- música;
- timeline;
- render final em MP4;

pertence ao escopo do:

```text
Video Agent
```

Agora estamos tratando de um **novo agente**, separado, com responsabilidade específica de:

```text
geração de imagens
```

Ou seja, este documento não trata de HyperFrames como engine principal, e sim da arquitetura para um:

```text
Image Agent
```

dedicado a imagens estáticas, assets e edição visual.

---

# 2. Objetivo do Image Agent

O objetivo do Image Agent é permitir que o Jarvis gere e manipule imagens para diferentes usos, por exemplo:

- hero images;
- ilustrações;
- imagens conceituais;
- thumbnails;
- banners;
- capas;
- mockups;
- assets com fundo transparente;
- ícones;
- fundos;
- imagens para landing pages;
- imagens para documentação;
- variações visuais de uma mesma ideia;
- imagens que depois podem ser reutilizadas no agente de vídeo.

Exemplo de pedidos que o agente deve ser capaz de atender:

```text
Crie uma hero image futurista para o Jarvis.
```

```text
Gere um fundo abstrato escuro com pegada tecnológica.
```

```text
Crie uma ilustração do mascote do Jarvis com fundo transparente.
```

```text
Edite esta imagem removendo o fundo e melhorando a nitidez.
```

```text
Gere 4 variações de um banner no estilo One Dark.
```

---

# 3. Decisão principal

A decisão proposta é:

> Utilizar o **ComfyUI** como a engine principal/orquestrador do Image Agent.

Repositório principal:

https://github.com/comfy-org/ComfyUI

---

# 4. Por que ComfyUI

O ComfyUI não deve ser visto apenas como “uma UI para Stable Diffusion”.

Para o Jarvis, ele faz mais sentido como:

```text
engine de workflows de geração visual
```

Ele permite:

- execução por workflow;
- composição modular;
- execução headless/API;
- suporte a múltiplos modelos;
- inpainting;
- outpainting;
- upscale;
- control/adapters;
- background removal;
- pipelines de refino;
- reutilização de fluxos;
- automação;
- integração com agentes.

Em outras palavras, ele não “melhora magicamente” um modelo fechado como o Gemini, mas ele dá ao Jarvis a capacidade de transformar uma geração simples em uma pipeline visual mais profissional.

---


# 4.1 Repositório local de referência

Para facilitar o estudo da integração pelo Codex, manter um clone raso do ComfyUI dentro de:

```text
docs/multimedia/image-agent/comfyui/
```

Estrutura esperada:

```text
docs/
└── multimedia/
    └── image-agent/
        └── comfyui/
```

Repositório:

https://github.com/Comfy-Org/ComfyUI

Clone:

```bash
mkdir -p docs/multimedia/image-agent

git clone --depth 1 https://github.com/Comfy-Org/ComfyUI.git \
  docs/multimedia/image-agent/comfyui
```

O diretório deve ser tratado como **material de referência para o Codex**, permitindo estudar:

- API;
- execução headless;
- estrutura de workflows;
- formato de workflow JSON;
- filas/jobs;
- nodes;
- providers;
- gerenciamento de assets;
- integração programática.

Não assumir que o clone dentro de `docs/` será uma dependência vendorizada do Jarvis. A integração real pode utilizar instalação/serviço separado, enquanto o clone serve para documentação e análise do código-fonte.

---

# 5. Esclarecimento importante sobre Gemini

Hoje já existe uso de modelos que geram imagem, como o Gemini.

Porém, é importante separar duas coisas:

## 5.1 Modelo

O modelo gera a imagem.

Exemplo:

```text
Gemini
Flux
SDXL
Qwen-Image
```

## 5.2 Orquestração

A pipeline decide:

- como gerar;
- como refinar;
- se haverá referências;
- se haverá upscale;
- se haverá edição;
- se haverá correção;
- se haverá múltiplas iterações;
- como exportar.

Essa é a parte em que o **ComfyUI** entra.

Portanto, a proposta não é “substituir obrigatoriamente o Gemini”, e sim:

> Colocar o Gemini (ou outro modelo) dentro de uma arquitetura de geração de imagem mais poderosa e extensível.

---

# 6. Separação de escopos no Jarvis

A arquitetura conceitual deve deixar claro que existem dois agentes diferentes:

```text
Jarvis
 ├── Video Agent
 │    └── HyperFrames
 │
 └── Image Agent
      └── ComfyUI
```

## Video Agent

Responsável por:

- vídeos;
- cenas;
- animações;
- narração;
- música;
- render final.

Engine principal:

```text
HyperFrames
```

## Image Agent

Responsável por:

- imagens estáticas;
- assets;
- edição de imagens;
- variações;
- refinamento;
- export para web/app/design/video.

Engine principal:

```text
ComfyUI
```

Esses dois agentes podem se conversar, mas **não devem ser tratados como o mesmo fluxo**.

---

# 7. Responsabilidades do Image Agent

O Image Agent deve ser capaz de cobrir, no médio prazo, os seguintes grupos de tarefas.

## 7.1 Geração

- text-to-image;
- image-to-image;
- variações;
- lote de imagens;
- imagens orientadas por referências.

## 7.2 Edição

- inpainting;
- outpainting;
- remoção de fundo;
- troca de fundo;
- correção localizada;
- ajuste de composição;
- expansão de canvas;
- remoção de elementos.

## 7.3 Refino

- upscale;
- detail enhancement;
- sharpen leve;
- refinamento generativo;
- conversão para formatos finais;
- adaptação de proporção.

## 7.4 Consistência visual

- manter estilo;
- manter paleta;
- manter um mascote/personagem;
- manter identidade visual do Jarvis;
- usar referências para assets recorrentes.

## 7.5 Produção de assets

- hero image;
- banner;
- thumbnail;
- card image;
- capa;
- asset transparente;
- imagem para post;
- mockup;
- assets reaproveitáveis pelo Video Agent.

---

# 8. Como enxergar o ComfyUI dentro do Jarvis

Para o Jarvis, o ComfyUI deve ser tratado como:

```text
workflow engine
```

e não como uma ferramenta manual de interface gráfica.

Em termos arquiteturais:

```text
Image Agent
   │
   ├── Prompt Planner
   ├── Provider Router
   ├── Workflow Builder
   ├── ComfyUI Runner
   ├── Post-Processor
   └── Asset Exporter
```

---

# 9. Componentes sugeridos do Image Agent

## 9.1 Prompt Planner

Responsável por converter o pedido do usuário em uma estrutura mais explícita.

Exemplo:

Pedido do usuário:

```text
Crie uma hero image futurista do Jarvis
```

Estrutura interna:

```yaml
task: hero-image
subject: jarvis-platform
style: futuristic
theme: one-dark
mood: technological
format: png
background: dark
variant_count: 4
resolution: 2048x2048
```

O Prompt Planner deve extrair:

- intenção;
- tipo de asset;
- estilo;
- paleta;
- formato;
- proporção;
- quantidade;
- necessidade de fundo transparente;
- necessidade de referência.

---

## 9.2 Provider Router

Responsável por escolher qual modelo/provedor usar.

Exemplo conceitual:

```text
ImageProvider
├── Gemini
├── Flux
├── SDXL
├── Qwen-Image
└── outros
```

No começo, é aceitável usar algo simples, como:

```text
provider padrão = Gemini
```

Mas o desenho deve permitir evolução.

---

## 9.3 Workflow Builder

Responsável por montar o workflow do ComfyUI.

Exemplos de workflows:

- geração simples;
- geração com referências;
- fundo transparente;
- inpainting;
- upscale;
- variações;
- edição local.

Esse componente deve transformar a intenção abstrata em algo próximo de:

```text
workflow.json
```

---

## 9.4 ComfyUI Runner

Responsável por:

- iniciar/usar o ComfyUI;
- enviar o workflow;
- acompanhar progresso;
- receber resultado;
- lidar com erros;
- salvar artefatos;
- repetir quando necessário.

---

## 9.5 Post-Processor

Responsável por pós-processamento.

Exemplos:

- upscale final;
- crop final;
- remoção de fundo;
- conversão PNG/JPG/WEBP;
- versões em múltiplos tamanhos;
- geração de preview;
- compressão web.

---

## 9.6 Asset Exporter

Responsável por organizar a saída final.

Exemplo:

```text
/output
  /images
    hero-01.png
    hero-02.png
    banner-01.webp
    mascot-cutout.png
    thumbnail-01.jpg
```

---

# 10. Pipeline conceitual

A pipeline base do Image Agent pode ser vista assim:

```text
Usuário
  │
  ▼
Pedido em linguagem natural
  │
  ▼
Prompt Planner
  │
  ▼
Provider Router
  │
  ▼
Workflow Builder
  │
  ▼
ComfyUI
  │
  ▼
Post-Processor
  │
  ▼
Asset final
```

Versão mais detalhada:

```text
Prompt
  │
  ▼
interpretação da tarefa
  │
  ▼
workflow
  │
  ├── geração inicial
  ├── referências
  ├── inpaint / edit
  ├── refine
  ├── upscale
  └── export
  │
  ▼
imagem final
```

---

# 11. O que significa “dar mais poder” ao gerador de imagem

A pergunta que motivou esta conversa foi, em essência:

> Existe algum repositório que auxilie modelos de geração de imagem, dando mais “poder” ou “capacidade” do que simplesmente chamar o modelo direto?

A resposta proposta é:

> Sim. O ComfyUI não aumenta a inteligência interna do modelo, mas aumenta muito a **capacidade operacional do pipeline**.

Isso significa que, em vez de:

```text
prompt → Gemini → imagem
```

podemos ter:

```text
prompt
  ↓
Gemini
  ↓
imagem inicial
  ↓
referências
  ↓
edição
  ↓
refino
  ↓
upscale
  ↓
export
  ↓
imagem final
```

Ou seja, o ganho está em:

- melhor controle;
- maior previsibilidade;
- melhor consistência;
- melhor qualidade final;
- workflows reutilizáveis;
- evolução sem reescrever tudo.

---

# 12. Exemplos de workflows úteis

## 12.1 Geração simples

Uso:

- conceito rápido;
- hero image;
- banner.

Pipeline:

```text
prompt
  ↓
generate
  ↓
upscale opcional
  ↓
export
```

---

## 12.2 Geração com referência

Uso:

- manter identidade do Jarvis;
- manter estilo de um produto;
- usar logo/paleta/mascote como referência.

Pipeline:

```text
prompt
  + referência
  ↓
generate
  ↓
refine
  ↓
export
```

---

## 12.3 Edição com inpainting

Uso:

- corrigir área;
- trocar detalhe;
- remover objeto;
- ajustar composição.

Pipeline:

```text
imagem
  + máscara
  + instrução
  ↓
inpaint
  ↓
export
```

---

## 12.4 Asset transparente

Uso:

- ícones;
- mascote;
- objeto isolado;
- ilustração recortada.

Pipeline:

```text
generate/edit
  ↓
background removal
  ↓
cleanup
  ↓
png transparente
```

---

## 12.5 Upscale e refinement

Uso:

- preparar asset final para uso em landing page ou vídeo;
- melhorar detalhe visual.

Pipeline:

```text
imagem base
  ↓
upscale
  ↓
detail/refine
  ↓
export final
```

---

# 13. Sugestão de abstrações

O Jarvis não deve ficar diretamente acoplado a um único provedor.

Criar interfaces é importante.

Exemplo conceitual em TypeScript:

```ts
interface ImageGenerationRequest {
  prompt: string;
  negativePrompt?: string;
  width?: number;
  height?: number;
  transparentBackground?: boolean;
  references?: string[];
  variants?: number;
  outputFormat?: "png" | "jpg" | "webp";
}

interface ImageProvider {
  generate(request: ImageGenerationRequest): Promise<ImageGenerationResult>;
  edit(request: ImageEditRequest): Promise<ImageGenerationResult>;
}
```

Implementações possíveis:

```text
ImageProvider
├── GeminiProvider
├── FluxProvider
├── SDXLProvider
└── QwenImageProvider
```

Já o ComfyUI pode ficar como uma camada abaixo:

```text
WorkflowEngine
└── ComfyUIEngine
```

Assim, o Jarvis pensa em termos de:

- intenção;
- provider;
- workflow;
- asset final.

e não em detalhes internos de cada node.

---

# 14. O papel exato do ComfyUI

Existem duas formas de encaixar o ComfyUI na arquitetura.

## Opção A — ComfyUI como engine única de imagem

```text
Image Agent
   ↓
ComfyUI
   ↓
Model providers / nodes / workflows
```

Essa é a direção recomendada.

## Opção B — ComfyUI apenas como etapa de refino

```text
Gemini
  ↓
imagem
  ↓
ComfyUI
  ↓
refino / upscale / edit
```

Essa opção é válida, mas menos elegante como arquitetura principal.

### Recomendação

Adotar a:

```text
Opção A
```

isto é:

> O Image Agent do Jarvis deve usar o **ComfyUI como engine principal**.

---

# 15. Relação com o Video Agent

O Image Agent é separado do Video Agent, mas pode fornecer assets para ele.

Exemplo:

```text
Image Agent
  ├── hero.png
  ├── mascot-cutout.png
  ├── feature-banner.png
  └── background-01.png

Video Agent
  └── usa esses assets no HyperFrames
```

Ou seja:

```text
Image Agent → produz assets
Video Agent → consome assets
```

Mas isso é uma integração entre agentes, e não significa unificar os dois escopos.

---

# 16. Casos de uso prioritários

Sugestão de ordem de importância.

## Fase inicial

1. geração de imagem nova;
2. múltiplas variações;
3. upscale;
4. export padrão;
5. transparência opcional.

## Fase intermediária

6. referências;
7. edição de imagem existente;
8. inpainting;
9. background removal;
10. formatos orientados ao uso.

## Fase avançada

11. consistência visual de personagem/mascote;
12. workflows especializados por tipo de asset;
13. batch generation;
14. pipelines automáticos para materiais de marketing;
15. integração mais forte com o Video Agent.

---

# 17. Interface conceitual do agente

Exemplo de UX desejada:

```bash
jarvis image create "hero image futurista do Jarvis"
```

ou:

```bash
jarvis image create \
  --type hero \
  --style futuristic \
  --theme one-dark \
  --format png
```

E para edição:

```bash
jarvis image edit input.png \
  --task "remover fundo e melhorar nitidez"
```

E para transparência:

```bash
jarvis image create \
  --type mascot \
  --transparent \
  --format png
```

Ou ainda algo mais estruturado em nível de skill/ferramenta.

---

# 18. Manifesto intermediário sugerido

Assim como no caso do vídeo, é interessante ter uma representação intermediária.

Exemplo:

```json
{
  "task": "hero-image",
  "project": "Jarvis",
  "intent": "Create a futuristic hero image for product presentation",
  "style": {
    "theme": "one-dark",
    "mood": "precision-industrial",
    "visual_language": "clean, dark, technological"
  },
  "subject": {
    "type": "software-platform",
    "name": "Jarvis"
  },
  "output": {
    "width": 2048,
    "height": 2048,
    "format": "png",
    "transparent_background": false
  },
  "provider": {
    "mode": "auto",
    "preferred": "gemini"
  },
  "refinement": {
    "enabled": true,
    "upscale": true,
    "variants": 4
  },
  "references": []
}
```

Nome sugerido:

```text
image-task.json
```

ou:

```text
image-task.yaml
```

---

# 19. Por que usar um manifesto de imagem

Sem manifesto:

```text
LLM
 ↓
prompt solto
 ↓
imagem
```

Com manifesto:

```text
LLM
 ↓
estrutura da tarefa
 ↓
provider + workflow + export
 ↓
imagem consistente
```

Vantagens:

- reprodutibilidade;
- debug;
- retry;
- mudança de provider sem mudar toda a lógica;
- histórico de geração;
- parametrização clara;
- integração com o restante do Jarvis.

---

# 20. Capacidade de evolução

A escolha do ComfyUI é importante porque ela evita que o Jarvis nasça dependente de um pipeline simplista.

Se hoje o fluxo é:

```text
prompt → Gemini
```

amanhã ele pode se tornar:

```text
prompt
  ↓
Gemini
  ↓
edit
  ↓
refine
  ↓
upscale
  ↓
asset final
```

e depois:

```text
prompt
  + referências
  + workflow específico
  + export multi-formato
  ↓
asset final profissional
```

Sem precisar recomeçar a arquitetura do zero.

---

# 21. Tecnologias/recursos relevantes no ecossistema ComfyUI

O Codex pode considerar futuramente integrações ou workflows com recursos como:

- geração base;
- image-to-image;
- inpainting;
- outpainting;
- upscale;
- background removal;
- LoRA;
- ControlNet;
- IP-Adapter;
- pipelines com referência;
- export multi-resolução.

Nem todos precisam entrar no MVP.

A proposta é:

> começar simples, mas em uma arquitetura que permita crescer.

---

# 22. Estratégia recomendada de implementação

## Fase 1 — MVP simples

Objetivo:

Colocar o Image Agent de pé.

Fluxo:

```text
Jarvis
  ↓
Image Agent
  ↓
ComfyUI
  ↓
provider padrão
  ↓
imagem
  ↓
export
```

Capacidades mínimas:

- text-to-image;
- variações;
- tamanho configurável;
- PNG/JPG/WEBP;
- upscale opcional.

---

## Fase 2 — Estruturação interna

Adicionar:

- Prompt Planner;
- Provider Router;
- Workflow Builder;
- Asset Exporter;
- image-task.json.

---

## Fase 3 — Edição

Adicionar:

- image edit;
- inpainting;
- background removal;
- transparência;
- refinamento localizado.

---

## Fase 4 — Referências e consistência

Adicionar:

- references;
- identidade visual do Jarvis;
- assets recorrentes;
- personagem/mascote consistente;
- presets visuais.

---

## Fase 5 — Integração com outros agentes

Adicionar:

- assets compartilháveis com o Video Agent;
- reaproveitamento de outputs;
- catálogo interno de assets;
- geração de pacotes visuais por projeto.

---

# 23. Estrutura possível dentro do Jarvis

Exemplo conceitual:

```text
src/
└── image/
    ├── planner/
    │   ├── image-prompt-planner.ts
    │   └── schema.ts
    │
    ├── providers/
    │   ├── provider.ts
    │   ├── gemini.ts
    │   ├── flux.ts
    │   └── sdxl.ts
    │
    ├── workflow/
    │   ├── builder.ts
    │   ├── presets.ts
    │   └── comfyui.ts
    │
    ├── processing/
    │   ├── upscale.ts
    │   ├── transparency.ts
    │   └── export.ts
    │
    └── image-service.ts
```

Essa estrutura é apenas uma referência.

O Codex deve adaptar ao desenho real do Jarvis.

---

# 24. Saídas esperadas

Exemplo de outputs padronizados:

```text
/output
  /images
    hero-01.png
    hero-02.png
    hero-03.png
    hero-04.png
    mascot-transparent.png
    banner-home.webp
    feature-thumb.jpg
```

Também é interessante manter:

```text
/output
  /metadata
    image-task.json
    workflow.json
    generation-report.json
```

Isso ajuda em:

- auditoria;
- reprodutibilidade;
- debug;
- retry;
- comparação de resultados.

---

# 25. Requisitos arquiteturais

A implementação deve favorecer:

- separação clara entre Video Agent e Image Agent;
- ComfyUI como engine principal;
- providers substituíveis;
- workflows reutilizáveis;
- execução headless;
- cache;
- retries;
- progresso;
- cancelamento;
- export padronizado;
- refinamento opcional;
- baixo acoplamento;
- preparação para evolução futura.

Evitar:

- acoplar o Image Agent diretamente ao Gemini;
- criar um fluxo único rígido;
- tratar edição e geração como a mesma coisa sem abstração;
- ignorar outputs intermediários;
- misturar responsabilidades do Video Agent com o Image Agent.

---

# 26. Pontos que o Codex deve investigar

Antes da implementação, verificar:

1. Como o Jarvis registra agentes e skills atualmente.
2. Como subprocessos/serviços externos são executados.
3. Se o ComfyUI será executado como processo próprio, serviço local ou integração persistente.
4. Como salvar workflows.
5. Como lidar com assets de referência.
6. Como versionar presets.
7. Como detectar disponibilidade dos providers.
8. Como reportar progresso ao usuário.
9. Como cancelar jobs longos.
10. Como reaproveitar outputs em outros agentes.
11. Como armazenar metadata de geração.
12. Como organizar outputs por projeto.
13. Como lidar com transparência e formatos.
14. Como expor uma UX simples sem esconder capacidade de evolução.
15. Como usar o mesmo Image Agent tanto para geração quanto edição.

---

# 27. Resumo arquitetural

A proposta final pode ser resumida assim:

```text
Jarvis
 ├── Video Agent
 │    └── HyperFrames
 │
 └── Image Agent
      └── ComfyUI
```

Onde o:

## Video Agent
continua focado em:

- vídeos;
- narração;
- música;
- timeline;
- apresentações audiovisuais.

E o:

## Image Agent
passa a focar em:

- geração de imagens;
- edição;
- variações;
- upscale;
- assets;
- consistência visual;
- export.

---


# 27.1 Estrutura local de referências multimídia

A estrutura global esperada no repositório do Jarvis é:

```text
docs/
└── multimedia/
    ├── video-agent/
    │   ├── hyperframes/
    │   ├── chatterbox/
    │   ├── ace-step/
    │   ├── kokoro/
    │   └── audiocraft/
    │
    └── image-agent/
        └── comfyui/
```

Para este documento, o único projeto diretamente pertencente ao escopo do Image Agent é:

```text
docs/multimedia/image-agent/comfyui/
```

Os projetos existentes em `video-agent/` pertencem ao agente de vídeo e não devem ser acoplados ao Image Agent apenas por estarem sob a mesma pasta `multimedia`.

---

# 28. Resumo executivo para implementação

Em uma frase:

> Criar um Image Agent dedicado no Jarvis, separado do Video Agent, utilizando o ComfyUI como engine principal de workflows visuais para geração, edição, refinamento e export de imagens, mantendo suporte a múltiplos providers e preparando a arquitetura para crescimento futuro.

MVP:

```text
Jarvis
  ↓
Image Agent
  ↓
Prompt Planner
  ↓
Provider Router
  ↓
ComfyUI
  ↓
Post-Processor
  ↓
Asset final
```

Direção de longo prazo:

```text
Image Agent
├── geração
├── edição
├── inpainting
├── upscale
├── transparência
├── referências
├── consistência visual
└── integração com Video Agent
```

---

# 29. Conclusão

A escolha do **ComfyUI** para o agente de imagem faz sentido porque ele:

- dá estrutura ao pipeline;
- permite crescer sem reescrever a arquitetura;
- separa geração de orquestração;
- encaixa bem com um sistema agêntico como o Jarvis;
- mantém o Image Agent desacoplado do Video Agent;
- oferece um caminho claro de evolução, do simples ao avançado.

Portanto, a recomendação é:

> Implementar o **Image Agent** como um escopo separado dentro do Jarvis, usando o **ComfyUI** como engine principal, com abstrações de provider e workflows desde o início.
