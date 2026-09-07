//! Retry only an inference request: completed local tools are never replayed.
use super::*;
use serde::{Deserialize, Serialize};

pub(super) const MAX_RETRIES: u8 = 5;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    pub attempt: u8,
    pub max_attempts: u8,
    pub retry_at: u64,
    pub message: String,
}

fn retryable(error: &AgentError) -> bool {
    matches!(
        error.code.as_str(),
        "provider_network"
            | "provider_timeout"
            | "provider_unavailable"
            | "provider_limit"
            | "provider_protocol"
            | "provider_failed"
    )
}

pub(super) struct Request<'a> {
    pub credential: &'a CodexCredential,
    pub session_id: &'a str,
    pub options: &'a TurnOptions,
    pub instructions: &'a str,
    pub input: Vec<Value>,
    pub tools: Vec<Value>,
}

impl Request<'_> {
    pub(super) async fn run(
        self,
        mut signal: watch::Receiver<bool>,
        mut emit: impl FnMut(Delta) -> Result<(), AgentError>,
        base_delay: Duration,
    ) -> Result<Response, AgentError> {
        let mut retries = 0;
        loop {
            if *signal.borrow() {
                return Err(AgentError::cancelled());
            }
            if retries > 0 {
                emit(Delta::Reset)?;
            }
            let mut reconnecting = retries > 0;
            let result = stream_once(
                self.credential,
                self.session_id,
                self.options,
                self.instructions,
                self.input.clone(),
                self.tools.clone(),
                signal.clone(),
                |delta| {
                    // Streaming has resumed, but only a complete response resets
                    // the failure budget. Another truncated stream still counts.
                    if reconnecting && matches!(delta, Delta::Text(_) | Delta::Summary(_)) {
                        emit(Delta::Retry(None))?;
                        reconnecting = false;
                    }
                    emit(delta)
                },
            )
            .await
            .and_then(|response| {
                tool_calls(&response.output)?;
                Ok(response)
            });
            match result {
                Ok(response) => {
                    if reconnecting { emit(Delta::Retry(None))?; }
                    return Ok(response);
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
