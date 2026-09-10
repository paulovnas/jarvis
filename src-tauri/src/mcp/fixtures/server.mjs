// Deterministic MCP peer used only by Rust integration tests. No external access.
import { createInterface } from "node:readline";
import { createServer } from "node:http";
import { appendFileSync, readFileSync, writeFileSync } from "node:fs";

if (process.env.PID_FILE) writeFileSync(process.env.PID_FILE, String(process.pid));
if (process.env.STARTS_FILE) appendFileSync(process.env.STARTS_FILE, `${process.env.SERVER_NAME ?? "fixture"}\n`);
function extraToolCount() {
  const configured = process.env.EXTRA_TOOLS_FILE
    ? readFileSync(process.env.EXTRA_TOOLS_FILE, "utf8")
    : process.env.EXTRA_TOOLS ?? "0";
  return Math.min(94, Math.max(0, Number.parseInt(configured, 10) || 0));
}
function listedTools() {
  return [
    { name: "lookup", description: "Read documentation", inputSchema: { type: "object", properties: { query: { type: "string" }, delayMs: { type: "integer", minimum: 0, maximum: 5000 } }, required: ["query"], additionalProperties: false }, annotations: { readOnlyHint: true, destructiveHint: false } },
    { name: "mutate", description: "An unknown side effect", inputSchema: { type: "object", properties: { hang: { type: "boolean" } }, additionalProperties: false } },
    ...Array.from({ length: extraToolCount() }, (_, index) => ({
      name: `catalog_tool_${index}`,
      description: `Synthetic catalog operation ${index} for customer archive lookup and deterministic MCP discovery tests`,
      inputSchema: { type: "object", properties: { query: { type: "string", description: `Search query for catalog operation ${index}` } }, required: ["query"], additionalProperties: false },
      annotations: { readOnlyHint: true, destructiveHint: false },
    })),
  ];
}
function respond(request) {
  if (!Object.hasOwn(request, "id")) return null;
  let result;
  if (request.method === "initialize") {
    result = { protocolVersion: "2024-11-05", capabilities: { tools: { listChanged: true } }, serverInfo: { name: "jarvis-fixture", version: "1" } };
  } else if (request.method === "tools/list") {
    result = { tools: listedTools() };
  } else if (request.method === "tools/call") {
    if (process.env.CALLS_FILE) appendFileSync(process.env.CALLS_FILE, `${request.params.name}\n`);
    if (request.params.arguments?.query === "rpc-error") {
      return { jsonrpc: "2.0", id: request.id, error: { code: -32602, message: "Missing notebook_id", data: { path: "$.notebook_id", expected: "string", diagnostic: process.env.TEST_SECRET ?? "" } } };
    }
    if (request.params.arguments?.query === "hang" || request.params.arguments?.hang === true) return null;
    result = { content: [{ type: "text", text: `Documentation: ${request.params.arguments?.query ?? "mutation"}; ${process.env.TEST_SECRET ?? ""}` }], structuredContent: { source: "fixture", cwd: process.cwd() }, isError: request.params.arguments?.query === "fail" };
  } else if (request.method === "ping") result = {};
  else return { jsonrpc: "2.0", id: request.id, error: { code: -32601, message: "Unknown method" } };
  return { jsonrpc: "2.0", id: request.id, result };
}

if (process.argv.includes("--http")) {
  const server = createServer(async (request, response) => {
    if (request.method !== "POST") { response.writeHead(405).end(); return; }
    if (request.headers.authorization !== "Bearer fixture-token") { response.writeHead(401).end(); return; }
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    const message = JSON.parse(Buffer.concat(chunks).toString());
    const delay = message.method === "tools/call" ? (message.params.arguments?.delayMs ?? 0) : 0;
    if (delay > 0) await new Promise((resolve) => setTimeout(resolve, delay));
    const result = respond(message);
    if (!result) { response.writeHead(202).end(); return; }
    response.writeHead(200, { "content-type": "application/json" }).end(JSON.stringify(result));
  });
  server.listen(0, "127.0.0.1", () => process.stdout.write(`${server.address().port}\n`));
} else {
  const input = createInterface({ input: process.stdin });
  input.on("line", (line) => {
    const message = JSON.parse(line);
    const delay = message.method === "tools/call" ? (message.params.arguments?.delayMs ?? 0) : 0;
    const reply = () => { const result = respond(message); if (result) process.stdout.write(`${JSON.stringify(result)}\n`); };
    if (delay > 0) setTimeout(reply, delay); else reply();
  });
  input.on("close", () => process.exit(0));
}
