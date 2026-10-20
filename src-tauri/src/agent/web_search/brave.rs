use super::{add_source, AgentError, Value, TIMEOUT};
use reqwest::{header::HeaderValue, RequestBuilder};

const ENDPOINT: &str = "https://api.search.brave.com/res/v1/web/search";
const MAX_BYTES: usize = 2 * 1024 * 1024;

fn invalid_response() -> AgentError {
    AgentError::new(
        "web_search_brave_response",
        "O Brave Search retornou uma resposta inválida ou grande demais.",
    )
}

pub(super) fn request(key: &str, query: &str, limit: usize) -> Result<RequestBuilder, AgentError> {
    if query.trim().is_empty()
        || query.chars().count() > 600
        || query.split_whitespace().count() > 75
        || !(1..=10).contains(&limit)
    {
        return Err(AgentError::new(
            "web_search_arguments",
            "No Brave Search, use até 600 caracteres, 75 palavras e entre 1 e 10 fontes.",
        ));
    }
    let mut token = HeaderValue::from_str(key).map_err(|_| {
        AgentError::new(
            "web_search_brave_auth",
            "Atualize a chave do Brave Search em Provedores.",
        )
    })?;
    token.set_sensitive(true);
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(TIMEOUT)
        .user_agent(format!("jarvis/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| AgentError::internal())?;
    let mut url = reqwest::Url::parse(ENDPOINT).map_err(|_| AgentError::internal())?;
    url.query_pairs_mut().extend_pairs([
        ("q", query),
        ("count", &limit.to_string()),
        ("result_filter", "web"),
        ("text_decorations", "false"),
    ]);
    Ok(client
        .get(url)
        .header("X-Subscription-Token", token)
        .header("Accept", "application/json"))
}

pub(super) async fn receive(
    request: RequestBuilder,
    alias: &str,
    limit: usize,
) -> Result<String, AgentError> {
    let mut response = request.send().await.map_err(|error| {
        if error.is_timeout() {
            AgentError::new(
                "web_search_timeout",
                "A pesquisa no Brave Search excedeu o tempo limite. Tente novamente.",
            )
        } else {
            AgentError::new(
                "web_search_brave_connection",
                "Não foi possível conectar ao Brave Search.",
            )
        }
    })?;
    if !response.status().is_success() {
        let (code, message) = match response.status().as_u16() {
            401 | 403 => ("web_search_brave_auth", "O Brave Search recusou a chave de API. Revise a chave e o plano em Provedores."),
            429 => ("web_search_brave_limit", "O limite de requisições ou a cota do Brave Search foi atingido. Verifique seu plano e tente novamente mais tarde."),
            400 | 422 => ("web_search_arguments", "O Brave Search recusou a consulta. Tente uma busca mais curta e específica."),
            _ => ("web_search_brave_provider", "O Brave Search está indisponível. Tente novamente mais tarde."),
        };
        // Provider error bodies may echo credentials or private request metadata.
        return Err(AgentError::new(code, message));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_BYTES as u64)
    {
        return Err(invalid_response());
    }
    let mut bytes = vec![];
    while let Some(chunk) = response.chunk().await.map_err(|_| invalid_response())? {
        if bytes.len().saturating_add(chunk.len()) > MAX_BYTES {
            return Err(invalid_response());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| invalid_response())?;
    format_result(&value, alias, limit)
}

fn format_result(value: &Value, alias: &str, limit: usize) -> Result<String, AgentError> {
    if value["type"] != "search" {
        return Err(invalid_response());
    }
    let results = if value["web"].is_null() {
        &[][..]
    } else {
        value["web"]["results"]
            .as_array()
            .ok_or_else(invalid_response)?
            .as_slice()
    };
    let mut sources = vec![];
    let mut snippets = vec![];
    for result in results {
        let previous = sources.len();
        add_source(&mut sources, result, limit);
        if sources.len() > previous {
            let source = &sources[previous];
            let description: String = result["description"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .take(1000)
                .collect();
            snippets.push(format!(
                "{}. {}\n{}\n{}",
                previous + 1,
                source.title,
                description,
                source.url
            ));
        }
        if sources.len() == limit {
            break;
        }
    }
    if !results.is_empty() && sources.is_empty() {
        return Err(invalid_response());
    }
    let answer = if snippets.is_empty() {
        "Nenhum resultado encontrado para esta consulta.".into()
    } else {
        snippets.join("\n\n")
    };
    Ok(serde_json::json!({"accountAlias":alias, "provider":"Brave Search", "model":null, "answer":answer, "sources":sources}).to_string())
}

#[cfg(test)]
mod tests;
