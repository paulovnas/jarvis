//! Publish design changes through the existing owner record without restarting Live.
use super::*;

pub(super) fn refresh_binding(entry: &mut Entry) -> Result<(), CoreError> {
    let Some(identity) = entry
        .helper_identity
        .as_ref()
        .filter(|_| entry.helper_owned)
    else {
        return Ok(());
    };
    if !verify_helper_identity(identity)? {
        return Ok(());
    }
    let next = canonical_design(&entry.project_root, &entry.root)?;
    let previous = std::mem::replace(&mut entry.design_path, next);
    if let Err(cause) = save_owner(entry) {
        entry.design_path = previous;
        return Err(cause);
    }
    Ok(())
}

pub(super) fn refreshable(entry: &Entry, project: &Path) -> bool {
    entry.project_root == project && entry.helper_owned && !entry.stopping
}

pub(crate) fn refresh_design(app: &tauri::AppHandle, project: &Path) {
    let state = app.state::<ImpeccableLiveState>();
    let Ok(mut entries) = state.entries.lock() else {
        return;
    };
    let mut changed = Vec::new();
    for (id, entry) in entries
        .iter_mut()
        .filter(|(_, entry)| refreshable(entry, project))
    {
        match refresh_binding(entry) {
            Ok(()) => changed.push((id.clone(), entry.status.tab_id.clone(), None)),
            Err(cause) => {
                let mut status = entry.status.clone();
                status.error = Some(cause.message);
                changed.push((id.clone(), None, Some(status)));
            }
        }
    }
    drop(entries);
    for (id, tab, failure) in changed {
        if let Some(tab) = tab {
            agent::browser::live::design_changed(app, &id, &tab);
        }
        if let Some(status) = failure {
            notify(app, &status);
        }
    }
}
