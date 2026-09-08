use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum MessagePart {
    Text { text: String },
    Skill { id: String, name: String },
    Attachment { attachment: attachments::Attachment },
}

pub(super) fn normalize(
    home: &std::path::Path,
    project: &std::path::Path,
    content: String,
    mut parts: Vec<MessagePart>,
) -> Result<(String, Vec<MessagePart>), AgentError> {
    if parts.len() > 256 || ids(&parts).len() > 8 {
        return Err(AgentError::new(
            "invalid_message",
            "Selecione até 8 skills por mensagem.",
        ));
    }
    if parts.is_empty() {
        return Ok((content, parts));
    }
    let skills = crate::skills::validate_mentions(home, project, &ids(&parts))
        .map_err(|cause| AgentError::new("skill_error", &cause.message))?;
    let mut rendered = String::new();
    for part in &mut parts {
        match part {
            MessagePart::Attachment { .. } => {}
            MessagePart::Text { text } => rendered.push_str(text),
            MessagePart::Skill { id, name } => {
                *name = skills
                    .iter()
                    .find(|s| s.id == *id)
                    .ok_or_else(AgentError::internal)?
                    .name
                    .clone();
                rendered.push_str(&format!("/{name}"));
            }
        }
    }
    if rendered.trim().is_empty()
        && parts
            .iter()
            .any(|p| matches!(p, MessagePart::Attachment { .. }))
    {
        rendered = "Analise os anexos.".into();
    }
    if rendered.trim().is_empty() || rendered.len() > 100_000 {
        return Err(AgentError::new(
            "invalid_message",
            "Mensagem vazia ou muito longa.",
        ));
    }
    Ok((rendered.trim().into(), parts))
}

fn ids(parts: &[MessagePart]) -> Vec<String> {
    parts
        .iter()
        .filter_map(|part| match part {
            MessagePart::Skill { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect()
}

pub(super) async fn load(session: &Session, home: &std::path::Path) -> Result<(), AgentError> {
    let (content, parts) = {
        let data = session.data.lock().map_err(|_| AgentError::internal())?;
        let turn = &data.turns.last().ok_or_else(AgentError::internal)?.turn;
        (turn.user.clone(), turn.parts.clone())
    };
    let ids = ids(&parts);
    let attached = attachments::prompt(&parts);
    if ids.is_empty() && attached.is_empty() {
        return Ok(());
    }
    let expanded = if ids.is_empty() {
        String::new()
    } else {
        crate::skills::explicit(home, &session.root, ids)
            .await
            .map_err(|cause| AgentError::new("skill_error", &cause.message))?
    };
    session.update(true, |data| {
        data.turns.last_mut().unwrap().wire[0] =
            json!({"role":"user", "content": format!("{content}{expanded}{attached}")});
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn explicit_skills_are_user_input_keep_badges_and_survive_queue_restart() {
        let fixture = super::super::tests::Fixture::new();
        let session = super::super::tests::session(&fixture);
        let dir = fixture.root.join(".jarvis/skills/manual");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "---\nname: manual\ndescription: Only when requested\ndisable-model-invocation: true\n---\nUse the fixture workflow.").unwrap();
        let available = crate::skills::active(&fixture.root, &fixture.root)
            .await
            .unwrap();
        assert!(crate::skills::prompt(&available).is_empty());
        let parts = vec![
            MessagePart::Skill {
                id: available[0].id.clone(),
                name: "spoofed-name".into(),
            },
            MessagePart::Text {
                text: " Verifique 🦀".into(),
            },
        ];
        let (content, parts) =
            normalize(&fixture.root, &fixture.root, "ignored".into(), parts).unwrap();
        assert_eq!(content, "/manual Verifique 🦀");
        let options = TurnOptions {
            account: "test".into(),
            model: "test".into(),
            reasoning: None,
            mode: Mode::Plan,
            workflow: None,
            approval_mode: ApprovalMode::Manual,
        };
        session
            .submit_message("first".into(), options.clone(), vec![])
            .unwrap();
        session
            .submit_message(content.clone(), options, parts)
            .unwrap();
        let (_, extras) = journal::load_all(&session.journal).unwrap();
        assert_eq!(extras.queue[0].parts.len(), 2);
        assert_eq!(extras.queue[0].content, content);
        finish(&session, Ok(()));
        session.reserve_next().unwrap().unwrap();
        load(&session, &fixture.root).await.unwrap();
        let (stored, extras) = journal::load_all(&session.journal).unwrap();
        let turn = &stored[1];
        assert!(extras.queue.is_empty());
        assert_eq!(turn.turn.user, content);
        assert_eq!(turn.turn.parts.len(), 2);
        assert_eq!(turn.wire[0]["role"], "user");
        assert!(turn.wire[0]["content"]
            .as_str()
            .unwrap()
            .contains("Use the fixture workflow."));
        assert!(turn.wire[0]["content"]
            .as_str()
            .unwrap()
            .contains(dir.to_str().unwrap()));
        assert_eq!(turn.turn.options.mode, Mode::Plan);
        fs::remove_file(dir.join("SKILL.md")).unwrap();
        assert!(load(&session, &fixture.root).await.is_err());
        assert!(normalize(
            &fixture.root,
            &fixture.root,
            content,
            turn.turn.parts.clone()
        )
        .is_err());
    }

    #[test]
    fn legacy_queue_and_turn_input_remain_readable_and_mentions_are_bounded() {
        let legacy = json!({"id":"queued","content":"Hello", "options":{"account":"test","model":"test","reasoning":null,"mode":"build","approvalMode":"yolo"}});
        let queued: queue::QueuedMessage = serde_json::from_value(legacy).unwrap();
        assert!(queued.parts.is_empty());
        let home = std::path::Path::new("/unused");
        assert_eq!(
            normalize(home, home, "Hello".into(), vec![]).unwrap().0,
            "Hello"
        );
        assert!(normalize(
            home,
            home,
            "Hello".into(),
            vec![
                MessagePart::Skill {
                    id: "unknown".into(),
                    name: "test".into()
                };
                9
            ]
        )
        .is_err());
    }
}
