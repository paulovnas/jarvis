#!/usr/bin/env python3
"""Deterministic stdio language server for native diagnostics regression tests."""
import json
from pathlib import Path
import sys
from urllib.parse import unquote, urlparse

documents = {}
while True:
    length = None
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            sys.exit(0)
        if line in (b"\r\n", b"\n"):
            break
        if line.lower().startswith(b"content-length:"):
            length = int(line.split(b":", 1)[1])
    message = json.loads(sys.stdin.buffer.read(length))
    method = message.get("method", "")
    params = message.get("params", {})
    with Path("lsp-test.log").open("a") as log:
        log.write(method + "\n")
    if Path("fail-server").exists():
        sys.exit(1)
    if method == "textDocument/didOpen":
        document = params["textDocument"]
        documents[document["uri"]] = document["text"]
    elif method == "textDocument/didChange":
        documents[params["textDocument"]["uri"]] = params["contentChanges"][0]["text"]
    if "id" not in message:
        continue
    result = None
    if method == "initialize":
        result = {"capabilities": {} if Path("push-only").exists() else {"diagnosticProvider": True}}
    elif method == "textDocument/diagnostic":
        uri = params["textDocument"]["uri"]
        text = documents[uri]
        if "CHANGE_DURING_QUERY" in text:
            Path(unquote(urlparse(uri).path)).write_text("changed externally")
        result = {"kind": "full", "items": [{"severity": 1, "message": "Fixture type error", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}}] if "BROKEN" in text else []}
    body = json.dumps({"jsonrpc": "2.0", "id": message["id"], "result": result}).encode()
    sys.stdout.buffer.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
    sys.stdout.buffer.flush()
