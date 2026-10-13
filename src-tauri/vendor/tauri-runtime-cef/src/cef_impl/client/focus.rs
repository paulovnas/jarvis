// Copyright 2019-2024 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use std::sync::{Mutex, mpsc::Sender};

use cef::*;
use tauri_runtime::UserEvent;
use winit::event_loop::EventLoopProxy;

use crate::{macros::wrap_with_args, runtime::Message};

static FOCUS: Mutex<(u64, Option<u32>)> = Mutex::new((0, None));

pub(crate) fn focus_snapshot() -> (u64, Option<u32>) {
  *FOCUS.lock().unwrap()
}

fn record_focus(webview_id: u32) {
  let mut focus = FOCUS.lock().unwrap();
  focus.0 = focus.0.wrapping_add(1);
  focus.1 = Some(webview_id);
}

wrap_with_args! {
  wrap_focus_handler => TauriCefFocusHandlerArgs;

  pub(crate) struct TauriCefFocusHandler<T: UserEvent> {
    sender: Sender<Message<T>>,
    proxy: EventLoopProxy,
    webview_id: u32,
    frame_navigation_state: crate::FrameNavigationState,
  }

  impl FocusHandler {
    fn on_got_focus(&self, browser: Option<&mut Browser>) {
      if let Some(browser) = browser
        && browser.is_popup() == 0
        && self.frame_navigation_state.has_browser_id(browser.identifier())
      {
        // GLib can dispatch restoration idles before Winit drains the focus message.
        record_focus(self.webview_id);
        // Queue the callback: setting CEF focus can happen while Winit borrows a child.
        let _ = self.sender.send(Message::WebviewFocused(self.webview_id));
        self.proxy.wake_up();
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn newer_focus_is_visible_before_the_runtime_queue_is_drained() {
    let before = focus_snapshot();
    record_focus(7);
    assert_eq!(focus_snapshot(), (before.0.wrapping_add(1), Some(7)));
    record_focus(12);
    assert_eq!(focus_snapshot(), (before.0.wrapping_add(2), Some(12)));
  }
}
