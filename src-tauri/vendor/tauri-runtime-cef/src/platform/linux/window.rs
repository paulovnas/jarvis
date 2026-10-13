// Copyright 2019-2024 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

#[cfg(target_os = "linux")]
use raw_window_handle::HasDisplayHandle;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
  cell::{Cell, RefCell},
  os::raw::c_ulong,
  rc::Rc,
};
use tauri_runtime::ProgressBarState;
use tauri_runtime::dpi::PhysicalSize;
use tauri_utils::config::Color;
use winit::platform::gtk4::WindowExtGtk4;

use crate::window::AppWindow;

use super::{taskbar, utils::set_wm_state};

/// 24-bit X11 parent for CEF browser children.
///
/// GTK owns the toplevel layout and menu widgets, while CEF creates native X11
/// child windows. This host is kept sized to GTK's content box so CEF renders
/// below GTK UI instead of covering it. The host uses a 24-bit TrueColor visual
/// because CEF does not render correctly with the inherited GTK window visual on
/// all X11 setups.
///
/// Hierarchy:
/// - GtkApplicationWindow
///   - GtkBox
///     - menu
///     - content GtkBox
///   - CefX11Host, positioned over the content GtkBox
///     - CEF webview
///     - CEF webview
///     - CEF webview
pub(crate) struct CefX11Host {
  default_vbox: gtk::Box,
  xid: c_ulong,
  colormap: c_ulong,
  geometry: Rc<HostGeometry>,
  /// CSS provider currently backing this window's background color, kept so it can be removed
  /// from the display instead of accumulating one provider per `set_background_color` call.
  background_color_provider: RefCell<Option<gtk::CssProvider>>,
  #[cfg(target_os = "linux")]
  focusable: Rc<Cell<bool>>,
  #[cfg(target_os = "linux")]
  gtk_display: std::ptr::NonNull<x11_dl::xlib::Display>,
}

/// Geometry of the X11 host, shared with the GTK `layout` handler that keeps it up to date.
#[derive(Default)]
struct HostGeometry {
  size: Cell<PhysicalSize<u32>>,
  /// Set when [`Self::size`] changed and the CEF children have not been laid out against it yet.
  needs_relayout: Cell<bool>,
}

impl CefX11Host {
  pub(crate) fn new(window: &dyn winit::window::Window) -> Option<Self> {
    use gtk::prelude::*;

    let gtk_window = window.gtk_window()?;
    let default_vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    default_vbox.set_hexpand(true);
    default_vbox.set_vexpand(true);

    let webview_area = gtk::Box::new(gtk::Orientation::Vertical, 0);
    webview_area.set_hexpand(true);
    webview_area.set_vexpand(true);

    default_vbox.append(&webview_area);
    gtk_window.set_child(Some(&default_vbox));

    let parent_xid = window_xid(window);
    let initial_size = window.surface_size();
    let (xid, colormap) = create_cef_container(parent_xid, initial_size)?;

    #[cfg(target_os = "linux")]
    let focusable = Rc::new(Cell::new(true));
    #[cfg(target_os = "linux")]
    let gtk_display = match window.display_handle().ok()?.as_raw() {
      raw_window_handle::RawDisplayHandle::Xlib(handle) => handle.display?.cast(),
      _ => return None,
    };

    let geometry = Rc::new(HostGeometry {
      size: Cell::new(initial_size),
      needs_relayout: Cell::new(false),
    });

    if let Some(surface) = gtk_window.surface() {
      #[cfg(target_os = "linux")]
      for property in ["state", "mapped"] {
        let focusable = focusable.clone();
        surface.connect_notify_local(Some(property), move |surface, _| {
          if !surface.is_destroyed() && surface.is_mapped() && !focusable.get() {
            apply_focusable(gtk_display, parent_xid, false);
          }
        });
      }
      let gtk_window = gtk_window.clone();
      let layout_webview_area = webview_area.clone();
      let layout_geometry = geometry.clone();
      surface.connect_layout(move |surface, _, _| {
        let size = set_cef_container_bounds(
          xid,
          &gtk_window,
          &layout_webview_area,
          surface.scale_factor().max(1) as f64,
        );
        if layout_geometry.size.replace(size) != size {
          // The content area changed without the toplevel being resized - a menu bar was
          // attached, hidden or shown - so winit emits no `SurfaceResized` and the CEF children
          // would keep the bounds computed against the previous host size.
          layout_geometry.needs_relayout.set(true);
        }
      });
    }

    Some(Self {
      default_vbox,
      xid,
      colormap,
      geometry,
      background_color_provider: RefCell::new(None),
      #[cfg(target_os = "linux")]
      focusable,
      #[cfg(target_os = "linux")]
      gtk_display,
    })
  }

  pub(crate) fn default_vbox(&self) -> gtk::Box {
    self.default_vbox.clone()
  }

  /// CSS class carrying this window's background color. The X11 host id makes it unique per
  /// window, since the providers below are registered display-wide.
  fn background_color_class(&self) -> String {
    format!("tauri-cef-window-background-{}", self.xid)
  }

  pub(crate) fn size(&self) -> PhysicalSize<u32> {
    self.geometry.size.get()
  }

  /// Whether the host was resized by GTK since the last time the CEF children were laid out.
  pub(crate) fn take_needs_relayout(&self) -> bool {
    self.geometry.needs_relayout.replace(false)
  }

  fn take_background_color_provider(&self) -> Option<gtk::CssProvider> {
    self.background_color_provider.borrow_mut().take()
  }

  fn set_background_color_provider(&self, provider: gtk::CssProvider) {
    self.background_color_provider.replace(Some(provider));
  }
}

impl Drop for CefX11Host {
  fn drop(&mut self) {
    if let Some(provider) = self.background_color_provider.borrow_mut().take() {
      gtk::style_context_remove_provider_for_display(
        &gtk::prelude::WidgetExt::display(&self.default_vbox),
        &provider,
      );
    }

    super::utils::with_x11((), |xlib, display| unsafe {
      (xlib.XDestroyWindow)(display, self.xid);
      (xlib.XFreeColormap)(display, self.colormap);
    });
  }
}

#[cfg(target_os = "linux")]
fn focused_window_ancestors() -> Vec<c_ulong> {
  use x11_dl::xlib;

  super::utils::with_x11(Vec::new(), |xlib, display| unsafe {
    let (mut focus, mut revert) = (0, 0);
    let mut ancestors = Vec::new();
    (xlib.XGetInputFocus)(display, &mut focus, &mut revert);
    while focus > xlib::PointerRoot as xlib::Window {
      ancestors.push(focus);
      let (mut root, mut parent, mut count) = (0, 0, 0);
      let mut children = std::ptr::null_mut();
      let status = (xlib.XQueryTree)(
        display,
        focus,
        &mut root,
        &mut parent,
        &mut children,
        &mut count,
      );
      if !children.is_null() {
        (xlib.XFree)(children.cast());
      }
      if status == 0 || parent == focus {
        break;
      }
      focus = parent;
    }
    ancestors
  })
}

impl AppWindow {
  #[cfg(target_os = "linux")]
  pub(crate) fn set_focusable(&self, focusable: bool) {
    use gtk::prelude::*;

    self.cef_host.focusable.set(focusable);
    if self
      .window
      .gtk_window()
      .and_then(|window| window.surface())
      .is_some_and(|surface| !surface.is_destroyed())
    {
      apply_focusable(self.cef_host.gtk_display, self.xid(), focusable);
    }
  }

  #[cfg(target_os = "linux")]
  pub(crate) fn restore_webview_stacking(&self) {
    use cef::ImplBrowserHost;

    if self.children.len() < 2 {
      return;
    }
    // CEF raises the focused X11 child; keep the main renderer behind embedded views.
    super::utils::with_cef_display((), |xlib, display| unsafe {
      for child in self.children.iter().skip(1) {
        (xlib.XRaiseWindow)(display, child.host.window_handle() as c_ulong);
      }
    });
  }

  #[cfg(target_os = "linux")]
  pub(crate) fn focus_webview(&self) {
    use cef::{ImplBrowser, ImplBrowserHost};

    let ancestors = focused_window_ancestors();
    let (generation, recent) = crate::cef_impl::client::focus_snapshot();
    let focused_webview = self
      .children
      .iter()
      .find(|child| {
        ancestors.contains(&(child.host.window_handle() as c_ulong))
          && child.native_visible() == Some(true)
      })
      .map(|child| child.webview_id);
    let focused_webview = focused_webview.or_else(|| {
      recent.filter(|id| {
        self
          .children
          .iter()
          .any(|child| child.webview_id == *id && child.native_visible() == Some(true))
      })
    });

    // GTK's focus proxy does not forward keyboard events to foreign X11 children.
    if let Some(child) = self
      .children
      .iter()
      .find(|child| {
        Some(child.webview_id) == focused_webview.or(self.focused_webview)
          && child.native_visible() == Some(true)
      })
      .or_else(|| {
        self
          .children
          .iter()
          .find(|child| child.native_visible() == Some(true))
      })
    {
      let host = child.host.clone();
      let selected = Some(child.webview_id);
      let container = self.cef_host.xid;
      let window = self.window.clone();
      let focusable = self.cef_host.focusable.clone();
      // GTK presents asynchronously; restore CEF focus after its proxy takes focus.
      gtk::glib::idle_add_local_once(move || {
        let (current_generation, focused) = crate::cef_impl::client::focus_snapshot();
        if !focusable.get()
          || !window.has_focus()
          || (current_generation != generation && focused != selected)
          || !host
            .browser()
            .is_some_and(|browser| browser.is_valid() != 0)
          || super::webview::native_parent_matches(host.window_handle() as c_ulong, container)
            != Some(true)
          || focused_window_ancestors().contains(&container)
        {
          return;
        }
        let visible = super::utils::with_x11(false, |xlib, display| unsafe {
          let mut attributes = std::mem::MaybeUninit::<x11_dl::xlib::XWindowAttributes>::uninit();
          (xlib.XGetWindowAttributes)(
            display,
            host.window_handle() as c_ulong,
            attributes.as_mut_ptr(),
          ) != 0
            && attributes.assume_init().map_state == x11_dl::xlib::IsViewable
        });
        if visible {
          host.set_focus(1);
          // SetFocus can leave GTK's proxy focused when CEF already considers itself active.
          super::utils::with_cef_display((), |xlib, display| unsafe {
            (xlib.XSetInputFocus)(
              display,
              host.window_handle() as c_ulong,
              x11_dl::xlib::RevertToParent,
              x11_dl::xlib::CurrentTime,
            );
          });
        }
      });
    }
  }

  pub(crate) fn cef_host_handle(&self) -> cef::sys::cef_window_handle_t {
    self.cef_host.xid as cef::sys::cef_window_handle_t
  }

  pub(crate) fn xid(&self) -> c_ulong {
    window_xid(self.window.as_ref())
  }

  pub(crate) fn raise_native(&self) {
    super::utils::activate_window(self.xid());
  }

  /// Applies the transient parent recorded by the window builder, if any.
  pub(crate) fn apply_transient_for(&self) {
    use gtk::prelude::GtkWindowExt;

    let Some(parent) = &self.attrs.transient_for else {
      return;
    };
    if let Some(window) = self.window.gtk_window() {
      window.set_transient_for(Some(parent));
    }
  }

  /// Note that this only covers the GTK widget tree: the CEF browsers are foreign X11 children
  /// that GTK does not dispatch events for, so the web contents stay interactive.
  pub(crate) fn set_enabled(&self, enabled: bool) {
    use gtk::prelude::*;

    if let Some(window) = self.window.gtk_window() {
      window.set_sensitive(enabled);
    }
  }

  pub(crate) fn is_enabled(&self) -> bool {
    use gtk::prelude::*;

    self
      .window
      .gtk_window()
      .map(|window| window.is_sensitive())
      .unwrap_or(true)
  }

  pub(crate) fn set_background_color(&self, color: Option<Color>) {
    use gtk::prelude::*;

    let Some(window) = self.window.gtk_window() else {
      return;
    };

    let display = gtk::prelude::WidgetExt::display(&window);
    let class = self.cef_host.background_color_class();

    // GTK has no way to replace a provider, so drop the one installed by the previous call -
    // otherwise every call leaves another provider registered on the display for good.
    if let Some(previous) = self.cef_host.take_background_color_provider() {
      gtk::style_context_remove_provider_for_display(&display, &previous);
    }

    let Some(color) = color else {
      window.remove_css_class(&class);
      return;
    };

    let provider = gtk::CssProvider::new();
    let css = format!(
      ".{class} {{ background-color: rgba({}, {}, {}, {:.3}); }}",
      color.0,
      color.1,
      color.2,
      f64::from(color.3) / 255.
    );
    provider.load_from_bytes(&gtk::glib::Bytes::from_owned(css));
    gtk::style_context_add_provider_for_display(
      &display,
      &provider,
      gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    window.add_css_class(&class);
    self.cef_host.set_background_color_provider(provider);
  }

  pub(crate) fn set_skip_taskbar(&self, skip: bool) {
    use gtk::prelude::*;

    if let Some(surface) = self
      .window
      .gtk_window()
      .and_then(|window| window.surface())
      .and_then(|surface| surface.downcast::<gdkx11::X11Surface>().ok())
    {
      surface.set_skip_taskbar_hint(skip);
    } else {
      set_wm_state(self.xid(), skip, "_NET_WM_STATE_SKIP_TASKBAR", None);
    }
  }

  pub(crate) fn set_visible_on_all_workspaces(&self, visible: bool) {
    set_wm_state(self.xid(), visible, "_NET_WM_STATE_STICKY", None);
  }

  pub(crate) fn set_progress_bar(&self, state: ProgressBarState) {
    taskbar::set_progress_bar(state);
  }
}

fn window_xid(window: &dyn winit::window::Window) -> c_ulong {
  let handle = window.window_handle().expect("failed to get window handle");
  match handle.as_raw() {
    RawWindowHandle::Xlib(handle) => handle.window as c_ulong,
    RawWindowHandle::Xcb(handle) => handle.window.get() as c_ulong,
    other => panic!("expected X11 window handle, got {other:?}"),
  }
}

#[cfg(target_os = "linux")]
fn apply_focusable(
  display: std::ptr::NonNull<x11_dl::xlib::Display>,
  window: c_ulong,
  focusable: bool,
) {
  // Use GTK's connection on its thread so its queued default hint cannot overwrite ours.
  super::utils::with_x11((), |xlib, _| unsafe {
    let display = display.as_ptr();
    let pointer = (xlib.XGetWMHints)(display, window);
    let mut hints = if pointer.is_null() {
      std::mem::zeroed()
    } else {
      *pointer
    };
    if !pointer.is_null() {
      (xlib.XFree)(pointer.cast());
    }
    hints.flags |= x11_dl::xlib::InputHint;
    hints.input = i32::from(focusable);
    (xlib.XSetWMHints)(display, window, &mut hints);
    let take_focus = (xlib.XInternAtom)(display, c"WM_TAKE_FOCUS".as_ptr(), 0);
    let (mut pointer, mut count) = (std::ptr::null_mut(), 0);
    (xlib.XGetWMProtocols)(display, window, &mut pointer, &mut count);
    let mut protocols = if pointer.is_null() {
      Vec::new()
    } else {
      std::slice::from_raw_parts(pointer, count.max(0) as usize).to_vec()
    };
    if !pointer.is_null() {
      (xlib.XFree)(pointer.cast());
    }
    protocols.retain(|protocol| *protocol != take_focus);
    if focusable {
      protocols.push(take_focus);
    }
    (xlib.XSetWMProtocols)(
      display,
      window,
      protocols.as_mut_ptr(),
      protocols.len() as i32,
    );
    if !focusable {
      let user_time = (xlib.XInternAtom)(display, c"_NET_WM_USER_TIME".as_ptr(), 0);
      let timestamp: c_ulong = 0;
      (xlib.XChangeProperty)(
        display,
        window,
        user_time,
        x11_dl::xlib::XA_CARDINAL,
        32,
        x11_dl::xlib::PropModeReplace,
        (&timestamp as *const c_ulong).cast(),
        1,
      );
    }
    (xlib.XFlush)(display);
  });
}

fn create_cef_container(
  parent_xid: c_ulong,
  initial_size: PhysicalSize<u32>,
) -> Option<(c_ulong, c_ulong)> {
  use x11_dl::xlib::*;

  super::utils::with_x11(None, |xlib, display| unsafe {
    let screen = (xlib.XDefaultScreen)(display);
    let root = (xlib.XRootWindow)(display, screen);
    let mut visual_info: x11_dl::xlib::XVisualInfo = std::mem::zeroed();

    if (xlib.XMatchVisualInfo)(
      display,
      screen,
      24,
      x11_dl::xlib::TrueColor,
      &mut visual_info,
    ) == 0
    {
      return None;
    }

    let colormap = (xlib.XCreateColormap)(display, root, visual_info.visual, AllocNone);
    if colormap == 0 {
      return None;
    }

    let mut attrs: XSetWindowAttributes = std::mem::zeroed();
    attrs.event_mask = ExposureMask | StructureNotifyMask;
    attrs.colormap = colormap;
    attrs.border_pixel = 0;

    let xid = (xlib.XCreateWindow)(
      display,
      parent_xid as Window,
      0,
      0,
      initial_size.width,
      initial_size.height,
      0,
      visual_info.depth,
      InputOutput as _,
      visual_info.visual,
      CWEventMask | CWColormap | CWBorderPixel,
      &mut attrs,
    );
    if xid == 0 {
      (xlib.XFreeColormap)(display, colormap);
      return None;
    }

    (xlib.XMapWindow)(display, xid);
    Some((xid, colormap))
  })
}

/// Moves and resizes the X11 host over the GTK content area, returning its new size.
///
/// GTK4 widget geometry is in logical units while the X11 toplevel GDK creates is sized in device
/// pixels (logical * scale), so every value handed to `XMoveResizeWindow` must be scaled - without
/// it the host covers only 1/scale of the window on HiDPI screens.
fn set_cef_container_bounds(
  xid: c_ulong,
  gtk_window: &gtk::ApplicationWindow,
  webview_area: &gtk::Box,
  scale_factor: f64,
) -> PhysicalSize<u32> {
  use gtk::prelude::*;

  let width = (webview_area.width() as f64 * scale_factor).round() as u32;
  let height = (webview_area.height() as f64 * scale_factor).round() as u32;
  let point = gtk::graphene::Point::new(0.0, 0.0);
  let point = webview_area
    .compute_point(gtk_window, &point)
    .unwrap_or_else(|| gtk::graphene::Point::new(0.0, 0.0));
  let x = (point.x() as f64 * scale_factor).round() as i32;
  let y = (point.y() as f64 * scale_factor).round() as i32;

  super::utils::with_x11((), |xlib, display| unsafe {
    (xlib.XMoveResizeWindow)(display, xid as _, x, y, width, height);
  });

  PhysicalSize::new(width, height)
}
