use super::{error, AgentError};

pub(super) async fn capture(view: &tauri::Webview) -> Result<Vec<u8>, AgentError> {
    let window = view.window();
    if window.is_minimized().unwrap_or(true) || !window.is_visible().unwrap_or(false) {
        return Err(error("Restaure a janela do Jarvis para capturar a página."));
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    #[cfg(windows)]
    let restore = std::sync::Arc::new(std::sync::Mutex::new(None));
    #[cfg(windows)]
    let saved = restore.clone();
    #[cfg(windows)]
    let _guard = CaptureGuard {
        view: view.clone(),
        restore,
    };
    #[cfg(windows)]
    view.with_webview(move |native| capture_native(native, tx, saved))
        .map_err(|_| error("Não foi possível acessar a captura nativa."))?;
    #[cfg(not(windows))]
    view.with_webview(move |native| capture_native(native, tx))
        .map_err(|_| error("Não foi possível acessar a captura nativa."))?;
    let result = tokio::time::timeout(std::time::Duration::from_secs(15), rx).await;
    let bytes = result
        .map_err(|_| error("A captura não respondeu em 15 segundos."))?
        .map_err(|_| error("A aba foi fechada durante a captura."))??;
    if bytes.len() > 20 * 1024 * 1024 {
        return Err(error(
            "A captura excedeu 20 MB. Reduza o tamanho da janela.",
        ));
    }
    Ok(bytes)
}
type Reply = tokio::sync::oneshot::Sender<Result<Vec<u8>, AgentError>>;

#[cfg(windows)]
struct CaptureGuard {
    view: tauri::Webview,
    restore: std::sync::Arc<std::sync::Mutex<Option<windows::Win32::Foundation::RECT>>>,
}
#[cfg(windows)]
impl Drop for CaptureGuard {
    fn drop(&mut self) {
        let restore = self.restore.clone();
        let _ = self.view.with_webview(move |native| {
            if let Ok(mut saved) = restore.lock() {
                if let Some(bounds) = saved.take() {
                    // Cancellation and timeouts restore the hidden view as well.
                    unsafe {
                        let _ = native.controller().SetIsVisible(false);
                        let _ = native.controller().SetBounds(bounds);
                    }
                }
            }
        });
    }
}

#[cfg(windows)]
fn capture_native(
    native: tauri::webview::PlatformWebview,
    tx: Reply,
    restore: std::sync::Arc<std::sync::Mutex<Option<windows::Win32::Foundation::RECT>>>,
) {
    #[cfg(feature = "browser-probe")]
    println!("Probe: entered native capture callback");
    use webview2_com::{
        CapturePreviewCompletedHandler,
        Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
    };
    use windows::Win32::{
        System::Com::{STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET},
        UI::Shell::SHCreateMemStream,
    };
    let controller = native.controller();
    let mut visible = windows::core::BOOL::default();
    let mut bounds = windows::Win32::Foundation::RECT::default();
    // WebView2 does not complete CapturePreview for an IsVisible=false controller.
    // Give a background view a renderable surface outside the window's client area.
    unsafe {
        if controller.IsVisible(&mut visible).is_ok()
            && !visible.as_bool()
            && controller.Bounds(&mut bounds).is_ok()
        {
            if let Ok(mut saved) = restore.lock() {
                *saved = Some(bounds);
            }
            let offscreen = windows::Win32::Foundation::RECT {
                left: -(bounds.right - bounds.left) - 10,
                top: -(bounds.bottom - bounds.top) - 10,
                right: -10,
                bottom: -10,
            };
            let _ = controller.SetBounds(offscreen);
            let _ = controller.SetIsVisible(true);
        }
    }
    // This COM stream is created, used and released on the UI thread.
    let Some(stream) = (unsafe { SHCreateMemStream(None) }) else {
        let _ = tx.send(Err(error("Não foi possível criar o buffer da captura.")));
        return;
    };
    let captured = stream.clone();
    let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
    let reply = tx.clone();
    let callback = CapturePreviewCompletedHandler::create(Box::new(move |status| {
        #[cfg(feature = "browser-probe")]
        println!("Probe: CapturePreview completed: {status:?}");
        let result = status
            .map_err(|_| error("A página ainda não está pronta para captura."))
            .and_then(|()| {
                let mut stat = STATSTG::default();
                unsafe { captured.Stat(&mut stat, STATFLAG_NONAME) }
                    .map_err(|_| error("Não foi possível ler a captura."))?;
                if stat.cbSize == 0 || stat.cbSize > 20 * 1024 * 1024 {
                    return Err(error("A captura está vazia ou excede 20 MB."));
                }
                let mut bytes = vec![0; stat.cbSize as usize];
                let mut read = 0;
                unsafe {
                    captured
                        .Seek(0, STREAM_SEEK_SET, None)
                        .map_err(|_| error("Não foi possível ler a captura."))?;
                    captured
                        .Read(
                            bytes.as_mut_ptr().cast(),
                            bytes.len() as u32,
                            Some(&mut read),
                        )
                        .ok()
                        .map_err(|_| error("Não foi possível ler a captura."))?;
                }
                if read as usize != bytes.len() {
                    return Err(error("A captura ficou incompleta."));
                }
                Ok(bytes)
            });
        if let Ok(mut reply) = reply.lock() {
            if let Some(tx) = reply.take() {
                let _ = tx.send(result);
            }
        }
        Ok(())
    }));
    // The platform handle and its COM controller are accessed only on Tauri's UI thread.
    let result = unsafe {
        native.controller().CoreWebView2().and_then(|view| {
            view.CapturePreview(
                COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
                &stream,
                &callback,
            )
        })
    };
    #[cfg(feature = "browser-probe")]
    println!("Probe: CapturePreview scheduled: {result:?}");
    if result.is_err() {
        if let Ok(mut tx) = tx.lock() {
            if let Some(tx) = tx.take() {
                let _ = tx.send(Err(error("WebView2 não conseguiu iniciar a captura.")));
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn capture_native(native: tauri::webview::PlatformWebview, tx: Reply) {
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
    use objc2_foundation::{NSDictionary, NSError};
    use objc2_web_kit::WKWebView;
    let tx = std::sync::Mutex::new(Some(tx));
    let completion = block2::RcBlock::new(move |image: *mut NSImage, failure: *mut NSError| {
        // WKWebView supplies retained-for-callback AppKit objects on the main thread.
        let result = unsafe {
            image
                .as_ref()
                .filter(|_| failure.is_null())
                .and_then(|image| image.TIFFRepresentation())
                .and_then(|data| NSBitmapImageRep::imageRepWithData(&data))
                .and_then(|image| {
                    image.representationUsingType_properties(
                        NSBitmapImageFileType::PNG,
                        &NSDictionary::new(),
                    )
                })
                .map(|data| data.to_vec())
                .ok_or_else(|| error("WKWebView não conseguiu capturar esta página."))
        };
        if let Ok(mut tx) = tx.lock() {
            if let Some(tx) = tx.take() {
                let _ = tx.send(result);
            }
        }
    });
    // Tauri owns the WKWebView; this borrow never escapes the UI-thread callback.
    unsafe {
        (&*native.inner().cast::<WKWebView>())
            .takeSnapshotWithConfiguration_completionHandler(None, &completion);
    }
}

#[cfg(target_os = "linux")]
fn capture_native(native: tauri::webview::PlatformWebview, tx: Reply) {
    use webkit2gtk::{SnapshotOptions, SnapshotRegion, WebViewExt};
    native.inner().snapshot(
        SnapshotRegion::Visible,
        SnapshotOptions::NONE,
        None::<&webkit2gtk::gio::Cancellable>,
        move |result| {
            let result = result
                .map_err(|_| error("WebKitGTK não conseguiu capturar esta página."))
                .and_then(|surface| {
                    let mut bytes = Vec::new();
                    surface
                        .write_to_png(&mut bytes)
                        .map_err(|_| error("Não foi possível codificar a captura."))?;
                    Ok(bytes)
                });
            let _ = tx.send(result);
        },
    );
}
