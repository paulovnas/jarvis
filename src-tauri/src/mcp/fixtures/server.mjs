// Deterministic MCP peer used only by Rust integration tests. No external access.
import { createInterface } from "node:readline";
import { createServer } from "node:http";
import { appendFileSync, writeFileSync } from "node:fs";

if (process.env.PID_FILE) writeFileSync(process.env.PID_FILE, String(process.pid));
function respond(request) {
  if (!Object.hasOwn(request, "id")) return null;
  let result;
  if (request.method === "initialize") {
    result = { protocolVersion: "2024-11-05", capabilities: { tools: { listChanged: true } }, serverInfo: { name: "jarvis-fixture", version: "1" } };
  } else if (request.method === "tools/list") {
    result = { tools: [
      { name: "lookup", description: "Read documentation", inputSchema: { type: "object", properties: { query: { type: "string" } }, required: ["query"], additionalProperties: false }, annotations: { readOnlyHint: true, destructiveHint: false } },
      { name: "mutate", description: "An unknown side effect", inputSchema: { type: "object", properties: {} } },
    ] };
  } else if (request.method === "tools/call") {
    if (process.env.CALLS_FILE) appendFileSync(process.env.CALLS_FILE, `${request.params.name}\n`);
    if (request.params.arguments?.query === "hang") return null;
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
    const result = respond(JSON.parse(Buffer.concat(chunks).toString()));
    if (!result) { response.writeHead(202).end(); return; }
    response.writeHead(200, { "content-type": "application/json" }).end(JSON.stringify(result));
  });
  server.listen(0, "127.0.0.1", () => process.stdout.write(`${server.address().port}\n`));
} else {
  const input = createInterface({ input: process.stdin });
  input.on("line", (line) => { const result = respond(JSON.parse(line)); if (result) process.stdout.write(`${JSON.stringify(result)}\n`); });
  input.on("close", () => process.exit(0));
}
