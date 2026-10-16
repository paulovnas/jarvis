use super::{home_dir, ProviderError};
use crate::persistence::AppState;
use tauri::Emitter;

pub(super) fn validate(value: &str) -> Result<&str, ProviderError> {
    let name = value.trim();
    if name.is_empty() || name.chars().count() > 18 || value.chars().any(char::is_control) {
        return Err(ProviderError::new(
            "invalid_display_name",
            "Informe um nome de exibição de 1–18 caracteres, sem quebras de linha.",
        ));
    }
    Ok(name)
}

pub(crate) fn update(
    db: &rusqlite::Connection,
    alias: &str,
    display_name: &str,
) -> Result<(), ProviderError> {
    super::custom::validate_alias(alias)?;
    let name = validate(display_name)?;
    let changed = db
        .execute(
            "UPDATE provider_accounts SET display_name=?2 WHERE alias=?1",
            rusqlite::params![alias, name],
        )
        .map_err(|_| ProviderError::database())?;
    if changed != 1 {
        return Err(ProviderError::new(
            "account_missing",
            "A conta não está mais cadastrada.",
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn set_provider_display_name(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, AppState>,
    alias: String,
    display_name: String,
) -> Result<(), ProviderError> {
    let home = home_dir(&app)?;
    let state = persistence_state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.with_connection(&home, |db| update(db, &alias, &display_name))
    })
    .await
    .map_err(|_| ProviderError::internal())??;
    let _ = app.emit("companion:usage", ());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_names_without_alias_restrictions_and_counts_unicode_characters() {
        assert_eq!(validate("  Minha conta 💻  ").unwrap(), "Minha conta 💻");
        assert!(validate(&"💻".repeat(18)).is_ok());
        for invalid in ["", "  ", "a\nb", "a\tb", "a\u{7f}b", &"💻".repeat(19)] {
            assert!(validate(invalid).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn invalid_names_leave_public_and_internal_metadata_untouched() {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::persistence::initialize_database(&mut db).unwrap();
        let alias = "openai-codex-long-legacy-alias";
        crate::persistence::insert_provider_account(&db, alias, "credential-id", None).unwrap();
        update(&db, alias, "Minha conta").unwrap();
        let previous = crate::persistence::list_provider_accounts(&db).unwrap();
        assert!(update(&db, alias, " ").is_err());
        assert!(update(&db, alias, &"x".repeat(19)).is_err());
        assert_eq!(
            crate::persistence::list_provider_accounts(&db).unwrap(),
            previous
        );
        assert!(update(&db, "missing", "Conta").is_err());
    }
}
