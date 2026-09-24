//! Bounded read groups end at the first dependency, mutation or approval barrier.
use super::{
    tools::{ExecutionResult, ParallelExecution},
    AgentError, ToolCall,
};
use futures_util::{stream, StreamExt};
use std::{
    collections::{BTreeMap, HashSet},
    future::Future,
    time::Instant,
};

const CONCURRENCY: usize = 4;
const MAX_BATCH: usize = 16;

pub(super) fn prefix_len(calls: &[ToolCall], mut eligible: impl FnMut(&ToolCall) -> bool) -> usize {
    let mut seen = HashSet::new();
    calls
        .iter()
        .take(MAX_BATCH)
        .take_while(|tool| {
            eligible(tool) && seen.insert((tool.name.clone(), tool.args.to_string()))
        })
        .count()
}

pub(super) async fn execute<F, Fut>(
    calls: &[ToolCall],
    execute: F,
) -> BTreeMap<String, ParallelExecution>
where
    F: Fn(ToolCall) -> Fut,
    Fut: Future<Output = Result<ExecutionResult, AgentError>>,
{
    stream::iter(calls.iter().cloned().map(|tool| {
        let id = tool.id.clone();
        let future = execute(tool);
        async move {
            let start = Instant::now();
            let result = future.await;
            (
                id,
                ParallelExecution {
                    result,
                    duration_ms: start.elapsed().as_millis() as u64,
                },
            )
        }
    }))
    .buffer_unordered(CONCURRENCY)
    .collect()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn tool(id: usize, name: &str) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            name: name.into(),
            args: json!({"path":id.to_string()}),
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        }
    }

    #[test]
    fn read_groups_stop_before_mutations_and_repeated_queries() {
        let calls = vec![
            tool(1, "read"),
            tool(2, "mcp_read"),
            tool(3, "write"),
            tool(4, "read"),
        ];
        assert_eq!(prefix_len(&calls, |tool| tool.name != "write"), 2);
        assert_eq!(prefix_len(&calls[2..], |tool| tool.name != "write"), 0);
        assert_eq!(prefix_len(&calls[3..], |_| true), 1);
        assert_eq!(prefix_len(&[tool(1, "read"), tool(1, "read")], |_| true), 1);
    }

    #[tokio::test]
    async fn independent_reads_overlap_with_a_bound_and_preserve_call_identity() {
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let barrier = tokio::sync::Barrier::new(CONCURRENCY);
        let calls: Vec<_> = (0..8).map(|id| tool(id, "read")).collect();
        let (active, peak, barrier) = (&active, &peak, &barrier);
        let result = execute(&calls, |tool| async move {
            let count = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(count, Ordering::SeqCst);
            barrier.wait().await;
            active.fetch_sub(1, Ordering::SeqCst);
            if tool.id == "2" {
                return Err(AgentError::new("read_failed", "Falha isolada"));
            }
            Ok(ExecutionResult {
                output: tool.id.clone(),
                revision: None,
                read: None,
            })
        })
        .await;
        assert_eq!(peak.load(Ordering::SeqCst), CONCURRENCY);
        assert_eq!(result.len(), calls.len());
        assert!(result["2"].result.is_err());
        assert_eq!(result["7"].result.as_ref().unwrap().output, "7");
    }
}
