//! Presentation-only inference throughput. Tool execution never owns this clock.
use super::{provider, Step};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "GenerationMetrics"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct Metrics {
    #[cfg_attr(test, ts(type = "number"))]
    pub output_tokens: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub duration_ms: u64,
    pub estimated: bool,
}

pub(super) fn aggregate(steps: &[Step]) -> Option<Metrics> {
    steps
        .iter()
        .filter_map(|step| step.generation.as_ref())
        .filter(|sample| sample.duration_ms > 0)
        .fold(None, |total, sample| {
            Some(match total {
                None => sample.clone(),
                Some(total) => Metrics {
                    output_tokens: total.output_tokens.saturating_add(sample.output_tokens),
                    duration_ms: total.duration_ms.saturating_add(sample.duration_ms),
                    estimated: total.estimated || sample.estimated,
                },
            })
        })
}

pub(super) struct Clock {
    started: Instant,
    bytes: u64,
    stopped: Option<Instant>,
    output_tokens: Option<u64>,
}

impl Clock {
    pub(super) fn new(started: Instant) -> Self {
        Self {
            started,
            bytes: 0,
            stopped: None,
            output_tokens: None,
        }
    }

    pub(super) fn append(&mut self, bytes: usize) {
        self.bytes = self
            .bytes
            .saturating_add(bytes.try_into().unwrap_or(u64::MAX));
    }

    pub(super) fn reported(&mut self, output_tokens: Option<u64>) {
        if let Some(tokens) = output_tokens.filter(|tokens| *tokens > 0) {
            self.output_tokens = Some(tokens);
        }
    }

    pub(super) fn observe(&mut self, delta: &provider::Delta) -> bool {
        match delta {
            provider::Delta::Text(text) | provider::Delta::Summary(text) => {
                self.append(text.len());
                !text.is_empty()
            }
            provider::Delta::ToolReady(ready) => {
                self.append(ready.call.name.len());
                self.append(ready.call.args.to_string().len());
                true
            }
            _ => false,
        }
    }

    pub(super) fn finish(&mut self, now: Instant, output_tokens: Option<u64>) {
        self.stopped.get_or_insert(now);
        if output_tokens.is_some() {
            self.output_tokens = output_tokens;
        }
    }

    pub(super) fn complete_response(&mut self, response: &provider::Response) {
        // Some protocols buffer complete calls and expose no argument deltas.
        let output_tokens = response
            .usage
            .as_ref()
            .map(|usage| usage.output_tokens)
            .filter(|tokens| *tokens > 0);
        if output_tokens.is_none() {
            self.bytes = response.text.len().saturating_add(response.summary.len()) as u64;
            for call in response.tool_calls() {
                self.append(call.name.len());
                self.append(call.args.to_string().len());
            }
        }
        self.finish(Instant::now(), output_tokens);
    }

    pub(super) fn snapshot(&self, now: Instant) -> Option<Metrics> {
        let output_tokens = self.output_tokens.unwrap_or_else(|| self.bytes.div_ceil(4));
        (output_tokens > 0).then(|| Metrics {
            output_tokens,
            duration_ms: self
                .stopped
                .unwrap_or(now)
                .saturating_duration_since(self.started)
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX),
            estimated: self.output_tokens.is_none(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn estimate_is_reconciled_with_usage_and_frozen_during_tools() {
        let start = Instant::now();
        let mut clock = Clock::new(start);
        clock.append(80);
        assert_eq!(
            clock.snapshot(start + Duration::from_secs(1)),
            Some(Metrics {
                output_tokens: 20,
                duration_ms: 1_000,
                estimated: true,
            })
        );
        clock.finish(start + Duration::from_secs(2), Some(100));
        assert_eq!(
            clock.snapshot(start + Duration::from_secs(60)),
            Some(Metrics {
                output_tokens: 100,
                duration_ms: 2_000,
                estimated: false,
            })
        );
    }

    #[test]
    fn new_attempt_does_not_include_backoff_or_old_preview() {
        let start = Instant::now();
        let mut failed = Clock::new(start);
        failed.append(400);
        let mut retry = Clock::new(start + Duration::from_secs(30));
        retry.append(40);
        retry.finish(start + Duration::from_secs(32), Some(20));
        assert_eq!(
            retry.snapshot(start + Duration::from_secs(100)),
            Some(Metrics {
                output_tokens: 20,
                duration_ms: 2_000,
                estimated: false,
            })
        );
    }

    #[test]
    fn weighted_totals_exclude_tool_only_steps_and_preserve_estimate() {
        let steps = vec![
            Step {
                duration_ms: 80_000,
                generation: Some(Metrics {
                    output_tokens: 100,
                    duration_ms: 1_000,
                    estimated: false,
                }),
                ..Step::default()
            },
            Step {
                duration_ms: 40_000,
                ..Step::default()
            },
            Step {
                generation: Some(Metrics {
                    output_tokens: 10,
                    duration_ms: 2_000,
                    estimated: true,
                }),
                ..Step::default()
            },
        ];
        assert_eq!(
            aggregate(&steps),
            Some(Metrics {
                output_tokens: 110,
                duration_ms: 3_000,
                estimated: true,
            })
        );
    }

    #[test]
    fn legacy_steps_remain_without_metrics() {
        let step: Step = serde_json::from_value(serde_json::json!({
            "text":"old", "summary":"", "tools":[], "usage":null,
        }))
        .unwrap();
        assert!(step.generation.is_none());
        assert!(aggregate(&[step]).is_none());
    }

    #[test]
    fn input_only_usage_keeps_the_visible_estimate() {
        let mut clock = Clock::new(Instant::now() - Duration::from_secs(1));
        let response = provider::Response::from_output(vec![serde_json::json!({
            "type":"message", "role":"assistant", "content":[{"type":"output_text","text":"abcdefgh"}],
        })], Some(super::super::Usage { input_tokens: 10, ..super::super::Usage::default() })).unwrap();
        clock.complete_response(&response);
        let metrics = clock.snapshot(Instant::now()).unwrap();
        assert_eq!(metrics.output_tokens, 2);
        assert!(metrics.estimated);
    }

    #[test]
    fn zero_duration_samples_do_not_pollute_the_weighted_average() {
        let steps = vec![Step {
            generation: Some(Metrics {
                output_tokens: 1_000,
                duration_ms: 0,
                estimated: true,
            }),
            ..Step::default()
        }];
        assert!(aggregate(&steps).is_none());
    }
}
