//! Reject known credential-bearing proposal forms before snapshots or journals.

use serde_json::{json, Value};

fn rejected() -> Value {
    json!({"_jarvisMcpRejected":"Envie apenas a proposta do servidor, sem valores de credenciais. Use envKeys/headerKeys e preencha os valores privados no painel de aprovação."})
}

fn credential_assignment(argument: &str) -> bool {
    let Some((key, value)) = argument.split_once('=') else {
        return false;
    };
    !value.is_empty()
        && matches!(
            key,
            "API_KEY"
                | "API_TOKEN"
                | "ACCESS_TOKEN"
                | "CLIENT_SECRET"
                | "TOKEN"
                | "PASSWORD"
                | "SECRET"
                | "AUTHORIZATION"
                | "GITHUB_TOKEN"
                | "GH_TOKEN"
                | "FIREBASE_TOKEN"
                | "SENTRY_AUTH_TOKEN"
                | "MONDAY_API_TOKEN"
                | "OPENAI_API_KEY"
                | "ANTHROPIC_API_KEY"
                | "GOOGLE_API_KEY"
                | "DATABASE_PASSWORD"
        )
}

fn credential_argument(arguments: &[String]) -> bool {
    arguments.iter().enumerate().any(|(index, argument)| {
        let (flag, inline) = argument
            .split_once('=')
            .map_or((argument.as_str(), None), |(flag, value)| {
                (flag, Some(value))
            });
        credential_assignment(argument)
            || (matches!(
                flag,
                "--api-key"
                    | "--token"
                    | "--password"
                    | "--secret"
                    | "--authorization"
                    | "--client-secret"
                    | "--access-token"
                    | "--header"
                    | "-H"
            ) && inline.map_or_else(
                || {
                    arguments
                        .get(index + 1)
                        .is_some_and(|value| !value.is_empty() && !value.starts_with('-'))
                },
                |value| !value.is_empty(),
            ))
    })
}

pub(in crate::agent) fn sanitize_mcp_args(name: &str, args: Value) -> Value {
    if name != "jarvis_propose_mcp" {
        return args;
    }
    // Decode the existing strict draft instead of maintaining a second field
    // allowlist. Private value maps and other unknown fields never reach disk.
    let Ok(request) = serde_json::from_value::<super::McpRequest>(args.clone()) else {
        return rejected();
    };
    let server = request.server;
    let credential_url = server.url.as_deref().is_some_and(|value| {
        value.contains('?')
            || url::Url::parse(value)
                .is_ok_and(|url| !url.username().is_empty() || url.password().is_some())
    });
    // ponytail: exact credential forms only; arbitrary prose or wrapped shell
    // strings cannot be classified safely. Private values belong in the panel.
    if credential_url
        || credential_argument(&server.args)
        || server.command.as_deref().is_some_and(credential_assignment)
    {
        rejected()
    } else {
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposal() -> Value {
        json!({"summary":"Adicionar documentação","server":{"name":"docs","transport":"stdio","command":"npx","args":["-y","docs-mcp"],"url":null,"enabled":true,"cwd":null,"envKeys":["TOKEN"],"headerKeys":[]}})
    }

    #[test]
    fn rejects_value_maps_and_known_inline_credential_forms_without_retaining_values() {
        let mut cases = Vec::new();
        for field in ["environment", "headers", "env"] {
            let mut args = proposal();
            args["server"][field] = json!({"TOKEN":"private-test-value"});
            cases.push(args);
        }
        let mut args = proposal();
        args["mcpValues"] = json!({"environment":{"TOKEN":"private-test-value"}});
        cases.push(args);
        for cli in [
            json!(["--token", "private-test-value"]),
            json!(["--api-key=private-test-value"]),
            json!(["--api-key=-private-test-value"]),
            json!(["--password", "private-test-value"]),
            json!(["--secret=private-test-value"]),
            json!(["--authorization", "Bearer private-test-value"]),
            json!(["--client-secret=private-test-value"]),
            json!(["--access-token", "private-test-value"]),
            json!(["-H", "Authorization: Bearer private-test-value"]),
            json!(["GITHUB_TOKEN=private-test-value"]),
        ] {
            let mut args = proposal();
            args["server"]["args"] = cli;
            cases.push(args);
        }
        for url in [
            "https://example.test/mcp?api_key=private-test-value",
            "https://user:private-test-value@example.test/mcp",
        ] {
            let mut args = proposal();
            args["server"]["url"] = json!(url);
            cases.push(args);
        }
        for args in cases {
            let safe = sanitize_mcp_args("jarvis_propose_mcp", args);
            assert!(safe.get("_jarvisMcpRejected").is_some());
            assert!(!safe.to_string().contains("private-test-value"));
            assert!(serde_json::from_value::<super::super::McpRequest>(safe).is_err());
        }
    }

    #[test]
    fn preserves_supported_drafts_and_all_unrelated_tool_arguments() {
        let args = proposal();
        assert_eq!(sanitize_mcp_args("jarvis_propose_mcp", args.clone()), args);
        let unrelated = json!({"url":"https://example.test?api_key=private-test-value","args":["--token","private-test-value"]});
        assert_eq!(
            sanitize_mcp_args("http_request", unrelated.clone()),
            unrelated
        );
    }
}
