// Keep Graft's extractors while using portable, locally packaged grammar binaries.
import { Parser, Language } from "web-tree-sitter";
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";

const require = createRequire(import.meta.url);
await Parser.init();
const load = async (name) =>
  Language.load(
    readFileSync(
      require.resolve(`tree-sitter-wasm/${name}/tree-sitter-${name}.wasm`),
    ),
  );
const [typescript, tsx, Python, Go, R, Java, Kotlin, Swift, php] =
  await Promise.all(
    ["typescript", "tsx", "python", "go", "r", "java", "kotlin", "swift", "php"].map(load),
  );
const TypeScript = { typescript, tsx };
const PHP = { php };
export { Parser, TypeScript, Python, Go, R, Java, Kotlin, Swift, PHP };
