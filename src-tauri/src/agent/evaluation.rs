use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const CASES: [&str; 2] = [
    include_str!("fixtures/evaluations/movarte-explicit-mcp.json"),
    include_str!("fixtures/evaluations/movarte-planned-flow.json"),
];
const RUNTIME_SUITE: &str = include_str!("fixtures/evaluations/movarte-runtime-scenarios.json");

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvaluationCase {
    schema_version: u64,
    id: String,
    description: String,
    sanitized_input: String,
    expectations: Expectations,
    observed: Observation,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expectations {
    required_outcome: Outcome,
    #[serde(default)]
    required_mcp: Option<String>,
    #[serde(default)]
    forbid_other_mcps: bool,
    #[serde(default)]
    baseline_violations: Vec<Violation>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Observation {
    wall_time_ms: u64,
    agents: Vec<AgentObservation>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentObservation {
    role: String,
    outcome: Outcome,
    provider_steps: u64,
    measured_provider_steps: u64,
    tool_calls: u64,
    tool_errors: u64,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    duration_ms: u64,
    tools: Vec<ToolObservation>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolObservation {
    name: String,
    #[serde(default)]
    mcp: Option<String>,
    calls: u64,
    errors: u64,
    #[serde(default)]
    error_classes: BTreeMap<String, u64>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Completed,
    Cancelled,
    Error,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Violation {
    code: ViolationCode,
    count: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ViolationCode {
    IncompleteRun,
    MissingRequiredMcp,
    UnrequestedMcp,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    agents: u64,
    provider_steps: u64,
    measured_provider_steps: u64,
    tool_calls: u64,
    tool_errors: u64,
    error_classes: BTreeMap<String, u64>,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    cache_read_basis_points: u64,
    wall_time_ms: u64,
    agent_time_ms: u64,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalReadReuseSummary {
    reads: u64,
    reused_reads: u64,
    original_bytes: u64,
    retained_bytes: u64,
    reduction_basis_points: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeSuite {
    schema_version: u64,
    id: String,
    description: String,
    scenarios: Vec<RuntimeScenario>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeScenario {
    id: String,
    incident: String,
    expected: RuntimeReport,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RuntimeReport {
    state: String,
    metrics: BTreeMap<String, u64>,
}

impl RuntimeReport {
    pub(crate) fn new<const N: usize>(state: &str, metrics: [(&str, u64); N]) -> Self {
        Self {
            state: state.into(),
            metrics: metrics
                .into_iter()
                .map(|(name, value)| (name.into(), value))
                .collect(),
        }
    }
}

fn load_runtime_suite() -> RuntimeSuite {
    serde_json::from_str(RUNTIME_SUITE).expect("valid scripted harness evaluation fixture")
}

fn validate_runtime_suite(suite: &RuntimeSuite) -> Result<(), String> {
    if suite.schema_version != 1 {
        return Err(format!("{} uses an unsupported schema version", suite.id));
    }
    if suite.id.trim().is_empty()
        || suite.description.trim().is_empty()
        || suite.scenarios.is_empty()
    {
        return Err("runtime evaluation suite is incomplete".into());
    }
    let mut ids = std::collections::HashSet::new();
    for scenario in &suite.scenarios {
        if scenario.id.trim().is_empty()
            || scenario.incident.trim().is_empty()
            || scenario.expected.state.trim().is_empty()
            || scenario.expected.metrics.is_empty()
            || scenario
                .expected
                .metrics
                .keys()
                .any(|metric| metric.trim().is_empty())
        {
            return Err(format!("{} is incomplete", scenario.id));
        }
        if !ids.insert(&scenario.id) {
            return Err(format!("{} is duplicated", scenario.id));
        }
    }
    Ok(())
}

pub(crate) fn assert_runtime_report(id: &str, actual: RuntimeReport) {
    let suite = load_runtime_suite();
    validate_runtime_suite(&suite).unwrap();
    let scenario = suite
        .scenarios
        .iter()
        .find(|scenario| scenario.id == id)
        .unwrap_or_else(|| panic!("scripted harness scenario '{id}' is not registered"));
    assert_eq!(actual, scenario.expected, "runtime evaluation {id}");
    println!(
        "HARNESS_EVAL case={} current={}",
        id,
        serde_json::to_string(&actual).unwrap()
    );
}

fn load_cases() -> Vec<EvaluationCase> {
    CASES
        .iter()
        .map(|fixture| serde_json::from_str(fixture).expect("valid harness evaluation fixture"))
        .collect()
}

fn validate(case: &EvaluationCase) -> Result<(), String> {
    if case.schema_version != 1 {
        return Err(format!("{} uses an unsupported schema version", case.id));
    }
    if case.description.trim().is_empty()
        || case.sanitized_input.trim().is_empty()
        || case.observed.agents.is_empty()
    {
        return Err(format!("{} is missing its sanitized scenario", case.id));
    }
    for agent in &case.observed.agents {
        if agent.role.trim().is_empty() {
            return Err(format!("{} contains an unnamed agent", case.id));
        }
        if agent.measured_provider_steps > agent.provider_steps {
            return Err(format!(
                "{} has more measured steps than provider steps",
                agent.role
            ));
        }
        if agent.cache_read_tokens > agent.input_tokens
            || agent.cache_write_tokens > agent.input_tokens
        {
            return Err(format!("{} has an invalid cache breakdown", agent.role));
        }
        let calls: u64 = agent.tools.iter().map(|tool| tool.calls).sum();
        let errors: u64 = agent.tools.iter().map(|tool| tool.errors).sum();
        if agent.tools.iter().any(|tool| {
            tool.name.trim().is_empty()
                || tool.mcp.as_ref().is_some_and(|mcp| mcp.trim().is_empty())
                || tool.errors > tool.calls
                || tool
                    .error_classes
                    .keys()
                    .any(|class| class.trim().is_empty())
                || tool.error_classes.values().sum::<u64>() != tool.errors
        }) {
            return Err(format!("{} contains invalid tool statistics", agent.role));
        }
        if (calls, errors) != (agent.tool_calls, agent.tool_errors) {
            return Err(format!("{} tool totals do not match its trace", agent.role));
        }
    }
    Ok(())
}

fn summarize(case: &EvaluationCase) -> Summary {
    let mut summary = Summary {
        agents: case.observed.agents.len() as u64,
        provider_steps: 0,
        measured_provider_steps: 0,
        tool_calls: 0,
        tool_errors: 0,
        error_classes: BTreeMap::new(),
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        cache_read_basis_points: 0,
        wall_time_ms: case.observed.wall_time_ms,
        agent_time_ms: 0,
    };
    for agent in &case.observed.agents {
        summary.provider_steps += agent.provider_steps;
        summary.measured_provider_steps += agent.measured_provider_steps;
        summary.tool_calls += agent.tool_calls;
        summary.tool_errors += agent.tool_errors;
        for tool in &agent.tools {
            for (class, count) in &tool.error_classes {
                *summary.error_classes.entry(class.clone()).or_default() += count;
            }
        }
        summary.input_tokens += agent.input_tokens;
        summary.output_tokens += agent.output_tokens;
        summary.cache_read_tokens += agent.cache_read_tokens;
        summary.cache_write_tokens += agent.cache_write_tokens;
        summary.agent_time_ms += agent.duration_ms;
    }
    if summary.input_tokens > 0 {
        let input = u128::from(summary.input_tokens);
        summary.cache_read_basis_points =
            ((u128::from(summary.cache_read_tokens) * 10_000 + input / 2) / input) as u64;
    }
    summary
}

fn assess(case: &EvaluationCase) -> Vec<Violation> {
    let mut violations = Vec::new();
    let incomplete = case
        .observed
        .agents
        .iter()
        .filter(|agent| agent.outcome != case.expectations.required_outcome)
        .count() as u64;
    if incomplete > 0 {
        violations.push(Violation {
            code: ViolationCode::IncompleteRun,
            count: incomplete,
        });
    }
    if let Some(required) = &case.expectations.required_mcp {
        let required_calls: u64 = case
            .observed
            .agents
            .iter()
            .flat_map(|agent| &agent.tools)
            .filter(|tool| tool.mcp.as_ref() == Some(required))
            .map(|tool| tool.calls)
            .sum();
        if required_calls == 0 {
            violations.push(Violation {
                code: ViolationCode::MissingRequiredMcp,
                count: 1,
            });
        }
        if case.expectations.forbid_other_mcps {
            let unrelated_calls: u64 = case
                .observed
                .agents
                .iter()
                .flat_map(|agent| &agent.tools)
                .filter(|tool| tool.mcp.as_ref().is_some_and(|mcp| mcp != required))
                .map(|tool| tool.calls)
                .sum();
            if unrelated_calls > 0 {
                violations.push(Violation {
                    code: ViolationCode::UnrequestedMcp,
                    count: unrelated_calls,
                });
            }
        }
    }
    violations
}

#[test]
fn harness_evaluation_reports_the_sanitized_movarte_baselines() {
    let cases = load_cases();
    assert_eq!(cases.len(), 2);
    for case in &cases {
        validate(case).unwrap();
        assert_eq!(assess(case), case.expectations.baseline_violations);
        println!(
            "HARNESS_EVAL case={} baseline={}",
            case.id,
            serde_json::to_string(&summarize(case)).unwrap()
        );
    }

    let explicit = cases
        .iter()
        .find(|case| case.id == "movarte-explicit-mcp-fidelity")
        .unwrap();
    assert_eq!(
        summarize(explicit),
        Summary {
            agents: 1,
            provider_steps: 15,
            measured_provider_steps: 14,
            tool_calls: 14,
            tool_errors: 2,
            error_classes: BTreeMap::from([
                ("missing_active_task".into(), 1),
                ("request_timeout".into(), 1),
            ]),
            input_tokens: 395_516,
            output_tokens: 1_955,
            cache_read_tokens: 331_520,
            cache_write_tokens: 0,
            cache_read_basis_points: 8_382,
            wall_time_ms: 128_755,
            agent_time_ms: 128_755,
        }
    );

    let planned = cases
        .iter()
        .find(|case| case.id == "movarte-planned-status-selection")
        .unwrap();
    assert_eq!(
        summarize(planned),
        Summary {
            agents: 3,
            provider_steps: 409,
            measured_provider_steps: 409,
            tool_calls: 407,
            tool_errors: 28,
            error_classes: BTreeMap::from([
                ("agent_execution_failed".into(), 5),
                ("disallowed_role".into(), 2),
                ("empty_context_index".into(), 1),
                ("missing_package_script".into(), 2),
                ("no_op_patch".into(), 1),
                ("project_check_failed".into(), 1),
                ("scoped_instruction_unreadable".into(), 11),
                ("stale_edit_context".into(), 5),
            ]),
            input_tokens: 35_022_502,
            output_tokens: 76_321,
            cache_read_tokens: 34_188_544,
            cache_write_tokens: 0,
            cache_read_basis_points: 9_762,
            wall_time_ms: 2_334_422,
            agent_time_ms: 4_343_697,
        }
    );
}

#[test]
fn harness_evaluation_runtime_manifest_is_versioned_and_complete() {
    let suite = load_runtime_suite();
    validate_runtime_suite(&suite).unwrap();
    assert_eq!(suite.id, "movarte-runtime-regressions");
    assert_eq!(suite.scenarios.len(), 15);
}

#[tokio::test]
async fn harness_evaluation_measures_validated_local_read_reuse() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    std::fs::write(
        root.join("source.txt"),
        "reusable source line\n".repeat(300),
    )
    .unwrap();
    let tool = super::ToolCall {
        id: "read".into(),
        name: "read".into(),
        args: serde_json::json!({"path":"source.txt","offset":1,"limit":200}),
        status: "running".into(),
        output: String::new(),
        duration_ms: 0,
    };
    let (_send, signal) = tokio::sync::watch::channel(false);
    let first =
        super::tools::execute_with_revision(&root, &tool, super::Mode::Plan, signal.clone())
            .await
            .unwrap()
            .read
            .unwrap();
    let mut cache = super::tool_loop::ReadReuseCache::default();
    cache.remember(first, true);
    let repeated = super::tools::execute_with_revision(&root, &tool, super::Mode::Plan, signal)
        .await
        .unwrap()
        .read
        .unwrap();
    let reused = cache.resolve(&repeated).unwrap();
    let retained_bytes = super::tool_loop::READ_REUSE_MESSAGE.len() as u64;
    let reduction_basis_points = ((u128::from(reused.original_bytes - retained_bytes) * 10_000
        + u128::from(reused.original_bytes) / 2)
        / u128::from(reused.original_bytes)) as u64;
    let summary = LocalReadReuseSummary {
        reads: 2,
        reused_reads: 1,
        original_bytes: reused.original_bytes,
        retained_bytes,
        reduction_basis_points,
    };
    assert!(summary.reduction_basis_points > 9_000);
    println!(
        "HARNESS_EVAL case=local-read-reuse-validity current={}",
        serde_json::to_string(&summary).unwrap()
    );
}
