---
name: criar-agentes-e-fluxos
description: Cria ou aprimora agentes e fluxos personalizados do Jarvis com revisão e aprovação antes de salvar.
---

# Autoria assistida do Jarvis

Use esta skill quando o usuário pedir um novo agente, um novo fluxo ou a melhoria de uma definição personalizada existente.

1. Entenda o resultado esperado, os limites, as ferramentas necessárias e como o usuário reconhecerá uma boa execução. Use `ask_user` apenas para decisões materiais que não possam ser inferidas do pedido e do projeto.
2. Consulte `jarvis_catalog` com `view: overview`. Para editar, leia também o item exato com `view: agent` ou `view: flow`.
3. Preserve IDs ao editar. Para novos agentes, fluxos e etapas, gere IDs distintos com 32 caracteres hexadecimais.
4. Escreva instruções específicas, verificáveis e proporcionais ao papel. Conceda somente a capacidade e as ferramentas necessárias. Defina o uso como `solo` para o agente principal de um chat, `flow_only` para uso exclusivo em fluxos ou `mixed` para ambos. Um modelo nulo herda o modelo atual do chat.
5. Em fluxos, use somente agentes `mixed` ou `flow_only`, conecte todas as etapas a partir da entrada, mantenha a saída normal sem ciclos e use `onRework` somente para retornos intencionais de correção.
6. Envie uma única proposta coerente com `jarvis_propose_agent` ou `jarvis_propose_flow`, usando a revisão exata recém-lida. O Jarvis abrirá a revisão para o usuário e só salvará após aprovação explícita.
7. Se a proposta for recusada, incorpore a observação do usuário. Não reenvie a mesma proposta sem uma mudança solicitada.

Agentes e fluxos nativos do Jarvis são imutáveis. Nunca tente alterá-los por arquivos, terminal ou outras ferramentas.
