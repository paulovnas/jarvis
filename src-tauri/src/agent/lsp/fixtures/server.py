#!/usr/bin/env python3
"""Deterministic stdio language server for native diagnostics regression tests."""
import json
from pathlib import Path
import sys
import threading
import time
from urllib.parse import unquote, urlparse

documents = {}
write_lock = threading.Lock()


def send(message):
    body = json.dumps(message).encode()
    with write_lock:
        sys.stdout.buffer.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
        sys.stdout.buffer.flush()


def diagnostics(text):
    broken = "BROKEN" in text
    if "USES_DEPENDENCY" in text:
        broken = not any(uri.endswith("/z-dependency.ts") and "EXPORTED" in value for uri, value in documents.items())
    return [{"severity": 1, "message": "Fixture type error", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}}] if broken else []


def publish_later(document, text):
    if Path("unversioned-push").exists() or Path("incremental-push").exists():
        params = {"uri": document["uri"], "diagnostics": []}
        if Path("incremental-push").exists():
            params["version"] = document["version"]
        send({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": params})
        time.sleep(0.15)
        params["diagnostics"] = diagnostics(text)
        send({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": params})
        return
    # Neither another URI nor a superseded version may complete the wait.
    for uri, version in [(document["uri"] + ".other", 1), (document["uri"], document["version"] - 1)]:
        send({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": {"uri": uri, "version": version, "diagnostics": []}})
    time.sleep(1)
    send({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": {
        "uri": document["uri"], "version": document["version"], "diagnostics": diagnostics(text)
    }})


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
    if method in ("textDocument/didOpen", "textDocument/didChange") and any(Path(name).exists() for name in ["delayed-push", "unversioned-push", "incremental-push"]):
        document = params["textDocument"]
        threading.Thread(target=publish_later, args=(document, documents[document["uri"]]), daemon=True).start()
    if "id" not in message:
        continue
    result = None
    if method == "initialize":
        result = {"capabilities": {} if any(Path(name).exists() for name in ["push-only", "delayed-push", "unversioned-push", "incremental-push"]) else {"diagnosticProvider": True}}
    elif method == "textDocument/diagnostic":
        uri = params["textDocument"]["uri"]
        text = documents[uri]
        if "CHANGE_DURING_QUERY" in text:
            Path(unquote(urlparse(uri).path)).write_text("changed externally")
        if "CHANGE_DEPENDENCY_DURING_QUERY" in text:
            Path("z-dependency.ts").write_text("changed externally")
        result = {"kind": "full", "items": diagnostics(text)}
    send({"jsonrpc": "2.0", "id": message["id"], "result": result})
