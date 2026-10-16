use super::super::{generation, Step};
use serde_json::Value;

pub(super) fn record(steps: &mut Vec<Step>, messages: &[String], params: &Value) {
    let Some(output_tokens) = params["usage"]["outputTokens"]
        .as_u64()
        .filter(|tokens| *tokens > 0)
    else {
        return;
    };
    let indices: Vec<_> = steps
        .iter()
        .enumerate()
        .filter_map(|(index, step)| {
            let id = step.context_id.as_deref()?.strip_prefix("claude:")?;
            messages
                .iter()
                .any(|message| message == id)
                .then_some(index)
        })
        .collect();
    let observed_duration = indices
        .iter()
        .filter_map(|index| steps[*index].generation.as_ref())
        .fold(0_u64, |total, metrics| {
            total.saturating_add(metrics.duration_ms)
        });
    let Some(duration_ms) = params["durationMs"]
        .as_u64()
        .filter(|duration| *duration > 0)
        .or((observed_duration > 0).then_some(observed_duration))
    else {
        return;
    };
    let target = indices.last().copied().unwrap_or_else(|| {
        steps.push(Step::default());
        steps.len() - 1
    });
    for index in indices {
        steps[index].generation = None;
    }
    steps[target].generation = Some(generation::Metrics {
        output_tokens,
        duration_ms,
        estimated: false,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn preview(id: &str, tokens: u64, duration_ms: u64) -> Step {
        Step {
            context_id: Some(format!("claude:{id}")),
            generation: Some(generation::Metrics {
                output_tokens: tokens,
                duration_ms,
                estimated: true,
            }),
            ..Step::default()
        }
    }

    #[test]
    fn one_native_completion_replaces_split_reasoning_and_text_estimates() {
        let mut steps = vec![preview("reasoning", 5, 1_000), preview("answer", 5, 1_000)];
        record(
            &mut steps,
            &["reasoning".into(), "answer".into()],
            &json!({
                "usage":{"outputTokens":80}, "durationMs":2_000,
            }),
        );
        assert!(steps[0].generation.is_none());
        assert_eq!(
            generation::aggregate(&steps),
            Some(generation::Metrics {
                output_tokens: 80,
                duration_ms: 2_000,
                estimated: false,
            })
        );
        record(
            &mut steps,
            &[],
            &json!({"usage":{"outputTokens":20}, "durationMs":500}),
        );
        assert_eq!(
            generation::aggregate(&steps),
            Some(generation::Metrics {
                output_tokens: 100,
                duration_ms: 2_500,
                estimated: false,
            })
        );
    }

    #[test]
    fn later_usage_keeps_the_previous_estimate_after_a_completed_tool() {
        let mut messages = super::super::wire::Messages::default();
        for (method, item) in [
            (
                "item/completed",
                json!({"itemId":"a","kind":"agentMessage","text":"First answer"}),
            ),
            ("item/started", json!({"itemId":"tool","kind":"toolCall"})),
            ("item/completed", json!({"itemId":"tool","kind":"toolCall"})),
            (
                "item/completed",
                json!({"itemId":"b","kind":"agentMessage","text":"Second answer"}),
            ),
        ] {
            messages.apply(method, &json!({"item":item})).unwrap();
        }
        let mut steps = vec![preview("a", 100, 2_000), preview("b", 50, 1_000)];
        record(
            &mut steps,
            &messages.take_generation("usage-b").unwrap(),
            &json!({
                "usage":{"outputTokens":80},"durationMs":1_000,
            }),
        );
        assert_eq!(
            generation::aggregate(&steps),
            Some(generation::Metrics {
                output_tokens: 180,
                duration_ms: 3_000,
                estimated: true,
            })
        );
        assert!(steps[0].generation.as_ref().unwrap().estimated);
        assert!(!steps[1].generation.as_ref().unwrap().estimated);
    }

    #[test]
    fn reported_tokens_can_use_observed_generation_time_without_tool_wall_time() {
        let mut steps = vec![Step {
            duration_ms: 90_000,
            ..preview("answer", 2, 2_000)
        }];
        record(
            &mut steps,
            &["answer".into()],
            &json!({"usage":{"outputTokens":80}}),
        );
        assert_eq!(
            steps[0].generation,
            Some(generation::Metrics {
                output_tokens: 80,
                duration_ms: 2_000,
                estimated: false,
            })
        );
    }

    #[test]
    fn absent_or_zero_usage_and_missing_time_never_create_a_speed() {
        for params in [
            json!({"usage":null,"durationMs":2_000}),
            json!({"usage":{"outputTokens":0},"durationMs":2_000}),
            json!({"usage":{"outputTokens":80}}),
            json!({"usage":{"outputTokens":80},"durationMs":0}),
        ] {
            let mut steps = vec![Step::default()];
            record(&mut steps, &[], &params);
            assert!(generation::aggregate(&steps).is_none());
        }
        let mut steps = vec![preview("answer", 2, 2_000)];
        record(&mut steps, &["answer".into()], &json!({"usage":null}));
        assert!(steps[0].generation.as_ref().unwrap().estimated);
    }
}
