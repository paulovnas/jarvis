//! Explicit provider replacements, committed with deletion in the same database transaction.
//! The original choice is a guard: later user edits cannot inherit a stale replacement.
use crate::{agent::workflow::settings::ModelChoice, persistence::PersistenceError};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Binding {
    pub item_key: String,
    pub source: ModelChoice,
    pub target: ModelChoice,
}

pub(crate) fn encode(choice: &ModelChoice) -> Result<String, PersistenceError> {
    serde_json::to_string(choice).map_err(|_| PersistenceError::new("Invalid model binding"))
}

pub(crate) fn resolve(
    db: &Connection,
    key: &str,
    source: &ModelChoice,
) -> Result<ModelChoice, PersistenceError> {
    let target: Option<String> = db
        .query_row(
            "SELECT target FROM provider_model_bindings WHERE item_key=?1 AND source=?2",
            params![key, encode(source)?],
            |row| row.get(0),
        )
        .optional()?;
    target.map_or_else(
        || Ok(source.clone()),
        |target| {
            serde_json::from_str(&target)
                .map_err(|_| PersistenceError::new("Invalid model replacement"))
        },
    )
}

pub(crate) fn replace(
    db: &Connection,
    key: &str,
    source: &ModelChoice,
    target: &ModelChoice,
) -> Result<(), PersistenceError> {
    db.execute(
        "INSERT INTO provider_model_bindings(item_key,source,target) VALUES(?1,?2,?3)
         ON CONFLICT(item_key,source) DO UPDATE SET target=excluded.target",
        params![key, encode(source)?, encode(target)?],
    )?;
    Ok(())
}

pub(crate) fn revision(db: &Connection) -> Result<u64, PersistenceError> {
    let value: i64 = db.query_row(
        "SELECT revision FROM provider_bindings_revision WHERE id=1",
        [],
        |row| row.get(0),
    )?;
    value
        .try_into()
        .map_err(|_| PersistenceError::new("Invalid model bindings revision"))
}

pub(crate) fn forget_item(db: &Connection, key: &str) -> Result<(), PersistenceError> {
    db.execute(
        "DELETE FROM provider_model_bindings WHERE item_key=?1",
        [key],
    )?;
    Ok(())
}

pub(crate) fn forget_choice(
    db: &Connection,
    key: &str,
    choice: &ModelChoice,
) -> Result<(), PersistenceError> {
    db.execute(
        "DELETE FROM provider_model_bindings WHERE item_key=?1 AND source=?2",
        params![key, encode(choice)?],
    )?;
    Ok(())
}

pub(crate) fn list(db: &Connection) -> Result<Vec<Binding>, PersistenceError> {
    let mut statement = db.prepare(
        "SELECT item_key,source,target FROM provider_model_bindings ORDER BY item_key,source",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    rows.map(|row| {
        let (item_key, source, target) = row?;
        Ok(Binding {
            item_key,
            source: serde_json::from_str(&source)
                .map_err(|_| PersistenceError::new("Invalid model binding"))?,
            target: serde_json::from_str(&target)
                .map_err(|_| PersistenceError::new("Invalid model replacement"))?,
        })
    })
    .collect()
}
