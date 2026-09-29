# Native HTTP workspace

Jarvis executes API requests in Rust with `reqwest`. The HTTP workspace and agent tools share request drafts, project environments and preserved execution results. It requires no curl, Python process or separate API client installation.

## Using the workspace

Select **Nova requisição HTTP** in the chat toolbar to create a tab. Configure the method, URL, environment, query parameters, headers, authentication and body, then select **Enviar**. Multiple HTTP tabs can coexist with Chat, files and browser tabs.

Supported authentication: none, Basic, Bearer and API key in a header or query parameter. Bodies support JSON, text, URL-encoded forms, multipart uploads and binary files. Repeated parameter/header names and disabled rows are preserved.

Edits are saved as conversation drafts. **Salvar no projeto** creates or updates a reusable request; another conversation in the same project can open it as its own draft. Concurrent user/agent edits require revision checks, with an explicit choice when versions conflict. Closing a tab preserves execution history and offers cancellation for a request that is still running.

The response area displays HTTP status, elapsed time, received/stored size, headers and formatted JSON or inert text. Binary responses can be saved. Copying copies the displayed segment; **Salvar original** exports the original stored bytes and may contain sensitive response data.

HTTP 4xx and 5xx responses are inspectable test results. A transport failure, cancellation or restart never causes automatic resending. When delivery cannot be confirmed, Jarvis reports that the server may already have applied the operation.

## Project environments

Under **Detalhes → Opções → Cliente HTTP**, define shared variables and named environments. Use references such as `{{base_url}}/orders` and `{{token}}` in requests.

- Shared variables belong to the project and work in every environment.
- Enabled environment variables override shared variables with the same name.
- A missing referenced variable prevents sending. An intentionally empty value remains valid.
- Mark a variable **Secreta** independently of its scope. Secret values use the operating system credential store; the UI preserves an existing secret when its value field is left empty.
- A running request keeps the configuration resolved when it started. Selecting another environment cannot change that execution.

Connection and idle-read timeouts, optional total timeout, response size/history retention, redirects, proxy and certificate options are configured in the same section. TLS verification is enabled and redirects are disabled by default. Redirect handling preserves method rules and prevents implicitly forwarding credentials to another origin.

Native JSON import/export includes settings and saved requests. It excludes secret values, response bodies and uploaded file bytes; imported credentials/files must be configured again. Project HTTP exports are separate from the existing global settings backup.

## AI integration

**Analisar com IA** inserts a reference to the selected execution into the existing composer. The user sends or queues it through the normal chat controls. Selecting an old result analyzes that exact `runId`, without executing the request again.

The common dispatcher exposes these tools to API providers and Claude, subject to the selected role and existing approval mode:

| Tool | Purpose |
|---|---|
| `http_requests` | Discover environments, variable names, drafts, saved requests and recent runs; read a request by ID. |
| `http_save_request` | Prepare a draft, save a reusable request or import a project file for upload. |
| `http_send` | Send a draft at its expected revision and return its execution ID. |
| `http_result` | Inspect or await an existing execution using bounded response pages. |
| `http_cancel` | Cancel an active execution without resending or undoing server effects. |

Agent requests appear in HTTP tabs. Background completion does not take focus from the user's current tab. Responses are untrusted data, and known credentials are masked in inspection output. This masking does not identify every kind of sensitive business data an API may return.

## Current scope

This client covers HTTP request/response testing. It does not include a JavaScript script runtime, collection runner, OAuth authorization wizard, cookie jar, cloud synchronization, Postman-format import or specialized WebSocket/gRPC UI. GraphQL POST requests and explicit HTTP token exchanges use the ordinary request editor.

Architecture decisions and source comparisons are in [HTTP-CLIENT-RESEARCH.md](HTTP-CLIENT-RESEARCH.md). Platform-specific certificate, proxy and credential-store behavior requires validation on each supported operating system.
