//! Retry only an inference request: completed local tools are never replayed.
use super::*;
use serde::{Deserialize, Serialize};

pub(super) const MAX_RETRIES: u8 = 5;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "RetryStatus"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    pub attempt: u8,
    pub max_attempts: u8,
    #[cfg_attr(test, ts(type = "number"))]
    pub retry_at: u64,
    pub message: String,
}

pub(super) fn retryable(error: &AgentError) -> bool {
    matches!(
        error.code.as_str(),
        "provider_network"
            | "provider_timeout"
            | "provider_unavailable"
            | "provider_limit"
            | "provider_protocol"
            | "provider_incomplete"
            | "provider_failed"
    )
}

pub(super) struct Request<'a> {
    pub client: reqwest::Client,
    pub credential: &'a CodexCredential,
    pub authentication: Option<&'a auth::Authentication>,
    pub session_id: &'a str,
    pub options: &'a TurnOptions,
    pub capabilities: ModelCapabilities,
    pub instructions: &'a str,
    pub input: Vec<Value>,
    pub tools: Vec<Value>,
    pub telemetry: super::super::telemetry::TraceContext,
}

impl Request<'_> {
    pub(super) async fn run(
        self,
        mut signal: watch::Receiver<bool>,
        mut emit: impl FnMut(Delta) -> Result<(), AgentError>,
        base_delay: Duration,
    ) -> Result<Response, AgentError> {
        let mut retries = 0_u8;
        let mut auth_retried = false;
        loop {
            if *signal.borrow() {
                return Err(AgentError::cancelled());
            }
            let credential = match self.authentication {
                Some(auth) => {
                    std::borrow::Cow::Owned(auth.credential(false, signal.clone()).await?)
                }
                None => std::borrow::Cow::Borrowed(self.credential),
            };
            if retries > 0 || auth_retried {
                emit(Delta::Reset)?;
            }
            let attempt = retries.saturating_add(1 + u8::from(auth_retried));
            let provider = super::super::telemetry::provider_kind(self.credential);
            let model_id = super::super::telemetry::model_id(&self.options.model);
            let input_bytes = super::super::telemetry::serialized_bytes(&self.input)
                .saturating_add(super::super::telemetry::serialized_bytes(&self.tools))
                .saturating_add(u64::try_from(self.instructions.len()).unwrap_or(u64::MAX))
                .min(512 * 1024 * 1024);
            super::super::telemetry::record(
                &self.telemetry,
                super::super::telemetry::Event::ProviderRequest {
                    provider,
                    model_id: model_id.clone(),
                    attempt,
                    input_items: u64::try_from(self.input.len()).unwrap_or(u64::MAX),
                    input_bytes,
                    advertised_tools: u64::try_from(self.tools.len()).unwrap_or(u64::MAX),
                },
            );
            let request_started = std::time::Instant::now();
            let mut first_event_ms = None;
            let mut reconnecting = retries > 0 || auth_retried;
            let result = stream_once(
                &self.client,
                &credential,
                self.session_id,
                self.options,
                &self.capabilities,
                self.instructions,
                self.input.clone(),
                self.tools.clone(),
                signal.clone(),
                |delta| {
                    if first_event_ms.is_none()
                        && matches!(delta, Delta::Text(_) | Delta::Summary(_))
                    {
                        first_event_ms = Some(
                            u64::try_from(request_started.elapsed().as_millis())
                                .unwrap_or(u64::MAX),
                        );
                    }
                    // Streaming has resumed, but only a complete response resets
                    // the failure budget. Another truncated stream still counts.
                    if reconnecting && matches!(delta, Delta::Text(_) | Delta::Summary(_)) {
                        emit(Delta::Retry(None))?;
                        reconnecting = false;
                    }
                    emit(delta)
                },
            )
            .await;
            let recover_auth = !auth_retried
                && self.authentication.is_some()
                && result.as_ref().err().is_some_and(auth::unauthorized);
            let will_retry = recover_auth
                || result
                    .as_ref()
                    .err()
                    .is_some_and(|error| retryable(error) && retries < MAX_RETRIES);
            let response_error = result.as_ref().err();
            let usage = result
                .as_ref()
                .ok()
                .and_then(|response| response.usage.as_ref());
            super::super::telemetry::record(
                &self.telemetry,
                super::super::telemetry::Event::ProviderResponse {
                    provider,
                    model_id,
                    attempt,
                    outcome: super::super::telemetry::outcome(response_error, will_retry),
                    duration_ms: u64::try_from(request_started.elapsed().as_millis())
                        .unwrap_or(u64::MAX),
                    first_event_ms,
                    input_tokens: usage.map(|usage| usage.input_tokens),
                    output_tokens: usage.map(|usage| usage.output_tokens),
                    cache_read_tokens: usage.and_then(|usage| usage.cache_read_tokens),
                    cache_write_tokens: usage.and_then(|usage| usage.cache_write_tokens),
                    failure: response_error.map(super::super::telemetry::failure_class),
                },
            );
            match result {
                Ok(response) => {
                    if reconnecting { emit(Delta::Retry(None))?; }
                    return Ok(response);
                }
                Err(_) if recover_auth => {
                    auth_retried = true;
                    if let Some(auth) = self.authentication {
                        emit(Delta::Retry(Some(Status { attempt: 1, max_attempts: 1, retry_at: super::super::now(), message: "Renovando a autenticação da conta para continuar.".into() })))?;
                        auth.credential(true, signal.clone()).await?;
                    }
                }
                Err(error) if !retryable(&error) => return Err(error),
                Err(error) if retries == MAX_RETRIES => return Err(AgentError::new("provider_retry_exhausted", &format!("Não foi possível reconectar após {MAX_RETRIES} tentativas consecutivas. {} O progresso concluído foi preservado.", error.message))),
                Err(error) => {
                    retries += 1;
                    let delay = backoff(retries, base_delay, error.retry_after);
                    emit(Delta::Retry(Some(Status { attempt: retries, max_attempts: MAX_RETRIES, retry_at: super::super::now().saturating_add(u64::try_from(delay.as_millis()).unwrap_or(u64::MAX)), message: error.message })))?;
                    tokio::select! {
                        biased;
                        _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
            }
        }
    }
}

fn backoff(attempt: u8, base: Duration, retry_after: Option<Duration>) -> Duration {
    base.saturating_mul(1 << (attempt - 1))
        .min(Duration::from_secs(30))
        .max(retry_after.unwrap_or_default())
}

#[cfg(test)]
mod tests;
