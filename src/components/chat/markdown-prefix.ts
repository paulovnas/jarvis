import type { Root, RootContent } from "mdast";
import type { Plugin } from "unified";

const MAX_PREFIX_BLOCKS = 100;
const MAX_SOURCE_LENGTH = 128 * 1024;

export class MarkdownPrefixCache {
  private previous = "";
  private streamId = "";
  private prefix = "";
  private nodes: RootContent[] = [];

  prepare(content: string, streaming: boolean, streamId: string): { content: string; plugin: Plugin<[], Root> } {
    if (streamId !== this.streamId || !content.startsWith(this.previous)) this.clear();
    const tail = content.slice(this.prefix.length);
    // Definitions can resolve earlier plain-text references; retain one whole-document parser scope.
    const incremental = streaming && content.length <= MAX_SOURCE_LENGTH && !tail.includes("]:") && !tail.includes("\r");
    if (!incremental) this.clear();
    this.previous = incremental ? content : "";
    this.streamId = streamId;
    const prefix = this.prefix;
    const nodes = this.nodes;
    const source = content.slice(prefix.length);
    const plugin: Plugin<[], Root> = () => tree => {
      if (incremental && this.previous === content && this.streamId === streamId) {
        const available = Math.min(MAX_PREFIX_BLOCKS - nodes.length, tree.children.length - 1);
        for (let index = available; index > 0; index--) {
          const start = tree.children[index].position?.start;
          if (start?.offset === undefined || start.column !== 1 || source.indexOf("\n", start.offset) < 0) continue;
          // Keep the final block mutable, including incomplete list markers and open tables/fences.
          this.prefix = content.slice(0, prefix.length + start.offset);
          this.nodes = [...nodes, ...tree.children.slice(0, index)];
          break;
        }
      }
      return nodes.length ? { ...tree, children: [...nodes, ...tree.children] } : tree;
    };
    return { content: source, plugin };
  }

  private clear() {
    this.previous = "";
    this.prefix = "";
    this.nodes = [];
  }
}
