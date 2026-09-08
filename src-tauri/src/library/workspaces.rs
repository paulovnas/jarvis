//! Workspace grouping never moves project source or session journals.
use super::*;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStorage {
    project: Project,
    conversations: usize,
    bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStorage {
    workspace: Workspace,
    projects: Vec<ProjectStorage>,
    conversations: usize,
    bytes: u64,
}

pub(super) fn move_project(connection: &mut Connection, id: &str, workspace_id: &str) -> Result<LibrarySnapshot, LibraryError> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    project(&tx, id)?;
    workspace(&tx, workspace_id)?;
    tx.execute("UPDATE projects SET workspace_id = ?1 WHERE id = ?2", params![workspace_id, id])?;
    tx.execute("UPDATE navigation_selection SET workspace_id = ?1 WHERE project_id = ?2", params![workspace_id, id])?;
    let result = snapshot(&tx)?;
    tx.commit()?;
    Ok(result)
}

pub(super) fn usage(connection: &Connection, home: &Path) -> Result<Vec<WorkspaceStorage>, LibraryError> {
    let library = snapshot(connection)?;
    library.workspaces.into_iter().map(|workspace| {
        let projects = library.projects.iter().filter(|p| p.workspace_id == workspace.id).map(|project| {
            let conversations: Vec<_> = library.conversations.iter().filter(|c| c.project_id == project.id).collect();
            let mut bytes = 0;
            for file in deletion::files_to_delete(home, &project.id, None)? {
                bytes += fs::symlink_metadata(file).map_err(|_| LibraryError::storage())?.len();
            }
            for conversation in &conversations {
                bytes += cleanup::related_size(home, &conversation.id)?;
            }
            Ok(ProjectStorage { project: project.clone(), conversations: conversations.len(), bytes })
        }).collect::<Result<Vec<_>, LibraryError>>()?;
        Ok(WorkspaceStorage { conversations: projects.iter().map(|p| p.conversations).sum(), bytes: projects.iter().map(|p| p.bytes).sum(), workspace, projects })
    }).collect()
}

#[tauri::command]
pub async fn move_project_workspace(app: AppHandle, state: State<'_, AppState>, id: String, workspace_id: String) -> Result<LibrarySnapshot, LibraryError> {
    let result = run(app.clone(), state.inner().clone(), move |db, _| move_project(db, &id, &workspace_id)).await?;
    let _ = app.emit("library:changed", ());
    Ok(result)
}

#[tauri::command]
pub async fn get_workspace_storage(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<WorkspaceStorage>, LibraryError> {
    run(app, state.inner().clone(), |db, home| usage(db, home)).await
}
