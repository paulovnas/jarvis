use super::AgentError;
use std::{fs, path::PathBuf};

const MAX_MARKDOWN_BYTES: usize = 8 * 1024 * 1024;

fn invalid(message: &str) -> AgentError {
    AgentError::new("invalid_markdown_export", message)
}

fn validate_content(content: String) -> Result<String, AgentError> {
    if content.trim().is_empty() {
        return Err(invalid("A resposta está vazia."));
    }
    if content.len() > MAX_MARKDOWN_BYTES {
        return Err(invalid("A resposta é grande demais para ser exportada."));
    }
    Ok(content)
}

fn file_name(value: &str) -> String {
    let stem = value
        .trim()
        .strip_suffix(".md")
        .or_else(|| value.trim().strip_suffix(".MD"))
        .unwrap_or(value.trim());
    let normalized = stem
        .chars()
        .map(|character| {
            if character.is_control() || r#"<>:"/\|?*"#.contains(character) {
                '-'
            } else {
                character
            }
        })
        .collect::<String>();
    let normalized = normalized.trim_matches([' ', '.', '-']);
    let normalized = if normalized.is_empty() {
        "Resposta-Jarvis"
    } else {
        normalized
    };
    format!("{normalized}.md")
}

fn markdown_path(mut path: PathBuf) -> PathBuf {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
    {
        path.set_extension("md");
    }
    path
}

#[tauri::command]
pub async fn save_markdown_document(
    app: tauri::AppHandle,
    content: String,
    suggested_file_name: String,
) -> Result<bool, AgentError> {
    use tauri_plugin_dialog::DialogExt;
    let content = validate_content(content)?;
    let suggested_file_name = file_name(&suggested_file_name);
    tauri::async_runtime::spawn_blocking(move || {
        let Some(file) = app
            .dialog()
            .file()
            .set_title("Salvar resposta em Markdown")
            .add_filter("Documento Markdown", &["md"])
            .set_file_name(&suggested_file_name)
            .blocking_save_file()
        else {
            return Ok(false);
        };
        let path = file
            .into_path()
            .map_err(|_| invalid("Selecione um caminho local."))?;
        fs::write(markdown_path(path), content.as_bytes())
            .map_err(|_| invalid("Não foi possível salvar o documento."))?;
        Ok(true)
    })
    .await
    .map_err(|_| AgentError::internal())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_names_are_portable_markdown_files() {
        assert_eq!(file_name("Resposta: análise?.md"), "Resposta- análise.md");
        assert_eq!(file_name("..."), "Resposta-Jarvis.md");
        assert_eq!(
            markdown_path(PathBuf::from("resposta.txt")),
            PathBuf::from("resposta.md")
        );
        assert_eq!(
            markdown_path(PathBuf::from("resposta.MD")),
            PathBuf::from("resposta.MD")
        );
    }

    #[test]
    fn empty_or_oversized_documents_are_rejected_before_the_dialog() {
        assert!(validate_content(" \n".into()).is_err());
        assert!(validate_content("a".repeat(MAX_MARKDOWN_BYTES + 1)).is_err());
        assert_eq!(validate_content("# Resposta".into()).unwrap(), "# Resposta");
    }
}
