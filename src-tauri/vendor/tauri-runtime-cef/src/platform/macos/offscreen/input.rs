use cef::{CefString, ImplBrowserHost, KeyEvent, KeyEventType, MouseButtonType, MouseEvent, Range};
use objc2::{AnyThread, DefinedClass, rc::Retained, runtime::AnyObject};
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
use objc2_foundation::{
  NSArray, NSAttributedString, NSPoint, NSRange, NSRangePointer, NSRect, NSSize, NSString,
};

use super::SurfaceView;

const NOT_FOUND: usize = isize::MAX as usize;

pub(super) struct InputState {
  pub(super) selected: NSRange,
  pub(super) marked_range: NSRange,
  pub(super) marked: String,
  pub(super) selected_text: String,
  pub(super) character_bounds: Vec<cef::Rect>,
  replacement: NSRange,
  handling_key: bool,
  inserted: String,
  unmarked: bool,
}

impl Default for InputState {
  fn default() -> Self {
    Self {
      selected: NSRange::new(0, 0),
      marked_range: NSRange::new(NOT_FOUND, 0),
      marked: String::new(),
      selected_text: String::new(),
      character_bounds: Vec::new(),
      replacement: NSRange::new(NOT_FOUND, 0),
      handling_key: false,
      inserted: String::new(),
      unmarked: false,
    }
  }
}

impl SurfaceView {
  fn mouse_event(&self, event: &NSEvent) -> MouseEvent {
    let point = self.convertPoint_fromView(event.locationInWindow(), None);
    MouseEvent {
      x: point.x.round() as i32,
      y: point.y.round() as i32,
      modifiers: modifiers(event),
    }
  }

  pub(super) fn mouse_click(&self, event: &NSEvent, button: MouseButtonType, up: bool) {
    if let Some(host) = self.host() {
      if !up && let Some(window) = self.window() {
        window.makeFirstResponder(Some(self));
      }
      host.send_mouse_click_event(
        Some(&self.mouse_event(event)),
        button,
        i32::from(up),
        event.clickCount().max(1) as i32,
      );
    }
  }

  pub(super) fn mouse_move(&self, event: &NSEvent, leave: bool) {
    if let Some(host) = self.host() {
      host.send_mouse_move_event(Some(&self.mouse_event(event)), i32::from(leave));
    }
  }

  pub(super) fn mouse_wheel(&self, event: &NSEvent) {
    if let Some(host) = self.host() {
      let multiplier = if event.hasPreciseScrollingDeltas() {
        1.0
      } else {
        40.0
      };
      host.send_mouse_wheel_event(
        Some(&self.mouse_event(event)),
        (event.scrollingDeltaX() * multiplier).round() as i32,
        (event.scrollingDeltaY() * multiplier).round() as i32,
      );
    }
  }

  pub(super) fn send_key(&self, event: &NSEvent, kind: KeyEventType) {
    if let Some(host) = self.host() {
      host.send_key_event(Some(&key_event(event, kind)));
    }
  }

  pub(super) fn send_modifier_change(&self, event: &NSEvent) {
    let flag = match event.keyCode() {
      54 | 55 => NSEventModifierFlags::Command,
      56 | 60 => NSEventModifierFlags::Shift,
      58 | 61 => NSEventModifierFlags::Option,
      59 | 62 => NSEventModifierFlags::Control,
      57 => NSEventModifierFlags::CapsLock,
      _ => return,
    };
    self.send_key(
      event,
      if event.modifierFlags().contains(flag) {
        KeyEventType::KEYDOWN
      } else {
        KeyEventType::KEYUP
      },
    );
  }

  pub(super) fn handle_key_equivalent(&self, event: &NSEvent) -> bool {
    if event
      .modifierFlags()
      .contains(NSEventModifierFlags::Command)
      && matches!(
        windows_key_code(event.keyCode()),
        65 | 67 | 86 | 88 | 89 | 90
      )
    {
      self.handle_key_down(event);
      return true;
    }
    false
  }

  pub(super) fn handle_key_down(&self, event: &NSEvent) {
    let Some(host) = self.host() else {
      return;
    };
    let old_marked = {
      let mut state = self.ivars().input.borrow_mut();
      let marked = !state.marked.is_empty();
      state.handling_key = true;
      state.inserted.clear();
      state.unmarked = false;
      state.replacement = NSRange::new(NOT_FOUND, 0);
      marked
    };
    // AppKit supplies dead keys, accent composition and input-method commands through this protocol.
    self.interpretKeyEvents(&NSArray::from_slice(&[event]));
    let (inserted, marked, selected, replacement, unmarked) = {
      let mut state = self.ivars().input.borrow_mut();
      state.handling_key = false;
      (
        std::mem::take(&mut state.inserted),
        state.marked.clone(),
        state.selected,
        state.replacement,
        state.unmarked,
      )
    };
    if marked.is_empty() && !old_marked && inserted.encode_utf16().count() <= 1 {
      let mut key = key_event(event, KeyEventType::KEYDOWN);
      if let Some(character) = inserted.encode_utf16().next() {
        key.character = character;
      }
      host.send_key_event(Some(&key));
      // AppKit's numeric-keypad Clear is an editing command, never text input.
      if event.keyCode() != 71 {
        key.type_ = KeyEventType::CHAR;
        host.send_key_event(Some(&key));
      }
    } else if !inserted.is_empty() {
      host.ime_commit_text(
        Some(&CefString::from(inserted.as_str())),
        Some(&cef_range(replacement)),
        0,
      );
    }
    if !marked.is_empty() {
      host.ime_set_composition(
        Some(&CefString::from(marked.as_str())),
        None,
        Some(&cef_range(replacement)),
        Some(&cef_range(selected)),
      );
    } else if old_marked && inserted.is_empty() {
      if unmarked {
        host.ime_finish_composing_text(0);
      } else {
        host.ime_cancel_composition();
      }
    }
  }

  pub(super) fn insert_text(&self, object: &AnyObject, replacement: NSRange) {
    let Some(text) = plain_text(object) else {
      return;
    };
    let handling = {
      let mut state = self.ivars().input.borrow_mut();
      state.marked.clear();
      state.marked_range = NSRange::new(NOT_FOUND, 0);
      state.replacement = replacement;
      if state.handling_key {
        state.inserted.push_str(&text);
      }
      state.handling_key
    };
    if !handling && let Some(host) = self.host() {
      host.ime_commit_text(
        Some(&CefString::from(text.as_str())),
        Some(&cef_range(replacement)),
        0,
      );
    }
  }

  pub(super) fn set_marked_text(
    &self,
    object: &AnyObject,
    selected: NSRange,
    replacement: NSRange,
  ) {
    let Some(text) = plain_text(object) else {
      return;
    };
    let handling = {
      let mut state = self.ivars().input.borrow_mut();
      state.marked = text.clone();
      state.selected = selected;
      state.replacement = replacement;
      state.marked_range = if text.is_empty() {
        NSRange::new(NOT_FOUND, 0)
      } else {
        NSRange::new(0, text.encode_utf16().count())
      };
      state.handling_key
    };
    if !handling && let Some(host) = self.host() {
      host.ime_set_composition(
        Some(&CefString::from(text.as_str())),
        None,
        Some(&cef_range(replacement)),
        Some(&cef_range(selected)),
      );
    }
  }

  pub(super) fn unmark_text(&self) {
    let handling = {
      let mut state = self.ivars().input.borrow_mut();
      state.marked.clear();
      state.marked_range = NSRange::new(NOT_FOUND, 0);
      state.unmarked = true;
      state.handling_key
    };
    if !handling && let Some(host) = self.host() {
      host.ime_finish_composing_text(0);
    }
  }

  pub(super) fn attributed_substring(
    &self,
    range: NSRange,
    actual: NSRangePointer,
  ) -> Option<Retained<NSAttributedString>> {
    let state = self.ivars().input.borrow();
    let origin = state.selected.location;
    let start = range.location.checked_sub(origin)?;
    let text: Vec<_> = state.selected_text.encode_utf16().collect();
    let end = start.checked_add(range.length)?.min(text.len());
    if start >= end {
      return None;
    }
    if !actual.is_null() {
      unsafe {
        *actual = NSRange::new(range.location, end - start);
      }
    }
    let text = NSString::from_str(&String::from_utf16_lossy(&text[start..end]));
    Some(NSAttributedString::initWithString(
      NSAttributedString::alloc(),
      &text,
    ))
  }

  pub(super) fn character_rect(&self, range: NSRange, actual: NSRangePointer) -> NSRect {
    let state = self.ivars().input.borrow();
    let offset = range.location.saturating_sub(state.marked_range.location);
    let bounds = state
      .character_bounds
      .get(offset)
      .or_else(|| state.character_bounds.last());
    let rect = bounds
      .map(|rect| {
        NSRect::new(
          NSPoint::new(f64::from(rect.x), f64::from(rect.y)),
          NSSize::new(f64::from(rect.width), f64::from(rect.height)),
        )
      })
      .unwrap_or_else(|| NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1.0, 1.0)));
    if !actual.is_null() {
      unsafe {
        *actual = range;
      }
    }
    if let Some(window) = self.window() {
      window.convertRectToScreen(self.convertRect_toView(rect, None))
    } else {
      rect
    }
  }
}

fn plain_text(object: &AnyObject) -> Option<String> {
  if let Some(text) = object.downcast_ref::<NSString>() {
    Some(text.to_string())
  } else {
    object
      .downcast_ref::<NSAttributedString>()
      .map(|text| text.string().to_string())
  }
}

fn cef_range(range: NSRange) -> Range {
  if range.location == NOT_FOUND {
    return Range {
      from: u32::MAX,
      to: u32::MAX,
    };
  }
  Range {
    from: range.location.min(u32::MAX as usize) as u32,
    to: range
      .location
      .saturating_add(range.length)
      .min(u32::MAX as usize) as u32,
  }
}

fn modifiers(event: &NSEvent) -> u32 {
  use cef::sys::cef_event_flags_t as CefFlags;
  let native = event.modifierFlags();
  let mut value = 0;
  for (flag, cef) in [
    (
      NSEventModifierFlags::CapsLock,
      CefFlags::EVENTFLAG_CAPS_LOCK_ON,
    ),
    (NSEventModifierFlags::Shift, CefFlags::EVENTFLAG_SHIFT_DOWN),
    (
      NSEventModifierFlags::Control,
      CefFlags::EVENTFLAG_CONTROL_DOWN,
    ),
    (NSEventModifierFlags::Option, CefFlags::EVENTFLAG_ALT_DOWN),
    (
      NSEventModifierFlags::Command,
      CefFlags::EVENTFLAG_COMMAND_DOWN,
    ),
    (
      NSEventModifierFlags::NumericPad,
      CefFlags::EVENTFLAG_IS_KEY_PAD,
    ),
  ] {
    if native.contains(flag) {
      value |= cef.0 as u32;
    }
  }
  let buttons = NSEvent::pressedMouseButtons();
  for (mask, cef) in [
    (1, CefFlags::EVENTFLAG_LEFT_MOUSE_BUTTON),
    (2, CefFlags::EVENTFLAG_RIGHT_MOUSE_BUTTON),
    (4, CefFlags::EVENTFLAG_MIDDLE_MOUSE_BUTTON),
  ] {
    if buttons & mask != 0 {
      value |= cef.0 as u32;
    }
  }
  if event.r#type() == NSEventType::KeyDown && event.isARepeat() {
    value |= CefFlags::EVENTFLAG_IS_REPEAT.0 as u32;
  }
  value
}

fn key_event(event: &NSEvent, kind: KeyEventType) -> KeyEvent {
  let first = |text: Option<Retained<NSString>>| {
    text
      .and_then(|text| text.to_string().encode_utf16().next())
      .unwrap_or(0)
  };
  let normalize = |character| match character {
    0x7f => 8,
    0x0a => 13,
    _ => character,
  };
  let (character, unmodified) = if event.r#type() == NSEventType::FlagsChanged {
    (0, 0)
  } else {
    (
      first(event.characters()),
      first(event.charactersIgnoringModifiers()),
    )
  };
  KeyEvent {
    type_: kind,
    modifiers: modifiers(event),
    windows_key_code: windows_key_code(event.keyCode()),
    native_key_code: i32::from(event.keyCode()),
    character: normalize(character),
    unmodified_character: normalize(unmodified),
    ..Default::default()
  }
}

fn windows_key_code(code: u16) -> i32 {
  match code {
    0 => 65,
    1 => 83,
    2 => 68,
    3 => 70,
    4 => 72,
    5 => 71,
    6 => 90,
    7 => 88,
    8 => 67,
    9 => 86,
    11 => 66,
    12 => 81,
    13 => 87,
    14 => 69,
    15 => 82,
    16 => 89,
    17 => 84,
    18 => 49,
    19 => 50,
    20 => 51,
    21 => 52,
    22 => 54,
    23 => 53,
    24 => 187,
    25 => 57,
    26 => 55,
    27 => 189,
    28 => 56,
    29 => 48,
    30 => 221,
    31 => 79,
    32 => 85,
    33 => 219,
    34 => 73,
    35 => 80,
    36 | 76 => 13,
    37 => 76,
    38 => 74,
    39 => 222,
    40 => 75,
    41 => 186,
    42 => 220,
    43 => 188,
    44 => 191,
    45 => 78,
    46 => 77,
    47 => 190,
    48 => 9,
    49 => 32,
    50 => 192,
    51 => 8,
    53 => 27,
    54 | 55 => 91,
    56 | 60 => 16,
    57 => 20,
    58 | 61 => 18,
    59 | 62 => 17,
    65 => 110,
    67 => 106,
    69 => 107,
    71 => 12,
    75 => 111,
    78 => 109,
    81 => 187,
    82 => 96,
    83 => 97,
    84 => 98,
    85 => 99,
    86 => 100,
    87 => 101,
    88 => 102,
    89 => 103,
    91 => 104,
    92 => 105,
    96 => 116,
    97 => 117,
    98 => 118,
    99 => 114,
    100 => 119,
    101 => 120,
    103 => 122,
    105 => 124,
    106 => 127,
    107 => 125,
    109 => 121,
    111 => 123,
    113 => 126,
    114 => 45,
    115 => 36,
    116 => 33,
    117 => 46,
    118 => 115,
    119 => 35,
    120 => 113,
    121 => 34,
    122 => 112,
    123 => 37,
    124 => 39,
    125 => 40,
    126 => 38,
    _ => 0,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn keeps_native_editing_keys_and_ime_ranges_distinct() {
    assert_eq!(windows_key_code(51), 8);
    assert_eq!(windows_key_code(117), 46);
    assert_eq!(windows_key_code(123), 37);
    assert_eq!(windows_key_code(8), 67);
    let absent = cef_range(NSRange::new(NOT_FOUND, 0));
    assert_eq!((absent.from, absent.to), (u32::MAX, u32::MAX));
    let selected = cef_range(NSRange::new(4, 2));
    assert_eq!((selected.from, selected.to), (4, 6));
  }
}
