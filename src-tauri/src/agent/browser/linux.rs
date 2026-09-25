//! Keep native pages over the main webview without participating in its layout.
use super::*;
use gtk::glib::translate::ToGlibPtr;
use gtk::prelude::*;

async fn update(
    view: &Webview,
    apply: impl FnOnce(webkit2gtk::WebView) -> Result<(), AgentError> + Send + 'static,
) -> Result<(), AgentError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_webview(move |native| {
        let _ = tx.send(apply(native.inner()));
    })
    .map_err(|_| error("Não foi possível ajustar a aba do navegador."))?;
    tokio::time::timeout(Duration::from_secs(5), rx)
        .await
        .map_err(|_| error("O navegador não respondeu ao ajuste da aba."))?
        .map_err(|_| error("A aba do navegador foi fechada."))?
}

pub(super) async fn attach(view: &Webview) -> Result<(), AgentError> {
    update(view, |page| {
        page.hide();
        let parent = page
            .parent()
            .and_then(|parent| parent.downcast::<gtk::Box>().ok())
            .ok_or_else(|| error("Contêiner do navegador indisponível."))?;
        let overlay = parent
            .children()
            .into_iter()
            .find_map(|child| child.downcast::<gtk::Overlay>().ok());
        let overlay = if let Some(overlay) = overlay {
            overlay
        } else {
            let main = parent
                .children()
                .into_iter()
                .filter_map(|child| child.downcast::<webkit2gtk::WebView>().ok())
                .find(|child| child != &page)
                .ok_or_else(|| error("Interface principal indisponível."))?;
            let overlay = gtk::Overlay::new();
            overlay.connect_local("get-child-position", false, |values| {
                let child = values[1].get::<gtk::Widget>().expect("overlay child");
                let (width, height) = child.size_request();
                if width < 0 || height < 0 {
                    return Some(false.to_value());
                }
                // WebKit's natural size follows page content. A size request is
                // only a minimum, so override the overlay's allocation as well.
                // GTK passes a writable, static-scope GdkRectangle here; gtk-rs
                // 0.18 has no binding for this out parameter. Do not copy its value.
                unsafe {
                    let allocation =
                        gtk::glib::gobject_ffi::g_value_get_boxed(values[2].to_glib_none().0)
                            .cast::<gtk::gdk::ffi::GdkRectangle>();
                    *allocation = gtk::gdk::ffi::GdkRectangle {
                        x: 0,
                        y: 0,
                        // GtkOverlay subtracts margins when allocating the child.
                        width: width + child.margin_start(),
                        height: height + child.margin_top(),
                    };
                }
                Some(true.to_value())
            });
            parent.remove(&main);
            overlay.add(&main);
            parent.pack_start(&overlay, true, true, 0);
            overlay.show();
            overlay
        };
        parent.remove(&page);
        page.set_halign(gtk::Align::Start);
        page.set_valign(gtk::Align::Start);
        overlay.add_overlay(&page);
        Ok(())
    })
    .await
}

pub(super) async fn set_bounds(view: &Webview, rect: Viewport) -> Result<(), AgentError> {
    update(view, move |page| {
        // Wry's GtkBox children ignore set_bounds. Overlay margins use logical
        // GTK units and do not change the main webview's allocation or minimum size.
        page.set_margin_start(rect.x.round() as i32);
        page.set_margin_top(rect.y.round() as i32);
        page.set_size_request(rect.width.round() as i32, rect.height.round() as i32);
        Ok(())
    })
    .await
}
