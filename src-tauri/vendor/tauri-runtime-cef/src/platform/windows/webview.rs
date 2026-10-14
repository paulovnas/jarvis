// Copyright 2019-2024 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use cef::ImplBrowserHost;
use tauri_runtime::dpi::{PhysicalPosition, PhysicalSize, Rect};
use tauri_utils::config::Color;
use windows::Win32::{
  Foundation::{
    ERROR_SUCCESS, GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, SetLastError, WPARAM,
  },
  Graphics::Gdi::MapWindowPoints,
  UI::Shell::{DefSubclassProc, SetWindowSubclass},
  UI::WindowsAndMessaging::{
    DestroyWindow, GetParent, GetWindowRect, HWND_TOP, IsWindow, IsWindowVisible, SW_HIDE,
    SW_SHOWNA, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetParent, SetWindowPos,
    ShowWindow, WINDOWPOS, WM_WINDOWPOSCHANGING,
  },
};

use crate::{webview::AppWebview, window::AppWindow};

impl AppWebview {
  pub(crate) fn native_parent_matches(&self, parent: &AppWindow) -> Option<bool> {
    let hwnd = self.hwnd();
    unsafe {
      if !IsWindow(Some(hwnd)).as_bool() {
        return None;
      }
      // `GetParent` returns NULL both for a window that has no parent and when
      // the call itself fails, and the binding maps that NULL to an `Err`
      // carrying the last error — so clear it first to tell the two apart.
      SetLastError(ERROR_SUCCESS);
      match GetParent(hwnd) {
        Ok(native_parent) => Some(native_parent == parent.hwnd()),
        // `ERROR_SUCCESS`: the window really has no parent, which is an
        // observation of the relationship and not a failure to establish it.
        Err(err) if err.code().is_ok() => Some(false),
        Err(_) => None,
      }
    }
  }

  pub(crate) fn native_visible(&self) -> Option<bool> {
    let hwnd = self.hwnd();
    unsafe {
      IsWindow(Some(hwnd))
        .as_bool()
        .then(|| IsWindowVisible(hwnd).as_bool())
    }
  }

  pub(crate) fn hwnd(&self) -> HWND {
    let hwnd = self.host.window_handle();
    HWND(hwnd.0 as _)
  }

  pub(crate) fn set_background_color(&self, _color: Option<Color>) {
    // TODO: might not be supported on Windows
  }

  pub(crate) fn bounds(&self) -> Option<Rect> {
    Self::window_bounds(self.hwnd())
  }

  fn window_bounds(hwnd: HWND) -> Option<Rect> {
    let mut rect = RECT::default();
    unsafe {
      let parent = GetParent(hwnd).ok()?;
      if parent.0.is_null() {
        return None;
      }

      GetWindowRect(hwnd, &mut rect).ok()?;

      let mut points = [
        POINT {
          x: rect.left,
          y: rect.top,
        },
        POINT {
          x: rect.right,
          y: rect.bottom,
        },
      ];
      // A zero translation is valid when Win32 reports no error.
      SetLastError(ERROR_SUCCESS);
      if MapWindowPoints(None, Some(parent), &mut points) == 0 && GetLastError() != ERROR_SUCCESS {
        return None;
      }

      let x = points[0].x;
      let y = points[0].y;
      let width = (points[1].x - points[0].x).max(0) as u32;
      let height = (points[1].y - points[0].y).max(0) as u32;
      Some(Rect {
        position: PhysicalPosition::new(x, y).into(),
        size: PhysicalSize::new(width, height).into(),
      })
    }
  }

  pub(crate) fn reparent(&self, parent: &AppWindow) {
    let parent = parent.hwnd();
    let _ = unsafe { SetParent(self.hwnd(), Some(parent)) };
  }

  pub(crate) fn apply_visible(&self, visible: bool) {
    let _ = unsafe { ShowWindow(self.hwnd(), if visible { SW_SHOWNA } else { SW_HIDE }) };
  }

  /// Destroys CEF's own window for this browser, completing a close that
  /// `do_close` took over. CEF's browser window procedure reports
  /// `WindowDestroyed` back to CEF on `WM_NCDESTROY`.
  pub(crate) fn destroy_host_window(&self) {
    let _ = unsafe { DestroyWindow(self.hwnd()) };
  }

  const PIN_Z_ORDER_SUBCLASS_ID: usize = 124;
  /// `dwRefData` of the pin subclass: whether it is currently vetoing.
  const Z_ORDER_UNPINNED: usize = 0;
  const Z_ORDER_PINNED: usize = 1;

  /// Refuses every z-order change to this webview while the pin is engaged.
  unsafe extern "system" fn pin_z_order_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    pinned: usize,
  ) -> LRESULT {
    unsafe {
      if pinned == Self::Z_ORDER_PINNED && msg == WM_WINDOWPOSCHANGING && lparam.0 != 0 {
        let window_pos = &mut *(lparam.0 as *mut WINDOWPOS);
        window_pos.flags |= SWP_NOZORDER;
      }

      DefSubclassProc(hwnd, msg, wparam, lparam)
    }
  }

  /// Engages or disengages the z-order pin.
  ///
  /// Re-installing the same proc under the same id does not chain a second
  /// subclass, it just updates `dwRefData` — so this both installs the pin the
  /// first time and toggles it afterwards.
  fn set_z_order_pinned(&self, pinned: bool) {
    let _ = unsafe {
      SetWindowSubclass(
        self.hwnd(),
        Some(Self::pin_z_order_subclass_proc),
        Self::PIN_Z_ORDER_SUBCLASS_ID,
        if pinned {
          Self::Z_ORDER_PINNED
        } else {
          Self::Z_ORDER_UNPINNED
        },
      )
    };
  }

  /// Raises this webview above its siblings and pins it there, so nothing but
  /// this runtime can move it again. See [`Self::pin_z_order_subclass_proc`].
  pub(crate) fn raise_to_top(&self) {
    self.set_z_order_pinned(false);

    let _ = unsafe {
      SetWindowPos(
        self.hwnd(),
        Some(HWND_TOP),
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
      )
    };

    self.set_z_order_pinned(true);
  }

  pub(crate) fn apply_physical_bounds(&self, _scale: f64, x: i32, y: i32, width: i32, height: i32) {
    unsafe {
      let _ = SetWindowPos(
        self.hwnd(),
        None,
        x,
        y,
        width,
        height,
        SWP_NOZORDER | SWP_NOACTIVATE,
      );
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use windows::{
    Win32::{
      Foundation::ERROR_INVALID_WINDOW_HANDLE,
      UI::WindowsAndMessaging::{CreateWindowExW, WS_CHILD, WS_POPUP},
    },
    core::w,
  };

  #[test]
  fn bounds_at_screen_origin_preserve_zero_offset_mapping() {
    // SAFETY: Builtin class strings outlive creation; the invisible windows belong to this thread.
    let create = |style, parent| unsafe {
      CreateWindowExW(
        Default::default(),
        w!("STATIC"),
        w!(""),
        style,
        0,
        0,
        64,
        48,
        parent,
        None,
        None,
        None,
      )
    };
    let parent = create(WS_POPUP, None).expect("create invisible parent");
    let child = match create(WS_CHILD, Some(parent)) {
      Ok(child) => child,
      Err(error) => {
        // SAFETY: The parent was created by this test on the current thread.
        let _ = unsafe { DestroyWindow(parent) };
        panic!("create invisible child: {error}");
      }
    };
    // SAFETY: Last-error state is thread-local; both handles still belong to this test.
    let (origin, bounds, child_removed, parent_removed) = unsafe {
      let mut origin = [POINT { x: 0, y: 0 }];
      MapWindowPoints(Some(parent), None, &mut origin);
      SetLastError(ERROR_INVALID_WINDOW_HANDLE);
      let bounds = AppWebview::window_bounds(child);
      (
        origin[0],
        bounds,
        DestroyWindow(child),
        DestroyWindow(parent),
      )
    };
    child_removed.expect("destroy child");
    parent_removed.expect("destroy parent");
    assert_eq!((origin.x, origin.y), (0, 0));
    let bounds = bounds.expect("identity mapping must preserve native bounds");
    assert_eq!(
      bounds.position.to_physical::<i32>(1.),
      PhysicalPosition::new(0, 0)
    );
    assert_eq!(
      bounds.size.to_physical::<u32>(1.),
      PhysicalSize::new(64, 48)
    );
  }
}
