use super::{error, store, SkillError};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    sync::Mutex,
    time::{Duration, Instant},
};

type Cache = BTreeMap<String, (Instant, Vec<Entry>)>;
static CACHE: Mutex<Cache> = Mutex::new(BTreeMap::new());
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub skill_id: String,
    pub name: String,
    pub source: String,
    pub installs: u64,
}

fn entry(value: &Value) -> Option<Entry> {
    let source = value["source"].as_str()?;
    let skill_id = value
        .get("skillId")
        .or_else(|| value.get("skill_id"))
        .or_else(|| value.get("id"))?
        .as_str()?;
    store::validate(source, skill_id).ok()?;
    Some(Entry {
        id: format!("{source}/{skill_id}"),
        source: source.into(),
        skill_id: skill_id.into(),
        name: value["name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(skill_id)
            .chars()
            .take(128)
            .collect(),
        installs: value["installs"].as_u64().unwrap_or(0),
    })
}
fn unique(values: impl IntoIterator<Item = Entry>) -> Vec<Entry> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|v| seen.insert(v.id.clone()))
        .take(2000)
        .collect()
}
pub(super) fn parse_search(raw: &str) -> Result<Vec<Entry>, SkillError> {
    let value: Value =
        serde_json::from_str(raw).map_err(|_| error("Resposta inválida do Marketplace."))?;
    let values = value
        .as_array()
        .or_else(|| value["skills"].as_array())
        .ok_or_else(|| error("Resposta inválida do Marketplace."))?;
    Ok(unique(values.iter().filter_map(entry)))
}
pub(super) fn parse_board(raw: &str) -> Result<Vec<Entry>, SkillError> {
    // Next.js may put the records in plain JSON or in escaped RSC strings.
    let decoded = raw.replace("\\\"", "\"").replace("\\/", "/");
    let objects = regex::Regex::new(r#"\{[^{}]{0,4096}"(?:skillId|skill_id)"[^{}]{0,4096}\}"#)
        .map_err(|_| error("Não foi possível ler o catálogo."))?;
    let entries = unique(
        objects
            .find_iter(&decoded)
            .filter_map(|m| serde_json::from_str::<Value>(m.as_str()).ok())
            .filter_map(|v| entry(&v)),
    );
    if entries.is_empty() {
        return Err(error(
            "O catálogo está indisponível. Tente pesquisar uma skill.",
        ));
    }
    Ok(entries)
}
pub(super) fn browse(query: &str, ranking: &str, limit: usize) -> Result<Vec<Entry>, SkillError> {
    let query = query.trim();
    if query.len() > 160 || !matches!(ranking, "alltime" | "trending" | "hot") {
        return Err(error("Pesquisa inválida."));
    }
    let limit = limit.clamp(24, 600);
    let key = format!("{ranking}:{limit}:{query}");
    if let Some((time, entries)) = CACHE
        .lock()
        .map_err(|_| error("Marketplace ocupado."))?
        .get(&key)
    {
        if time.elapsed() < Duration::from_secs(120) {
            return Ok(entries.clone());
        }
    }
    let mut url = url::Url::parse(if query.is_empty() {
        match ranking {
            "trending" => "https://skills.sh/trending",
            "hot" => "https://skills.sh/hot",
            _ => "https://skills.sh/",
        }
    } else {
        "https://skills.sh/api/search"
    })
    .map_err(|_| error("Endereço do Marketplace inválido."))?;
    if !query.is_empty() {
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("limit", &limit.to_string());
    }
    let client = reqwest::blocking::Client::builder()
        .user_agent("Jarvis-Skills/0.1")
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| error("Não foi possível conectar ao Marketplace."))?;
    let response = client
        .get(url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|_| error("Marketplace indisponível. Verifique a conexão."))?;
    let mut bytes = Vec::new();
    response
        .take(12 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error("Não foi possível carregar o catálogo."))?;
    if bytes.len() > 12 * 1024 * 1024 {
        return Err(error("O catálogo excede o tamanho suportado."));
    }
    let raw = String::from_utf8(bytes).map_err(|_| error("Resposta inválida do Marketplace."))?;
    let entries = if query.is_empty() {
        parse_board(&raw)?
    } else {
        parse_search(&raw)?
    };
    let mut cache = CACHE.lock().map_err(|_| error("Marketplace ocupado."))?;
    cache.retain(|_, (time, _)| time.elapsed() < Duration::from_secs(120));
    if cache.len() >= 30 {
        cache.clear();
    }
    cache.insert(key, (Instant::now(), entries.clone()));
    Ok(entries)
}
