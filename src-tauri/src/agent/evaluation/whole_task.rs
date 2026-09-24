//! Paired, outcome-based task evaluation. Runtime completion alone is not success.
use super::super::{tools, Mode, ToolCall};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Instant};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Identity {
    task: String,
    model: String,
    reasoning: String,
    configuration: String,
    sample: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Run {
    identity: Identity,
    variant: String,
    source: Source,
    runtime_completed: bool,
    /// Actual assertions against the resulting artifacts or authorized effects.
    checks: BTreeMap<String, bool>,
    wall_ms: u64,
    human_wait_ms: u64,
    calls: u64,
    errors: u64,
    recoveries: u64,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_tokens: Option<u64>,
    cache_write_tokens: Option<u64>,
    lost_events: u64,
    /// Raw duration samples, never added to infer wall time.
    phases: BTreeMap<String, Vec<u64>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Source {
    Deterministic,
    Live,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Comparison {
    pairs: usize,
    baseline_successes: usize,
    candidate_successes: usize,
    baseline: Metrics,
    candidate: Metrics,
    cohorts: Vec<Cohort>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Cohort {
    task: String,
    model: String,
    reasoning: String,
    configuration: String,
    baseline_successes: usize,
    candidate_successes: usize,
    baseline: Metrics,
    candidate: Metrics,
}

#[derive(Deserialize)]
struct CorpusTask {
    id: String,
    prompt: String,
    checks: Vec<String>,
}

fn corpus() -> Vec<CorpusTask> {
    serde_json::from_str(include_str!(
        "../fixtures/evaluations/whole-task-corpus.json"
    ))
    .unwrap()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Metrics {
    wall: Percentiles,
    human_wait: Percentiles,
    calls: u64,
    errors: u64,
    recoveries: u64,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_tokens: Option<u64>,
    cache_write_tokens: Option<u64>,
    lost_events: u64,
    phases: BTreeMap<String, Percentiles>,
}

#[derive(Debug, Serialize)]
struct Percentiles {
    samples: usize,
    p50: u64,
    p95: u64,
}

fn percentiles(mut samples: Vec<u64>) -> Percentiles {
    samples.sort_unstable();
    let percentile = |percent: usize| {
        samples
            .get((samples.len() * percent).div_ceil(100).saturating_sub(1))
            .copied()
            .unwrap_or(0)
    };
    Percentiles {
        samples: samples.len(),
        p50: percentile(50),
        p95: percentile(95),
    }
}

fn metrics(runs: &[&Run]) -> Metrics {
    let tokens = |get: fn(&Run) -> Option<u64>| {
        runs.iter()
            .try_fold(0_u64, |sum, run| sum.checked_add(get(run)?))
    };
    let mut phases = BTreeMap::<String, Vec<u64>>::new();
    for run in runs {
        for (phase, samples) in &run.phases {
            phases.entry(phase.clone()).or_default().extend(samples);
        }
    }
    Metrics {
        wall: percentiles(runs.iter().map(|run| run.wall_ms).collect()),
        human_wait: percentiles(runs.iter().map(|run| run.human_wait_ms).collect()),
        calls: runs.iter().map(|run| run.calls).sum(),
        errors: runs.iter().map(|run| run.errors).sum(),
        recoveries: runs.iter().map(|run| run.recoveries).sum(),
        input_tokens: tokens(|run| run.input_tokens),
        output_tokens: tokens(|run| run.output_tokens),
        cache_read_tokens: tokens(|run| run.cache_read_tokens),
        cache_write_tokens: tokens(|run| run.cache_write_tokens),
        lost_events: runs.iter().map(|run| run.lost_events).sum(),
        phases: phases
            .into_iter()
            .map(|(name, values)| (name, percentiles(values)))
            .collect(),
    }
}

fn compare(runs: &[Run], source: Source) -> Result<Comparison, String> {
    let corpus = corpus();
    let mut baseline = BTreeMap::new();
    let mut candidate = BTreeMap::new();
    for run in runs {
        if run.source != source || run.checks.is_empty() || run.human_wait_ms > run.wall_ms {
            return Err("mixed sources or missing/invalid evidence".into());
        }
        if source == Source::Live
            && !corpus.iter().any(|task| {
                task.id == run.identity.task
                    && !task.prompt.is_empty()
                    && task.checks.iter().all(|name| run.checks.contains_key(name))
            })
        {
            return Err("live run is missing the corpus task's observable criteria".into());
        }
        let target = match run.variant.as_str() {
            "baseline" => &mut baseline,
            "candidate" => &mut candidate,
            _ => return Err("unknown comparison variant".into()),
        };
        if target.insert(&run.identity, run).is_some() {
            return Err("duplicate sample".into());
        }
    }
    if baseline.is_empty() || !baseline.keys().eq(candidate.keys()) {
        return Err("pair the same task, model, reasoning, configuration and sample".into());
    }
    for (identity, run) in &baseline {
        if !run.checks.keys().eq(candidate[identity].checks.keys()) {
            return Err("functional criteria differ between paired runs".into());
        }
    }
    let success = |run: &&&Run| run.runtime_completed && run.checks.values().all(|ok| *ok);
    let mut cohorts = Vec::new();
    let mut groups = BTreeMap::<_, Vec<&Run>>::new();
    for run in baseline.values() {
        let i = &run.identity;
        groups
            .entry((&i.task, &i.model, &i.reasoning, &i.configuration))
            .or_default()
            .push(run);
    }
    for ((task, model, reasoning, configuration), group) in groups {
        let paired = group
            .iter()
            .map(|run| candidate[&run.identity])
            .collect::<Vec<_>>();
        cohorts.push(Cohort {
            task: task.clone(),
            model: model.clone(),
            reasoning: reasoning.clone(),
            configuration: configuration.clone(),
            baseline_successes: group
                .iter()
                .filter(|run| run.runtime_completed && run.checks.values().all(|ok| *ok))
                .count(),
            candidate_successes: paired
                .iter()
                .filter(|run| run.runtime_completed && run.checks.values().all(|ok| *ok))
                .count(),
            baseline: metrics(&group),
            candidate: metrics(&paired),
        });
    }
    Ok(Comparison {
        pairs: baseline.len(),
        baseline_successes: baseline.values().filter(success).count(),
        candidate_successes: candidate.values().filter(success).count(),
        baseline: metrics(&baseline.into_values().collect::<Vec<_>>()),
        candidate: metrics(&candidate.into_values().collect::<Vec<_>>()),
        cohorts,
    })
}

async fn local_task(variant: &str, recover: bool, sample: u32) -> Run {
    let root = tempfile::tempdir().unwrap();
    let canonical_root = root.path().canonicalize().unwrap();
    let (_cancel, signal) = tokio::sync::watch::channel(false);
    let start = Instant::now();
    let mut calls = vec![];
    if recover {
        calls.push(("read", serde_json::json!({"path":"missing.txt"})));
    }
    calls.push((
        "write",
        serde_json::json!({"path":"answer.txt", "content":"42\n"}),
    ));
    calls.push(("read", serde_json::json!({"path":"answer.txt"})));
    let mut errors = 0;
    let mut handler = vec![];
    let mut last_output = String::new();
    for (i, (name, args)) in calls.iter().enumerate() {
        let started = Instant::now();
        match tools::execute(
            &canonical_root,
            &ToolCall {
                id: format!("call_{i}"),
                name: (*name).into(),
                args: args.clone(),
                status: "running".into(),
                output: String::new(),
                duration_ms: 0,
            },
            Mode::Build,
            signal.clone(),
        )
        .await
        {
            Ok(output) => last_output = output,
            Err(_) => errors += 1,
        }
        handler.push(started.elapsed().as_millis() as u64);
    }
    Run {
        identity: Identity {
            task: if recover {
                "create-after-read-error"
            } else {
                "create-and-verify"
            }
            .into(),
            model: "scripted".into(),
            reasoning: "none".into(),
            configuration: "local-file-tools-v1".into(),
            sample,
        },
        variant: variant.into(),
        source: Source::Deterministic,
        runtime_completed: true,
        checks: BTreeMap::from([
            (
                "file_has_requested_content".into(),
                std::fs::read_to_string(root.path().join("answer.txt")).unwrap() == "42\n",
            ),
            ("read_confirms_content".into(), last_output.contains("42")),
            ("only_expected_errors".into(), errors == u64::from(recover)),
        ]),
        wall_ms: start.elapsed().as_millis() as u64,
        human_wait_ms: 0,
        calls: calls.len() as u64,
        errors,
        recoveries: u64::from(recover),
        input_tokens: None,
        output_tokens: None,
        cache_read_tokens: None,
        cache_write_tokens: None,
        lost_events: 0,
        phases: BTreeMap::from([("tool_handler".into(), handler)]),
    }
}

#[tokio::test]
async fn harness_evaluation_whole_task_checks_artifacts_and_pairs_actual_samples() {
    let mut runs = vec![];
    for sample in 0..3 {
        for recover in [false, true] {
            for variant in ["baseline", "candidate"] {
                runs.push(local_task(variant, recover, sample).await);
            }
        }
    }
    let report = compare(&runs, Source::Deterministic).unwrap();
    assert_eq!(report.pairs, 6);
    assert_eq!(report.candidate_successes, 6);
    assert_eq!(report.candidate.calls, 15);
    assert_eq!(report.candidate.recoveries, 3);
    assert!(report.candidate.input_tokens.is_none());
    println!(
        "WHOLE_TASK_EVAL {}",
        serde_json::to_string(&report).unwrap()
    );
    runs[1]
        .checks
        .insert("file_has_requested_content".into(), false);
    assert_eq!(
        compare(&runs, Source::Deterministic)
            .unwrap()
            .candidate_successes,
        5
    );
    runs[1].identity.model = "different-model".into();
    assert!(compare(&runs, Source::Deterministic).is_err());
}

#[test]
#[ignore = "requires paired observations from explicitly authorized real-provider runs"]
fn harness_evaluation_whole_task_live_pairs() {
    let path =
        std::env::var("JARVIS_EVAL_RUNS").expect("set JARVIS_EVAL_RUNS to the paired run JSON");
    let runs: Vec<Run> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let report = compare(&runs, Source::Live).unwrap();
    println!(
        "WHOLE_TASK_EVAL_LIVE {}",
        serde_json::to_string(&report).unwrap()
    );
    assert_eq!(report.candidate_successes, report.pairs);
    assert_eq!(
        report.candidate.lost_events, 0,
        "incomplete telemetry cannot prove an efficiency gain"
    );
}
