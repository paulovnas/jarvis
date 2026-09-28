use super::*;
use std::path::PathBuf;

async fn location(
    app: &tauri::AppHandle,
    state: &AppState,
    project: &str,
) -> Result<(PathBuf, PathBuf), AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::internal())?;
    let state = state.clone();
    let project = project.to_owned();
    let db_home = home.clone();
    let root = tauri::async_runtime::spawn_blocking(move || {
        library::project_directory(&state, &db_home, &project)
    })
    .await
    .map_err(|_| AgentError::internal())??;
    Ok((home, root))
}

#[tauri::command]
pub(crate) async fn get_project_learning(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    project_id: String,
) -> Result<Snapshot, AgentError> {
    let (home, _) = location(&app, &state, &project_id).await?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.with_connection(&home, |db| load(db, &project_id).map(Into::into))
    })
    .await
    .map_err(|_| AgentError::internal())?
}
#[tauri::command]
pub(crate) async fn set_project_learning(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    project_id: String,
    enabled: bool,
    revision: u64,
) -> Result<Snapshot, AgentError> {
    let (home, _) = location(&app, &state, &project_id).await?;
    let state = state.inner().clone();
    let project = project_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        change(&state, &home, &project, |data| {
            if revision != data.revision {
                return Err(error("As opções mudaram. Recarregue antes de salvar."));
            }
            if enabled && !data.enabled {
                data.min_source_at = data.min_source_at.max(super::super::now());
            }
            data.enabled = enabled;
            data.revision += 1;
            if !enabled {
                for f in data.pending.drain(..) {
                    data.processed.push(hash(&format!(
                        "{}:{}",
                        f.evidence.conversation_id, f.evidence.message_id
                    )));
                }
            }
            Ok(data.clone().into())
        })
    })
    .await
    .map_err(|_| AgentError::internal())??;
    if !enabled {
        app.state::<capture::LearningJobs>().cancel(&project_id);
    }
    let _ = app.emit(EVENT, &project_id);
    Ok(result)
}
#[tauri::command]
pub(crate) async fn save_project_lesson(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    project_id: String,
    lesson: Edit,
) -> Result<Snapshot, AgentError> {
    let (home, root) = location(&app, &state, &project_id).await?;
    let state = state.inner().clone();
    let project = project_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        change(&state, &home, &project, |data| {
            edit(data, &root, lesson)?;
            Ok(data.clone().into())
        })
    })
    .await
    .map_err(|_| AgentError::internal())??;
    let _ = app.emit(EVENT, &project_id);
    Ok(result)
}
#[tauri::command]
pub(crate) async fn delete_project_lesson(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    project_id: String,
    id: String,
    revision: u64,
) -> Result<Snapshot, AgentError> {
    let (home, _) = location(&app, &state, &project_id).await?;
    let state = state.inner().clone();
    let project = project_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        change(&state, &home, &project, |data| {
            forget(data, &id, revision)?;
            Ok(data.clone().into())
        })
    })
    .await
    .map_err(|_| AgentError::internal())??;
    let _ = app.emit(EVENT, &project_id);
    Ok(result)
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Export {
    version: u8,
    lessons: Vec<PortableLesson>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PortableLesson {
    scope: String,
    content: String,
    topics: Vec<String>,
    check: String,
}
fn portable(data: Store) -> Export {
    Export {
        version: 1,
        lessons: data
            .lessons
            .into_iter()
            .map(|l| PortableLesson {
                scope: l.scope,
                content: l.content,
                topics: l.topics,
                check: l.check,
            })
            .collect(),
    }
}
fn parse_import(text: &str) -> Result<Export, AgentError> {
    if text.len() > 512 * 1024 {
        return Err(error("O arquivo excede 512 KiB."));
    }
    let data: Export =
        serde_json::from_str(text).map_err(|_| error("Arquivo de aprendizados inválido."))?;
    if data.version != 1 || data.lessons.len() > MAX_LESSONS {
        return Err(error("Versão ou quantidade de aprendizados não suportada."));
    }
    for lesson in &data.lessons {
        validate(&lesson.content, &lesson.topics, &lesson.check)?;
    }
    Ok(data)
}
fn import(data: &mut Store, root: &Path, export: Export) -> Result<(), AgentError> {
    // Validate every path before touching the store, including missing repository mappings.
    let scopes = export
        .lessons
        .iter()
        .map(|l| scope(root, &l.scope))
        .collect::<Result<Vec<_>, _>>()?;
    if data.lessons.len() + export.lessons.len() > MAX_LESSONS {
        return Err(error("A importação ultrapassa o limite de 200 aprendizados. Revise os itens antes de importar."));
    }
    for (lesson, scope) in export.lessons.into_iter().zip(scopes) {
        let content = redact(lesson.content.trim());
        if data
            .lessons
            .iter()
            .any(|l| fingerprint(&l.scope, &l.content) == fingerprint(&scope, &content))
        {
            continue;
        }
        data.lessons.push(Lesson {
            id: library::new_id()?,
            scope,
            content,
            topics: lesson.topics.into_iter().map(|v| redact(&v)).collect(),
            check: redact(&lesson.check),
            status: Status::Suggested,
            origin: Origin::Imported,
            evidence: vec![],
            revision: 1,
            updated_at: super::super::now(),
        });
    }
    data.revision += 1;
    Ok(())
}
#[tauri::command]
pub(crate) async fn export_project_learning(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    project_id: String,
) -> Result<bool, AgentError> {
    use tauri_plugin_dialog::DialogExt;
    let (home, _) = location(&app, &state, &project_id).await?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let data = state.with_connection(&home, |db| load(db, &project_id))?;
        let text =
            serde_json::to_string_pretty(&portable(data)).map_err(|_| AgentError::internal())?;
        let Some(file) = app
            .dialog()
            .file()
            .set_title("Exportar aprendizados do projeto")
            .add_filter("Aprendizados Jarvis", &["json"])
            .set_file_name("aprendizados-jarvis.json")
            .blocking_save_file()
        else {
            return Ok(false);
        };
        let path = file
            .into_path()
            .map_err(|_| error("Selecione um caminho local."))?;
        tools::write_atomic(&path, &text)?;
        Ok(true)
    })
    .await
    .map_err(|_| AgentError::internal())?
}
#[tauri::command]
pub(crate) async fn preview_project_learning_import(
    app: tauri::AppHandle,
) -> Result<Option<Export>, AgentError> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(file) = app
            .dialog()
            .file()
            .set_title("Importar aprendizados do projeto")
            .add_filter("Aprendizados Jarvis", &["json"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = file
            .into_path()
            .map_err(|_| error("Selecione um arquivo local."))?;
        parse_import(&tools::read_text(&path)?).map(Some)
    })
    .await
    .map_err(|_| AgentError::internal())?
}
#[tauri::command]
pub(crate) async fn import_project_learning(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    project_id: String,
    content: String,
) -> Result<Snapshot, AgentError> {
    let export = parse_import(&content)?;
    let (home, root) = location(&app, &state, &project_id).await?;
    let state = state.inner().clone();
    let project = project_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        change(&state, &home, &project, |data| {
            import(data, &root, export)?;
            Ok(data.clone().into())
        })
    })
    .await
    .map_err(|_| AgentError::internal())??;
    let _ = app.emit(EVENT, &project_id);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_lessons_exclude_conversation_evidence_and_import_as_suggestions() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::empty();
        import(&mut store,root.path(),parse_import(r#"{"version":1,"lessons":[{"scope":".","content":"Exibir o label do Select.","topics":["select"],"check":"Verificar o texto visível."}]}"#).unwrap()).unwrap();
        assert_eq!(store.lessons[0].status, Status::Suggested);
        assert!(store.lessons[0].evidence.is_empty());
        let exported = serde_json::to_string(&portable(store)).unwrap();
        assert!(!exported.contains("evidence"));
        assert!(!exported.contains("conversationId"));
        assert!(parse_import(r#"{"version":99,"lessons":[]}"#).is_err());
    }
}
