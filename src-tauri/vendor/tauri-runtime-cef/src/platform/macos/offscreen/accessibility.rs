//! CEF's windowless mode supplies tree data, so the native surface owns its VoiceOver nodes.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Weak},
};

use cef::rc::Rc as _;
use cef::{
    AccessibilityHandler, CefString, ImplAccessibilityHandler, ImplBrowser, ImplBrowserHost,
    ImplFrame, JsonWriterOptions, MouseButtonType, MouseEvent, Value, WrapAccessibilityHandler,
    wrap_accessibility_handler, write_json,
};
use dispatch2::MainThreadBound;
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::AnyObject,
};
use objc2_app_kit::{
    NSAccessibilityElement, NSAccessibilityFocusedUIElementChangedNotification,
    NSAccessibilityLayoutChangedNotification, NSAccessibilityPostNotification,
    NSAccessibilityUIElementDestroyedNotification, NSAccessibilityValueChangedNotification,
};
use objc2_foundation::{NSArray, NSNumber, NSPoint, NSRect, NSSize, NSString};
use serde_json::Value as Json;

use super::{OffscreenView, SurfaceView};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Key {
    tree: String,
    id: i64,
}

#[derive(Default)]
struct Tree {
    root: Option<i64>,
    nodes: HashMap<i64, Json>,
}

#[derive(Default)]
pub(super) struct State {
    trees: HashMap<String, Tree>,
    root: Option<String>,
    focus: Option<Key>,
    elements: HashMap<Key, Retained<AccessibleNode>>,
    surface: Weak<MainThreadBound<Retained<SurfaceView>>>,
}

impl State {
    fn node(&self, key: &Key) -> Option<&Json> {
        self.trees.get(&key.tree)?.nodes.get(&key.id)
    }

    fn root(&self) -> Option<Key> {
        let tree = self.root.as_ref()?;
        Some(Key {
            tree: tree.clone(),
            id: self.trees.get(tree)?.root?,
        })
    }

    fn children(&self, key: &Key) -> Vec<Key> {
        let Some(node) = self.node(key) else {
            return vec![];
        };
        let mut children = node["child_ids"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Json::as_i64)
            .map(|id| Key {
                tree: key.tree.clone(),
                id,
            })
            .collect::<Vec<_>>();
        if let Some(tree) = node["attributes"]["childTreeId"].as_str()
            && let Some(id) = self.trees.get(tree).and_then(|tree| tree.root)
        {
            children.push(Key {
                tree: tree.into(),
                id,
            });
        }
        children
    }

    fn parent(&self, key: &Key) -> Option<Key> {
        self.trees.iter().find_map(|(tree, nodes)| {
            nodes.nodes.keys().find_map(|id| {
                let parent = Key {
                    tree: tree.clone(),
                    id: *id,
                };
                self.children(&parent).contains(key).then_some(parent)
            })
        })
    }

    fn clear(&mut self, key: Key) {
        let mut pending = vec![key];
        let mut visited = HashSet::new();
        while let Some(key) = pending.pop() {
            if !visited.insert(key.clone()) {
                continue;
            }
            pending.extend(self.children(&key));
            if let Some(tree) = self.trees.get_mut(&key.tree) {
                tree.nodes.remove(&key.id);
                if tree.root == Some(key.id) {
                    tree.root = None;
                }
            }
            self.elements.remove(&key);
            if self.focus.as_ref() == Some(&key) {
                self.focus = None;
            }
        }
    }

    fn update(&mut self, data: &Json) {
        let Some(tree) = data["ax_tree_id"].as_str() else {
            return;
        };
        for update in data["updates"].as_array().into_iter().flatten() {
            if let Some(id) = update["node_id_to_clear"].as_i64() {
                self.clear(Key {
                    tree: tree.into(),
                    id,
                });
            }
            let entry = self.trees.entry(tree.into()).or_default();
            for node in update["nodes"].as_array().into_iter().flatten() {
                if let Some(id) = node["id"].as_i64() {
                    entry.nodes.insert(id, node.clone());
                }
            }
            if let Some(id) = update["root_id"].as_i64() {
                entry.root = Some(id);
            }
            if update["has_tree_data"].as_bool() == Some(true) {
                let info = &update["tree_data"];
                if info["parent_tree_id"].as_str().is_none_or(str::is_empty) {
                    self.root = Some(tree.into());
                }
                if let Some(id) = info["focus_id"].as_i64() {
                    self.focus = Some(Key {
                        tree: info["focused_tree_id"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .unwrap_or(tree)
                            .into(),
                        id,
                    });
                }
            }
            if self.root.is_none() && entry.root.is_some() {
                self.root = Some(tree.into());
            }
        }
    }

    fn update_locations(&mut self, data: &Json) {
        for update in data.as_array().into_iter().flatten() {
            let (Some(tree), Some(id)) = (update["ax_tree_id"].as_str(), update["id"].as_i64())
            else {
                continue;
            };
            if let Some(node) = self
                .trees
                .get_mut(tree)
                .and_then(|tree| tree.nodes.get_mut(&id))
            {
                node["location"] = update["new_location"]["bounds"].clone();
                node["offset_container_id"] = update["new_location"]["offset_container_id"].clone();
                node["transform"] = update["new_location"]["transform"].clone();
            }
        }
    }

    fn rect(&self, key: &Key) -> NSRect {
        let mut key = key.clone();
        let mut visited = HashSet::new();
        let Some(node) = self.node(&key) else {
            return NSRect::default();
        };
        let mut rect = location(&node["location"]);
        while visited.insert(key.clone()) {
            let Some(node) = self.node(&key) else {
                break;
            };
            rect = transform_rect(rect, node["transform"].as_str());
            let container = node["offset_container_id"]
                .as_i64()
                .filter(|id| self.trees[&key.tree].nodes.contains_key(id))
                .or(self.trees[&key.tree].root)
                .map(|id| Key {
                    tree: key.tree.clone(),
                    id,
                });
            let container = match container {
                Some(container) if container == key => {
                    self.parent(&key).filter(|parent| parent.tree != key.tree)
                }
                container => container,
            };
            let Some(container) = container.filter(|container| !visited.contains(container)) else {
                break;
            };
            let Some(node) = self.node(&container) else {
                break;
            };
            let bounds = location(&node["location"]);
            rect.origin.x += bounds.origin.x - number(&node["attributes"]["scrollX"]);
            rect.origin.y += bounds.origin.y - number(&node["attributes"]["scrollY"]);
            if node["attributes"]["clipsChildren"].as_bool() == Some(true) {
                rect = intersection(rect, bounds);
            }
            key = container;
        }
        rect
    }
}

fn number(value: &Json) -> f64 {
    value
        .as_f64()
        .filter(|number| number.is_finite())
        .unwrap_or_default()
}

fn location(value: &Json) -> NSRect {
    NSRect::new(
        NSPoint::new(number(&value["x"]), number(&value["y"])),
        NSSize::new(
            number(&value["width"]).max(0.),
            number(&value["height"]).max(0.),
        ),
    )
}

fn intersection(rect: NSRect, bounds: NSRect) -> NSRect {
    let x = rect.origin.x.max(bounds.origin.x);
    let y = rect.origin.y.max(bounds.origin.y);
    NSRect::new(
        NSPoint::new(x, y),
        NSSize::new(
            ((rect.origin.x + rect.size.width).min(bounds.origin.x + bounds.size.width) - x)
                .max(0.),
            ((rect.origin.y + rect.size.height).min(bounds.origin.y + bounds.size.height) - y)
                .max(0.),
        ),
    )
}

fn transform_rect(rect: NSRect, transform: Option<&str>) -> NSRect {
    let Some(transform) = transform else {
        return rect;
    };
    let values = transform
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split_whitespace()
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>();
    let Ok(values) = values else {
        return rect;
    };
    let Ok(matrix) = <[f64; 16]>::try_from(values) else {
        return rect;
    };
    if !matrix.iter().all(|value| value.is_finite()) {
        return rect;
    }
    let mut low = NSPoint::new(f64::INFINITY, f64::INFINITY);
    let mut high = NSPoint::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    for x in [rect.origin.x, rect.origin.x + rect.size.width] {
        for y in [rect.origin.y, rect.origin.y + rect.size.height] {
            let divisor = matrix[12] * x + matrix[13] * y + matrix[15];
            if divisor.abs() < f64::EPSILON {
                return NSRect::default();
            }
            let transformed_x = (matrix[0] * x + matrix[1] * y + matrix[3]) / divisor;
            let transformed_y = (matrix[4] * x + matrix[5] * y + matrix[7]) / divisor;
            if !transformed_x.is_finite() || !transformed_y.is_finite() {
                return NSRect::default();
            }
            low.x = low.x.min(transformed_x);
            low.y = low.y.min(transformed_y);
            high.x = high.x.max(transformed_x);
            high.y = high.y.max(transformed_y);
        }
    }
    NSRect::new(low, NSSize::new(high.x - low.x, high.y - low.y))
}

fn exposed(data: &Json) -> bool {
    !matches!(
        data["role"].as_str(),
        Some("ignored" | "none" | "inlineTextBox")
    ) && !data["state"].as_array().is_some_and(|states| {
        states
            .iter()
            .any(|state| state.as_str() == Some("invisible"))
    })
}

fn editable(data: &Json) -> bool {
    matches!(
        data["role"].as_str(),
        Some("textField" | "searchBox" | "textArea" | "textFieldWithComboBox")
    ) && !matches!(
        data["attributes"]["restriction"].as_str(),
        Some("disabled" | "readOnly")
    )
}

fn pressable(data: &Json) -> bool {
    matches!(
        data["role"].as_str(),
        Some(
            "button"
                | "buttonDropDown"
                | "menuButton"
                | "link"
                | "checkBox"
                | "switch"
                | "radioButton"
                | "tab"
                | "popUpButton"
                | "menuItem"
                | "menuItemCheckBox"
                | "menuItemRadio"
        )
    ) || data["actions"].as_array().is_some_and(|actions| {
        actions
            .iter()
            .any(|action| action.as_str() == Some("doDefault"))
    })
}

fn role(role: &str) -> &'static str {
    match role {
        "button" | "buttonDropDown" | "menuButton" => "AXButton",
        "link" => "AXLink",
        "textField" | "searchBox" | "textFieldWithComboBox" => "AXTextField",
        "textArea" => "AXTextArea",
        "checkBox" | "switch" => "AXCheckBox",
        "radioButton" => "AXRadioButton",
        "slider" => "AXSlider",
        "spinButton" => "AXIncrementor",
        "comboBox" => "AXComboBox",
        "popUpButton" => "AXPopUpButton",
        "tab" => "AXRadioButton",
        "tabList" => "AXTabGroup",
        "image" | "canvas" => "AXImage",
        "staticText" | "inlineTextBox" => "AXStaticText",
        "list" | "listBox" => "AXList",
        "table" | "grid" => "AXTable",
        "row" => "AXRow",
        "cell" | "columnHeader" | "rowHeader" => "AXCell",
        "menu" => "AXMenu",
        "menuItem" | "menuItemCheckBox" | "menuItemRadio" => "AXMenuItem",
        "progressIndicator" | "meter" => "AXProgressIndicator",
        "scrollBar" => "AXScrollBar",
        "rootWebArea" | "webArea" => "AXWebArea",
        _ => "AXGroup",
    }
}

struct NodeIvars {
    surface: Weak<MainThreadBound<Retained<SurfaceView>>>,
    key: Key,
}

define_class!(
  #[unsafe(super(NSAccessibilityElement))]
  #[thread_kind = MainThreadOnly]
  #[name = "JarvisCefAccessibleNode"]
  #[ivars = NodeIvars]
  struct AccessibleNode;

  impl AccessibleNode {
    #[unsafe(method(isAccessibilityElement))]
    fn is_accessibility_element(&self) -> bool {
      self.is_exposed()
    }
    #[unsafe(method(accessibilityIsIgnored))]
    fn accessibility_is_ignored(&self) -> bool { !self.is_exposed() }
    #[unsafe(method_id(accessibilityRole))]
    fn accessibility_role(&self) -> Retained<NSString> { NSString::from_str(role(self.data().as_ref().and_then(|d| d["role"].as_str()).unwrap_or_default())) }
    #[unsafe(method_id(accessibilityLabel))]
    fn accessibility_label(&self) -> Option<Retained<NSString>> { self.attribute("name") }
    #[unsafe(method_id(accessibilityTitle))]
    fn accessibility_title(&self) -> Option<Retained<NSString>> { self.attribute("name") }
    #[unsafe(method_id(accessibilityHelp))]
    fn accessibility_help(&self) -> Option<Retained<NSString>> { self.attribute("description") }
    #[unsafe(method_id(accessibilityValue))]
    fn accessibility_value(&self) -> Option<Retained<AnyObject>> { self.value_object() }
    #[unsafe(method(isAccessibilityEnabled))]
    fn accessibility_enabled(&self) -> bool { self.enabled() }
    #[unsafe(method(isAccessibilityFocused))]
    fn accessibility_focused(&self) -> bool { self.data().is_some() && self.with_surface(|view| view.ivars().accessibility.borrow().focus.as_ref() == Some(&self.ivars().key)).unwrap_or(false) }
    #[unsafe(method(setAccessibilityFocused:))]
    fn set_accessibility_focused(&self, focused: bool) {
      if focused && self.data().is_some() {
        self.with_surface(|view| {
          if let Some(window) = view.window() { window.makeFirstResponder(Some(view)); }
          if let Some(host) = view.host() { host.set_focus(1); }
        });
      }
    }
    #[unsafe(method_id(accessibilityParent))]
    fn accessibility_parent(&self) -> Option<Retained<AnyObject>> { self.parent_object() }
    #[unsafe(method_id(accessibilityChildren))]
    fn accessibility_children(&self) -> Retained<NSArray<AnyObject>> { self.child_objects() }
    #[unsafe(method(accessibilityFrame))]
    fn accessibility_frame(&self) -> NSRect { self.frame() }
    #[unsafe(method_id(accessibilityWindow))]
    fn accessibility_window(&self) -> Option<Retained<AnyObject>> { self.window_object() }
    #[unsafe(method_id(accessibilityTopLevelUIElement))]
    fn accessibility_top_level(&self) -> Option<Retained<AnyObject>> { self.window_object() }
    #[unsafe(method(accessibilityPerformPress))]
    fn accessibility_press(&self) -> bool { self.data().is_some_and(|data| pressable(&data)) && self.click(MouseButtonType::LEFT) }
    #[unsafe(method(accessibilityPerformShowMenu))]
    fn accessibility_show_menu(&self) -> bool { self.click(MouseButtonType::RIGHT) }
    #[unsafe(method(setAccessibilityValue:))]
    fn set_accessibility_value(&self, value: &AnyObject) {
      let Some(value) = value.downcast_ref::<NSString>() else { return; };
      if !self.data().is_some_and(|data| editable(&data)) || !self.click(MouseButtonType::LEFT) { return; }
      self.with_surface(|view| {
        let browser = view.ivars().browser.borrow().clone();
        if let Some(browser) = browser && let Some(frame) = browser.focused_frame() && let Some(host) = browser.host() {
          frame.select_all();
          host.ime_commit_text(Some(&CefString::from(value.to_string().as_str())), None, 0);
        }
      });
    }
  }
);

impl AccessibleNode {
    fn value_object(&self) -> Option<Retained<AnyObject>> {
        let data = self.data()?;
        if let Some(value) = data["attributes"]["valueForRange"].as_f64() {
            return Some(NSNumber::numberWithDouble(value).into());
        }
        if let Some(checked) = data["attributes"]["checkedState"].as_str() {
            return Some(
                NSNumber::numberWithInteger(match checked {
                    "true" => 1,
                    "mixed" => 2,
                    _ => 0,
                })
                .into(),
            );
        }
        if let Some(selected) = data["attributes"]["selected"].as_bool() {
            return Some(NSNumber::numberWithBool(selected).into());
        }
        self.attribute("value").map(Into::into)
    }
    fn parent_object(&self) -> Option<Retained<AnyObject>> {
        self.data()?;
        self.with_surface(|view| {
            let parent = view
                .ivars()
                .accessibility
                .borrow()
                .parent(&self.ivars().key);
            match parent {
                Some(key) => {
                    element(view, &self.ivars().surface, key).map(Retained::<AnyObject>::from)
                }
                None => Some(Retained::from(view)),
            }
        })
        .flatten()
    }
    fn child_objects(&self) -> Retained<NSArray<AnyObject>> {
        if self.data().is_none() {
            return NSArray::new();
        }
        self.with_surface(|view| {
            let keys = view
                .ivars()
                .accessibility
                .borrow()
                .children(&self.ivars().key);
            node_array(view, &self.ivars().surface, keys)
        })
        .unwrap_or_else(NSArray::new)
    }
    fn is_exposed(&self) -> bool {
        self.data().is_some_and(|data| exposed(&data))
    }
    fn enabled(&self) -> bool {
        self.data()
            .is_some_and(|data| data["attributes"]["restriction"].as_str() != Some("disabled"))
    }
    fn frame(&self) -> NSRect {
        if self.data().is_none() {
            return NSRect::default();
        }
        self.with_surface(|view| {
            let rect = view.ivars().accessibility.borrow().rect(&self.ivars().key);
            let rect = view.convertRect_toView(rect, None);
            view.window()
                .map(|window| window.convertRectToScreen(rect))
                .unwrap_or(rect)
        })
        .unwrap_or_default()
    }
    fn window_object(&self) -> Option<Retained<AnyObject>> {
        self.data()?;
        self.with_surface(|view| view.window().map(Retained::<AnyObject>::from))
            .flatten()
    }
    fn with_surface<T>(&self, f: impl FnOnce(&SurfaceView) -> T) -> Option<T> {
        let surface = self.ivars().surface.upgrade()?;
        Some(f(surface.get(MainThreadMarker::new()?)))
    }
    fn data(&self) -> Option<Json> {
        self.with_surface(|view| {
            let state = view.ivars().accessibility.borrow();
            // A screen reader can retain a removed wrapper while CEF reuses its node id.
            if !state
                .elements
                .get(&self.ivars().key)
                .is_some_and(|node| std::ptr::eq(Retained::as_ptr(node), self))
            {
                return None;
            }
            state.node(&self.ivars().key).cloned()
        })
        .flatten()
    }
    fn attribute(&self, attribute: &str) -> Option<Retained<NSString>> {
        self.data()?["attributes"][attribute]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(NSString::from_str)
    }
    fn click(&self, button: MouseButtonType) -> bool {
        if !self.enabled() {
            return false;
        }
        self.with_surface(|view| {
            let rect = view.ivars().accessibility.borrow().rect(&self.ivars().key);
            if rect.size.width <= 0. || rect.size.height <= 0. {
                return false;
            }
            let Some(host) = view.host() else {
                return false;
            };
            if let Some(window) = view.window() {
                window.makeFirstResponder(Some(view));
            }
            host.set_focus(1);
            let event = MouseEvent {
                x: (rect.origin.x + rect.size.width / 2.) as i32,
                y: (rect.origin.y + rect.size.height / 2.) as i32,
                modifiers: 0,
            };
            host.send_mouse_click_event(Some(&event), button, 0, 1);
            host.send_mouse_click_event(Some(&event), button, 1, 1);
            true
        })
        .unwrap_or(false)
    }
}

fn element(
    view: &SurfaceView,
    surface: &Weak<MainThreadBound<Retained<SurfaceView>>>,
    key: Key,
) -> Option<Retained<AccessibleNode>> {
    let mut state = view.ivars().accessibility.borrow_mut();
    state.node(&key)?;
    Some(
        state
            .elements
            .entry(key.clone())
            .or_insert_with(|| {
                let allocated = AccessibleNode::alloc(
                    MainThreadMarker::new().expect("AX is queried on the main thread"),
                )
                .set_ivars(NodeIvars {
                    surface: surface.clone(),
                    key,
                });
                unsafe { msg_send![super(allocated), init] }
            })
            .clone(),
    )
}

fn node_array(
    view: &SurfaceView,
    surface: &Weak<MainThreadBound<Retained<SurfaceView>>>,
    keys: Vec<Key>,
) -> Retained<NSArray<AnyObject>> {
    let nodes = keys
        .into_iter()
        .filter_map(|key| element(view, surface, key))
        .map(Retained::<AnyObject>::from)
        .collect::<Vec<_>>();
    NSArray::from_retained_slice(&nodes)
}

pub(super) fn children(view: &SurfaceView) -> Retained<NSArray<AnyObject>> {
    let state = view.ivars().accessibility.borrow();
    let keys = state.root().into_iter().collect();
    let surface = state.surface.clone();
    drop(state);
    node_array(view, &surface, keys)
}

pub(super) fn focused(view: &SurfaceView) -> Option<Retained<AnyObject>> {
    let state = view.ivars().accessibility.borrow();
    let key = state.focus.clone()?;
    let surface = state.surface.clone();
    drop(state);
    element(view, &surface, key).map(Retained::<AnyObject>::from)
}

pub(super) fn hit_test(view: &SurfaceView, point: NSPoint) -> Option<Retained<AnyObject>> {
    let state = view.ivars().accessibility.borrow();
    let mut pending = state.root().into_iter().collect::<Vec<_>>();
    let surface = state.surface.clone();
    drop(state);
    let mut visited = HashSet::new();
    let mut found = None;
    while let Some(key) = pending.pop() {
        if !visited.insert(key.clone()) {
            continue;
        }
        let Some(node) = element(view, &surface, key.clone()) else {
            continue;
        };
        let rect = node.frame();
        if point.x >= rect.origin.x
            && point.y >= rect.origin.y
            && point.x <= rect.origin.x + rect.size.width
            && point.y <= rect.origin.y + rect.size.height
        {
            if node.is_exposed() {
                found = Some(Retained::<AnyObject>::from(node));
            }
            pending.extend(
                view.ivars()
                    .accessibility
                    .borrow()
                    .children(&key)
                    .into_iter()
                    .rev(),
            );
        }
    }
    found
}

pub(super) fn handler(view: OffscreenView) -> AccessibilityHandler {
    view.surface().ivars().accessibility.borrow_mut().surface = Arc::downgrade(&view.view);
    OffscreenAccessibilityHandler::new(view)
}

fn data(value: Option<&mut Value>) -> Option<Json> {
    let value = write_json(value, JsonWriterOptions::default());
    serde_json::from_str(&CefString::from(&value).to_string()).ok()
}

wrap_accessibility_handler! {
  struct OffscreenAccessibilityHandler { view: OffscreenView }
  impl AccessibilityHandler {
    fn on_accessibility_tree_change(&self, value: Option<&mut Value>) {
      let view = self.view.surface();
      if view.ivars().closed.get() || view.ivars().browser.borrow().is_none() { return; }
      let Some(data) = data(value) else { return; };
      let previous = view.ivars().accessibility.borrow().focus.clone();
      let removed = if data["updates"].as_array().is_some_and(|updates| updates.iter().any(|update| update["node_id_to_clear"].is_number())) {
        view.ivars().accessibility.borrow().elements.values().cloned().collect::<Vec<_>>()
      } else { vec![] };
      view.ivars().accessibility.borrow_mut().update(&data);
      for node in removed {
        if node.data().is_none() { unsafe { NSAccessibilityPostNotification(&node, NSAccessibilityUIElementDestroyedNotification); } }
      }
      unsafe { NSAccessibilityPostNotification(view, NSAccessibilityLayoutChangedNotification); }
      let focus_changed = previous != view.ivars().accessibility.borrow().focus;
      if focus_changed && let Some(node) = focused(view) {
        unsafe { NSAccessibilityPostNotification(&node, NSAccessibilityFocusedUIElementChangedNotification); }
      }
      if data["events"].as_array().is_some_and(|events| events.iter().any(|event| event["event_type"].as_str() == Some("valueChanged"))) && let Some(node) = focused(view) {
        unsafe { NSAccessibilityPostNotification(&node, NSAccessibilityValueChangedNotification); }
      }
    }
    fn on_accessibility_location_change(&self, value: Option<&mut Value>) {
      let view = self.view.surface();
      if view.ivars().closed.get() || view.ivars().browser.borrow().is_none() { return; }
      if let Some(data) = data(value) { view.ivars().accessibility.borrow_mut().update_locations(&data); }
    }
  }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn updates_replace_children_and_clear_detached_subtrees() {
        let mut state = State::default();
        state.update(&json!({"ax_tree_id":"a","updates":[{"root_id":1,"nodes":[{"id":1,"role":"rootWebArea","child_ids":[2]},{"id":2,"role":"button","attributes":{"name":"Abrir chat"}}]}]}));
        let root = state.root().unwrap();
        assert_eq!(
            state.children(&root),
            vec![Key {
                tree: "a".into(),
                id: 2
            }]
        );
        state.update(&json!({"ax_tree_id":"a","updates":[{"node_id_to_clear":1,"root_id":3,"nodes":[{"id":3,"role":"rootWebArea"}]}]}));
        assert_eq!(state.root().unwrap().id, 3);
        assert!(
            state
                .node(&Key {
                    tree: "a".into(),
                    id: 2
                })
                .is_none()
        );
    }

    #[test]
    fn focus_labels_and_scrolled_bounds_follow_cef_updates() {
        let mut state = State::default();
        state.update(&json!({"ax_tree_id":"a","updates":[{"has_tree_data":true,"tree_data":{"focus_id":2},"root_id":1,"nodes":[{"id":1,"location":{"x":10,"y":20},"attributes":{"scrollY":5}},{"id":2,"role":"textField","offset_container_id":1,"location":{"x":2,"y":3,"width":80,"height":24},"attributes":{"name":"Mensagem","value":"Olá"}}]}]}));
        let key = state.focus.clone().unwrap();
        assert_eq!(state.node(&key).unwrap()["attributes"]["name"], "Mensagem");
        assert_eq!(state.rect(&key).origin, NSPoint::new(12., 18.));
        state.update_locations(&json!([{"ax_tree_id":"a","id":2,"new_location":{"bounds":{"x":4,"y":8,"width":90,"height":24},"offset_container_id":1}}]));
        assert_eq!(state.rect(&key).origin, NSPoint::new(14., 23.));
        assert_eq!(role("button"), "AXButton");
        assert_eq!(role("textField"), "AXTextField");
    }

    #[test]
    fn inaccessible_and_read_only_nodes_do_not_offer_input_edits() {
        assert!(!exposed(&json!({"role":"button","state":["invisible"]})));
        assert!(!exposed(&json!({"role":"ignored"})));
        assert!(exposed(&json!({"role":"button"})));
        assert!(!editable(&json!({"role":"button"})));
        assert!(!editable(
            &json!({"role":"textField","attributes":{"restriction":"readOnly"}})
        ));
        assert!(editable(&json!({"role":"textField"})));
        assert!(!pressable(&json!({"role":"staticText"})));
        assert!(pressable(&json!({"role":"button"})));
        assert!(pressable(&json!({"role":"group","actions":["doDefault"]})));
    }

    #[test]
    fn transformed_robot_bounds_and_click_center_follow_scrolled_parent() {
        let mut state = State::default();
        let transform = "[ 0.35 0 0 40\n  0 0.35 0 30\n  0 0 1 0\n  0 0 0 1 ]\n";
        state.update(&json!({"ax_tree_id":"island","updates":[{"root_id":1,"nodes":[{"id":1,"role":"rootWebArea","child_ids":[2],"location":{"x":10,"y":20,"width":400,"height":300},"attributes":{"scrollY":5,"clipsChildren":true}},{"id":2,"role":"button","offset_container_id":1,"location":{"x":0,"y":0,"width":80,"height":80},"transform":transform,"attributes":{"name":"Abrir Jarvis"}}]}]}));
        let key = Key {
            tree: "island".into(),
            id: 2,
        };
        let rect = state.rect(&key);
        assert_eq!(
            rect,
            NSRect::new(NSPoint::new(50., 45.), NSSize::new(28., 28.))
        );
        assert_eq!(rect.origin.x + rect.size.width / 2., 64.);
        assert_eq!(rect.origin.y + rect.size.height / 2., 59.);
        state.update_locations(&json!([{"ax_tree_id":"island","id":2,"new_location":{"bounds":{"x":0,"y":0,"width":80,"height":80},"offset_container_id":1,"transform":"[ 0.35 0 0 80\n 0 0.35 0 30\n 0 0 1 0\n 0 0 0 1 ]\n"}}]));
        assert_eq!(state.rect(&key).origin, NSPoint::new(90., 45.));
        assert_eq!(transform_rect(rect, Some("invalid")), rect);
    }
}
