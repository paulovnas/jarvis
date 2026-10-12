// Copyright 2019-2024 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use std::sync::{Arc, Weak, mpsc::Sender};

use cef::*;
use tauri_runtime::{
    UserEvent,
    dpi::{LogicalPosition, LogicalSize},
    window::WindowId,
};
use winit::event_loop::EventLoopProxy as WinitEventLoopProxy;

use crate::{
    macros::wrap_with_args,
    runtime::{CefRuntime, Message, NewWindowOpener, RuntimeContext},
};

pub(super) type PopupClientFactory =
    dyn Fn(crate::popup::PopupRequest, crate::FrameNavigationState) -> Client;

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_runtime::webview::NewWindowFeatures;

    #[test]
    fn overdue_initial_navigation_is_not_interrupted_by_blank_recovery() {
        // Both URLs are reported before commit when the first response is slow.
        for current_url in ["", "about:blank"] {
            assert!(!initial_blank_recovery_needed(current_url, true));
            assert!(initial_blank_recovery_needed(current_url, false));
        }
    }

    #[test]
    fn blank_recovery_does_not_replace_an_initialized_or_deferred_document() {
        for current_url in [
            crate::webview::INITIAL_LOAD_URL,
            "http://tauri.localhost/",
            "http://localhost:3001/login",
            "http://localhost:3001/dashboard",
        ] {
            assert!(!initial_blank_recovery_needed(current_url, false));
        }
    }

    #[test]
    fn windowless_close_completes_without_destroying_the_parent_view() {
        assert!(!close_needs_native_destruction(false, true));
        assert!(!close_needs_native_destruction(true, true));
        assert!(!close_needs_native_destruction(true, false));
        assert!(close_needs_native_destruction(false, false));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn auxiliary_windows_never_inherit_the_island_surface_or_accessibility_tree() {
        let mut info = WindowInfo::default().set_as_windowless(std::ptr::null_mut());
        windowed_popup(&mut info);
        assert_eq!(info.windowless_rendering_enabled, 0);
        assert_eq!(info.runtime_style, RuntimeStyle::CHROME);
        assert!(info.parent_view.is_null());
        let client = NativeDevToolsClient::new();
        assert!(client.render_handler().is_none());
        assert!(client.life_span_handler().is_none());
    }

    #[test]
    fn popup_source_observation_is_available_without_dispatch_and_redacted_from_debug() {
        let features =
            NewWindowFeatures::<(), CefRuntime<()>>::new(None, None, NewWindowOpener::new(None));
        assert!(features.opener().source_url().is_none());
        assert!(format!("{features:?}").contains("source_url_observed: false"));

        let source = url::Url::parse("https://example.com/private?token=fixture-secret").unwrap();
        let features = NewWindowFeatures::<(), CefRuntime<()>>::new(
            None,
            None,
            NewWindowOpener::new(Some(source.clone())),
        );
        assert_eq!(features.opener().source_url(), Some(&source));
        let debug = format!("{features:?}");
        assert!(debug.contains("source_url_observed: true"));
        assert!(!debug.contains("private"));
        assert!(!debug.contains("fixture-secret"));
    }
}

fn initial_blank_recovery_needed(current_url: &str, is_loading: bool) -> bool {
    !is_loading && (current_url.is_empty() || current_url == "about:blank")
}

fn close_needs_native_destruction(is_popup: bool, windowless: bool) -> bool {
    !is_popup && !windowless
}

#[cfg(target_os = "macos")]
fn windowed_popup(info: &mut WindowInfo) {
    info.windowless_rendering_enabled = 0;
    info.parent_view = std::ptr::null_mut();
    info.runtime_style = RuntimeStyle::CHROME;
}

#[cfg(target_os = "macos")]
wrap_client! {
    struct NativeDevToolsClient;
    impl Client {}
}

wrap_task! {
  struct RecoverInitialBlankTask {
    browser: Browser,
    initial_url: String,
  }

  impl Task {
    fn execute(&self) {
      if self.browser.is_valid() == 0 {
        return;
      }
      if let Some(frame) = self.browser.main_frame() {
        let current_url = CefString::from(&frame.url()).to_string();
        if initial_blank_recovery_needed(&current_url, self.browser.is_loading() != 0) {
          frame.load_url(Some(&CefString::from(self.initial_url.as_str())));
        }
      }
    }
  }
}

// Recover a blank document left by an early network-service failure once.
// Never restart an in-flight initial navigation: its URL can remain blank
// while a slow local protocol handler is still producing the first response.
// Repeated load_url calls previously aborted that request every 100ms, replaying
// startup/focus IPC and causing visible flicker. CEF owns this single UI task;
// it neither creates a sleeping OS thread nor schedules further retries.
fn check_and_reload_if_blank(browser: cef::Browser, initial_url: String) {
    if initial_url == "about:blank" {
        return;
    }
    let mut task = RecoverInitialBlankTask::new(browser, initial_url);
    if cef::post_delayed_task(sys::cef_thread_id_t::TID_UI.into(), Some(&mut task), 1_000) == 0 {
        log::warn!("CEF could not schedule the initial blank-page recovery");
    }
}

wrap_with_args! {
  wrap_life_span_handler => TauriCefChildLifeSpanHandlerArgs;

  pub struct TauriCefChildLifeSpanHandler<T: UserEvent> {
    sender: Sender<Message<T>>,
    proxy: WinitEventLoopProxy,
    window_id: WindowId,
    webview_id: u32,
    context: RuntimeContext<T>,
    new_window_handler: Option<Arc<tauri_runtime::webview::NewWindowHandler<T, CefRuntime<T>>>>,
    initial_url: Option<String>,
    frame_navigation_state: crate::FrameNavigationState,
    popup_family: Weak<crate::popup::PopupFamily>,
    opener: Option<crate::popup::PopupRequest>,
    create_popup: Arc<PopupClientFactory>,
  }

  impl LifeSpanHandler {
    fn on_after_created(&self, browser: Option<&mut Browser>) {
      if let (Some(browser), Some(opener)) = (browser.as_deref(), self.opener.as_ref()) {
        if let Some(family) = self.popup_family.upgrade() {
          let _ = self.sender.send(Message::PopupCreated(opener.clone(), browser.identifier(), family.clone()));
          self.proxy.wake_up();
          family.created(browser, opener, &self.frame_navigation_state);
        } else if let Some(host) = browser.host() {
          host.close_browser(1);
        }
        return;
      }
      if let Some(browser) = browser
        && browser.is_popup() == 0
        && let Some(initial_url) = &self.initial_url
      {
        check_and_reload_if_blank(browser.clone(), initial_url.clone());
      }
    }

    fn on_before_popup(
      &self,
      browser: Option<&mut Browser>,
      _frame: Option<&mut Frame>,
      popup_id: std::os::raw::c_int,
      target_url: Option<&CefString>,
      _target_frame_name: Option<&CefString>,
      _target_disposition: WindowOpenDisposition,
      _user_gesture: std::os::raw::c_int,
      popup_features: Option<&PopupFeatures>,
      window_info: Option<&mut WindowInfo>,
      client: Option<&mut Option<Client>>,
      _settings: Option<&mut BrowserSettings>,
      _extra_info: Option<&mut Option<DictionaryValue>>,
      _no_javascript_access: Option<&mut i32>,
    ) -> std::os::raw::c_int {
      let url_str = target_url.map(ToString::to_string).unwrap_or_default();
      let response = if let Some(handler) = &self.new_window_handler {
        let Ok(url) = url::Url::parse(&url_str) else { return 1; };
        // window.open features are CSS pixels, which map to logical units.
        let size = popup_features.and_then(|features| {
          (features.width_set != 0 && features.height_set != 0)
            .then(|| LogicalSize::new(features.width as f64, features.height as f64))
        });
        let position = popup_features.and_then(|features| {
          (features.x_set != 0 && features.y_set != 0)
            .then(|| LogicalPosition::new(features.x as f64, features.y as f64))
        });
        let source_url = browser.as_deref()
          .and_then(|browser| browser.main_frame())
          .and_then(|frame| url::Url::parse(&CefString::from(&frame.url()).to_string()).ok());
        handler(url, tauri_runtime::webview::NewWindowFeatures::new(size, position, NewWindowOpener::new(source_url)))
      } else { tauri_runtime::webview::NewWindowResponse::Allow };
      match response {
        tauri_runtime::webview::NewWindowResponse::Allow => {
          let (Some(browser), Some(client), Some(family)) =
            (browser, client, self.popup_family.upgrade()) else { return 1; };
          let Some(request) = family.reserve(&self.frame_navigation_state, browser.identifier(), popup_id) else { return 1; };
          if self.sender.send(Message::PopupPending(request.clone(), family.clone())).is_err() {
            family.abort(&self.frame_navigation_state, popup_id);
            return 1;
          }
          self.proxy.wake_up();
          // A popup is an independent native browser, never the opener's OSR surface.
          #[cfg(target_os = "macos")]
          if browser.host().is_some_and(|host| host.is_window_rendering_disabled() != 0)
            && let Some(info) = window_info {
            windowed_popup(info);
          }
          #[cfg(not(target_os = "macos"))]
          let _ = window_info;
          // Keep CEF's popup creation and JavaScript opener relationship. Only
          // its client changes: root IPC, load/title callbacks and close handling
          // cannot be inherited by a different native browser lifetime.
          *client = Some((self.create_popup)(request, crate::FrameNavigationState::new()));
          0
        },
        tauri_runtime::webview::NewWindowResponse::Create { window_id } => {
          // CEF cannot transplant a popup's contents into an existing
          // browser, so cancel the popup and navigate the designated
          // window's first webview to the URL instead — the closest
          // equivalent of wry hosting the popup in that window's webview.
          // Note `window.opener` is not linked to the new document.
          let _ = self.context.send_message(Message::NavigateFirstWebview {
            window_id,
            url: url_str,
          });
          1
        }
        tauri_runtime::webview::NewWindowResponse::Deny => 1,
      }
    }

    fn on_before_popup_aborted(&self, browser: Option<&mut Browser>, popup_id: std::os::raw::c_int) {
      let Some(browser) = browser else { return; };
      if !self.frame_navigation_state.has_browser_id(browser.identifier()) { return; }
      if let Some(family) = self.popup_family.upgrade()
        && let Some(request) = family.abort(&self.frame_navigation_state, popup_id) {
        let _ = self.sender.send(Message::PopupAborted(request));
        self.proxy.wake_up();
      }
    }

    fn on_before_dev_tools_popup(
      &self,
      browser: Option<&mut Browser>,
      window_info: Option<&mut WindowInfo>,
      client: Option<&mut Option<Client>>,
      _settings: Option<&mut BrowserSettings>,
      _extra_info: Option<&mut Option<DictionaryValue>>,
      _use_default_window: Option<&mut i32>,
    ) {
      #[cfg(target_os = "macos")]
      if browser.and_then(|browser| browser.host())
        .is_some_and(|host| host.is_window_rendering_disabled() != 0) {
        if let Some(info) = window_info { windowed_popup(info); }
        // DevTools cannot inherit the island's rendering, AX tree or IPC handlers.
        if let Some(client) = client { *client = Some(NativeDevToolsClient::new()); }
      }
      #[cfg(not(target_os = "macos"))]
      let _ = (browser, window_info, client);
    }

    /// Take over the browser close so it does not take the window down with it.
    ///
    /// Returning 0 runs CEF's default, which sends the standard close
    /// notification to the browser's *top-level parent window* (`performClose:`
    /// on macOS, `WM_CLOSE` on Windows). Every Tauri webview is a child browser
    /// parented to a shared window, so that default turns "close this webview"
    /// into "close the window and every sibling webview" — and with the last
    /// window gone, the app exits.
    ///
    /// Returning 1 leaves the parent window alone and makes us responsible for
    /// completing the close, which means destroying this browser's own child
    /// view/window on the event loop. That destruction is what drives CEF's
    /// `WindowDestroyed` -> `on_before_close` sequence.
    ///
    /// On Linux the default only closes the browser's own X11 child window (and
    /// calls `WindowDestroyed` itself), so the default is already correct there.
    fn do_close(&self, browser: Option<&mut Browser>) -> std::os::raw::c_int {
      // OSR owns no CEF child view whose destruction could finish the close.
      let Some(browser) = browser else { return 0; };
      let windowless = browser.host().is_some_and(|host| host.is_window_rendering_disabled() != 0);
      if !close_needs_native_destruction(browser.is_popup() != 0, windowless) {
        return 0;
      }

      #[cfg(any(target_os = "macos", windows))]
      {
        let _ = self
          .sender
          .send(Message::DestroyWebviewHostWindow(self.webview_id));
        self.proxy.wake_up();
        return 1;
      }

      #[cfg(not(any(target_os = "macos", windows)))]
      0
    }

    fn on_before_close(&self, browser: Option<&mut Browser>) {
      let Some(browser) = browser else { return; };
      if let Some(family) = self.popup_family.upgrade() {
        family.closed(&self.frame_navigation_state, browser.identifier());
      }
      if browser.is_popup() != 0 {
        if self.opener.is_some() {
          let _ = self.sender.send(Message::PopupClosed(browser.identifier()));
          self.proxy.wake_up();
        }
        return;
      }
      let _ = self
        .sender
        .send(Message::BrowserClosed(self.window_id, self.webview_id));
      self.proxy.wake_up();
    }
  }
}
