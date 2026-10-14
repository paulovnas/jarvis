use super::{cdp, error, AgentError};
use base64::{engine::general_purpose::STANDARD, Engine};

const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;

pub(super) async fn capture(view: &tauri::Webview) -> Result<Vec<u8>, AgentError> {
    let window = view.window();
    if window.is_minimized().unwrap_or(true) || !window.is_visible().unwrap_or(false) {
        return Err(error("Restaure a janela do Jarvis para capturar a página."));
    }
    let session = cdp::session(view)?;
    #[cfg(windows)]
    let _surface = CaptureSurface::prepare(view).await?;
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

#[cfg(windows)]
#[derive(Default)]
pub(super) struct State(std::sync::Mutex<std::collections::BTreeMap<String, Surface>>);

#[cfg(windows)]
struct Surface {
    bounds: Option<tauri::Rect>,
    offscreen: Option<tauri::Rect>,
    hidden: bool,
}

#[cfg(windows)]
pub(super) struct CaptureSurface<'a> {
    view: &'a tauri::Webview,
}

#[cfg(windows)]
impl<'a> CaptureSurface<'a> {
    pub(super) fn reserve(view: &'a tauri::Webview) -> Result<Self, AgentError> {
        use tauri::Manager;

        let state = view.app_handle().state::<super::BrowserState>();
        {
            let mut surfaces = state.capture.0.lock().map_err(|_| AgentError::internal())?;
            if surfaces.contains_key(view.label()) {
                return Err(error("A aba já está sendo capturada."));
            }
            surfaces.insert(
                view.label().to_owned(),
                Surface {
                    bounds: None,
                    offscreen: None,
                    hidden: false,
                },
            );
        }
        Ok(Self { view })
    }

    pub(super) async fn prepare(view: &'a tauri::Webview) -> Result<Self, AgentError> {
        use tauri::Manager;
        use tauri_runtime_cef::WebviewCefExt;

        let surface = Self::reserve(view)?;
        let state = view.app_handle().state::<super::BrowserState>();
        let (reply, received) = tokio::sync::oneshot::channel();
        view.with_cef_webview(move |native| {
            let _ = reply.send((native.snapshot().visible, native.snapshot().bounds));
        })
        .map_err(|_| error("Não foi possível verificar a superfície da aba."))?;
        let (visible, bounds) = tokio::time::timeout(std::time::Duration::from_secs(5), received)
            .await
            .map_err(|_| error("O Chromium não confirmou a superfície da aba."))?
            .map_err(|_| error("A aba foi fechada antes da captura."))?;
        let visible =
            visible.ok_or_else(|| error("Não foi possível verificar a visibilidade da aba."))?;
        let bounds = bounds.ok_or_else(|| error("As dimensões da aba estão indisponíveis."))?;
        let scale = view
            .window()
            .scale_factor()
            .map_err(|_| error("Não foi possível verificar a escala da aba."))?;
        let offscreen = offscreen_bounds(&bounds, scale);
        let prepared = {
            let mut surfaces = state.capture.0.lock().map_err(|_| AgentError::internal())?;
            let entry = surfaces
                .get_mut(view.label())
                .ok_or_else(AgentError::internal)?;
            entry.bounds = Some(bounds);
            entry.offscreen = Some(offscreen);
            entry.hidden |= !visible;
            if entry.hidden {
                // Windowed CEF on Windows needs a shown child to produce fresh pixels.
                view.set_bounds(offscreen).and_then(|()| view.show())
            } else {
                Ok(())
            }
        };
        prepared.map_err(|_| error("Não foi possível preparar a aba para captura."))?;
        Ok(surface)
    }
}

#[cfg(windows)]
pub(super) fn hide(view: &tauri::Webview) -> Result<(), AgentError> {
    use tauri::Manager;

    let state = view.app_handle().state::<super::BrowserState>();
    let mut surfaces = state.capture.0.lock().map_err(|_| AgentError::internal())?;
    if let Some(surface) = surfaces.get_mut(view.label()) {
        surface.hidden = true;
        // Keep rendering a pending capture while honoring the frontend's hide immediately.
        if let Some(offscreen) = surface.offscreen {
            view.set_bounds(offscreen)
                .map_err(|_| error("Não foi possível ocultar a aba durante a captura."))?;
        } else {
            view.hide()
                .map_err(|_| error("Não foi possível ocultar a aba antes da captura."))?;
        }
    } else {
        let _ = view.hide();
    }
    Ok(())
}

#[cfg(windows)]
fn offscreen_bounds(bounds: &tauri::Rect, scale: f64) -> tauri::Rect {
    let size = bounds.size.to_physical::<u32>(scale);
    tauri::Rect {
        position: tauri::PhysicalPosition::new(
            -i32::try_from(size.width).unwrap_or(i32::MAX),
            -i32::try_from(size.height).unwrap_or(i32::MAX),
        )
        .into(),
        size: bounds.size,
    }
}

#[cfg(windows)]
impl Drop for CaptureSurface<'_> {
    fn drop(&mut self) {
        use tauri::Manager;

        let state = self.view.app_handle().state::<super::BrowserState>();
        if let Ok(mut surfaces) = state.capture.0.lock() {
            if let Some(surface) = surfaces.remove(self.view.label()) {
                if surface.hidden {
                    let _ = self.view.hide();
                    if let Some(bounds) = surface.bounds {
                        let _ = self.view.set_bounds(bounds);
                    }
                }
            }
        };
    }
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

    #[cfg(windows)]
    #[test]
    fn hidden_capture_preserves_page_size_outside_the_host_at_any_display_scale() {
        for scale in [1., 1.25, 2.] {
            let bounds = tauri::Rect {
                position: tauri::LogicalPosition::new(20., 60.).into(),
                size: tauri::LogicalSize::new(900., 600.).into(),
            };
            let offscreen = offscreen_bounds(&bounds, scale);
            let size = offscreen.size.to_physical::<u32>(scale);
            let position = offscreen.position.to_physical::<i32>(scale);
            assert_eq!(size, bounds.size.to_physical::<u32>(scale));
            assert!(i64::from(position.x) + i64::from(size.width) <= 0);
            assert!(i64::from(position.y) + i64::from(size.height) <= 0);
        }
    }
}
