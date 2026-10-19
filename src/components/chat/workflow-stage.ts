export function workflowStage(text: string) {
  const match = /^Etapa ([1-9]\d*) · ([^\r\n]+) em execução\.$/.exec(text.trim());
  return match ? { number: match[1], name: match[2] } : undefined;
}
