// Copyright 2019-2024 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

//! Windowless CEF surfaces preserve the page's alpha without replacing the native window.

mod accessibility;
mod input;

use std::{
  cell::{Cell, RefCell},
  ffi::c_void,
  ptr,
  sync::Arc,
};

use cef::*;
use dispatch2::{MainThreadBound, run_on_main};
use objc2::runtime::{AnyObject, Sel};
use objc2::{
  AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
};
use objc2_app_kit::{
  NSAutoresizingMaskOptions, NSEvent, NSScreen, NSTextInputClient, NSTrackingArea,
  NSTrackingAreaOptions, NSView,
};
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{
  CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
  CGImageByteOrderInfo,
};
use objc2_foundation::{
  NSArray, NSAttributedString, NSAttributedStringKey, NSInteger, NSPoint, NSRange, NSRangePointer,
  NSRect, NSSize, NSString, NSUInteger,
};
use objc2_quartz_core::{CALayer, CATransaction};

struct SurfaceIvars {
  browser: RefCell<Option<Browser>>,
  closed: Cell<bool>,
  input: RefCell<input::InputState>,
  popup_layer: Retained<CALayer>,
  accessibility: RefCell<accessibility::State>,
}

define_class!(
  #[unsafe(super(NSView))]
  #[name = "JarvisCefOffscreenView"]
  #[ivars = SurfaceIvars]
  struct SurfaceView;

  impl SurfaceView {
    #[unsafe(method(isOpaque))]
    fn is_opaque(&self) -> bool { false }
    #[unsafe(method(isFlipped))]
    fn is_flipped(&self) -> bool { true }
    #[unsafe(method(acceptsFirstResponder))]
    fn accepts_first_responder(&self) -> bool { true }
    #[unsafe(method(acceptsFirstMouse:))]
    fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool { true }
    #[unsafe(method(isAccessibilityElement))]
    fn is_accessibility_element(&self) -> bool { true }
    #[unsafe(method_id(accessibilityRole))]
    fn accessibility_role(&self) -> Retained<NSString> { NSString::from_str("AXGroup") }
    #[unsafe(method_id(accessibilityLabel))]
    fn accessibility_label(&self) -> Retained<NSString> { NSString::from_str("Jarvis") }
    #[unsafe(method_id(accessibilityChildren))]
    fn accessibility_children(&self) -> Retained<NSArray<AnyObject>> { accessibility::children(self) }
    #[unsafe(method_id(accessibilityFocusedUIElement))]
    fn accessibility_focused_ui_element(&self) -> Option<Retained<AnyObject>> { accessibility::focused(self) }
    #[unsafe(method_id(accessibilityHitTest:))]
    fn accessibility_hit_test(&self, point: NSPoint) -> Option<Retained<AnyObject>> { accessibility::hit_test(self, point) }
    #[unsafe(method(becomeFirstResponder))]
    fn become_first_responder(&self) -> bool { self.set_focus(true); true }
    #[unsafe(method(resignFirstResponder))]
    fn resign_first_responder(&self) -> bool { self.set_focus(false); true }
    #[unsafe(method(viewDidChangeBackingProperties))]
    fn backing_changed(&self) { self.screen_changed(); }
    #[unsafe(method(viewDidMoveToWindow))]
    fn moved_to_window(&self) { self.screen_changed(); }
    #[unsafe(method(setFrameSize:))]
    fn set_frame_size(&self, size: NSSize) {
      // The superclass updates its backing layer before CEF asks for the next view rectangle.
      unsafe { let _: () = msg_send![super(self), setFrameSize: size]; }
      if let Some(host) = self.host() { host.was_resized(); }
    }
    #[unsafe(method(setHidden:))]
    fn set_hidden(&self, hidden: bool) {
      unsafe { let _: () = msg_send![super(self), setHidden: hidden]; }
      if let Some(host) = self.host() { host.was_hidden(i32::from(hidden)); }
    }
    #[unsafe(method(mouseDown:))]
    fn mouse_down(&self, event: &NSEvent) { self.mouse_click(event, MouseButtonType::LEFT, false); }
    #[unsafe(method(mouseUp:))]
    fn mouse_up(&self, event: &NSEvent) { self.mouse_click(event, MouseButtonType::LEFT, true); }
    #[unsafe(method(rightMouseDown:))]
    fn right_mouse_down(&self, event: &NSEvent) { self.mouse_click(event, MouseButtonType::RIGHT, false); }
    #[unsafe(method(rightMouseUp:))]
    fn right_mouse_up(&self, event: &NSEvent) { self.mouse_click(event, MouseButtonType::RIGHT, true); }
    #[unsafe(method(otherMouseDown:))]
    fn other_mouse_down(&self, event: &NSEvent) { self.mouse_click(event, MouseButtonType::MIDDLE, false); }
    #[unsafe(method(otherMouseUp:))]
    fn other_mouse_up(&self, event: &NSEvent) { self.mouse_click(event, MouseButtonType::MIDDLE, true); }
    #[unsafe(method(mouseMoved:))]
    fn mouse_moved(&self, event: &NSEvent) { self.mouse_move(event, false); }
    #[unsafe(method(mouseEntered:))]
    fn mouse_entered(&self, event: &NSEvent) { self.mouse_move(event, false); }
    #[unsafe(method(mouseExited:))]
    fn mouse_exited(&self, event: &NSEvent) { self.mouse_move(event, true); }
    #[unsafe(method(mouseDragged:))]
    fn mouse_dragged(&self, event: &NSEvent) { self.mouse_move(event, false); }
    #[unsafe(method(rightMouseDragged:))]
    fn right_mouse_dragged(&self, event: &NSEvent) { self.mouse_move(event, false); }
    #[unsafe(method(otherMouseDragged:))]
    fn other_mouse_dragged(&self, event: &NSEvent) { self.mouse_move(event, false); }
    #[unsafe(method(scrollWheel:))]
    fn scroll_wheel(&self, event: &NSEvent) { self.mouse_wheel(event); }
    #[unsafe(method(keyDown:))]
    fn key_down(&self, event: &NSEvent) { self.handle_key_down(event); }
    #[unsafe(method(keyUp:))]
    fn key_up(&self, event: &NSEvent) { self.send_key(event, KeyEventType::KEYUP); }
    #[unsafe(method(flagsChanged:))]
    fn flags_changed(&self, event: &NSEvent) { self.send_modifier_change(event); }
    #[unsafe(method(performKeyEquivalent:))]
    fn perform_key_equivalent(&self, event: &NSEvent) -> bool { self.handle_key_equivalent(event) }
  }

  // objc2's protocol requires the Objective-C method names.
  #[allow(non_snake_case)]
  unsafe impl NSTextInputClient for SurfaceView {
    #[unsafe(method(insertText:replacementRange:))]
    unsafe fn insertText_replacementRange(&self, text: &AnyObject, range: NSRange) {
      self.insert_text(text, range);
    }
    #[unsafe(method(doCommandBySelector:))]
    unsafe fn doCommandBySelector(&self, _selector: Sel) {}
    #[unsafe(method(setMarkedText:selectedRange:replacementRange:))]
    unsafe fn setMarkedText_selectedRange_replacementRange(&self, text: &AnyObject, selected: NSRange, replacement: NSRange) {
      self.set_marked_text(text, selected, replacement);
    }
    #[unsafe(method(unmarkText))]
    fn unmarkText(&self) { self.unmark_text(); }
    #[unsafe(method(selectedRange))]
    fn selectedRange(&self) -> NSRange { self.ivars().input.borrow().selected }
    #[unsafe(method(markedRange))]
    fn markedRange(&self) -> NSRange { self.ivars().input.borrow().marked_range }
    #[unsafe(method(hasMarkedText))]
    fn hasMarkedText(&self) -> bool { !self.ivars().input.borrow().marked.is_empty() }
    #[unsafe(method_id(attributedSubstringForProposedRange:actualRange:))]
    unsafe fn attributedSubstringForProposedRange_actualRange(&self, range: NSRange, actual: NSRangePointer) -> Option<Retained<NSAttributedString>> {
      self.attributed_substring(range, actual)
    }
    #[unsafe(method_id(validAttributesForMarkedText))]
    fn validAttributesForMarkedText(&self) -> Retained<NSArray<NSAttributedStringKey>> { NSArray::new() }
    #[unsafe(method(firstRectForCharacterRange:actualRange:))]
    unsafe fn firstRectForCharacterRange_actualRange(&self, range: NSRange, actual: NSRangePointer) -> NSRect {
      self.character_rect(range, actual)
    }
    #[unsafe(method(characterIndexForPoint:))]
    fn characterIndexForPoint(&self, _point: NSPoint) -> NSUInteger { isize::MAX as usize }
    #[unsafe(method(windowLevel))]
    fn windowLevel(&self) -> NSInteger { self.window().map(|window| window.level()).unwrap_or(0) }
  }
);

impl SurfaceView {
  fn owns_browser(&self, browser: Option<&Browser>) -> bool {
    owns_browser_id(
      self
        .ivars()
        .browser
        .borrow()
        .as_ref()
        .map(ImplBrowser::identifier),
      browser.map(ImplBrowser::identifier),
      self.ivars().closed.get(),
    )
  }

  fn host(&self) -> Option<BrowserHost> {
    self
      .ivars()
      .browser
      .borrow()
      .as_ref()
      .and_then(ImplBrowser::host)
  }

  fn set_focus(&self, focused: bool) {
    if let Some(host) = self.host() {
      host.set_focus(i32::from(focused));
    }
  }

  fn screen_changed(&self) {
    let scale = self
      .window()
      .map(|window| window.backingScaleFactor())
      .unwrap_or(1.0);
    if let Some(layer) = self.layer() {
      layer.setContentsScale(scale);
    }
    self.ivars().popup_layer.setContentsScale(scale);
    if let Some(host) = self.host() {
      host.notify_screen_info_changed();
      host.was_resized();
    }
  }

  fn screen_point(&self, point: NSPoint) -> Option<NSPoint> {
    let window = self.window()?;
    let point = self.convertPoint_toView(point, None);
    Some(window.convertPointToScreen(point))
  }
}

#[derive(Clone)]
pub(crate) struct OffscreenView {
  view: Arc<MainThreadBound<Retained<SurfaceView>>>,
}

impl OffscreenView {
  pub(crate) fn new(parent: cef::sys::cef_window_handle_t, bounds: cef::Rect) -> Self {
    let mtm = MainThreadMarker::new().expect("CEF creates its macOS views on the main thread");
    // The runtime retains the parent window throughout browser creation.
    let parent = unsafe { &*parent.cast::<NSView>() };
    let size = NSSize::new(
      f64::from(bounds.width.max(1)),
      f64::from(bounds.height.max(1)),
    );
    let top = if parent.isFlipped() {
      f64::from(bounds.y)
    } else {
      parent.bounds().size.height - f64::from(bounds.y) - size.height
    };
    let frame = NSRect::new(NSPoint::new(f64::from(bounds.x), top), size);
    let popup_layer = CALayer::new();
    popup_layer.setOpaque(false);
    popup_layer.setHidden(true);
    let allocated = SurfaceView::alloc(mtm).set_ivars(SurfaceIvars {
      browser: RefCell::new(None),
      closed: Cell::new(false),
      input: RefCell::new(input::InputState::default()),
      popup_layer,
      accessibility: RefCell::new(accessibility::State::default()),
    });
    let view: Retained<SurfaceView> = unsafe { msg_send![super(allocated), initWithFrame: frame] };
    view.setWantsLayer(true);
    if let Some(layer) = view.layer() {
      layer.setOpaque(false);
      layer.setGeometryFlipped(true);
      layer.addSublayer(&view.ivars().popup_layer);
    }
    view.setAutoresizingMask(
      NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    let tracking = unsafe {
      NSTrackingArea::initWithRect_options_owner_userInfo(
        NSTrackingArea::alloc(),
        view.bounds(),
        NSTrackingAreaOptions::MouseEnteredAndExited
          | NSTrackingAreaOptions::MouseMoved
          | NSTrackingAreaOptions::ActiveAlways
          | NSTrackingAreaOptions::InVisibleRect
          | NSTrackingAreaOptions::EnabledDuringMouseDrag,
        Some(view.as_ref()),
        None,
      )
    };
    view.addTrackingArea(&tracking);
    parent.addSubview(&view);
    Self {
      view: Arc::new(MainThreadBound::new(view, mtm)),
    }
  }

  fn surface(&self) -> &SurfaceView {
    self
      .view
      .get(MainThreadMarker::new().expect("CEF accesses macOS surfaces on its UI thread"))
  }

  pub(crate) fn nsview(&self) -> Retained<NSView> {
    let view = self
      .view
      .get(MainThreadMarker::new().expect("native views require the main thread"));
    Retained::into_super(view.clone())
  }

  pub(crate) fn render_handler(&self) -> RenderHandler {
    OffscreenRenderHandler::new(self.clone())
  }

  pub(crate) fn set_browser(&self, browser: &Browser) {
    if !self.surface().owns_browser(Some(browser)) {
      return;
    }
    *self.surface().ivars().browser.borrow_mut() = Some(browser.clone());
    if let Some(host) = browser.host() {
      host.set_accessibility_state(cef::State::ENABLED);
    }
    self.surface().screen_changed();
  }

  pub(crate) fn browser_closed(&self) {
    run_on_main(|mtm| {
      let view = self.view.get(mtm);
      view.ivars().closed.set(true);
      view.ivars().browser.borrow_mut().take();
      *view.ivars().accessibility.borrow_mut() = accessibility::State::default();
      if let Some(layer) = view.layer() {
        unsafe {
          layer.setContents(None);
        }
      }
      unsafe {
        view.ivars().popup_layer.setContents(None);
      }
      view.removeFromSuperview();
    });
  }
}

wrap_render_handler! {
  struct OffscreenRenderHandler { view: OffscreenView }

  impl RenderHandler {
    fn accessibility_handler(&self) -> Option<AccessibilityHandler> {
      if self.view.surface().ivars().closed.get() { return None; }
      Some(accessibility::handler(self.view.clone()))
    }
    fn root_screen_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) -> i32 {
      if let (Some(window), Some(rect), Some(primary_top)) = (self.view.surface().window(), rect, primary_screen_top()) {
        *rect = cef_screen_rect(window.frame(), primary_top);
        return 1;
      }
      0
    }

    fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
      if let Some(rect) = rect {
        let size = self.view.surface().bounds().size;
        *rect = Rect { x: 0, y: 0, width: size.width.round().max(1.0) as i32, height: size.height.round().max(1.0) as i32 };
      }
    }

    fn screen_point(&self, _browser: Option<&mut Browser>, x: i32, y: i32, screen_x: Option<&mut i32>, screen_y: Option<&mut i32>) -> i32 {
      let Some(point) = self.view.surface().screen_point(NSPoint::new(f64::from(x), f64::from(y))) else { return 0; };
      if let (Some(screen_x), Some(screen_y)) = (screen_x, screen_y) {
        *screen_x = point.x.round() as i32;
        *screen_y = point.y.round() as i32;
        return 1;
      }
      0
    }

    fn screen_info(&self, _browser: Option<&mut Browser>, info: Option<&mut ScreenInfo>) -> i32 {
      let Some(info) = info else { return 0; };
      let view = self.view.surface();
      let mtm = MainThreadMarker::new().expect("CEF screen callbacks require the main thread");
      let screen = view.window().and_then(|window| window.screen()).or_else(|| NSScreen::mainScreen(mtm));
      let Some(screen) = screen else { return 0; };
      let Some(primary_top) = primary_screen_top() else { return 0; };
      info.device_scale_factor = screen.backingScaleFactor() as f32;
      info.depth = 32;
      info.depth_per_component = 8;
      info.is_monochrome = 0;
      info.rect = cef_screen_rect(screen.frame(), primary_top);
      info.available_rect = cef_screen_rect(screen.visibleFrame(), primary_top);
      1
    }

    fn on_popup_show(&self, browser: Option<&mut Browser>, show: i32) {
      if !self.view.surface().owns_browser(browser.as_deref()) { return; }
      let layer = &self.view.surface().ivars().popup_layer;
      CATransaction::begin(); CATransaction::setDisableActions(true);
      layer.setHidden(show == 0);
      if show == 0 { unsafe { layer.setContents(None); } }
      CATransaction::commit();
    }

    fn on_popup_size(&self, browser: Option<&mut Browser>, rect: Option<&Rect>) {
      if !self.view.surface().owns_browser(browser.as_deref()) { return; }
      if let Some(rect) = rect {
        CATransaction::begin(); CATransaction::setDisableActions(true);
        self.view.surface().ivars().popup_layer.setFrame(NSRect::new(
          NSPoint::new(f64::from(rect.x), f64::from(rect.y)),
          NSSize::new(f64::from(rect.width), f64::from(rect.height)),
        ));
        CATransaction::commit();
      }
    }

    fn on_paint(&self, browser: Option<&mut Browser>, kind: PaintElementType, _dirty: Option<&[Rect]>, buffer: *const u8, width: i32, height: i32) {
      if !self.view.surface().owns_browser(browser.as_deref()) { return; }
      let Some(length) = paint_length(width, height) else { return; };
      if buffer.is_null() { return; }
      // CEF owns this buffer only until the callback returns; the provider owns our copy.
      let pixels = unsafe { std::slice::from_raw_parts(buffer, length) }.to_vec();
      let Some(image) = image_from_bgra(pixels, width as usize, height as usize) else { return; };
      let view = self.view.surface();
      let layer = if kind == PaintElementType::POPUP { Some(view.ivars().popup_layer.clone()) } else { view.layer() };
      let Some(layer) = layer else { return; };
      CATransaction::begin(); CATransaction::setDisableActions(true);
      // CGImage is a supported, retained CALayer contents object.
      unsafe { layer.setContents(Some(&*(image.as_ref() as *const CGImage).cast::<AnyObject>())); }
      CATransaction::commit();
    }

    fn on_ime_composition_range_changed(&self, browser: Option<&mut Browser>, range: Option<&Range>, bounds: Option<&[Rect]>) {
      if !self.view.surface().owns_browser(browser.as_deref()) { return; }
      if let Some(range) = range {
        let mut input = self.view.surface().ivars().input.borrow_mut();
        input.marked_range = NSRange::new(range.from as usize, range.to.saturating_sub(range.from) as usize);
        input.character_bounds = bounds.unwrap_or_default().to_vec();
      }
    }

    fn on_text_selection_changed(&self, browser: Option<&mut Browser>, text: Option<&CefString>, range: Option<&Range>) {
      if !self.view.surface().owns_browser(browser.as_deref()) { return; }
      let mut input = self.view.surface().ivars().input.borrow_mut();
      input.selected_text = text.map(ToString::to_string).unwrap_or_default();
      if let Some(range) = range { input.selected = NSRange::new(range.from as usize, range.to.saturating_sub(range.from) as usize); }
    }
  }
}

fn owns_browser_id(owned: Option<i32>, incoming: Option<i32>, closed: bool) -> bool {
  // Creation may paint before create_browser_sync returns and binds the root ID.
  // After binding, popup/DevTools callbacks must never mutate this surface.
  !closed && incoming.is_some_and(|incoming| owned.is_none_or(|owned| owned == incoming))
}

fn primary_screen_top() -> Option<f64> {
  let mtm = MainThreadMarker::new().expect("CEF screen callbacks require the main thread");
  let frame = NSScreen::screens(mtm).firstObject()?.frame();
  Some(frame.origin.y + frame.size.height)
}

fn cef_screen_rect(rect: NSRect, primary_top: f64) -> Rect {
  // Chromium's screen DIP origin is the top-left of the primary display.
  Rect {
    x: rect.origin.x.round() as i32,
    y: (primary_top - rect.origin.y - rect.size.height).round() as i32,
    width: rect.size.width.round() as i32,
    height: rect.size.height.round() as i32,
  }
}

fn paint_length(width: i32, height: i32) -> Option<usize> {
  if width <= 0 || height <= 0 {
    return None;
  }
  (width as usize)
    .checked_mul(height as usize)?
    .checked_mul(4)
    .filter(|length| *length <= isize::MAX as usize)
}

fn image_from_bgra(pixels: Vec<u8>, width: usize, height: usize) -> Option<CFRetained<CGImage>> {
  unsafe extern "C-unwind" fn release(
    info: *mut c_void,
    _data: ptr::NonNull<c_void>,
    _size: usize,
  ) {
    // Only the provider receives this Box, and it calls release once when its image retires.
    unsafe {
      drop(Box::from_raw(info.cast::<Vec<u8>>()));
    }
  }
  let pixels = Box::new(pixels);
  let data = pixels.as_ptr();
  let length = pixels.len();
  let info = Box::into_raw(pixels).cast::<c_void>();
  let provider = unsafe { CGDataProvider::with_data(info, data.cast(), length, Some(release)) };
  let Some(provider) = provider else {
    unsafe {
      drop(Box::from_raw(info.cast::<Vec<u8>>()));
    }
    return None;
  };
  let color_space = CGColorSpace::new_device_rgb()?;
  let bitmap =
    CGBitmapInfo(CGImageAlphaInfo::PremultipliedFirst.0 | CGImageByteOrderInfo::Order32Little.0);
  unsafe {
    CGImage::new(
      width,
      height,
      8,
      32,
      width * 4,
      Some(&color_space),
      bitmap,
      Some(&provider),
      ptr::null(),
      false,
      CGColorRenderingIntent::RenderingIntentDefault,
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn screen_rectangles_use_primary_display_top_left_across_monitors() {
    for (origin, size, expected) in [
      ((40.0, 120.0), (320.0, 200.0), (40, 760, 320, 200)),
      ((-1280.0, 1080.0), (1280.0, 720.0), (-1280, -720, 1280, 720)),
      ((1920.0, -900.0), (1440.0, 900.0), (1920, 1080, 1440, 900)),
    ] {
      let rect = cef_screen_rect(
        NSRect::new(NSPoint::new(origin.0, origin.1), NSSize::new(size.0, size.1)),
        1080.0,
      );
      assert_eq!((rect.x, rect.y, rect.width, rect.height), expected);
    }
  }

  #[test]
  fn isolates_browser_callbacks_after_binding_and_never_revives_a_closed_surface() {
    assert!(owns_browser_id(None, Some(7), false));
    assert!(owns_browser_id(Some(7), Some(7), false));
    assert!(!owns_browser_id(Some(7), Some(9), false));
    assert!(!owns_browser_id(Some(7), None, false));
    assert!(!owns_browser_id(None, Some(7), true));
    assert!(!owns_browser_id(Some(7), Some(7), true));
  }

  #[test]
  fn validates_paint_dimensions_without_losing_transparent_pixel_bytes() {
    assert_eq!(paint_length(2, 1), Some(8));
    assert_eq!(paint_length(0, 1), None);
    assert_eq!(paint_length(-1, 2), None);
    assert_eq!(paint_length(i32::MAX, i32::MAX), None);
    let image = image_from_bgra(vec![0, 0, 0, 0, 16, 32, 64, 128], 2, 1).unwrap();
    assert_eq!(
      CGImage::alpha_info(Some(&image)),
      CGImageAlphaInfo::PremultipliedFirst
    );
    assert_eq!(
      CGImage::byte_order_info(Some(&image)),
      CGImageByteOrderInfo::Order32Little
    );
    assert_eq!(CGImage::width(Some(&image)), 2);
  }
}
