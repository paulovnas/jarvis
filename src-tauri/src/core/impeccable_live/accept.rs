//! Promote the browser's explicit choice before asking a model for source cleanup.
use super::{error, run_bound, tasks, CoreError};
use serde_json::{json, Value};
use std::{path::Path, time::Duration};
use tokio::sync::watch;

fn sequence(task: &tasks::Task) -> Result<u64, CoreError> {
    let pending = task.event["_jarvisPendingEventSeq"]
        .as_u64()
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            error("A sequência da escolha Live está incompleta. O pedido foi preservado.")
        })?;
    task.request_id
        .strip_prefix(&format!("{}:", task.action_id))
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0 && *value <= pending)
        .ok_or_else(|| {
            error("A identidade da escolha Live está inválida. O pedido foi preservado.")
        })
}

fn arguments(task: &tasks::Task, event: &Value) -> Result<Vec<String>, CoreError> {
    let id = event["id"]
        .as_str()
        .filter(|id| {
            *id == task.action_id
                && !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
        })
        .ok_or_else(|| error("A escolha Live não corresponde ao pedido reservado."))?;
    let mut args = vec!["--id".into(), id.into()];
    match event["type"].as_str() {
        Some("accept") => {
            let variant = match &event["variantId"] {
                Value::String(value) => Some(value.clone()),
                Value::Number(value) => value.as_u64().map(|value| value.to_string()),
                _ => None,
            }
            .filter(|value| {
                !value.is_empty()
                    && value.len() <= 3
                    && value.bytes().all(|byte| byte.is_ascii_digit())
            })
            .ok_or_else(|| {
                error("A variante selecionada está inválida. Nenhuma outra variante foi aplicada.")
            })?;
            args.extend(["--variant".into(), variant, "--bake".into()]);
            if let Some(values) = event.get("paramValues") {
                let values = values
                    .as_object()
                    .ok_or_else(|| error("Os parâmetros da escolha Live estão inválidos."))?;
                if !values.is_empty() {
                    args.extend([
                        "--param-values".into(),
                        Value::Object(values.clone()).to_string(),
                    ]);
                }
            }
        }
        Some("discard") => args.push("--discard".into()),
        _ => return Err(error("A ação Live não é uma escolha de variante.")),
    }
    if let Some(page) = event.get("pageUrl") {
        let page = page
            .as_str()
            .ok_or_else(|| error("A página da escolha Live está inválida."))?;
        if !page.starts_with('/') || page.starts_with("//") || page.contains('\\') {
            super::local_url(page)?;
        }
        args.extend(["--page-url".into(), page.into()]);
    }
    Ok(args)
}

fn handled(result: &Value) -> Result<bool, CoreError> {
    if result["mode"] == "error" || result.get("error").is_some_and(|value| !value.is_null()) {
        let cause = result["error"].as_str().unwrap_or("resposta inválida");
        let cause: String = cause.chars().take(600).collect();
        return Err(error(format!("O Impeccable não aplicou esta escolha: {cause}. O pedido e os resultados anteriores foram preservados.")));
    }
    match result["handled"].as_bool() {
        Some(value) => Ok(value),
        None => Err(error(
            "O Impeccable não confirmou a aplicação da escolha. O pedido foi preservado.",
        )),
    }
}

fn completion_ack(kind: &str, result: &Value) -> Result<Value, CoreError> {
    if result["ok"] == false && result["error"] == "source_dirty" && kind == "accept" {
        return Ok(
            json!({"ok":false,"final":false,"requiresComplete":true,"error":"source_dirty","file":result["file"],"findings":result["findings"]}),
        );
    }
    let phase = if kind == "discard" {
        "discarded"
    } else {
        "completed"
    };
    if result["ok"] == true && result["phase"] == phase {
        return Ok(
            json!({"ok":true,"final":true,"type":if kind == "discard" { "discarded" } else { "complete" }}),
        );
    }
    let cause = result["error"]
        .as_str()
        .unwrap_or("a conclusão canônica não foi confirmada");
    let cause: String = cause.chars().take(600).collect();
    Err(error(format!("A escolha foi processada, mas o Impeccable não confirmou sua conclusão: {cause}. O progresso foi preservado; nenhuma escolha será repetida automaticamente.")))
}

fn current(root: &Path, task: &tasks::Task) -> Result<(), CoreError> {
    if tasks::is_current_event(root, task)? {
        Ok(())
    } else {
        Err(error("Uma nova solicitação substituiu esta escolha Live. Os resultados confirmados foram preservados sem concluir o novo pedido."))
    }
}

fn eligible(root: &Path, task: &tasks::Task) -> Result<(), CoreError> {
    if tasks::can_mutate_current_event(root, task)? {
        Ok(())
    } else {
        Err(error("Esta escolha Live já foi encerrada ou substituída. O resultado confirmado foi preservado; nenhuma escolha será repetida."))
    }
}

pub(super) async fn prepare(
    home: &Path,
    root: &Path,
    project: &Path,
    task: &tasks::Task,
    event: &mut Value,
    cancel: &mut watch::Receiver<bool>,
) -> Result<(), CoreError> {
    let kind = match event["type"].as_str() {
        Some("accept") => "accept",
        Some("discard") => "discard",
        _ => return Ok(()),
    };
    let sequence = sequence(task)?;
    let pending = task.event["_jarvisPendingEventSeq"].as_u64().unwrap();
    let args = arguments(task, event)?;
    eligible(root, task)?;
    let result = run_bound(
        home,
        root,
        project,
        "live-accept",
        &args,
        None,
        Duration::from_secs(30),
        cancel,
        Some((sequence, pending, kind)),
    )
    .await?;
    current(root, task)?;
    event["_acceptResult"] = result.clone();
    if !handled(&result)? {
        event["_completionAck"] = json!({"ok":false,"final":false,"requiresComplete":true});
        return Ok(());
    }
    let mut args = vec!["--id".into(), task.action_id.clone()];
    if kind == "discard" {
        args.push("--discarded".into());
    }
    eligible(root, task)?;
    let result = run_bound(
        home,
        root,
        project,
        "live-complete",
        &args,
        None,
        Duration::from_secs(30),
        cancel,
        Some((sequence, pending, kind)),
    )
    .await?;
    current(root, task)?;
    event["_completionAck"] = completion_ack(kind, &result)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> tasks::Task {
        tasks::Task {
            request_id: "choice:1".into(),
            action_id: "choice".into(),
            revision: 1,
            content: "Move the badge".into(),
            event: json!({"id":"choice","type":"accept","variantId":"2","_jarvisPendingEventSeq":3}),
            status: tasks::Status::Pending,
            bead_id: None,
            turn_id: None,
            created_at: 1,
        }
    }

    #[test]
    fn choice_keeps_the_literal_variant_page_and_parameters() {
        let event = json!({"id":"choice","type":"accept","variantId":"02","pageUrl":"http://localhost:3001/dashboard?tab=overview","paramValues":{"spacing":0.7}});
        assert_eq!(
            arguments(&task(), &event).unwrap(),
            [
                "--id",
                "choice",
                "--variant",
                "02",
                "--bake",
                "--param-values",
                "{\"spacing\":0.7}",
                "--page-url",
                "http://localhost:3001/dashboard?tab=overview"
            ]
        );
        assert_eq!(
            arguments(
                &task(),
                &json!({"id":"choice","type":"accept","variantId":0})
            )
            .unwrap(),
            ["--id", "choice", "--variant", "0", "--bake"]
        );
        let mut relative = event;
        relative["pageUrl"] = json!("/dashboard");
        assert!(arguments(&task(), &relative)
            .unwrap()
            .ends_with(&["--page-url".into(), "/dashboard".into()]));
    }

    #[test]
    fn discard_never_accepts_or_bakes_a_variant() {
        assert_eq!(
            arguments(
                &task(),
                &json!({"id":"choice","type":"discard","variantId":"2"})
            )
            .unwrap(),
            ["--id", "choice", "--discard"]
        );
    }

    #[test]
    fn malformed_choices_cannot_reach_the_engine() {
        for event in [
            json!({"id":"other","type":"accept","variantId":"2"}),
            json!({"id":"choice","type":"accept","variantId":"2 --discard"}),
            json!({"id":"choice","type":"accept","variantId":-1}),
            json!({"id":"choice","type":"accept","variantId":1.5}),
            json!({"id":"choice","type":"accept","variantId":"2","paramValues":[]}),
            json!({"id":"choice","type":"accept","variantId":"2","pageUrl":"https://example.com"}),
        ] {
            assert!(arguments(&task(), &event).is_err());
        }
    }

    #[test]
    fn lifecycle_sequence_does_not_replace_original_request_binding() {
        let mut task = task();
        assert_eq!(sequence(&task).unwrap(), 1);
        task.event["_jarvisPendingEventSeq"] = json!(0);
        assert!(sequence(&task).is_err());
        task.event["_jarvisPendingEventSeq"] = json!(3);
        task.request_id = "choice:4".into();
        assert!(sequence(&task).is_err());
    }

    #[test]
    fn engine_errors_do_not_authorize_manual_recreation() {
        for result in [
            json!({"handled":false,"mode":"error","error":"source_locked"}),
            json!({"handled":false,"mode":"error","error":"accept_receipt_conflict"}),
            json!({"handled":false,"error":"Could not save accepted source"}),
        ] {
            assert!(handled(&result).is_err());
        }
        assert!(
            !handled(&json!({"handled":false,"mode":"fallback","file":"dist/index.html"})).unwrap()
        );
        assert!(handled(&json!({"handled":true,"alreadyApplied":true,"carbonize":true})).unwrap());
    }

    #[test]
    fn only_verified_completion_is_final_and_dirty_source_requests_cleanup() {
        assert_eq!(
            completion_ack("accept", &json!({"ok":true,"phase":"completed"})).unwrap(),
            json!({"ok":true,"final":true,"type":"complete"})
        );
        let dirty = completion_ack("accept", &json!({"ok":false,"error":"source_dirty","file":"sidebar.tsx","findings":[{"why":"wrapper"}]})).unwrap();
        assert_eq!(dirty["ok"], false);
        assert_eq!(dirty["requiresComplete"], true);
        assert_eq!(dirty["findings"][0]["why"], "wrapper");
        assert!(
            completion_ack("accept", &json!({"ok":true,"phase":"carbonize_required"})).is_err()
        );
        assert!(completion_ack("discard", &json!({"ok":true,"phase":"completed"})).is_err());
        assert_eq!(
            completion_ack("discard", &json!({"ok":true,"phase":"discarded"})).unwrap()["type"],
            "discarded"
        );
    }

    #[test]
    fn discard_can_complete_once_and_its_confirmed_result_remains_readable() {
        let fixture = tempfile::tempdir().unwrap();
        let directory = fixture.path().join(".impeccable/live/sessions");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("choice.jsonl");
        let mut events = vec![
            json!({"id":"choice","type":"generate","action":"impeccable","freeformPrompt":"Move the badge"}),
            json!({"id":"choice","type":"variants_ready"}),
            json!({"id":"choice","type":"discard"}),
        ];
        let write = |events: &[Value]| {
            let journal = events
                .iter()
                .enumerate()
                .map(|(index, event)| {
                    format!(
                        "{}\n",
                        json!({"seq":index + 1,"id":"choice","type":event["type"],"event":event})
                    )
                })
                .collect::<String>();
            std::fs::write(&path, journal).unwrap();
        };
        write(&events);
        let mut task = task();
        task.event["type"] = json!("discard");
        assert!(eligible(fixture.path(), &task).is_ok());
        // live-accept restores source and returns a receipt; live-complete
        // owns the terminal journal ACK. A receipt alone does not cancel it.
        assert!(handled(&json!({"handled":true,"carbonize":false})).unwrap());
        assert!(current(fixture.path(), &task).is_ok());
        assert!(eligible(fixture.path(), &task).is_ok());
        events.push(json!({"id":"choice","type":"discarded"}));
        write(&events);
        assert!(current(fixture.path(), &task).is_ok());
        assert_eq!(
            completion_ack("discard", &json!({"ok":true,"phase":"discarded"})).unwrap()["final"],
            true
        );
        assert!(eligible(fixture.path(), &task).is_err());
    }
}
