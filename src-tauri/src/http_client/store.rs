use super::*;
use crate::mcp::Secrets;
use rusqlite::{params, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashSet;

pub(super) fn encode(value: &impl Serialize) -> Result<String, HttpError> {
    serde_json::to_string(value).map_err(|_| storage())
}
pub(super) fn decode<T: DeserializeOwned>(value: String) -> Result<T, HttpError> {
    serde_json::from_str(&value).map_err(|_| storage())
}
pub(super) fn id() -> Result<String, HttpError> {
    crate::library::new_id().map_err(|_| storage())
}
pub(super) fn valid_id(value: &str) -> Result<(), HttpError> {
    if value.is_empty()
        || value.len() > 100
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(invalid("Identificador HTTP inválido."));
    }
    Ok(())
}
pub(super) fn project(db: &rusqlite::Connection, project: &str) -> Result<(), HttpError> {
    if !db.query_row(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
        [project],
        |row| row.get::<_, bool>(0),
    )? {
        return Err(invalid("Projeto não encontrado."));
    }
    Ok(())
}
pub(super) fn conversation(
    db: &rusqlite::Connection,
    conversation: &str,
) -> Result<String, HttpError> {
    db.query_row(
        "SELECT project_id FROM conversations WHERE id=?1",
        [conversation],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| invalid("Conversa não encontrada."))
}
pub(super) fn settings(db: &rusqlite::Connection, project_id: &str) -> Result<Settings, HttpError> {
    project(db, project_id)?;
    db.query_row(
        "SELECT payload FROM http_settings WHERE project_id=?1",
        [project_id],
        |row| row.get(0),
    )
    .optional()?
    .map(decode)
    .unwrap_or_else(|| {
        Ok(Settings {
            project_id: project_id.into(),
            ..Settings::default()
        })
    })
}
pub(super) fn list<T: DeserializeOwned>(
    db: &rusqlite::Connection,
    table: &str,
    column: &str,
    owner: &str,
) -> Result<Vec<T>, HttpError> {
    // Table/column come only from constant internal call sites.
    let mut stmt = db.prepare(&format!(
        "SELECT payload FROM {table} WHERE {column}=?1 ORDER BY rowid DESC"
    ))?;
    let result = stmt
        .query_map([owner], |row| row.get::<_, String>(0))?
        .map(|value| decode(value.map_err(HttpError::from)?))
        .collect();
    result
}
pub(super) fn draft(
    db: &rusqlite::Connection,
    conversation: &str,
    id: &str,
) -> Result<Draft, HttpError> {
    decode(
        db.query_row(
            "SELECT payload FROM http_drafts WHERE id=?1 AND conversation_id=?2",
            params![id, conversation],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| invalid("Requisição não encontrada nesta conversa."))?,
    )
}
pub(super) fn run(
    db: &rusqlite::Connection,
    conversation: &str,
    id: &str,
) -> Result<Run, HttpError> {
    decode(
        db.query_row(
            "SELECT payload FROM http_runs WHERE id=?1 AND conversation_id=?2",
            params![id, conversation],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| invalid("Resultado não encontrado nesta conversa."))?,
    )
}
pub(super) fn update_run(db: &rusqlite::Connection, value: &Run) -> Result<(), HttpError> {
    db.execute(
        "UPDATE http_runs SET payload=?1 WHERE id=?2 AND conversation_id=?3",
        params![encode(value)?, value.id, value.conversation_id],
    )?;
    Ok(())
}
pub(super) fn secret_key(project: &str, key: &str) -> String {
    format!("http:{project}:{key}")
}
pub(super) fn secret_load(project: &str, key: &str) -> Result<String, HttpError> {
    crate::mcp::Keychain.load(&secret_key(project,key)).map_err(|_| error("http_secret_unavailable", "Não foi possível ler a credencial HTTP. Desbloqueie o cofre do sistema ou configure novamente o segredo."))
}
pub(super) fn secret_store(project: &str, key: &str, value: &str) -> Result<(), HttpError> {
    crate::mcp::Keychain
        .store(&secret_key(project, key), value)
        .map_err(|_| {
            error(
                "http_secret_unavailable",
                "Não foi possível salvar a credencial HTTP no cofre do sistema.",
            )
        })
}
pub(super) fn secret_delete(project: &str, key: &str) {
    let _ = crate::mcp::Keychain.delete(&secret_key(project, key));
}
pub(super) fn masked_settings(mut value: Settings) -> Settings {
    for variable in value.variables.iter_mut().chain(
        value
            .environments
            .iter_mut()
            .flat_map(|env| &mut env.variables),
    ) {
        if variable.secret {
            variable.value.clear();
        }
        variable.secret_ref = None;
    }
    value
}
pub(super) fn variable_secret(project: &str, variable: &Variable) -> Result<String, HttpError> {
    secret_load(project, &variable_key(variable))
}
fn variable_key(variable: &Variable) -> String {
    variable
        .secret_ref
        .clone()
        .unwrap_or_else(|| format!("var:{}", variable.id))
}
pub(super) fn validate_settings(settings: &Settings) -> Result<(), HttpError> {
    let d = &settings.defaults;
    if !(1..=120).contains(&d.connect_timeout_seconds)
        || !(1..=3600).contains(&d.read_timeout_seconds)
        || d.total_timeout_seconds
            .is_some_and(|n| !(1..=86400).contains(&n))
        || !(1024..=100 * 1024 * 1024).contains(&d.max_response_bytes)
        || !(1..=1000).contains(&d.history_limit)
        || settings.environments.len() > 30
    {
        return Err(invalid("Limites do cliente HTTP inválidos."));
    }
    if !d.proxy_url.is_empty() {
        let proxy = url::Url::parse(&d.proxy_url).map_err(|_| invalid("URL do proxy inválida."))?;
        if !matches!(proxy.scheme(), "http" | "https")
            || !proxy.username().is_empty()
            || proxy.password().is_some()
        {
            return Err(invalid("Use um proxy HTTP(S) sem credenciais na URL."));
        }
    }
    if d.ca_file.len() > 4096 {
        return Err(invalid("Caminho do certificado muito longo."));
    }
    let mut ids = HashSet::new();
    validate_variables(&settings.variables, &mut ids)?;
    let mut env_ids = HashSet::new();
    for env in &settings.environments {
        valid_id(&env.id)?;
        if env.name.trim().is_empty()
            || env.name.len() > 100
            || env.color.len() > 32
            || !env_ids.insert(&env.id)
        {
            return Err(invalid("Ambiente HTTP inválido ou duplicado."));
        }
        validate_variables(&env.variables, &mut ids)?;
    }
    Ok(())
}
fn validate_variables<'a>(
    variables: &'a [Variable],
    ids: &mut HashSet<&'a str>,
) -> Result<(), HttpError> {
    if variables.len() > 200 {
        return Err(invalid("Use até 200 variáveis por ambiente."));
    }
    let mut names = HashSet::new();
    for v in variables {
        valid_id(&v.id)?;
        if v.name.is_empty()
            || v.name.len() > 100
            || !v
                .name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
            || v.value.len() > 32000
            || !ids.insert(&v.id)
            || !names.insert(&v.name)
        {
            return Err(invalid("Variável HTTP inválida ou duplicada."));
        }
    }
    Ok(())
}
pub(super) fn save_settings(
    db: &mut rusqlite::Connection,
    project: &str,
    input: Settings,
) -> Result<Settings, HttpError> {
    save_settings_with(db, project, input, |_| Ok(()))
}
pub(super) fn save_settings_with(
    db: &mut rusqlite::Connection,
    project: &str,
    input: Settings,
    related: impl FnOnce(&rusqlite::Connection) -> Result<(), HttpError>,
) -> Result<Settings, HttpError> {
    save_settings_using(db, project, input, &crate::mcp::Keychain, related)
}
pub(super) fn save_settings_using(
    db: &mut rusqlite::Connection,
    project: &str,
    mut input: Settings,
    vault: &dyn Secrets,
    related: impl FnOnce(&rusqlite::Connection) -> Result<(), HttpError>,
) -> Result<Settings, HttpError> {
    validate_settings(&input)?;
    let mut created = Vec::new();
    let mut obsolete = Vec::new();
    let result = (|| {
        let tx = db.transaction()?;
        let old = settings(&tx, project)?;
        if input.project_id != project || input.revision != old.revision {
            return Err(conflict());
        }
        let old_variables: Vec<_> = old
            .variables
            .iter()
            .chain(old.environments.iter().flat_map(|env| &env.variables))
            .collect();
        for variable in input.variables.iter_mut().chain(
            input
                .environments
                .iter_mut()
                .flat_map(|env| &mut env.variables),
        ) {
            // Never trust a client-supplied vault reference. New values are copy-on-write.
            variable.secret_ref = None;
            if variable.secret {
                if !variable.value.is_empty() {
                    let key = format!("var:{}", id()?);
                    let qualified = secret_key(project, &key);
                    created.push(qualified.clone());
                    vault.store(&qualified, &variable.value).map_err(|_| {
                        error(
                            "http_secret_unavailable",
                            "Não foi possível salvar a credencial HTTP no cofre do sistema.",
                        )
                    })?;
                    variable.secret_ref = Some(key);
                    variable.configured = true;
                } else if variable.configured {
                    let previous = old_variables
                        .iter()
                        .find(|old| old.id == variable.id && old.secret && old.configured)
                        .ok_or_else(|| invalid("Informe o valor da nova variável secreta."))?;
                    variable.secret_ref = Some(variable_key(previous));
                }
                variable.value.clear();
            } else {
                variable.configured = false;
            }
        }
        input.revision += 1;
        tx.execute("INSERT INTO http_settings(project_id,payload) VALUES(?1,?2) ON CONFLICT(project_id) DO UPDATE SET payload=excluded.payload",params![project,encode(&input)?])?;
        related(&tx)?;
        let retained: HashSet<_> = input
            .variables
            .iter()
            .chain(input.environments.iter().flat_map(|env| &env.variables))
            .filter(|v| v.secret && v.configured)
            .map(variable_key)
            .collect();
        for variable in old_variables
            .into_iter()
            .filter(|v| v.secret && v.configured)
        {
            let key = variable_key(variable);
            if !retained.contains(&key) {
                obsolete.push(secret_key(project, &key));
            }
        }
        tx.commit()?;
        Ok(input)
    })();
    // A failed SQL write/import cannot replace or delete the last working credential.
    for key in if result.is_ok() { &obsolete } else { &created } {
        let _ = vault.delete(key);
    }
    result
}
pub(super) fn sensitive(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
            | "x-api-key"
            | "api-key"
            | "api_key"
            | "token"
            | "access_token"
            | "refresh_token"
            | "password"
            | "secret"
    )
}
pub(super) fn validate_request(r: &Request) -> Result<(), HttpError> {
    if r.name.len() > 180
        || r.url.len() > 16000
        || r.params.len() > 200
        || r.headers.len() > 200
        || r.body.fields.len() > 200
        || r.body.text.len() > 2 * 1024 * 1024
        || reqwest::Method::from_bytes(r.method.as_bytes()).is_err()
        || !matches!(r.auth.kind.as_str(), "none" | "basic" | "bearer" | "apiKey")
        || !matches!(r.auth.location.as_str(), "header" | "query")
        || !matches!(
            r.body.kind.as_str(),
            "none" | "json" | "text" | "urlencoded" | "multipart" | "binary"
        )
    {
        return Err(invalid("Definição da requisição HTTP inválida."));
    }
    for p in r.params.iter().chain(&r.headers) {
        if p.name.len() > 1024 || p.value.len() > 32000 {
            return Err(invalid("Cabeçalho ou parâmetro excedeu o limite."));
        }
    }
    for p in &r.body.fields {
        if p.name.len() > 1024 || p.value.len() > 32000 {
            return Err(invalid("Campo de formulário excedeu o limite."));
        }
    }
    Ok(())
}
pub(super) fn protect_request(project: &str, r: &mut Request) -> Result<(), HttpError> {
    protect_request_using(project, r, &crate::mcp::Keychain)
}
pub(super) fn protect_request_using(
    project: &str,
    r: &mut Request,
    vault: &dyn Secrets,
) -> Result<(), HttpError> {
    validate_request(r)?;
    // URL query credentials enter the same protected/encoded path as editor rows.
    if let Some((base, query)) = r.url.split_once('?') {
        let (query, fragment) = query
            .split_once('#')
            .map_or((query, None), |(q, f)| (q, Some(f)));
        let pairs: Vec<_> = url::form_urlencoded::parse(query.as_bytes())
            .map(|(name, value)| Pair {
                name: name.into_owned(),
                value: value.into_owned(),
                enabled: true,
            })
            .collect();
        if pairs.iter().any(|p| sensitive(&p.name)) {
            let mut params = pairs;
            params.append(&mut r.params);
            r.params = params;
            r.url = match fragment {
                Some(fragment) => format!("{base}#{fragment}"),
                None => base.to_owned(),
            };
        }
    }
    validate_request(r)?;
    for value in [&mut r.auth.password, &mut r.auth.token, &mut r.auth.value] {
        protect_value(project, value, vault)?;
    }
    for p in r.headers.iter_mut().chain(&mut r.params) {
        if sensitive(&p.name) {
            protect_value(project, &mut p.value, vault)?;
        }
    }
    for field in &mut r.body.fields {
        if sensitive(&field.name) {
            protect_value(project, &mut field.value, vault)?;
        }
    }
    if r.body.kind == "json" {
        if let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&r.body.text) {
            protect_json(project, &mut json, vault)?;
            r.body.text = serde_json::to_string_pretty(&json).map_err(|_| storage())?;
        }
    }
    Ok(())
}
fn protect_json(
    project: &str,
    value: &mut serde_json::Value,
    vault: &dyn Secrets,
) -> Result<(), HttpError> {
    match value {
        serde_json::Value::Object(fields) => {
            for (name, value) in fields {
                if sensitive(name) {
                    if let Some(text) = value.as_str() {
                        let mut text = text.to_owned();
                        protect_value(project, &mut text, vault)?;
                        *value = serde_json::Value::String(text);
                    }
                } else {
                    protect_json(project, value, vault)?;
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                protect_json(project, item, vault)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn protect_value(project: &str, value: &mut String, vault: &dyn Secrets) -> Result<(), HttpError> {
    if !value.is_empty() && !value.contains("{{") {
        let key = format!("inline:{}", id()?);
        vault
            .store(&secret_key(project, &key), value)
            .map_err(|_| {
                error(
                    "http_secret_unavailable",
                    "Não foi possível salvar a credencial HTTP no cofre do sistema.",
                )
            })?;
        *value = format!("{{{{secret:{key}}}}}");
    }
    Ok(())
}
pub(super) fn secret_variants(secrets: &[String]) -> Vec<String> {
    let mut values = Vec::new();
    for secret in secrets.iter().filter(|s| !s.is_empty()) {
        for text in std::iter::once(secret.as_str())
            .chain(secret.strip_prefix("Bearer "))
            .chain(secret.strip_prefix("Basic "))
        {
            values.push(text.to_owned());
            let encoded = url::form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>();
            values.push(encoded.replace('+', "%20"));
            values.push(encoded);
            if let Ok(json) = serde_json::to_string(text) {
                values.push(json[1..json.len() - 1].to_owned());
            }
        }
    }
    values.retain(|s| !s.is_empty());
    values.sort_by_key(|s| std::cmp::Reverse(s.len()));
    values.dedup();
    values
}
pub(super) fn redact_bytes(bytes: &mut [u8], secrets: &[String]) {
    let original = bytes.to_vec();
    for value in secret_variants(secrets) {
        for (index, window) in original.windows(value.len()).enumerate() {
            if window == value.as_bytes() {
                bytes[index..index + value.len()].fill(b'*');
            }
        }
    }
}
pub(super) fn redact_truncated_tail(bytes: &mut [u8], secrets: &[String]) {
    let original = bytes.to_vec();
    redact_bytes(bytes, secrets);
    for value in secret_variants(secrets) {
        for length in (1..value.len().min(original.len() + 1)).rev() {
            if original.ends_with(&value.as_bytes()[..length]) {
                let start = bytes.len() - length;
                bytes[start..].fill(b'*');
                break;
            }
        }
    }
}
pub(super) fn redact(value: &str, secrets: &[String]) -> String {
    let mut value = value.to_string();
    for secret in secret_variants(secrets) {
        value = value.replace(&secret, "[REDACTED]");
    }
    value
}
pub(super) fn redact_request(request: &Request, secrets: &[String]) -> Request {
    let mut result = request.clone();
    if let Ok(value) = serde_json::to_value(&result) {
        fn clean(value: serde_json::Value, secrets: &[String]) -> serde_json::Value {
            match value {
                serde_json::Value::String(s) => serde_json::Value::String(redact(&s, secrets)),
                serde_json::Value::Array(v) => v.into_iter().map(|v| clean(v, secrets)).collect(),
                serde_json::Value::Object(v) => {
                    v.into_iter().map(|(k, v)| (k, clean(v, secrets))).collect()
                }
                other => other,
            }
        }
        if let Ok(parsed) = serde_json::from_value(clean(value, secrets)) {
            result = parsed;
        }
    }
    for s in [
        &mut result.auth.password,
        &mut result.auth.token,
        &mut result.auth.value,
    ] {
        if !s.is_empty() {
            *s = "[REDACTED]".into();
        }
    }
    for p in result.headers.iter_mut().chain(&mut result.params) {
        if sensitive(&p.name) && !p.value.is_empty() {
            p.value = "[REDACTED]".into();
        }
    }
    result
}
