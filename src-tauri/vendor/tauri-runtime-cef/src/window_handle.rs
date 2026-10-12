// Copyright 2019-2024 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use std::{
  collections::HashMap,
  sync::{Arc, Mutex},
};
use tauri_runtime::window::WindowId;

/// Shared ownership of live native windows, without retaining a callback's
/// borrowed ActiveEventLoop. Query callers clone the owner before entering
/// native code, so a reentrant getter never holds the registry lock.
pub(crate) struct NativeWindowRegistry<W: ?Sized> {
  windows: Mutex<HashMap<WindowId, Arc<W>>>,
}

impl<W: ?Sized> Default for NativeWindowRegistry<W> {
  fn default() -> Self {
    Self {
      windows: Mutex::new(HashMap::new()),
    }
  }
}

impl<W: ?Sized> NativeWindowRegistry<W> {
  pub(crate) fn insert(&self, id: WindowId, window: Arc<W>) {
    let replaced = self.windows.lock().unwrap().insert(id, window);
    drop(replaced);
  }

  pub(crate) fn get(&self, id: WindowId) -> Option<Arc<W>> {
    self.windows.lock().unwrap().get(&id).cloned()
  }

  pub(crate) fn remove(&self, id: WindowId) {
    let removed = self.windows.lock().unwrap().remove(&id);
    drop(removed);
  }
}

#[cfg(windows)]
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
#[cfg(windows)]
use winit::window::Window as WinitWindow;

pub(crate) struct SendRawWindowHandle(pub raw_window_handle::RawWindowHandle);
unsafe impl Send for SendRawWindowHandle {}

pub(crate) struct SendRawDisplayHandle(pub raw_window_handle::RawDisplayHandle);
unsafe impl Send for SendRawDisplayHandle {}

#[cfg(windows)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct SoftbufferWindowHandle {
  display: RawDisplayHandle,
  window: RawWindowHandle,
}

#[cfg(windows)]
impl SoftbufferWindowHandle {
  pub(crate) fn new(window: &dyn WinitWindow) -> Option<Self> {
    Some(Self {
      display: window.display_handle().ok()?.as_raw(),
      window: window.window_handle().ok()?.as_raw(),
    })
  }
}

#[cfg(windows)]
impl HasDisplayHandle for SoftbufferWindowHandle {
  fn display_handle(
    &self,
  ) -> std::result::Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
    Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(self.display) })
  }
}

#[cfg(windows)]
impl HasWindowHandle for SoftbufferWindowHandle {
  fn window_handle(
    &self,
  ) -> std::result::Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
    Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(self.window) })
  }
}

#[cfg(test)]
mod registry_tests {
  use super::*;
  use std::sync::atomic::{AtomicUsize, Ordering};

  struct NativeWindow(Arc<AtomicUsize>);

  impl Drop for NativeWindow {
    fn drop(&mut self) {
      self.0.fetch_add(1, Ordering::SeqCst);
    }
  }

  #[test]
  fn native_getter_releases_the_registry_lock_and_keeps_the_window_alive() {
    let registry = NativeWindowRegistry::default();
    let id = WindowId::from(1);
    let dropped = Arc::new(AtomicUsize::new(0));
    registry.insert(id, Arc::new(NativeWindow(dropped.clone())));
    let queried = registry.get(id).unwrap();
    // A nested native callback may query the same registry synchronously.
    assert!(registry.windows.try_lock().is_ok());
    let nested = registry.get(id).unwrap();
    assert!(Arc::ptr_eq(&queried, &nested));
    registry.remove(id);
    assert!(registry.get(id).is_none());
    assert_eq!(dropped.load(Ordering::SeqCst), 0);
    drop(queried);
    drop(nested);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
  }

  struct ReentrantWindow(Box<dyn Fn()>);

  impl Drop for ReentrantWindow {
    fn drop(&mut self) {
      (self.0)();
    }
  }

  #[test]
  fn native_getter_registry_drops_replaced_and_removed_windows_without_a_lock() {
    let registry = Arc::new(NativeWindowRegistry::<ReentrantWindow>::default());
    let dropped = Arc::new(AtomicUsize::new(0));
    let id = WindowId::from(1);
    for _ in 0..2 {
      let registry_weak = Arc::downgrade(&registry);
      let dropped = dropped.clone();
      registry.insert(
        id,
        Arc::new(ReentrantWindow(Box::new(move || {
          let registry = registry_weak.upgrade().unwrap();
          assert!(
            registry.windows.try_lock().is_ok(),
            "native destruction must permit reentrant queries"
          );
          dropped.fetch_add(1, Ordering::SeqCst);
        }))),
      );
    }
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    registry.remove(id);
    assert_eq!(dropped.load(Ordering::SeqCst), 2);
  }
}
