import "monaco-editor/nls/lang/pt-br";

// Monaco 0.56.0 ships an empty pt-br message table. Fill the controls exposed by
// the read-only viewer, before its modules capture their labels. The version is
// pinned; the contract test checks these indices against the shipped sources.
export const readerMessages: Record<number, readonly [string, string]> = {
  1: ["input", "Entrada"],
  2: ["Match Case", "Diferenciar maiúsculas de minúsculas"],
  3: ["Match Whole Word", "Palavra inteira"],
  4: ["Use Regular Expression", "Usar expressão regular"],
  5: ["input", "Entrada"],
  6: ["Preserve Case", "Preservar maiúsculas e minúsculas"],
  9: ["Error: {0}", "Erro: {0}"],
  10: ["Warning: {0}", "Aviso: {0}"],
  11: ["Info: {0}", "Informação: {0}"],
  14: ["Cleared Input", "Campo limpo"],
  981: ["{0} found for '{1}'", "{0} para '{1}'"],
  982: ["No matches. Try searching for something else.", "Nenhum resultado. Tente outro termo."],
  983: ["Type a number to go to a specific match (between 1 and {0})", "Digite o número do resultado (entre 1 e {0})"],
  984: ["Please type a number between 1 and {0}", "Digite um número entre 1 e {0}"],
  985: ["Please type a number between 1 and {0}", "Digite um número entre 1 e {0}"],
  987: ["Find", "Localizar"],
  989: ["Find with Selection", "Localizar seleção"],
  990: ["Find Next", "Localizar próximo"],
  991: ["Find Previous", "Localizar anterior"],
  992: ["Go to Match...", "Ir para resultado..."],
  1003: ["Find / Replace", "Localizar"],
  1004: ["Find", "Localizar"],
  1005: ["Find", "Localizar"],
  1006: ["Previous Match", "Resultado anterior"],
  1007: ["Next Match", "Próximo resultado"],
  1008: ["Find in Selection", "Localizar na seleção"],
  1009: ["Close", "Fechar"],
  1010: ["Replace", "Substituir"],
  1011: ["Replace", "Substituir"],
  1012: ["Replace", "Substituir"],
  1013: ["Replace All", "Substituir tudo"],
  1014: ["Toggle Replace", "Alternar substituição"],
  1015: ["Only the first {0} results are highlighted, but all find operations work on the entire text.", "Apenas os primeiros {0} resultados são destacados. A busca considera todo o arquivo."],
  1016: ["{0} of {1}", "{0} de {1}"],
  1017: ["No results", "Nenhum resultado"],
  1018: ["{0} found", "{0} encontrados"],
  1019: ["{0} found for '{1}'", "{0} para '{1}'"],
  1020: ["{0} found for '{1}', at {2}", "{0} para '{1}', em {2}"],
  1021: ["{0} found for '{1}'", "{0} para '{1}'"],
  1022: ["Press {0} for accessibility help", "Pressione {0} para ajuda de acessibilidade"],
};

const nls = globalThis as typeof globalThis & { _VSCODE_NLS_MESSAGES: Array<string | null> };
for (const [index, [, translated]] of Object.entries(readerMessages)) {
  if (!nls._VSCODE_NLS_MESSAGES[Number(index)]) nls._VSCODE_NLS_MESSAGES[Number(index)] = translated;
}
