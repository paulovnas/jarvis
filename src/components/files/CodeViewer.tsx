import { useEffect, useId, useRef } from "react";
import { fileLanguage, type FilePreview } from "@/core/project-files";
import { monaco } from "./monaco";

export default function CodeViewer({ file, paths, visible }: { file: FilePreview; paths: string[]; visible: boolean }) {
  const host = useRef<HTMLDivElement>(null);
  const instance = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const models = useRef(new Map<string, { model: monaco.editor.ITextModel; view: monaco.editor.ICodeEditorViewState | null }>());
  const current = useRef<string | null>(null);
  const id = useId();

  useEffect(() => {
    if (!host.current) return;
    const styles = getComputedStyle(host.current);
    monaco.editor.defineTheme("jarvis-reader", {
      base: "vs-dark", inherit: true,
      rules: [{ token: "comment", foreground: "969eac" }, { token: "keyword", foreground: "c678dd" }, { token: "string", foreground: "98c379" }, { token: "number", foreground: "e5c07b" }],
      colors: { "editor.background": styles.backgroundColor.startsWith("rgb(") ? rgbHex(styles.backgroundColor) : "#181b20", "editor.foreground": "#d7dce5", "editorLineNumber.foreground": "#969eac", "editorCursor.foreground": "#61afef" },
    });
    const editor = monaco.editor.create(host.current, {
      model: null, theme: "jarvis-reader", readOnly: true, domReadOnly: true,
      readOnlyMessage: { value: "Este arquivo está em modo somente leitura." },
      automaticLayout: true, minimap: { enabled: false }, fontFamily: '"JetBrains Mono", Consolas, monospace', fontSize: 13,
      scrollBeyondLastLine: false, renderLineHighlight: "line", padding: { top: 12 },
      lineNumbers: "on", contextmenu: false, links: false, stickyScroll: { enabled: false },
      ariaLabel: "Visualização do arquivo (somente leitura)",
    });
    instance.current = editor;
    const cache = models.current;
    return () => { editor.dispose(); for (const item of cache.values()) item.model.dispose(); cache.clear(); instance.current = null; current.current = null; };
  }, []);

  useEffect(() => {
    const editor = instance.current;
    if (!editor) return;
    const previous = current.current ? models.current.get(current.current) : undefined;
    if (previous) previous.view = editor.saveViewState();
    let entry = models.current.get(file.path);
    if (!entry) {
      const uri = monaco.Uri.from({ scheme: "jarvis-reader", authority: encodeURIComponent(id), path: `/${file.path}` });
      entry = { model: monaco.editor.createModel(file.content, fileLanguage(file.path), uri), view: null };
      models.current.set(file.path, entry);
    } else if (entry.model.getValue() !== file.content) entry.model.setValue(file.content);
    editor.setModel(entry.model);
    if (entry.view) editor.restoreViewState(entry.view);
    current.current = file.path;
    for (const [path, item] of models.current) if (!paths.includes(path)) { item.model.dispose(); models.current.delete(path); }
    editor.updateOptions({ ariaLabel: `Arquivo ${file.path} (somente leitura)` });
    if (visible) editor.layout();
  }, [file, id, paths, visible]);

  return <div ref={host} className="h-full min-h-0 min-w-0 bg-background" />;
}

function rgbHex(color: string): string {
  return `#${(color.match(/\d+/g) ?? ["24", "27", "32"]).slice(0, 3).map(value => Number(value).toString(16).padStart(2, "0")).join("")}`;
}
