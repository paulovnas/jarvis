use super::{cdp, error, AgentError};
use base64::{engine::general_purpose::STANDARD, Engine};

const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;

pub(super) async fn capture(view: &tauri::Webview) -> Result<Vec<u8>, AgentError> {
    let window = view.window();
    if window.is_minimized().unwrap_or(true) || !window.is_visible().unwrap_or(false) {
        return Err(error("Restaure a janela do Jarvis para capturar a página."));
    }
    let session = cdp::session(view)?;
    let result = session
        .request(
            view,
            "Page.captureScreenshot",
            serde_json::json!({"format":"png","captureBeyondViewport":false}),
            None,
            std::time::Duration::from_secs(15),
            MAX_IMAGE_BYTES.div_ceil(3) * 4 + 1024,
        )
        .await?;
    decode_capture(&result)
}

fn decode_capture(result: &serde_json::Value) -> Result<Vec<u8>, AgentError> {
    let data = result["data"]
        .as_str()
        .filter(|data| !data.is_empty() && data.len() <= MAX_IMAGE_BYTES.div_ceil(3) * 4)
        .ok_or_else(|| error("A captura está vazia ou excede 20 MB."))?;
    let bytes = STANDARD
        .decode(data)
        .map_err(|_| error("O Chromium retornou uma captura inválida."))?;
    if bytes.len() > MAX_IMAGE_BYTES || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(error("O Chromium não retornou uma captura PNG válida."));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chromium_capture_decodes_original_png_bytes() {
        let bytes = b"\x89PNG\r\n\x1a\noriginal pixels";
        assert_eq!(
            decode_capture(&json!({"data":STANDARD.encode(bytes)})).unwrap(),
            bytes
        );
    }

    #[test]
    fn chromium_capture_rejects_missing_malformed_or_non_png_data() {
        for result in [
            json!({}),
            json!({"data":""}),
            json!({"data":"%%%"}),
            json!({"data":STANDARD.encode(b"not png")}),
        ] {
            assert!(decode_capture(&result).is_err());
        }
    }
}
