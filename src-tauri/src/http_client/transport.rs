use super::*;
use base64::Engine;
use reqwest::{
    header::{
        HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, COOKIE,
        LOCATION,
    },
    Method, Url,
};
use std::{
    collections::BTreeMap,
    io::{Read, Seek, SeekFrom},
    time::Instant,
};
use tokio::io::AsyncWriteExt;

enum Payload {
    None,
    Bytes(Vec<u8>),
    Form(Vec<(String, String)>),
    Multipart(Vec<(String, Part)>),
}
enum Part {
    Text(String),
    File(String, Vec<u8>),
}
pub(super) struct Prepared {
    pub run: Run,
    client: reqwest::Client,
    method: Method,
    url: Url,
    headers: HeaderMap,
    body: Payload,
    secrets: Vec<String>,
    defaults: Defaults,
    home: PathBuf,
}

pub(super) fn interpolate(
    value: &str,
    values: &BTreeMap<String, String>,
    mut secret: impl FnMut(&str) -> Result<String, HttpError>,
) -> Result<String, HttpError> {
    let mut result = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("{{") {
        result.push_str(&rest[..start]);
        let tail = &rest[start + 2..];
        let end = tail
            .find("}}")
            .ok_or_else(|| invalid("Variável HTTP sem fechamento }}."))?;
        let name = tail[..end].trim();
        let resolved = if let Some(key) = name.strip_prefix("secret:") {
            secret(key)?
        } else {
            values.get(name).cloned().ok_or_else(||invalid(&format!("Variável HTTP ausente: {name}. Configure o valor compartilhado ou no ambiente selecionado.")))?
        };
        result.push_str(&resolved);
        rest = &tail[end + 2..];
        if result.len() > 4 * 1024 * 1024 {
            return Err(invalid("Requisição resolvida excede 4 MB."));
        }
    }
    result.push_str(rest);
    Ok(result)
}
fn resolve_template(
    value: &str,
    definitions: &BTreeMap<String, &Variable>,
    values: &mut BTreeMap<String, String>,
    secrets: &mut Vec<String>,
    project: &str,
) -> Result<String, HttpError> {
    let mut rest = value;
    while let Some(start) = rest.find("{{") {
        let tail = &rest[start + 2..];
        let end = tail
            .find("}}")
            .ok_or_else(|| invalid("Variável HTTP sem fechamento }}."))?;
        let name = tail[..end].trim();
        if !name.starts_with("secret:") && !values.contains_key(name) {
            let variable=definitions.get(name).ok_or_else(||invalid(&format!("Variável HTTP ausente: {name}. Configure o valor compartilhado ou no ambiente selecionado.")))?;
            let resolved = if variable.secret {
                if !variable.configured {
                    return Err(invalid(&format!(
                        "Configure a variável secreta {name} antes de enviar."
                    )));
                }
                let resolved = store::variable_secret(project, variable)?;
                secrets.push(resolved.clone());
                resolved
            } else {
                variable.value.clone()
            };
            values.insert(name.to_string(), resolved);
        }
        rest = &tail[end + 2..];
    }
    interpolate(value, values, |key| {
        if !key.starts_with("inline:") {
            return Err(invalid("Referência de segredo inválida."));
        }
        store::valid_id(key.trim_start_matches("inline:"))?;
        let value = store::secret_load(project, key)?;
        secrets.push(value.clone());
        Ok(value)
    })
}
fn resolve_json(
    text: &str,
    resolve: &mut impl FnMut(&str) -> Result<String, HttpError>,
) -> Result<String, HttpError> {
    fn walk(
        value: &mut serde_json::Value,
        resolve: &mut impl FnMut(&str) -> Result<String, HttpError>,
    ) -> Result<(), HttpError> {
        match value {
            serde_json::Value::String(s) => *s = resolve(s)?,
            serde_json::Value::Array(values) => {
                for v in values {
                    walk(v, resolve)?;
                }
            }
            serde_json::Value::Object(values) => {
                for v in values.values_mut() {
                    walk(v, resolve)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(text) {
        walk(&mut value, resolve)?;
        store::encode(&value)
    } else {
        resolve(text)
    }
}
fn file(
    home: &Path,
    project: &str,
    id: &str,
    root: Option<&Path>,
) -> Result<(String, Vec<u8>), HttpError> {
    store::valid_id(id)?;
    let dir = directory(home, project)?.join("files").join(id);
    let meta: FileRef = serde_json::from_slice(&std::fs::read(dir.join("metadata.json"))?)
        .map_err(|_| storage())?;
    if root.is_some_and(|r| !Path::new(&meta.source).starts_with(r)) {
        return Err(error(
            "http_file_scope",
            "O arquivo selecionado está fora do escopo do agente. Escolha um arquivo do projeto.",
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(dir.join("body"))?
        .take(20 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 20 * 1024 * 1024 {
        return Err(invalid("Arquivo HTTP excede 20 MB."));
    }
    Ok((meta.name, bytes))
}
fn add_header(headers: &mut HeaderMap, name: &str, value: &str) -> Result<(), HttpError> {
    let name = HeaderName::from_bytes(name.as_bytes())
        .map_err(|_| invalid("Nome de cabeçalho inválido."))?;
    if matches!(
        name.as_str(),
        "host" | "content-length" | "transfer-encoding" | "connection" | "upgrade"
    ) {
        return Err(invalid(
            "O transporte gerencia Host, Content-Length e cabeçalhos de conexão.",
        ));
    }
    let value =
        HeaderValue::from_str(value).map_err(|_| invalid("Valor de cabeçalho inválido."))?;
    headers.append(name, value);
    Ok(())
}
pub(super) fn prepare(
    app: &tauri::AppHandle,
    home: &Path,
    draft: &Draft,
    settings: &Settings,
    root: Option<&Path>,
) -> Result<Prepared, HttpError> {
    store::validate_settings(settings)?;
    store::validate_request(&draft.request)?;
    let environment = if let Some(id) = &draft.request.environment_id {
        Some(
            settings
                .environments
                .iter()
                .find(|e| &e.id == id)
                .ok_or_else(|| {
                    invalid("O ambiente selecionado não existe mais. Selecione outro ambiente.")
                })?,
        )
    } else {
        None
    };
    let mut values = BTreeMap::new();
    let mut secrets = Vec::new();
    let mut definitions = BTreeMap::new();
    for variable in settings
        .variables
        .iter()
        .chain(environment.into_iter().flat_map(|e| &e.variables))
        .filter(|v| v.enabled)
    {
        definitions.insert(variable.name.clone(), variable);
    }
    let mut request = draft.request.clone();
    let mut resolve = |value: &str| {
        resolve_template(
            value,
            &definitions,
            &mut values,
            &mut secrets,
            &draft.project_id,
        )
    };
    request.url = resolve(&request.url)?;
    if request.body.kind == "json" {
        request.body.text = resolve_json(&request.body.text, &mut resolve)?;
    } else if request.body.kind == "text" {
        request.body.text = resolve(&request.body.text)?;
    }
    for pair in request
        .params
        .iter_mut()
        .chain(&mut request.headers)
        .filter(|p| p.enabled)
    {
        pair.name = resolve(&pair.name)?;
        pair.value = resolve(&pair.value)?;
    }
    match request.auth.kind.as_str() {
        "basic" => {
            request.auth.username = resolve(&request.auth.username)?;
            request.auth.password = resolve(&request.auth.password)?;
        }
        "bearer" => request.auth.token = resolve(&request.auth.token)?,
        "apiKey" => {
            request.auth.name = resolve(&request.auth.name)?;
            request.auth.value = resolve(&request.auth.value)?;
        }
        _ => {}
    }
    if matches!(request.body.kind.as_str(), "urlencoded" | "multipart") {
        for field in request.body.fields.iter_mut().filter(|f| f.enabled) {
            field.name = resolve(&field.name)?;
            if field.file_id.is_none() {
                field.value = resolve(&field.value)?;
            }
        }
    }
    let mut url =
        Url::parse(&request.url).map_err(|_| invalid("Informe uma URL HTTP(S) completa."))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(invalid(
            "Use HTTP(S) e configure autenticação na seção própria.",
        ));
    }
    url.set_fragment(None);
    for pair in request
        .params
        .iter()
        .filter(|p| p.enabled && !p.name.is_empty())
    {
        url.query_pairs_mut().append_pair(&pair.name, &pair.value);
        if store::sensitive(&pair.name) {
            secrets.push(pair.value.clone());
        }
    }
    let mut headers = HeaderMap::new();
    for pair in request
        .headers
        .iter()
        .filter(|p| p.enabled && !p.name.is_empty())
    {
        add_header(&mut headers, &pair.name, &pair.value)?;
        if store::sensitive(&pair.name) {
            secrets.push(pair.value.clone());
        }
    }
    match request.auth.kind.as_str() {
        "basic" => {
            if headers.contains_key(AUTHORIZATION) {
                return Err(invalid(
                    "Configure Authorization apenas em Autenticação ou em Cabeçalhos.",
                ));
            }
            let value = format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(format!(
                    "{}:{}",
                    request.auth.username, request.auth.password
                ))
            );
            secrets.push(value.clone());
            add_header(&mut headers, "Authorization", &value)?;
        }
        "bearer" => {
            if headers.contains_key(AUTHORIZATION) {
                return Err(invalid(
                    "Configure Authorization apenas em Autenticação ou em Cabeçalhos.",
                ));
            }
            add_header(
                &mut headers,
                "Authorization",
                &format!("Bearer {}", request.auth.token),
            )?;
        }
        "apiKey" => {
            if request.auth.name.trim().is_empty() {
                return Err(invalid("Informe o nome da chave de API."));
            }
            if request.auth.location == "query" {
                url.query_pairs_mut()
                    .append_pair(&request.auth.name, &request.auth.value);
            } else {
                add_header(&mut headers, &request.auth.name, &request.auth.value)?;
            }
        }
        _ => {}
    }
    if let Some(value) = match request.auth.kind.as_str() {
        "basic" => Some(&request.auth.password),
        "bearer" => Some(&request.auth.token),
        "apiKey" => Some(&request.auth.value),
        _ => None,
    } {
        if !value.is_empty() {
            secrets.push(value.clone());
        }
    }
    let body = match request.body.kind.as_str() {
        "none" => Payload::None,
        "json" => {
            serde_json::from_str::<serde_json::Value>(&request.body.text).map_err(|_| {
                invalid("O corpo JSON é inválido depois de substituir as variáveis.")
            })?;
            headers
                .entry(CONTENT_TYPE)
                .or_insert(HeaderValue::from_static("application/json"));
            Payload::Bytes(request.body.text.as_bytes().to_vec())
        }
        "text" => {
            headers
                .entry(CONTENT_TYPE)
                .or_insert(HeaderValue::from_static("text/plain; charset=utf-8"));
            Payload::Bytes(request.body.text.as_bytes().to_vec())
        }
        "urlencoded" => Payload::Form(
            request
                .body
                .fields
                .iter()
                .filter(|f| f.enabled)
                .map(|f| (f.name.clone(), f.value.clone()))
                .collect(),
        ),
        "binary" => {
            let id = request
                .body
                .file_id
                .as_deref()
                .ok_or_else(|| invalid("Selecione o arquivo do corpo binário."))?;
            Payload::Bytes(file(home, &draft.project_id, id, root)?.1)
        }
        "multipart" => {
            if headers.contains_key(CONTENT_TYPE) {
                return Err(invalid(
                    "Remova Content-Type: o cliente gera o boundary multipart automaticamente.",
                ));
            }
            let mut fields = Vec::new();
            let mut total = 0;
            for field in request.body.fields.iter().filter(|f| f.enabled) {
                fields.push((
                    field.name.clone(),
                    if let Some(id) = &field.file_id {
                        let (name, bytes) = file(home, &draft.project_id, id, root)?;
                        total += bytes.len();
                        if total > 50 * 1024 * 1024 {
                            return Err(invalid("Os arquivos multipart excedem 50 MB no total."));
                        }
                        Part::File(name, bytes)
                    } else {
                        Part::Text(field.value.clone())
                    },
                ));
            }
            Payload::Multipart(fields)
        }
        _ => return Err(invalid("Tipo de corpo HTTP inválido.")),
    };
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(Duration::from_secs(
            settings.defaults.connect_timeout_seconds,
        ))
        .read_timeout(Duration::from_secs(settings.defaults.read_timeout_seconds))
        .tls_danger_accept_invalid_certs(!settings.defaults.verify_tls)
        .no_proxy();
    if let Some(seconds) = settings.defaults.total_timeout_seconds {
        builder = builder.timeout(Duration::from_secs(seconds));
    }
    if !settings.defaults.proxy_url.is_empty() {
        builder = builder.proxy(
            reqwest::Proxy::all(&settings.defaults.proxy_url)
                .map_err(|_| invalid("Proxy HTTP inválido."))?,
        );
    }
    let ca = if settings.defaults.ca_file.is_empty() {
        Vec::new()
    } else {
        let mut bytes = Vec::new();
        std::fs::File::open(&settings.defaults.ca_file)?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 1024 * 1024 {
            return Err(invalid("Certificado excede 1 MB."));
        }
        builder = builder.tls_certs_merge([reqwest::Certificate::from_pem(&bytes)
            .map_err(|_| invalid("Certificado CA PEM inválido."))?]);
        bytes
    };
    use sha2::Digest;
    let cache_key = format!(
        "{}:{:x}",
        store::encode(&settings.defaults)?,
        sha2::Sha256::digest(&ca)
    );
    let state = app.state::<HttpState>();
    let mut cache = state.clients.lock().map_err(|_| storage())?;
    let client = if let Some(client) = cache.get(&cache_key) {
        client.clone()
    } else {
        let client = builder
            .build()
            .map_err(|_| invalid("Não foi possível configurar o transporte HTTP."))?;
        if cache.len() >= 8 {
            cache.clear();
        }
        cache.insert(cache_key, client.clone());
        client
    };
    drop(cache);
    secrets.retain(|s| !s.is_empty());
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    secrets.dedup();
    let id = store::id()?;
    let dir = run_dir(home, &draft.project_id, &id)?;
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    if !secrets.is_empty() {
        store::secret_store(
            &draft.project_id,
            &format!("run:{id}"),
            &store::encode(&secrets)?,
        )?;
        std::fs::write(dir.join("redacted"), b"1")?;
    }
    let run = Run {
        id,
        project_id: draft.project_id.clone(),
        conversation_id: draft.conversation_id.clone(),
        draft_id: draft.id.clone(),
        draft_revision: draft.revision,
        environment_id: draft.request.environment_id.clone(),
        environment_revision: settings.revision,
        request: store::redact_request(&request, &secrets),
        status: "running".into(),
        http_status: None,
        error: None,
        outcome_uncertain: false,
        started_at: now(),
        finished_at: None,
        elapsed_ms: 0,
        received_bytes: 0,
        stored_bytes: 0,
        mime: String::new(),
        url: store::redact(url.as_str(), &secrets),
        headers: Vec::new(),
        redirects: Vec::new(),
        truncated: false,
        body_expired: false,
        preview: String::new(),
    };
    Ok(Prepared {
        run,
        client,
        method: Method::from_bytes(request.method.as_bytes())
            .map_err(|_| invalid("Método HTTP inválido."))?,
        url,
        headers,
        body,
        secrets,
        defaults: settings.defaults.clone(),
        home: home.into(),
    })
}
fn request(
    prepared: &Prepared,
    url: Url,
    method: Method,
    headers: HeaderMap,
    body: bool,
) -> reqwest::RequestBuilder {
    let mut request = prepared.client.request(method, url).headers(headers);
    if body {
        request = match &prepared.body {
            Payload::None => request,
            Payload::Bytes(bytes) => request.body(bytes.clone()),
            Payload::Form(fields) => request
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(
                    url::form_urlencoded::Serializer::new(String::new())
                        .extend_pairs(fields)
                        .finish(),
                ),
            Payload::Multipart(fields) => {
                let mut form = reqwest::multipart::Form::new();
                for (name, part) in fields {
                    form = match part {
                        Part::Text(text) => form.text(name.clone(), text.clone()),
                        Part::File(filename, bytes) => form.part(
                            name.clone(),
                            reqwest::multipart::Part::bytes(bytes.clone())
                                .file_name(filename.clone()),
                        ),
                    };
                }
                request.multipart(form)
            }
        };
    }
    request
}
fn transport_error(cause: &reqwest::Error) -> HttpError {
    if cause.is_timeout() {
        error("http_timeout","A conexão ou leitura excedeu o tempo configurado. O servidor pode ter recebido a requisição.")
    } else if cause.is_connect() {
        error(
            "http_connection",
            "Não foi possível conectar. Confira endereço, TLS, proxy e conectividade.",
        )
    } else {
        error(
            "http_transport",
            "A conexão HTTP foi interrompida. Verifique o resultado no servidor antes de reenviar.",
        )
    }
}
async fn perform(prepared: &Prepared, run: &mut Run) -> Result<(), HttpError> {
    let mut url = prepared.url.clone();
    let mut method = prepared.method.clone();
    let mut headers = prepared.headers.clone();
    let mut include_body = true;
    let mut response = loop {
        let response = request(
            prepared,
            url.clone(),
            method.clone(),
            headers.clone(),
            include_body,
        )
        .send()
        .await
        .map_err(|e| transport_error(&e))?;
        let status = response.status();
        run.http_status = Some(status.as_u16());
        run.url = store::redact(response.url().as_str(), &prepared.secrets);
        if !prepared.defaults.follow_redirects
            || !matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308)
            || !response.headers().contains_key(LOCATION)
        {
            break response;
        }
        if run.redirects.len() >= 10 {
            return Err(error(
                "http_redirect",
                "A requisição excedeu 10 redirecionamentos.",
            ));
        }
        let location = response
            .headers()
            .get(LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| invalid("Redirecionamento inválido."))?;
        let next = url
            .join(location)
            .map_err(|_| invalid("URL de redirecionamento inválida."))?;
        if !matches!(next.scheme(), "http" | "https")
            || !next.username().is_empty()
            || next.password().is_some()
        {
            return Err(invalid("Redirecionamento para URL não suportada."));
        }
        if (matches!(status.as_u16(), 301 | 302) && method == Method::POST)
            || (status.as_u16() == 303 && method != Method::HEAD)
        {
            method = Method::GET;
            include_body = false;
            headers.remove(CONTENT_TYPE);
            headers.remove(CONTENT_LENGTH);
        }
        if next.origin() != url.origin() {
            let variants = store::secret_variants(&prepared.secrets);
            let has_secret_body = include_body
                && match &prepared.body {
                    Payload::Bytes(b) => variants
                        .iter()
                        .filter(|s| !s.is_empty())
                        .any(|s| b.windows(s.len()).any(|v| v == s.as_bytes())),
                    Payload::Form(p) => p
                        .iter()
                        .any(|(_, v)| variants.iter().any(|s| v.contains(s))),
                    Payload::Multipart(p) => p.iter().any(|(_, v)| match v {
                        Part::Text(v) => variants.iter().any(|s| v.contains(s)),
                        Part::File(_, _) => true,
                    }),
                    Payload::None => false,
                };
            if has_secret_body
                || variants.iter().any(|s| next.as_str().contains(s))
                || next.query_pairs().any(|(_, value)| {
                    prepared
                        .secrets
                        .iter()
                        .filter(|s| !s.is_empty())
                        .any(|s| value.contains(s))
                })
                || !prepared.secrets.is_empty()
                    && url.scheme() == "https"
                    && next.scheme() == "http"
            {
                return Err(error("http_redirect_credentials","O redirecionamento exige enviar dados protegidos a outra origem. Inspecione o destino e configure uma requisição explícita."));
            }
            let remove: Vec<_> = headers
                .iter()
                .filter(|(name, value)| {
                    store::sensitive(name.as_str())
                        || value
                            .to_str()
                            .is_ok_and(|v| prepared.secrets.iter().any(|s| v.contains(s)))
                })
                .map(|(name, _)| name.clone())
                .collect();
            for name in remove {
                headers.remove(name);
            }
            headers.remove(AUTHORIZATION);
            headers.remove(COOKIE);
        }
        run.redirects
            .push(store::redact(next.as_str(), &prepared.secrets));
        url = next;
    };
    run.http_status = Some(response.status().as_u16());
    run.url = store::redact(response.url().as_str(), &prepared.secrets);
    run.mime = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .into();
    run.headers = response
        .headers()
        .iter()
        .map(|(name, value)| Pair {
            name: name.to_string(),
            value: if store::sensitive(name.as_str()) {
                "[REDACTED]".into()
            } else {
                store::redact(
                    &String::from_utf8_lossy(value.as_bytes()),
                    &prepared.secrets,
                )
            },
            enabled: true,
        })
        .collect();
    let path = run_dir(&prepared.home, &run.project_id, &run.id)?.join("body");
    let mut file = tokio::fs::File::create(path).await?;
    let mut preview = Vec::new();
    let preview_limit = 4096
        + store::secret_variants(&prepared.secrets)
            .iter()
            .map(String::len)
            .max()
            .unwrap_or(1)
            .saturating_sub(1);
    while let Some(bytes) = response.chunk().await.map_err(|e| transport_error(&e))? {
        run.received_bytes = run.received_bytes.saturating_add(bytes.len() as u64);
        let remaining = prepared
            .defaults
            .max_response_bytes
            .saturating_sub(run.stored_bytes) as usize;
        let kept = bytes.len().min(remaining);
        file.write_all(&bytes[..kept]).await?;
        run.stored_bytes += kept as u64;
        if preview.len() < preview_limit {
            preview.extend_from_slice(&bytes[..kept.min(preview_limit - preview.len())]);
        }
        if kept < bytes.len() {
            run.truncated = true;
            break;
        }
    }
    file.flush().await?;
    if run.truncated {
        store::redact_truncated_tail(&mut preview, &prepared.secrets);
    }
    store::redact_bytes(&mut preview, &prepared.secrets);
    preview.truncate(4096);
    run.preview = if std::str::from_utf8(&preview).is_ok() {
        String::from_utf8_lossy(&preview).into_owned()
    } else {
        String::new()
    };
    Ok(())
}
pub(super) async fn execute(
    app: tauri::AppHandle,
    prepared: Prepared,
    mut cancel: watch::Receiver<bool>,
    done: watch::Sender<bool>,
) {
    let started = Instant::now();
    let mut run = prepared.run.clone();
    let operation = async {
        if let Some(seconds) = prepared.defaults.total_timeout_seconds {
            tokio::time::timeout(Duration::from_secs(seconds), perform(&prepared, &mut run))
                .await
                .map_err(|_| {
                    error(
                        "http_timeout",
                        "A requisição excedeu o prazo total configurado.",
                    )
                })?
        } else {
            perform(&prepared, &mut run).await
        }
    };
    let result = tokio::select! { biased; _=async {if !*cancel.borrow(){let _=cancel.changed().await;}}=>Err(error("http_cancelled","Requisição cancelada. Isso não desfaz alterações já recebidas pelo servidor.")),result=operation=>result};
    run.elapsed_ms = started.elapsed().as_millis() as u64;
    run.finished_at = Some(now());
    match result {
        Ok(()) => run.status = "completed".into(),
        Err(cause) => {
            run.status = if cause.code == "http_cancelled" {
                "cancelled"
            } else {
                "failed"
            }
            .into();
            run.error = Some(cause.message);
            run.outcome_uncertain = true;
            run.truncated = run.stored_bytes > 0;
        }
    }
    let persisted = run.clone();
    let persistence_app = app.clone();
    let home = prepared.home.clone();
    let limit = prepared.defaults.history_limit;
    let saved = tauri::async_runtime::spawn_blocking(move || {
        let state = persistence_app.state::<AppState>();
        state.with_connection(&home, |db| {
            store::update_run(db, &persisted)?;
            let runs: Vec<Run> = store::list(
                db,
                "http_runs",
                "conversation_id",
                &persisted.conversation_id,
            )?;
            for (position, mut old) in runs
                .into_iter()
                .filter(|r| r.status != "running")
                .enumerate()
                .skip(limit)
            {
                if !old.body_expired {
                    let path = run_dir(&home, &old.project_id, &old.id)?;
                    let _ = std::fs::remove_file(path.join("body"));
                    let _ = std::fs::remove_file(path.join("redacted"));
                    store::secret_delete(&old.project_id, &format!("run:{}", old.id));
                    old.body_expired = true;
                    old.preview.clear();
                    store::update_run(db, &old)?;
                }
                if position >= 1000 {
                    db.execute(
                        "DELETE FROM http_runs WHERE id=?1 AND conversation_id=?2",
                        params![old.id, old.conversation_id],
                    )?;
                    let _ = std::fs::remove_dir_all(run_dir(&home, &old.project_id, &old.id)?);
                }
            }
            Ok::<_, HttpError>(())
        })
    })
    .await;
    if !matches!(saved, Ok(Ok(()))) {
        crate::diagnostics::record_storage_failure("http_run", None);
        run.error=Some(format!("{} O resultado permanece disponível nesta sessão, mas houve falha ao salvar o histórico.",run.error.as_deref().unwrap_or("Requisição concluída.")));
        if let Ok(mut terminal) = app.state::<HttpState>().terminal.lock() {
            terminal.insert(run.id.clone(), run.clone());
        }
    }
    if let Ok(mut running) = app.state::<HttpState>().running.lock() {
        running.remove(&run.id);
    }
    let _ = done.send(true);
    changed(&app, &run.conversation_id, Some(&run.id));
}
pub(super) fn read_result(
    home: &Path,
    run: Run,
    offset: u64,
    limit: usize,
) -> Result<ResultPage, HttpError> {
    let path = run_dir(home, &run.project_id, &run.id)?;
    let secrets = if !run.body_expired && path.join("redacted").exists() {
        store::decode(store::secret_load(
            &run.project_id,
            &format!("run:{}", run.id),
        )?)?
    } else {
        Vec::new()
    };
    read_result_with_secrets(home, run, offset, limit, &secrets)
}
pub(super) fn read_result_with_secrets(
    home: &Path,
    run: Run,
    offset: u64,
    limit: usize,
    secrets: &[String],
) -> Result<ResultPage, HttpError> {
    let path = run_dir(home, &run.project_id, &run.id)?;
    if run.body_expired {
        return Ok(ResultPage {
            total_bytes: run.stored_bytes,
            run,
            text: String::new(),
            offset: 0,
            next_offset: None,
            binary: false,
        });
    }
    let mut file = match std::fs::File::open(path.join("body")) {
        Ok(file) => file,
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ResultPage {
                total_bytes: 0,
                run,
                text: String::new(),
                offset: 0,
                next_offset: None,
                binary: false,
            })
        }
        Err(_) => return Err(storage()),
    };
    let total = file.metadata()?.len();
    let offset = offset.min(total);
    let overlap = store::secret_variants(secrets)
        .iter()
        .map(String::len)
        .max()
        .unwrap_or(1)
        .saturating_sub(1) as u64;
    let start = offset.saturating_sub(overlap);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.take(
        (offset - start)
            .saturating_add(limit as u64)
            .saturating_add(overlap),
    )
    .read_to_end(&mut bytes)?;
    let binary = bytes.contains(&0)
        || (!run.mime.contains("text")
            && !run.mime.contains("json")
            && !run.mime.contains("xml")
            && std::str::from_utf8(&bytes).is_err());
    // Read a halo before redacting, then slice; page boundaries cannot reveal token fragments.
    if run.truncated && start + bytes.len() as u64 == total {
        store::redact_truncated_tail(&mut bytes, secrets);
    }
    store::redact_bytes(&mut bytes, secrets);
    let relative = (offset - start) as usize;
    let selected = &bytes[relative..bytes.len().min(relative.saturating_add(limit))];
    let next = offset + selected.len() as u64;
    let text = if binary {
        String::new()
    } else {
        String::from_utf8_lossy(selected).into_owned()
    };
    Ok(ResultPage {
        run,
        text,
        offset,
        next_offset: (next < total).then_some(next),
        total_bytes: total,
        binary,
    })
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
