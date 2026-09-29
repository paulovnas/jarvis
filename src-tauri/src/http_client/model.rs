use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct Pair {
    pub name: String,
    pub value: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}
pub(super) fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct Variable {
    pub id: String,
    pub name: String,
    pub value: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub secret: bool,
    pub configured: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_ref: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct Environment {
    pub id: String,
    pub name: String,
    pub color: String,
    pub variables: Vec<Variable>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct Defaults {
    pub connect_timeout_seconds: u64,
    pub read_timeout_seconds: u64,
    pub total_timeout_seconds: Option<u64>,
    pub max_response_bytes: u64,
    pub history_limit: usize,
    pub follow_redirects: bool,
    pub verify_tls: bool,
    pub proxy_url: String,
    pub ca_file: String,
}
impl Default for Defaults {
    fn default() -> Self {
        Self {
            connect_timeout_seconds: 10,
            read_timeout_seconds: 30,
            total_timeout_seconds: None,
            max_response_bytes: 10 * 1024 * 1024,
            history_limit: 100,
            follow_redirects: false,
            verify_tls: true,
            proxy_url: String::new(),
            ca_file: String::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct Settings {
    pub project_id: String,
    pub revision: u64,
    pub variables: Vec<Variable>,
    pub environments: Vec<Environment>,
    pub defaults: Defaults,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct Auth {
    #[serde(rename = "type")]
    pub kind: String,
    pub username: String,
    pub password: String,
    pub token: String,
    pub name: String,
    pub value: String,
    pub location: String,
}
impl Default for Auth {
    fn default() -> Self {
        Self {
            kind: "none".into(),
            username: String::new(),
            password: String::new(),
            token: String::new(),
            name: String::new(),
            value: String::new(),
            location: "header".into(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct BodyField {
    pub name: String,
    pub value: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub file_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct Body {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: String,
    pub fields: Vec<BodyField>,
    pub file_id: Option<String>,
}
impl Default for Body {
    fn default() -> Self {
        Self {
            kind: "none".into(),
            text: String::new(),
            fields: Vec::new(),
            file_id: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct Request {
    pub name: String,
    pub method: String,
    pub url: String,
    pub environment_id: Option<String>,
    pub params: Vec<Pair>,
    pub headers: Vec<Pair>,
    pub auth: Auth,
    pub body: Body,
}
impl Default for Request {
    fn default() -> Self {
        Self {
            name: "Nova requisição".into(),
            method: "GET".into(),
            url: String::new(),
            environment_id: None,
            params: Vec::new(),
            headers: Vec::new(),
            auth: Auth::default(),
            body: Body::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Draft {
    pub id: String,
    pub conversation_id: String,
    pub project_id: String,
    pub revision: u64,
    pub saved_request_id: Option<String>,
    pub request: Request,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SavedRequest {
    pub id: String,
    pub project_id: String,
    pub revision: u64,
    pub request: Request,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Run {
    pub id: String,
    pub project_id: String,
    pub conversation_id: String,
    pub draft_id: String,
    pub draft_revision: u64,
    pub environment_id: Option<String>,
    pub environment_revision: u64,
    pub request: Request,
    pub status: String,
    pub http_status: Option<u16>,
    pub error: Option<String>,
    pub outcome_uncertain: bool,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub elapsed_ms: u64,
    pub received_bytes: u64,
    pub stored_bytes: u64,
    pub mime: String,
    pub url: String,
    pub headers: Vec<Pair>,
    pub redirects: Vec<String>,
    pub truncated: bool,
    pub body_expired: bool,
    pub preview: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Snapshot {
    pub project_id: String,
    pub conversation_id: String,
    pub drafts: Vec<Draft>,
    pub saved_requests: Vec<SavedRequest>,
    pub runs: Vec<Run>,
    pub settings: Settings,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResultPage {
    pub run: Run,
    pub text: String,
    pub offset: u64,
    pub next_offset: Option<u64>,
    pub total_bytes: u64,
    pub binary: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileRef {
    pub id: String,
    pub name: String,
    pub size: u64,
    #[serde(skip_serializing, default)]
    pub source: String,
}
