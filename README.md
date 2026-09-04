<p align="center">
  <img src="public/logo.svg" width="112" height="112" alt="Jarvis Logo" />
</p>

<h1 align="center">Jarvis</h1>

<p align="center">
  <strong>Coding Agent Desktop GUI de Alta Performance</strong><br />
  Construído com <strong>Tauri v2 (Rust)</strong>, <strong>React 19</strong>, <strong>TypeScript</strong>, <strong>Tailwind CSS v4</strong> e <strong>shadcn/ui</strong>.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Tauri-v2.0-24C8D8?style=flat-square&logo=tauri&logoColor=white" alt="Tauri v2" />
  <img src="https://img.shields.io/badge/Rust-2021-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust" />
  <img src="https://img.shields.io/badge/React-19-61DAFB?style=flat-square&logo=react&logoColor=black" alt="React 19" />
  <img src="https://img.shields.io/badge/Tailwind-v4-38BDF8?style=flat-square&logo=tailwindcss&logoColor=white" alt="Tailwind CSS v4" />
  <img src="https://img.shields.io/badge/Theme-One%20Dark-61afef?style=flat-square" alt="One Dark Theme" />
  <img src="https://img.shields.io/badge/Tests-Vitest-729B1B?style=flat-square&logo=vitest&logoColor=white" alt="Vitest" />
</p>

---

## 🎯 Sobre o Jarvis

O **Jarvis** é uma interface desktop gráfica moderna, leve e autônoma projetada para engenharia de software assistida por IA. O projeto foi arquitetado do zero para oferecer uma experiência fluida, nativa e de baixo consumo de recursos através do **Tauri v2**:

- **Backend em Rust (`src-tauri`):** Gerenciamento do loop do agente, execução segura de comandos de terminal, manipulação direta de sistema de arquivos e comunicação com provedores de IA sem overhead de navegadores pesados.
- **Frontend em React 19 + TypeScript (`src`):** Interface reativa, modular e tipada, focada em renderização rápida de diffs, streaming de raciocínio, ferramentas e contexto de projeto.
- **Base de Conhecimento (`docs/metis`):** O projeto utiliza a arquitetura e fluxos do [metis](docs/metis) como referência conceitual e base de conhecimento obrigatória para projetar sessões, tools e integrações.

---

## 🎨 Design System & Interface

O Jarvis possui um design system coeso e minimalista:

- **Tema One Dark:** Paleta canônica inspirada no Atom e VS Code One Dark Pro:
  - Fundo principal: `#282c34`
  - Superfícies e Cards: `#21252b`
  - Painéis secundários: `#2c313a`
  - Bordas: `#3e4451`
  - Acentos semânticos: Azul `#61afef` (primário), Verde `#98c379` (status/terminal), Ciano `#56b6c2` (workspace), Amarelo `#e5c07b` (chaves de API) e Vermelho `#e06c75` (destrutivo).
- **Tipografia:** Fonte **Roboto** (`@fontsource/roboto`) em todos os pesos tipográficos.
- **Componentes:** Todo elemento de UI é baseado em **shadcn/ui** e primitivos acessíveis.
- **Custom TitleBar Integrada:** Barra de título exclusiva, sem molduras nativas do SO (`decorations: false`), com suporte nativo a arrastar janela (`data-tauri-drag-region`), alternância de maximização por clique duplo e controles estilizados com hover de fechamento em vermelho One Dark.
- **Dimensões Padrão:** Janela configurada em `1360x768`, centralizada na tela na inicialização.
- **Notificações:** Sistema de toasts elegante com **Sonner**.

---

## 📁 Estrutura do Projeto

```text
jarvis/
├── docs/
│   └── metis/               # Base de conhecimento de arquitetura (somente leitura)
├── public/
│   └── logo.svg             # Logo vetorial canônica do Jarvis (1024x1024)
├── src/
│   ├── components/
│   │   ├── layout/
│   │   │   ├── TitleBar.tsx      # Barra de título customizada com controles Tauri
│   │   │   └── TitleBar.test.tsx # Testes unitários da barra de título
│   │   ├── ui/                   # 25+ componentes instalados via shadcn/ui
│   │   └── JarvisLogo.tsx        # Componente SVG reutilizável da logo
│   ├── hooks/               # Custom React hooks (ex: use-mobile)
│   ├── lib/                 # Utilitários (ex: cn / tailwind-merge)
│   ├── test/                # Setup de testes unitários (Vitest + JSDOM)
│   ├── App.tsx              # Tela de Onboarding e ponto de entrada da aplicação
│   ├── App.test.tsx         # Testes de integração e comportamento da UI
│   ├── index.css            # Configuração de temas, One Dark tokens e Roboto
│   └── main.tsx             # Inicialização React DOM
├── src-tauri/
│   ├── capabilities/        # Permissões do Tauri v2 (janela, drag, opener)
│   ├── icons/               # Ícones de plataforma (.ico, .icns, PNGs)
│   ├── src/                 # Código Rust do backend nativo
│   ├── Cargo.toml           # Dependências Rust
│   └── tauri.conf.json      # Configurações da aplicação desktop
├── AGENTS.md                # Regras normativas e diretrizes para agentes de IA
├── eslint.config.js         # ESLint flat config (regras estritas, zero warnings)
├── tsconfig.json            # Configuração TypeScript estrita com alias @/
└── vite.config.ts           # Configuração Vite com plugin Tailwind v4 e Tauri
```

---

## 🚀 Como Executar

### Pré-requisitos

- [Bun](https://bun.sh/) (recomendado) ou Node.js 20+
- [Rust](https://www.rust-lang.org/) e Cargo (para rodar com Tauri desktop)

### Instalação

```bash
bun install
```

### Modo Web (apenas frontend no navegador)

```bash
bun run dev
```

Acesse em `http://localhost:1420`.

### Modo Desktop (aplicação nativa com Tauri)

```bash
bun run tauri dev
```

---

## 🛡️ Quality Gates & Testes

Testes unitários e qualidade de código são **obrigatórios** no Jarvis. Antes de entregar qualquer alteração, todos os gates devem passar com **zero erros e zero warnings**:

```bash
# Executa a cadeia completa de validação:
bun run check
```

Comandos individuais:

```bash
bun run lint         # ESLint com --max-warnings 0
bun run typecheck    # Checagem de tipos TypeScript estrita
bun run test         # Execução de testes unitários com Vitest
bun run build        # Build de produção do frontend
```

---

## 📜 Regras de Desenvolvimento

As regras completas e obrigatórias estão documentadas no arquivo [AGENTS.md](AGENTS.md):

1. **`docs/metis` como referência:** Estude o metis antes de planejar qualquer funcionalidade no Jarvis.
2. **Tudo é shadcn/ui:** Não crie componentes básicos do zero; instale com `bunx shadcn@latest add <componente>`.
3. **Cursor Pointer Obrigatório:** Todo elemento interativo (botões, selects, tabs, cards clicáveis) deve ter cursor pointer.
4. **Testes Unitários:** Toda feature ou correção deve acompanhar testes unitários colocalizados.
5. **Zero Warnings:** Commits só são aceitos quando `bun run check` passar 100% limpo.

---

## 📄 Licença

Projeto privado desenvolvido para o ecossistema Jarvis.
