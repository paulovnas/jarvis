//! Public macOS APIs, with every input bound to an observed app and window.
use super::*;
use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::{
    msg_send,
    rc::{autoreleasepool, Retained},
    runtime::AnyClass,
    runtime::AnyObject,
    AnyThread, MainThreadMarker,
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSBitmapImageFileType, NSBitmapImageRep,
    NSRunningApplication, NSWorkspace,
};
use objc2_application_services::{
    AXError, AXIsProcessTrusted, AXIsProcessTrustedWithOptions, AXUIElement, AXValue, AXValueType,
};
use objc2_core_foundation::{
    kCFBooleanTrue, CFArray, CFBoolean, CFDictionary, CFNumber, CFNumberType, CFRange, CFRetained,
    CFString, CFType, CGPoint, CGRect, CGSize, Type,
};
use objc2_core_graphics::{
    CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventType, CGImage,
    CGMouseButton, CGPreflightPostEventAccess, CGPreflightScreenCaptureAccess,
    CGRequestPostEventAccess, CGRequestScreenCaptureAccess, CGScrollEventUnit,
    CGSessionCopyCurrentDictionary, CGWindowListCopyWindowInfo, CGWindowListOption,
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSString, NSURL};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    ptr::NonNull,
    sync::{mpsc, Mutex, MutexGuard, TryLockError},
    thread,
    time::Duration,
};

const MAX_NODES: usize = 400;
const MAX_DEPTH: usize = 16;
const MAX_CHILDREN: usize = 64;
const MAX_SNAPSHOTS: usize = 16;
const MAX_PIXELS: u64 = 25_000_000;
const MAX_TEXT: usize = 4096;
const MAX_INPUT_TEXT: usize = 8000;
const MAX_EDIT_UNITS: isize = 65_536;
const MAX_SCROLL: f64 = 3000.0;
const OBSERVATION_TTL: Duration = Duration::from_secs(60);

#[link(name = "ScreenCaptureKit", kind = "framework")]
unsafe extern "C" {}

struct AxElement(CFRetained<AXUIElement>);
// Retained AX IPC proxies cross queues; calls into our own AppKit/CEF objects stay on main.
unsafe impl Send for AxElement {}

impl AxElement {
    fn element(&self) -> &AXUIElement {
        &self.0
    }
}

struct Snapshot {
    target: NativeTarget,
    root: Option<AxElement>,
    nodes: HashMap<String, (AxElement, ComputerElement)>,
    created: Instant,
}

#[derive(Default)]
pub(super) struct MacBackend {
    snapshots: Mutex<HashMap<String, Snapshot>>,
}

fn error(code: &str, message: &str) -> AgentError {
    AgentError::new(code, message)
}

fn stale() -> AgentError {
    error(
        "computer_stale_target",
        "O aplicativo, a janela ou o elemento mudou. Observe o alvo novamente.",
    )
}

fn unavailable() -> AgentError {
    error(
        "computer_target_unavailable",
        "A instância autorizada do aplicativo ou a janela selecionada não está mais disponível.",
    )
}

fn input_unavailable() -> AgentError {
    error(
        "computer_input_unavailable",
        "Entrada indisponível: não foi possível confirmar a janela de Acessibilidade. A captura é somente para leitura; este alvo não aceita entrada automática agora.",
    )
}

fn input_root(snapshot: &Snapshot) -> Result<&AXUIElement, AgentError> {
    snapshot
        .root
        .as_ref()
        .map(|root| &*root.0)
        .ok_or_else(input_unavailable)
}

fn partial_children(
    result: Result<Vec<CFRetained<AXUIElement>>, AgentError>,
    observation: &mut NativeObservation,
    guard: &OperationGuard,
) -> Result<Vec<CFRetained<AXUIElement>>, AgentError> {
    guard.check()?;
    match result {
        Ok(children) => Ok(children),
        Err(cause) => {
            observation.truncated = true;
            if observation.errors.len() < 16 {
                observation.errors.push(cause.message().to_owned());
            }
            Ok(Vec::new())
        }
    }
}

fn cf_value(dict: &CFDictionary, key: &str) -> Option<CFRetained<CFType>> {
    // System dictionaries used here have CFString keys and retained CF values.
    unsafe { dict.cast_unchecked::<CFString, CFType>() }.get(&CFString::from_str(key))
}

fn number(value: CFRetained<CFType>) -> Option<i64> {
    let value = value.downcast::<CFNumber>().ok()?;
    let mut result = 0_i64;
    // The output has exactly the requested signed 64-bit representation.
    unsafe { value.value(CFNumberType::SInt64Type, (&mut result as *mut i64).cast()) }
        .then_some(result)
}

fn dict_bool(dict: &CFDictionary, key: &str) -> bool {
    cf_value(dict, key)
        .and_then(|value| value.downcast::<CFBoolean>().ok())
        .is_some_and(|value| value.value())
}

fn gui_session() -> bool {
    CGSessionCopyCurrentDictionary().is_some_and(|dict| {
        dict_bool(&dict, "kCGSSessionOnConsoleKey") && dict_bool(&dict, "kCGSessionLoginDoneKey")
    })
}

fn session_check(guard: &OperationGuard) -> Result<(), AgentError> {
    guard.check()?;
    if !gui_session() {
        return Err(error(
            "computer_session_unavailable",
            "A sessão gráfica local não está ativa.",
        ));
    }
    Ok(())
}

fn valid_bounds(bounds: Bounds) -> bool {
    [bounds.x, bounds.y, bounds.width, bounds.height]
        .into_iter()
        .all(f64::is_finite)
        && bounds.width > 0.0
        && bounds.height > 0.0
}

fn bounds_match(left: Bounds, right: Bounds) -> bool {
    valid_bounds(left)
        && valid_bounds(right)
        && [
            left.x - right.x,
            left.y - right.y,
            left.width - right.width,
            left.height - right.height,
        ]
        .into_iter()
        .all(|delta| delta.abs() <= 0.5)
}

fn contains(bounds: Bounds, point: Point) -> bool {
    valid_bounds(bounds)
        && point.x.is_finite()
        && point.y.is_finite()
        && point.x >= bounds.x
        && point.y >= bounds.y
        && point.x < bounds.x + bounds.width
        && point.y < bounds.y + bounds.height
}

fn canonical_url(url: Option<Retained<NSURL>>, guard: &OperationGuard) -> Option<String> {
    guard.check().ok()?;
    let path = url?.path()?.to_string();
    let path = std::fs::canonicalize(Path::new(&path)).ok()?;
    guard.check().ok()?;
    path.to_str().map(str::to_owned)
}

fn app_record(app: &NSRunningApplication, guard: &OperationGuard) -> Option<ComputerApp> {
    guard.check().ok()?;
    if app.isTerminated() || app.processIdentifier() <= 0 {
        return None;
    }
    let launch = app.launchDate()?.timeIntervalSince1970();
    if !launch.is_finite() || launch <= 0.0 {
        return None;
    }
    let identity = AppIdentity {
        bundle_id: app.bundleIdentifier()?.to_string(),
        bundle_path: canonical_url(app.bundleURL(), guard)?,
        executable_path: canonical_url(app.executableURL(), guard)?,
    };
    let process_id = app.processIdentifier();
    let launch_id = format!("{:016x}", launch.to_bits());
    let app_id = format!(
        "{:x}",
        Sha256::digest(format!(
            "{}\0{}\0{}\0{process_id}\0{launch_id}",
            identity.bundle_id, identity.bundle_path, identity.executable_path
        ))
    );
    Some(ComputerApp {
        app_id,
        name: app
            .localizedName()
            .map(|name| bounded(&name.to_string()))
            .unwrap_or_else(|| identity.bundle_id.clone()),
        identity,
        process_id,
        launch_id,
        windows: Vec::new(),
    })
}

struct WindowRecord {
    pid: i32,
    layer: i64,
    window: ComputerWindow,
}

struct WindowInventory {
    records: Vec<WindowRecord>,
    identities: Vec<(i32, u32)>,
    complete: bool,
}

fn windows(guard: &OperationGuard) -> Result<Vec<WindowRecord>, AgentError> {
    Ok(window_inventory(
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements,
        0,
        guard,
    )?
    .records)
}

fn window_inventory(
    options: CGWindowListOption,
    window_id: u32,
    guard: &OperationGuard,
) -> Result<WindowInventory, AgentError> {
    session_check(guard)?;
    let array = CGWindowListCopyWindowInfo(options, window_id).ok_or_else(|| {
        error(
            "computer_windows_unavailable",
            "Não foi possível enumerar as janelas.",
        )
    })?;
    let record_count = usize::try_from(array.count()).map_err(|_| stale())?;
    // CGWindowListCopyWindowInfo documents an array of CF dictionaries.
    let array = unsafe { array.cast_unchecked::<CFType>() };
    let mut result = Vec::new();
    let mut identities = Vec::new();
    for value in array.iter().take(1024) {
        guard.check()?;
        let Ok(dict) = value.downcast::<CFDictionary>() else {
            continue;
        };
        let Some(pid) = cf_value(&dict, "kCGWindowOwnerPID")
            .and_then(number)
            .and_then(|n| i32::try_from(n).ok())
        else {
            continue;
        };
        let Some(id) = cf_value(&dict, "kCGWindowNumber")
            .and_then(number)
            .and_then(|n| u32::try_from(n).ok())
        else {
            continue;
        };
        identities.push((pid, id));
        let Some(rect) = cf_value(&dict, "kCGWindowBounds")
            .and_then(|value| value.downcast::<CFDictionary>().ok())
        else {
            continue;
        };
        let coordinate = |key| {
            cf_value(&rect, key)
                .and_then(|value| value.downcast::<CFNumber>().ok())
                .and_then(|value| {
                    let mut n = 0.0_f64;
                    // CGRect dictionary coordinates have a numeric representation.
                    unsafe { value.value(CFNumberType::Float64Type, (&mut n as *mut f64).cast()) }
                        .then_some(n)
                })
        };
        let (Some(x), Some(y), Some(width), Some(height)) = (
            coordinate("X"),
            coordinate("Y"),
            coordinate("Width"),
            coordinate("Height"),
        ) else {
            continue;
        };
        let bounds = Bounds {
            x,
            y,
            width,
            height,
        };
        if !valid_bounds(bounds) {
            continue;
        }
        let title = cf_value(&dict, "kCGWindowName")
            .and_then(|value| value.downcast::<CFString>().ok())
            .map(|text| bounded(&text.to_string()))
            .unwrap_or_default();
        result.push(WindowRecord {
            pid,
            layer: cf_value(&dict, "kCGWindowLayer")
                .and_then(number)
                .unwrap_or(-1),
            window: ComputerWindow { id, title, bounds },
        });
    }
    guard.check()?;
    Ok(WindowInventory {
        complete: record_count <= 1024 && identities.len() == record_count,
        records: result,
        identities,
    })
}

fn running_target_app(
    target: &NativeTarget,
    guard: &OperationGuard,
) -> Result<Retained<NSRunningApplication>, AgentError> {
    guard.check()?;
    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(target.app.process_id)
        .ok_or_else(unavailable)?;
    guard.check()?;
    if app.isTerminated() {
        return Err(unavailable());
    }
    let current = app_record(&app, guard);
    guard.check()?;
    let current = current.ok_or_else(stale)?;
    if current.identity != target.app.identity || current.launch_id != target.app.launch_id {
        return Err(unavailable());
    }
    if current.app_id != target.app.app_id {
        return Err(stale());
    }
    Ok(app)
}

fn current_window(
    target: &NativeTarget,
    guard: &OperationGuard,
) -> Result<ComputerWindow, AgentError> {
    running_target_app(target, guard)?;
    // Include offscreen windows. Absence is authoritative only when ID/owner metadata
    // covers every returned entry; truncated or malformed inventory is never evidence.
    let inventory = window_inventory(CGWindowListOption::OptionAll, 0, guard)?;
    inventory_window(inventory, target.app.process_id, target.window.id)
}

fn inventory_window(
    inventory: WindowInventory,
    process_id: i32,
    window_id: u32,
) -> Result<ComputerWindow, AgentError> {
    if let Some(record) = inventory
        .records
        .into_iter()
        .find(|record| record.pid == process_id && record.window.id == window_id)
    {
        if record.layer != 0 {
            return Err(stale());
        }
        return Ok(record.window);
    }
    if inventory.complete && !inventory.identities.contains(&(process_id, window_id)) {
        return Err(unavailable());
    }
    Err(stale())
}

fn bounded(text: &str) -> String {
    text.chars().take(MAX_TEXT).collect()
}

fn ax_timeout(element: &AXUIElement, guard: &OperationGuard) -> Result<(), AgentError> {
    guard.check()?;
    let remaining = guard
        .deadline
        .saturating_duration_since(Instant::now())
        .as_secs_f32()
        .min(0.15);
    if remaining <= 0.0 {
        return Err(stale());
    }
    // This retained IPC proxy is valid; the timeout applies to this exact object.
    ax_result(unsafe { element.set_messaging_timeout(remaining) })
}

fn ax_result(result: AXError) -> Result<(), AgentError> {
    if result == AXError::Success {
        return Ok(());
    }
    if result == AXError::InvalidUIElement {
        return Err(stale());
    }
    Err(error(
        "computer_accessibility_failed",
        &format!(
            "Acessibilidade retornou {}. Observe o alvo antes de repetir a ação.",
            result.0
        ),
    ))
}

fn ax_query<T: Send + 'static>(
    element: &AXUIElement,
    guard: &OperationGuard,
    query: impl FnOnce(&AXUIElement, &OperationGuard) -> Result<T, AgentError> + Send + 'static,
) -> Result<T, AgentError> {
    guard.check()?;
    if MainThreadMarker::new().is_some() {
        return query(element, guard);
    }
    let process_id = ax_pid(element, guard)?;
    if process_id != std::process::id() as i32 {
        return query(element, guard);
    }
    let element = AxElement(element.retain());
    ax_input(process_id, guard, move |guard| {
        query(element.element(), guard)
    })
}

fn ax_read(
    element: &AXUIElement,
    key: &str,
    guard: &OperationGuard,
) -> Result<Option<CFRetained<CFType>>, AgentError> {
    ax_timeout(element, guard)?;
    let key = CFString::from_str(key);
    let mut pointer = std::ptr::null();
    // The out pointer is writable; successful Copy calls transfer one retain.
    let result = unsafe { element.copy_attribute_value(&key, NonNull::from(&mut pointer)) };
    let value = if result == AXError::Success {
        NonNull::new(pointer.cast_mut()).map(|pointer| unsafe { CFRetained::from_raw(pointer) })
    } else {
        None
    };
    guard.check()?;
    if result == AXError::AttributeUnsupported || result == AXError::NoValue {
        return Ok(None);
    }
    ax_result(result)?;
    Ok(value)
}

fn ax_text(
    element: &AXUIElement,
    key: &str,
    guard: &OperationGuard,
) -> Result<Option<String>, AgentError> {
    let key = key.to_owned();
    ax_query(element, guard, move |element, guard| {
        Ok(ax_read(element, &key, guard)?
            .and_then(|value| value.downcast::<CFString>().ok())
            .map(|value| bounded(&value.to_string())))
    })
}

fn edit_value(element: &AXUIElement, guard: &OperationGuard) -> Result<Option<String>, AgentError> {
    ax_query(element, guard, |element, guard| {
        let value =
            ax_read(element, "AXValue", guard)?.and_then(|value| value.downcast::<CFString>().ok());
        Ok(value
            .filter(|value| value.length() <= MAX_EDIT_UNITS)
            .map(|value| value.to_string()))
    })
}

fn selected_range(
    element: &AXUIElement,
    guard: &OperationGuard,
) -> Result<Option<CFRange>, AgentError> {
    ax_query(element, guard, |element, guard| {
        let value = ax_read(element, "AXSelectedTextRange", guard)?
            .and_then(|value| value.downcast::<AXValue>().ok());
        let Some(value) = value else {
            return Ok(None);
        };
        let mut range = CFRange {
            location: 0,
            length: 0,
        };
        // Accessibility selection ranges use UTF-16 units.
        let valid = unsafe { value.value(AXValueType::CFRange, NonNull::from(&mut range).cast()) };
        Ok(valid.then_some(range))
    })
}

fn ax_bool(
    element: &AXUIElement,
    key: &str,
    guard: &OperationGuard,
) -> Result<Option<bool>, AgentError> {
    let key = key.to_owned();
    ax_query(element, guard, move |element, guard| {
        Ok(ax_read(element, &key, guard)?
            .and_then(|value| value.downcast::<CFBoolean>().ok())
            .map(|value| value.value()))
    })
}

fn ax_element(
    element: &AXUIElement,
    key: &str,
    guard: &OperationGuard,
) -> Result<Option<CFRetained<AXUIElement>>, AgentError> {
    let key = key.to_owned();
    ax_query(element, guard, move |element, guard| {
        Ok(ax_read(element, &key, guard)?
            .and_then(|value| value.downcast::<AXUIElement>().ok())
            .map(AxElement))
    })
    .map(|value| value.map(|element| element.0))
}

fn ax_child_page<T>(
    count: isize,
    limit: usize,
    copy: impl FnOnce(isize) -> Result<Vec<T>, AgentError>,
) -> Result<Vec<T>, AgentError> {
    let count = usize::try_from(count).map_err(|_| stale())?.min(limit);
    if count == 0 {
        return Ok(Vec::new());
    }
    copy(count as isize)
}

fn ax_children(
    element: &AXUIElement,
    key: &str,
    limit: usize,
    guard: &OperationGuard,
) -> Result<Vec<CFRetained<AXUIElement>>, AgentError> {
    let key = key.to_owned();
    ax_query(element, guard, move |element, guard| {
        ax_timeout(element, guard)?;
        let key = CFString::from_str(&key);
        let mut count = 0;
        let result = unsafe { element.attribute_value_count(&key, NonNull::from(&mut count)) };
        guard.check()?;
        if result == AXError::AttributeUnsupported || result == AXError::NoValue {
            return Ok(Vec::new());
        }
        ax_result(result)?;
        // Index zero is illegal for an empty AX array; query its count before paging.
        ax_child_page(count, limit, |count| {
            ax_timeout(element, guard)?;
            let mut pointer = std::ptr::null();
            // The successful Copy call transfers one array retain.
            let result = unsafe {
                element.copy_attribute_values(&key, 0, count, NonNull::from(&mut pointer))
            };
            let array = if result == AXError::Success {
                NonNull::new(pointer.cast_mut())
                    .map(|pointer| unsafe { CFRetained::<CFArray>::from_raw(pointer) })
            } else {
                None
            };
            guard.check()?;
            ax_result(result)?;
            let Some(array) = array else {
                return Ok(Vec::new());
            };
            // AX array-valued attributes contain retained CF objects.
            Ok(unsafe { array.cast_unchecked::<CFType>() }
                .iter()
                .take(limit)
                .filter_map(|value| value.downcast::<AXUIElement>().ok())
                .map(AxElement)
                .collect())
        })
    })
    .map(|nodes| nodes.into_iter().map(|element| element.0).collect())
}

fn ax_bounds(element: &AXUIElement, guard: &OperationGuard) -> Result<Option<Bounds>, AgentError> {
    ax_query(element, guard, |element, guard| {
        let position = ax_read(element, "AXPosition", guard)?
            .and_then(|value| value.downcast::<AXValue>().ok());
        let size =
            ax_read(element, "AXSize", guard)?.and_then(|value| value.downcast::<AXValue>().ok());
        let (Some(position), Some(size)) = (position, size) else {
            return Ok(None);
        };
        let mut point = CGPoint { x: 0.0, y: 0.0 };
        let mut dimensions = CGSize {
            width: 0.0,
            height: 0.0,
        };
        // AXValue verifies the requested public CGPoint/CGSize type before copying.
        if !unsafe {
            position.value(AXValueType::CGPoint, NonNull::from(&mut point).cast())
                && size.value(AXValueType::CGSize, NonNull::from(&mut dimensions).cast())
        } {
            return Ok(None);
        }
        let bounds = Bounds {
            x: point.x,
            y: point.y,
            width: dimensions.width,
            height: dimensions.height,
        };
        Ok(valid_bounds(bounds).then_some(bounds))
    })
}

fn ax_actions(element: &AXUIElement, guard: &OperationGuard) -> Result<Vec<String>, AgentError> {
    ax_query(element, guard, |element, guard| {
        ax_timeout(element, guard)?;
        let mut pointer = std::ptr::null();
        // The action-name Copy call returns a retained CFString array.
        let result = unsafe { element.copy_action_names(NonNull::from(&mut pointer)) };
        let array = if result == AXError::Success {
            NonNull::new(pointer.cast_mut())
                .map(|pointer| unsafe { CFRetained::<CFArray>::from_raw(pointer) })
        } else {
            None
        };
        guard.check()?;
        if result == AXError::ActionUnsupported {
            return Ok(Vec::new());
        }
        ax_result(result)?;
        let Some(array) = array else {
            return Ok(Vec::new());
        };
        Ok(unsafe { array.cast_unchecked::<CFType>() }
            .iter()
            .take(16)
            .filter_map(|value| value.downcast::<CFString>().ok())
            .map(|value| bounded(&value.to_string()))
            .collect())
    })
}

fn ax_pid(element: &AXUIElement, guard: &OperationGuard) -> Result<i32, AgentError> {
    ax_timeout(element, guard)?;
    let mut pid = 0;
    // The public AX PID getter fills this live pid_t.
    ax_result(unsafe { element.pid(NonNull::from(&mut pid)) })?;
    guard.check()?;
    Ok(pid)
}

fn ax_root(
    target: &NativeTarget,
    guard: &OperationGuard,
) -> Result<CFRetained<AXUIElement>, AgentError> {
    guard.check()?;
    // This constructor creates an IPC proxy for the already revalidated PID.
    let app = unsafe { AXUIElement::new_application(target.app.process_id) };
    if target.app.process_id == std::process::id() as i32 {
        let app = AxElement(app.clone());
        ax_input(target.app.process_id, guard, move |guard| {
            ax_timeout(app.element(), guard)?;
            // The CEF runtime publishes its web tree after this AppKit accessibility signal.
            let _ = unsafe {
                app.element().set_attribute_value(
                    &CFString::from_str("AXEnhancedUserInterface"),
                    CFBoolean::new(true),
                )
            };
            guard.check()
        })?;
    }
    let mut matching = Vec::new();
    for window in ax_children(&app, "AXWindows", 64, guard)? {
        if ax_bounds(&window, guard)?
            .is_some_and(|bounds| bounds_match(bounds, target.window.bounds))
            && (target.window.title.is_empty()
                || ax_text(&window, "AXTitle", guard)?.as_deref()
                    == Some(target.window.title.as_str()))
        {
            matching.push(window);
        }
    }
    if matching.len() != 1 {
        return Err(error(
            "computer_window_ambiguous",
            "Não foi possível identificar uma única janela de acessibilidade para este alvo.",
        ));
    }
    matching.pop().ok_or_else(stale)
}

fn node_data(
    element: &AXUIElement,
    id: String,
    inherited_protection: bool,
    read_value: bool,
    guard: &OperationGuard,
) -> Result<ComputerElement, AgentError> {
    ax_query(element, guard, move |element, guard| {
        let role = ax_text(element, "AXRole", guard)?.unwrap_or_default();
        let subrole = ax_text(element, "AXSubrole", guard)?.unwrap_or_default();
        let protected =
            inherited_protection || role == "AXSecureTextField" || subrole == "AXSecureTextField";
        let title = ax_text(element, "AXTitle", guard)?
            .filter(|title| !title.is_empty())
            .or(ax_text(element, "AXDescription", guard)?)
            .unwrap_or_default();
        let value = if protected || !read_value {
            None
        } else {
            ax_text(element, "AXValue", guard)?
        };
        Ok(ComputerElement {
            id,
            role,
            title,
            value,
            bounds: ax_bounds(element, guard)?,
            enabled: ax_bool(element, "AXEnabled", guard)?.unwrap_or(false),
            protected,
            actions: ax_actions(element, guard)?,
        })
    })
}

fn snapshot_lock<'a>(
    mutex: &'a Mutex<HashMap<String, Snapshot>>,
    guard: &OperationGuard,
) -> Result<MutexGuard<'a, HashMap<String, Snapshot>>, AgentError> {
    loop {
        guard.check()?;
        match mutex.try_lock() {
            Ok(lock) => return Ok(lock),
            Err(TryLockError::Poisoned(_)) => {
                return Err(error(
                    "computer_state_unavailable",
                    "O estado nativo do computador não está disponível.",
                ))
            }
            Err(TryLockError::WouldBlock) => thread::sleep(Duration::from_millis(5)),
        }
    }
}

fn same_element(left: &AXUIElement, right: &AXUIElement) -> bool {
    left == right
}

fn node_in_window(
    node: &AXUIElement,
    root: &AXUIElement,
    guard: &OperationGuard,
) -> Result<bool, AgentError> {
    if same_element(node, root) {
        return Ok(true);
    }
    if let Some(window) = ax_element(node, "AXWindow", guard)? {
        return Ok(same_element(&window, root));
    }
    let mut current = node.retain();
    for _ in 0..MAX_DEPTH {
        let Some(parent) = ax_element(&current, "AXParent", guard)? else {
            return Ok(false);
        };
        if same_element(&parent, root) {
            return Ok(true);
        }
        current = parent;
    }
    Ok(false)
}

fn protected_ancestry(
    node: &AXUIElement,
    root: &AXUIElement,
    guard: &OperationGuard,
) -> Result<bool, AgentError> {
    let mut current = node.retain();
    for _ in 0..=MAX_DEPTH {
        if ax_text(&current, "AXRole", guard)?.as_deref() == Some("AXSecureTextField")
            || ax_text(&current, "AXSubrole", guard)?.as_deref() == Some("AXSecureTextField")
        {
            return Ok(true);
        }
        if same_element(&current, root) {
            return Ok(false);
        }
        current = ax_element(&current, "AXParent", guard)?.ok_or_else(stale)?;
    }
    Err(stale())
}

fn selected_node<'a>(
    snapshot: &'a Snapshot,
    id: &str,
    guard: &OperationGuard,
) -> Result<&'a AXUIElement, AgentError> {
    let root = input_root(snapshot)?;
    let (element, observed) = snapshot.nodes.get(id).ok_or_else(stale)?;
    validate_node(&element.0, observed, &snapshot.target, root, guard)?;
    Ok(&element.0)
}

fn validate_node(
    element: &AXUIElement,
    observed: &ComputerElement,
    target: &NativeTarget,
    root: &AXUIElement,
    guard: &OperationGuard,
) -> Result<(), AgentError> {
    if ax_pid(element, guard)? != target.app.process_id || !node_in_window(element, root, guard)? {
        return Err(stale());
    }
    let protected = protected_ancestry(element, root, guard)?;
    let current = node_data(element, observed.id.clone(), protected, false, guard)?;
    if current.role != observed.role
        || current.title != observed.title
        || current.bounds != observed.bounds
        || current.protected != observed.protected
        || !current.enabled
    {
        return Err(stale());
    }
    if current.protected {
        return Err(error(
            "computer_protected_field",
            "Campos protegidos não aceitam entrada automática.",
        ));
    }
    Ok(())
}

fn focused_window(
    target: &NativeTarget,
    root: &AXUIElement,
    guard: &OperationGuard,
) -> Result<bool, AgentError> {
    guard.check()?;
    if NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .is_none_or(|app| app.processIdentifier() != target.app.process_id)
    {
        return Ok(false);
    }
    if target.app.process_id == std::process::id() as i32 {
        let window_id = target.window.id;
        return ax_input(target.app.process_id, guard, move |_| {
            let mtm = MainThreadMarker::new().ok_or_else(stale)?;
            let app = NSApplication::sharedApplication(mtm);
            Ok(app.isActive()
                && app.keyWindow().is_some_and(|window| {
                    window.windowNumber() == isize::try_from(window_id).unwrap_or(-1)
                }))
        });
    }
    // Public system/application AX proxies are queried with bounded messaging timeouts.
    let system = unsafe { AXUIElement::new_system_wide() };
    let Some(app) = ax_element(&system, "AXFocusedApplication", guard)? else {
        return Ok(false);
    };
    if ax_pid(&app, guard)? != target.app.process_id {
        return Ok(false);
    }
    Ok(ax_element(&app, "AXFocusedWindow", guard)?
        .is_some_and(|window| same_element(&window, root)))
}

pub(super) fn browser_target_focused(
    target: &NativeTarget,
    guard: &OperationGuard,
) -> Result<bool, AgentError> {
    autoreleasepool(|_| {
        let root = ax_root(target, guard)?;
        focused_window(target, &root, guard)
    })
}

pub(super) fn browser_target_frontmost(
    target: &NativeTarget,
    guard: &OperationGuard,
) -> Result<bool, AgentError> {
    guard.check()?;
    let frontmost = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .is_some_and(|app| app.processIdentifier() == target.app.process_id);
    guard.check()?;
    Ok(frontmost)
}

enum AxMutation {
    Raise,
    Press,
    Focus,
    SelectedText(String),
}

impl AxMutation {
    fn before_dispatch(&self, guard: &OperationGuard) -> Result<(), AgentError> {
        match self {
            Self::Raise | Self::Focus => guard.before_focus(),
            Self::Press | Self::SelectedText(_) => guard.before_dispatch(),
        }
    }
}

fn ax_mutate(
    target: &NativeTarget,
    root: &AXUIElement,
    element: &AXUIElement,
    mutation: AxMutation,
    guard: &OperationGuard,
) -> Result<(), AgentError> {
    let process_id = target.app.process_id;
    let observed =
        if process_id == std::process::id() as i32 && !matches!(&mutation, AxMutation::Raise) {
            Some(node_data(
                element,
                String::new(),
                protected_ancestry(element, root, guard)?,
                false,
                guard,
            )?)
        } else {
            None
        };
    let target = target.clone();
    let root = AxElement(root.retain());
    let element = AxElement(element.retain());
    ax_input(process_id, guard, move |guard| {
        let root = root.element();
        let element = element.element();
        if matches!(&mutation, AxMutation::Raise) {
            session_check(guard)?;
            if current_window(&target, guard)?.bounds != target.window.bounds
                || ax_pid(root, guard)? != target.app.process_id
            {
                return Err(stale());
            }
            if !unsafe { AXIsProcessTrusted() } || !CGPreflightPostEventAccess() {
                return Err(error(
                    "computer_permission_required",
                    "Permita Acessibilidade e envio de entrada para controlar esta janela.",
                ));
            }
        } else {
            input_check(&target, root, guard)?;
            if let Some(observed) = observed.as_ref() {
                validate_node(element, observed, &target, root, guard)?;
            }
            if matches!(&mutation, AxMutation::SelectedText(_))
                && !same_element(&*focused_text(&target, root, guard)?, element)
            {
                return Err(stale());
            }
        }
        ax_timeout(element, guard)?;
        if matches!(&mutation, AxMutation::Raise) && process_id == std::process::id() as i32 {
            let mtm = MainThreadMarker::new().ok_or_else(stale)?;
            let windows = NSApplication::sharedApplication(mtm).windows();
            let window = windows
                .iter()
                .find(|window| {
                    window.windowNumber() == isize::try_from(target.window.id).unwrap_or(-1)
                })
                .ok_or_else(stale)?;
            // CEF's AX focus proxy can lag; own windows use their public AppKit identity on main.
            mutation.before_dispatch(guard)?;
            window.makeKeyAndOrderFront(None);
            return guard.check();
        }
        // Focus is a side effect, separate from the requested input receipt.
        mutation.before_dispatch(guard)?;
        if matches!(&mutation, AxMutation::Press) {
            indicate_element(element, guard, true);
        }
        ax_result(match mutation {
            AxMutation::Raise => unsafe { element.perform_action(&CFString::from_str("AXRaise")) },
            AxMutation::Press => unsafe { element.perform_action(&CFString::from_str("AXPress")) },
            AxMutation::Focus => {
                let value = unsafe { kCFBooleanTrue }.ok_or_else(stale)?;
                unsafe { element.set_attribute_value(&CFString::from_str("AXFocused"), value) }
            }
            AxMutation::SelectedText(text) => unsafe {
                element.set_attribute_value(
                    &CFString::from_str("AXSelectedText"),
                    &CFString::from_str(&text),
                )
            },
        })
    })
}

fn ax_input<T: Send + 'static>(
    process_id: i32,
    guard: &OperationGuard,
    input: impl FnOnce(&OperationGuard) -> Result<T, AgentError> + Send + 'static,
) -> Result<T, AgentError> {
    guard.check()?;
    let queued_guard = guard.clone();
    let run = move || {
        queued_guard.check()?;
        let result = input(&queued_guard)?;
        queued_guard.check()?;
        Ok(result)
    };
    if process_id != std::process::id() as i32 || MainThreadMarker::new().is_some() {
        return run();
    }
    let (sender, receiver) = mpsc::channel();
    // Same-process AX invokes AppKit synchronously; external AX remains bounded IPC.
    DispatchQueue::main().exec_async(move || {
        let result = autoreleasepool(|_| run());
        let _ = sender.send(result);
    });
    loop {
        guard.check()?;
        let wait = guard
            .deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(10));
        match receiver.recv_timeout(wait) {
            Ok(result) => {
                guard.check()?;
                return result;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(error(
                    "computer_main_thread_unavailable",
                    "Não foi possível executar a ação na thread principal. Observe o alvo novamente.",
                ));
            }
        }
    }
}

fn input_check(
    target: &NativeTarget,
    root: &AXUIElement,
    guard: &OperationGuard,
) -> Result<(), AgentError> {
    session_check(guard)?;
    if !unsafe { AXIsProcessTrusted() } || !CGPreflightPostEventAccess() {
        return Err(error(
            "computer_permission_required",
            "Permita Acessibilidade e envio de entrada para controlar esta janela.",
        ));
    }
    let window = current_window(target, guard)?;
    if window.bounds != target.window.bounds || !focused_window(target, root, guard)? {
        return Err(stale());
    }
    guard.check()
}

fn focus(
    target: &NativeTarget,
    root: &AXUIElement,
    guard: &OperationGuard,
) -> Result<(), AgentError> {
    session_check(guard)?;
    if current_window(target, guard)?.bounds != target.window.bounds {
        return Err(stale());
    }
    if !unsafe { AXIsProcessTrusted() } || !CGPreflightPostEventAccess() {
        return Err(error(
            "computer_permission_required",
            "Permita Acessibilidade e envio de entrada para controlar esta janela.",
        ));
    }
    if focused_window(target, root, guard)? {
        return Ok(());
    }
    let app = running_target_app(target, guard)?;
    guard.before_focus()?;
    if !app.activateWithOptions(NSApplicationActivationOptions::empty()) {
        return Err(error(
            "computer_focus_failed",
            "O macOS não confirmou a ativação do aplicativo.",
        ));
    }
    if ax_actions(root, guard)?
        .iter()
        .any(|action| action == "AXRaise")
    {
        ax_mutate(target, root, root, AxMutation::Raise, guard)?;
    }
    let limit = (Instant::now() + Duration::from_millis(800)).min(guard.deadline);
    while Instant::now() < limit {
        guard.check()?;
        if focused_window(target, root, guard)? {
            return input_check(target, root, guard);
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err(error(
        "computer_focus_failed",
        "A janela selecionada não recebeu foco. Observe o alvo novamente.",
    ))
}

fn hit_check(
    target: &NativeTarget,
    root: &AXUIElement,
    point: Point,
    guard: &OperationGuard,
) -> Result<(), AgentError> {
    if target.app.process_id == std::process::id() as i32 && MainThreadMarker::new().is_none() {
        let target = target.clone();
        let root = AxElement(root.retain());
        return ax_input(target.app.process_id, guard, move |guard| {
            hit_check(&target, root.element(), point, guard)
        });
    }
    input_check(target, root, guard)?;
    if !contains(target.window.bounds, point) {
        return Err(error(
            "computer_invalid_coordinates",
            "As coordenadas estão fora da janela observada.",
        ));
    }
    let element = hit_element(point, guard)?;
    if ax_pid(&element, guard)? != target.app.process_id || !node_in_window(&element, root, guard)?
    {
        return Err(stale());
    }
    Ok(())
}

fn hit_element(
    point: Point,
    guard: &OperationGuard,
) -> Result<CFRetained<AXUIElement>, AgentError> {
    // A system-wide hit may reach our preview even when the selected app is external.
    ax_input(std::process::id() as i32, guard, move |guard| {
        // Hit testing uses top-left screen coordinates, matching Quartz window bounds.
        let system = unsafe { AXUIElement::new_system_wide() };
        ax_timeout(&system, guard)?;
        let mut pointer = std::ptr::null();
        let result = unsafe {
            system.copy_element_at_position(
                point.x as f32,
                point.y as f32,
                NonNull::from(&mut pointer),
            )
        };
        let element = if result == AXError::Success {
            NonNull::new(pointer.cast_mut())
                .map(|pointer| unsafe { CFRetained::<AXUIElement>::from_raw(pointer) })
        } else {
            None
        };
        guard.check()?;
        ax_result(result)?;
        element.map(AxElement).ok_or_else(stale)
    })
    .map(|element| element.0)
}

fn image_point(observation: &Observation, x: f64, y: f64) -> Result<Point, AgentError> {
    let capture = observation.capture.as_ref().ok_or_else(|| {
        error(
            "computer_capture_required",
            "Capture a janela antes de usar coordenadas.",
        )
    })?;
    if !x.is_finite()
        || !y.is_finite()
        || x < 0.0
        || y < 0.0
        || x >= f64::from(capture.pixel_width)
        || y >= f64::from(capture.pixel_height)
        || !capture.scale_x.is_finite()
        || !capture.scale_y.is_finite()
        || capture.scale_x <= 0.0
        || capture.scale_y <= 0.0
    {
        return Err(error(
            "computer_invalid_coordinates",
            "As coordenadas não pertencem à captura observada.",
        ));
    }
    let point = Point {
        x: observation.window.bounds.x + x / capture.scale_x,
        y: observation.window.bounds.y + y / capture.scale_y,
    };
    if !contains(observation.window.bounds, point) {
        return Err(stale());
    }
    Ok(point)
}

fn event_source() -> Result<CFRetained<CGEventSource>, AgentError> {
    let source = CGEventSource::new(CGEventSourceStateID::Private).ok_or_else(|| {
        error(
            "computer_input_failed",
            "Não foi possível preparar a entrada do macOS.",
        )
    })?;
    CGEventSource::set_local_events_suppression_interval(Some(&source), 0.0);
    Ok(source)
}

fn mouse_event(
    source: &CGEventSource,
    kind: CGEventType,
    point: Point,
) -> Result<CFRetained<CGEvent>, AgentError> {
    CGEvent::new_mouse_event(
        Some(source),
        kind,
        CGPoint {
            x: point.x,
            y: point.y,
        },
        CGMouseButton::Left,
    )
    .ok_or_else(|| {
        error(
            "computer_input_failed",
            "Não foi possível preparar o evento de ponteiro.",
        )
    })
}

fn post(process_id: i32, event: &CGEvent, guard: &OperationGuard) -> Result<(), AgentError> {
    guard.before_dispatch()?;
    CGEvent::post_to_pid(process_id, Some(event));
    guard.check()
}

fn release(process_id: i32, event: &CGEvent, guard: &OperationGuard) {
    // Cleanup must release already-held input even after cancellation or lost focus.
    let _ = guard.check();
    CGEvent::post_to_pid(process_id, Some(event));
}

fn post_pair(
    process_id: i32,
    down: &CGEvent,
    up: &CGEvent,
    guard: &OperationGuard,
) -> Result<(), AgentError> {
    guard.before_dispatch()?;
    // Targeted delivery preserves the selected process boundary if global focus changes.
    CGEvent::post_to_pid(process_id, Some(down));
    release(process_id, up, guard);
    guard.check()
}

fn click(
    target: &NativeTarget,
    root: &AXUIElement,
    point: Point,
    guard: &OperationGuard,
) -> Result<(), AgentError> {
    focus(target, root, guard)?;
    hit_check(target, root, point, guard)?;
    super::cursor::click(point, guard);
    let source = event_source()?;
    let down = mouse_event(&source, CGEventType::LeftMouseDown, point)?;
    let up = mouse_event(&source, CGEventType::LeftMouseUp, point)?;
    post_pair(target.app.process_id, &down, &up, guard)
}

fn settable(element: &AXUIElement, key: &str, guard: &OperationGuard) -> Result<bool, AgentError> {
    let key = key.to_owned();
    ax_query(element, guard, move |element, guard| {
        ax_timeout(element, guard)?;
        let mut value = 0;
        // The Boolean output remains live for this synchronous property query.
        let result = unsafe {
            element.is_attribute_settable(&CFString::from_str(&key), NonNull::from(&mut value))
        };
        guard.check()?;
        if result == AXError::AttributeUnsupported {
            return Ok(false);
        }
        ax_result(result)?;
        Ok(value != 0)
    })
}

fn replace_selection(before: &str, range: CFRange, text: &str) -> Option<String> {
    let start = usize::try_from(range.location).ok()?;
    let end = start.checked_add(usize::try_from(range.length).ok()?)?;
    let utf16: Vec<_> = before.encode_utf16().collect();
    let prefix = String::from_utf16(utf16.get(..start)?).ok()?;
    let suffix = String::from_utf16(utf16.get(end..)?).ok()?;
    Some(format!("{prefix}{text}{suffix}"))
}

fn focused_text(
    target: &NativeTarget,
    root: &AXUIElement,
    guard: &OperationGuard,
) -> Result<CFRetained<AXUIElement>, AgentError> {
    input_check(target, root, guard)?;
    let app = unsafe { AXUIElement::new_application(target.app.process_id) };
    let element = ax_element(&app, "AXFocusedUIElement", guard)?.ok_or_else(stale)?;
    if !node_in_window(&element, root, guard)? {
        return Err(stale());
    }
    let protected = protected_ancestry(&element, root, guard)?;
    let data = node_data(&element, String::new(), protected, false, guard)?;
    if data.protected {
        return Err(error(
            "computer_protected_field",
            "Campos protegidos não aceitam entrada automática.",
        ));
    }
    if !data.enabled {
        return Err(stale());
    }
    Ok(element)
}

fn unicode_text(
    target: &NativeTarget,
    root: &AXUIElement,
    element: &AXUIElement,
    text: &str,
    guard: &OperationGuard,
) -> Result<(), AgentError> {
    let source = event_source()?;
    for character in text.chars() {
        input_check(target, root, guard)?;
        if !same_element(&*focused_text(target, root, guard)?, element) {
            return Err(stale());
        }
        let mut units = [0_u16; 2];
        let units = character.encode_utf16(&mut units);
        let down = CGEvent::new_keyboard_event(Some(&source), 0, true).ok_or_else(stale)?;
        let up = CGEvent::new_keyboard_event(Some(&source), 0, false).ok_or_else(stale)?;
        // These arrays contain a complete Unicode scalar, including its surrogate pair.
        unsafe {
            CGEvent::keyboard_set_unicode_string(Some(&down), units.len() as u64, units.as_ptr());
            CGEvent::keyboard_set_unicode_string(Some(&up), units.len() as u64, units.as_ptr());
        }
        post_pair(target.app.process_id, &down, &up, guard)?;
    }
    guard.check()
}

fn insert_text(
    target: &NativeTarget,
    snapshot: &Snapshot,
    id: Option<&str>,
    text: &str,
    guard: &OperationGuard,
) -> Result<(), AgentError> {
    if text.is_empty() || text.chars().count() > MAX_INPUT_TEXT {
        return Err(error(
            "computer_invalid_text",
            "O texto deve conter entre 1 e 8000 caracteres.",
        ));
    }
    let root = input_root(snapshot)?;
    let chosen = id
        .map(|id| selected_node(snapshot, id, guard))
        .transpose()?;
    focus(target, root, guard)?;
    if let Some(chosen) = chosen {
        if !ax_bool(chosen, "AXFocused", guard)?.unwrap_or(false) {
            if !settable(chosen, "AXFocused", guard)? {
                return Err(error(
                    "computer_focus_failed",
                    "O elemento selecionado não aceita foco de acessibilidade.",
                ));
            }
            input_check(target, root, guard)?;
            selected_node(snapshot, id.ok_or_else(stale)?, guard)?;
            ax_mutate(target, root, chosen, AxMutation::Focus, guard)?;
        }
    }
    let element = focused_text(target, root, guard)?;
    if chosen.is_some_and(|chosen| !same_element(chosen, &element)) {
        return Err(stale());
    }
    indicate_element(&element, guard, false);
    let role = ax_text(&element, "AXRole", guard)?.unwrap_or_default();
    if matches!(role.as_str(), "AXTextField" | "AXTextArea" | "AXComboBox")
        && settable(&element, "AXSelectedText", guard)?
    {
        let before = edit_value(&element, guard)?;
        let selection = selected_range(&element, guard)?;
        if let (Some(before), Some(range)) = (before, selection) {
            if let Some(expected) = replace_selection(&before, range, text) {
                input_check(target, root, guard)?;
                if !same_element(&*focused_text(target, root, guard)?, &element) {
                    return Err(stale());
                }
                ax_mutate(
                    target,
                    root,
                    &element,
                    AxMutation::SelectedText(text.to_owned()),
                    guard,
                )?;
                if edit_value(&element, guard)?.as_deref() != Some(expected.as_str()) {
                    return Err(error("computer_text_unconfirmed", "O macOS recebeu o texto, mas seu efeito completo não foi confirmado. Observe o alvo antes de repetir."));
                }
                return Ok(());
            }
        }
    }
    unicode_text(target, root, &element, text, guard)
}

fn indicate_element(element: &AXUIElement, guard: &OperationGuard, click: bool) {
    if let Ok(Some(bounds)) = ax_bounds(element, guard) {
        if valid_bounds(bounds) {
            let point = Point {
                x: bounds.x + bounds.width / 2.0,
                y: bounds.y + bounds.height / 2.0,
            };
            if click {
                super::cursor::click(point, guard);
            } else {
                super::cursor::show(point, guard);
            }
        }
    }
}

fn key_combo(key: &str) -> Result<(u16, CGEventFlags), AgentError> {
    let mut flags = CGEventFlags::empty();
    let parts: Vec<_> = key.split('+').map(str::trim).collect();
    if parts.is_empty() || parts.len() > 5 {
        return Err(error(
            "computer_invalid_key",
            "Combinação de teclas inválida.",
        ));
    }
    for modifier in &parts[..parts.len() - 1] {
        let flag = match modifier.to_ascii_lowercase().as_str() {
            "cmd" | "command" | "meta" => CGEventFlags::MaskCommand,
            "ctrl" | "control" => CGEventFlags::MaskControl,
            "alt" | "option" => CGEventFlags::MaskAlternate,
            "shift" => CGEventFlags::MaskShift,
            _ => {
                return Err(error(
                    "computer_invalid_key",
                    "Modificador de teclado não suportado.",
                ))
            }
        };
        if flags.contains(flag) {
            return Err(error(
                "computer_invalid_key",
                "O modificador de teclado está repetido.",
            ));
        }
        flags |= flag;
    }
    // shortcut: letter shortcuts use ANSI virtual keys; add input-source mapping for other layouts.
    let code = match parts[parts.len() - 1].to_ascii_lowercase().as_str() {
        "a" => 0,
        "s" => 1,
        "d" => 2,
        "f" => 3,
        "h" => 4,
        "g" => 5,
        "z" => 6,
        "x" => 7,
        "c" => 8,
        "v" => 9,
        "b" => 11,
        "q" => 12,
        "w" => 13,
        "e" => 14,
        "r" => 15,
        "y" => 16,
        "t" => 17,
        "1" => 18,
        "2" => 19,
        "3" => 20,
        "4" => 21,
        "6" => 22,
        "5" => 23,
        "9" => 25,
        "7" => 26,
        "8" => 28,
        "0" => 29,
        "o" => 31,
        "u" => 32,
        "i" => 34,
        "p" => 35,
        "enter" | "return" => 36,
        "l" => 37,
        "j" => 38,
        "k" => 40,
        "n" => 45,
        "m" => 46,
        "tab" => 48,
        "space" => 49,
        "backspace" => 51,
        "escape" | "esc" => 53,
        "home" => 115,
        "pageup" => 116,
        "delete" => 117,
        "end" => 119,
        "pagedown" => 121,
        "arrowleft" | "left" => 123,
        "arrowright" | "right" => 124,
        "arrowdown" | "down" => 125,
        "arrowup" | "up" => 126,
        _ => {
            return Err(error(
                "computer_invalid_key",
                "Tecla não suportada. Use texto para inserir caracteres.",
            ))
        }
    };
    Ok((code, flags))
}

fn capture_geometry(
    bounds: Bounds,
    width: u32,
    height: u32,
) -> Result<CaptureGeometry, AgentError> {
    if !valid_bounds(bounds)
        || width == 0
        || height == 0
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err(error(
            "computer_capture_failed",
            "A captura tem dimensões inválidas ou excede o limite de pixels.",
        ));
    }
    Ok(CaptureGeometry {
        pixel_width: width,
        pixel_height: height,
        scale_x: f64::from(width) / bounds.width,
        scale_y: f64::from(height) / bounds.height,
    })
}

fn screenshot_supported() -> bool {
    AnyClass::get(c"SCScreenshotManager").is_some()
}

fn native_error(error: *mut NSError) -> AgentError {
    let message = if error.is_null() {
        "O macOS não retornou uma imagem.".to_owned()
    } else {
        // Completion-handler NSError is alive throughout this callback.
        unsafe { &*error }.localizedDescription().to_string()
    };
    super::AgentError::new("computer_capture_failed", &bounded(&message))
}

fn encode_image(
    image: *const CGImage,
    guard: &OperationGuard,
) -> Result<(Vec<u8>, u32, u32), AgentError> {
    guard.check()?;
    let image = unsafe { image.as_ref() }.ok_or_else(|| {
        error(
            "computer_capture_failed",
            "O macOS retornou uma captura vazia.",
        )
    })?;
    let width = u32::try_from(CGImage::width(Some(image))).map_err(|_| stale())?;
    let height = u32::try_from(CGImage::height(Some(image))).map_err(|_| stale())?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(error(
            "computer_capture_failed",
            "A captura excedeu o limite de pixels.",
        ));
    }
    // The completion-handler's image remains valid while AppKit copies and encodes it.
    let bitmap = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), image);
    guard.check()?;
    let data = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }
    .ok_or_else(|| {
        error(
            "computer_capture_failed",
            "Não foi possível codificar a captura em PNG.",
        )
    })?;
    guard.check()?;
    Ok((data.to_vec(), width, height))
}

fn capture(
    target: &NativeTarget,
    guard: &OperationGuard,
) -> Result<(Vec<u8>, CaptureGeometry), AgentError> {
    guard.check()?;
    if !screenshot_supported() {
        return Err(error(
            "computer_capture_unsupported",
            "A captura nativa de janela requer macOS 14 ou mais recente.",
        ));
    }
    if !CGPreflightScreenCaptureAccess() {
        return Err(error(
            "computer_permission_required",
            "Permita Gravação de Tela para capturar a janela.",
        ));
    }
    let shareable = AnyClass::get(c"SCShareableContent").ok_or_else(stale)?;
    let target = target.clone();
    let guard = guard.clone();
    let waiting_guard = guard.clone();
    let (tx, rx) = mpsc::sync_channel(1);
    let callback = RcBlock::new(move |content: *mut AnyObject, cause: *mut NSError| {
        let result = autoreleasepool(|_| -> Result<(), AgentError> {
            guard.check()?;
            if content.is_null() {
                return Err(native_error(cause));
            }
            if current_window(&target, &guard)?.bounds != target.window.bounds {
                return Err(stale());
            }
            // ScreenCaptureKit owns content for this callback and exposes a public SCWindow array.
            let native_windows: Retained<NSArray<AnyObject>> =
                unsafe { msg_send![content, windows] };
            let window = native_windows
                .iter()
                .find(|window| {
                    let id: u32 = unsafe { msg_send![&**window, windowID] };
                    let app: Option<Retained<AnyObject>> =
                        unsafe { msg_send![&**window, owningApplication] };
                    id == target.window.id
                        && app.is_some_and(|app| {
                            let pid: i32 = unsafe { msg_send![&*app, processID] };
                            pid == target.app.process_id
                        })
                })
                .ok_or_else(stale)?;
            let filter_class = AnyClass::get(c"SCContentFilter").ok_or_else(stale)?;
            let config_class = AnyClass::get(c"SCStreamConfiguration").ok_or_else(stale)?;
            let manager = AnyClass::get(c"SCScreenshotManager").ok_or_else(stale)?;
            guard.check()?;
            // These are documented ScreenCaptureKit initializers/properties available on macOS 14.
            let allocated: objc2::rc::Allocated<AnyObject> =
                unsafe { msg_send![filter_class, alloc] };
            let filter: Retained<AnyObject> =
                unsafe { msg_send![allocated, initWithDesktopIndependentWindow: &*window] };
            let scale: f32 = unsafe { msg_send![&*filter, pointPixelScale] };
            let rect: CGRect = unsafe { msg_send![&*filter, contentRect] };
            if !scale.is_finite()
                || scale <= 0.0
                || !rect.size.width.is_finite()
                || !rect.size.height.is_finite()
                || (rect.size.width - target.window.bounds.width).abs() > 0.5
                || (rect.size.height - target.window.bounds.height).abs() > 0.5
            {
                return Err(stale());
            }
            let width = (target.window.bounds.width * f64::from(scale)).ceil() as u32;
            let height = (target.window.bounds.height * f64::from(scale)).ceil() as u32;
            capture_geometry(target.window.bounds, width, height)?;
            let config: Retained<AnyObject> = unsafe { msg_send![config_class, new] };
            unsafe {
                let _: () = msg_send![&*config, setWidth: width as usize];
                let _: () = msg_send![&*config, setHeight: height as usize];
                let _: () = msg_send![&*config, setShowsCursor: false];
                let _: () = msg_send![&*config, setIgnoreShadowsSingleWindow: true];
                let _: () = msg_send![&*config, setScalesToFit: false];
            }
            let tx = tx.clone();
            let target = target.clone();
            let capture_guard = guard.clone();
            let completion = RcBlock::new(move |image: *const CGImage, cause: *mut NSError| {
                let result = autoreleasepool(|_| {
                    if !cause.is_null() {
                        return Err(native_error(cause));
                    }
                    let (png, width, height) = encode_image(image, &capture_guard)?;
                    if current_window(&target, &capture_guard)?.bounds != target.window.bounds {
                        return Err(stale());
                    }
                    Ok((png, capture_geometry(target.window.bounds, width, height)?))
                });
                let _ = tx.try_send(result);
            });
            guard.check()?;
            unsafe {
                let _: () = msg_send![manager, captureImageWithFilter: &*filter, configuration: &*config, completionHandler: &*completion];
            }
            Ok(())
        });
        if let Err(error) = result {
            let _ = tx.try_send(Err(error));
        }
    });
    waiting_guard.check()?;
    unsafe {
        let _: () = msg_send![shareable, getShareableContentExcludingDesktopWindows: true, onScreenWindowsOnly: true, completionHandler: &*callback];
    }
    loop {
        waiting_guard.check()?;
        match rx.recv_timeout(Duration::from_millis(25)) {
            Ok(result) => {
                waiting_guard.check()?;
                return result;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(error(
                    "computer_capture_failed",
                    "O macOS encerrou a captura sem retornar uma imagem.",
                ))
            }
        }
    }
}

impl NativeBackend for MacBackend {
    fn capabilities(&self) -> Result<ComputerCapabilities, AgentError> {
        autoreleasepool(|_| {
            Ok(ComputerCapabilities {
                platform: "macos".into(),
                supported: true,
                accessibility: unsafe { AXIsProcessTrusted() },
                screen_capture: CGPreflightScreenCaptureAccess(),
                input_posting: CGPreflightPostEventAccess(),
                capture_supported: screenshot_supported(),
                session_status: if gui_session() {
                    "interactive"
                } else {
                    "inactive"
                }
                .into(),
                reason: (!screenshot_supported())
                    .then(|| "Captura de janela requer macOS 14 ou mais recente.".into()),
            })
        })
    }

    fn apps(&self, guard: &OperationGuard) -> Result<Vec<ComputerApp>, AgentError> {
        autoreleasepool(|_| {
            let records = windows(guard)?;
            guard.check()?;
            let running = NSWorkspace::sharedWorkspace().runningApplications();
            let mut apps = Vec::new();
            for app in running.iter().take(512) {
                guard.check()?;
                let Some(mut app) = app_record(&app, guard) else {
                    continue;
                };
                app.windows = records
                    .iter()
                    .filter(|window| window.pid == app.process_id && window.layer == 0)
                    .take(64)
                    .map(|window| window.window.clone())
                    .collect();
                if !app.windows.is_empty() {
                    apps.push(app);
                }
            }
            guard.check()?;
            Ok(apps)
        })
    }

    fn window(
        &self,
        target: &NativeTarget,
        guard: &OperationGuard,
    ) -> Result<ComputerWindow, AgentError> {
        autoreleasepool(|_| current_window(target, guard))
    }

    fn observe(
        &self,
        target: &NativeTarget,
        observation_id: &str,
        capture_image: bool,
        guard: &OperationGuard,
    ) -> Result<NativeObservation, AgentError> {
        autoreleasepool(|_| {
            guard.check()?;
            let mut snapshots = snapshot_lock(&self.snapshots, guard)?;
            snapshots.retain(|_, snapshot| snapshot.created.elapsed() < OBSERVATION_TTL);
            let window = current_window(target, guard)?;
            let target = NativeTarget {
                app: target.app.clone(),
                window: window.clone(),
            };
            let mut observation = NativeObservation {
                window,
                elements: Vec::new(),
                truncated: false,
                png: None,
                capture: None,
                errors: Vec::new(),
            };
            let mut observed_root = None;
            let mut nodes = HashMap::new();
            if unsafe { AXIsProcessTrusted() } {
                match ax_root(&target, guard) {
                    Ok(root) => {
                        let mut queue = VecDeque::from([(root.clone(), 0, false)]);
                        while let Some((element, depth, protection)) = queue.pop_front() {
                            guard.check()?;
                            if observation.elements.len() >= MAX_NODES {
                                observation.truncated = true;
                                break;
                            }
                            let id = format!("{observation_id}:{}", observation.elements.len());
                            let data =
                                match node_data(&element, id.clone(), protection, true, guard) {
                                    Ok(data) => data,
                                    Err(cause) => {
                                        guard.check()?;
                                        observation.errors.push(cause.message().to_owned());
                                        observation.truncated = true;
                                        break;
                                    }
                                };
                            let children = if depth < MAX_DEPTH {
                                partial_children(
                                    ax_children(&element, "AXChildren", MAX_CHILDREN, guard),
                                    &mut observation,
                                    guard,
                                )?
                            } else {
                                observation.truncated = true;
                                Vec::new()
                            };
                            if children.len() == MAX_CHILDREN {
                                observation.truncated = true;
                            }
                            queue.extend(
                                children
                                    .into_iter()
                                    .map(|child| (child, depth + 1, data.protected)),
                            );
                            nodes.insert(id, (AxElement(element), data.clone()));
                            observation.elements.push(data);
                        }
                        observed_root = Some(AxElement(root));
                    }
                    Err(cause) => {
                        guard.check()?;
                        observation.errors.push(cause.message().to_owned());
                    }
                }
            } else {
                observation
                    .errors
                    .push("Permita Acessibilidade para observar elementos da janela.".into());
            }
            if observed_root.is_none() {
                observation
                    .errors
                    .push(input_unavailable().message().to_owned());
            }
            if capture_image {
                match capture(&target, guard) {
                    Ok((png, geometry)) => {
                        observation.png = Some(png);
                        observation.capture = Some(geometry);
                    }
                    Err(cause) => {
                        guard.check()?;
                        observation.errors.push(cause.message().to_owned());
                    }
                }
            }
            guard.check()?;
            if current_window(&target, guard)?.bounds != target.window.bounds {
                snapshots.remove(observation_id);
                return Err(stale());
            }
            snapshots.retain(|_, snapshot| {
                snapshot.target.app.app_id != target.app.app_id
                    || snapshot.target.window.id != target.window.id
            });
            if snapshots.len() >= MAX_SNAPSHOTS {
                if let Some(oldest) = snapshots
                    .iter()
                    .min_by_key(|(_, snapshot)| snapshot.created)
                    .map(|(id, _)| id.clone())
                {
                    snapshots.remove(&oldest);
                }
            }
            snapshots.insert(
                observation_id.to_owned(),
                Snapshot {
                    target,
                    root: observed_root,
                    nodes,
                    created: Instant::now(),
                },
            );
            Ok(observation)
        })
    }

    fn act(
        &self,
        target: &NativeTarget,
        observation: &Observation,
        action: &ComputerAction,
        guard: &OperationGuard,
    ) -> Result<(), AgentError> {
        autoreleasepool(|_| {
            guard.check()?;
            let snapshots = snapshot_lock(&self.snapshots, guard)?;
            let snapshot = snapshots.get(&observation.id).ok_or_else(stale)?;
            if snapshot.created.elapsed() >= OBSERVATION_TTL
                || snapshot.target.app.app_id != target.app.app_id
                || snapshot.target.app.launch_id != target.app.launch_id
                || snapshot.target.app.identity != target.app.identity
                || snapshot.target.window.id != observation.window.id
                || snapshot.target.window.bounds != observation.window.bounds
            {
                return Err(stale());
            }
            let target = NativeTarget {
                app: target.app.clone(),
                window: observation.window.clone(),
            };
            if current_window(&target, guard)?.bounds != observation.window.bounds {
                return Err(stale());
            }
            let root = input_root(snapshot)?;
            match action {
                ComputerAction::Click {
                    element: Some(id), ..
                } => {
                    let element = selected_node(snapshot, id, guard)?;
                    if ax_actions(element, guard)?
                        .iter()
                        .any(|action| action == "AXPress")
                    {
                        focus(&target, root, guard)?;
                        let element = selected_node(snapshot, id, guard)?;
                        input_check(&target, root, guard)?;
                        return ax_mutate(&target, root, element, AxMutation::Press, guard);
                    }
                    let bounds = ax_bounds(element, guard)?.ok_or_else(stale)?;
                    click(
                        &target,
                        root,
                        Point {
                            x: bounds.x + bounds.width / 2.0,
                            y: bounds.y + bounds.height / 2.0,
                        },
                        guard,
                    )
                }
                ComputerAction::Click {
                    element: None,
                    x: Some(x),
                    y: Some(y),
                } => click(&target, root, image_point(observation, *x, *y)?, guard),
                ComputerAction::Click { .. } => Err(error(
                    "computer_invalid_arguments",
                    "Informe um elemento ou um par de coordenadas da captura.",
                )),
                ComputerAction::Type { text, element } => {
                    insert_text(&target, snapshot, element.as_deref(), text, guard)
                }
                ComputerAction::Press { key } => {
                    let (key, flags) = key_combo(key)?;
                    focus(&target, root, guard)?;
                    input_check(&target, root, guard)?;
                    let app = unsafe { AXUIElement::new_application(target.app.process_id) };
                    if let Ok(Some(element)) = ax_element(&app, "AXFocusedUIElement", guard) {
                        indicate_element(&element, guard, false);
                    }
                    let source = event_source()?;
                    let down =
                        CGEvent::new_keyboard_event(Some(&source), key, true).ok_or_else(stale)?;
                    let up =
                        CGEvent::new_keyboard_event(Some(&source), key, false).ok_or_else(stale)?;
                    CGEvent::set_flags(Some(&down), flags);
                    CGEvent::set_flags(Some(&up), CGEventFlags::empty());
                    post_pair(target.app.process_id, &down, &up, guard)
                }
                ComputerAction::Scroll {
                    delta_x,
                    delta_y,
                    x,
                    y,
                } => {
                    if !delta_x.is_finite()
                        || !delta_y.is_finite()
                        || delta_x.abs() > MAX_SCROLL
                        || delta_y.abs() > MAX_SCROLL
                    {
                        return Err(error(
                            "computer_invalid_arguments",
                            "A rolagem excede o limite permitido.",
                        ));
                    }
                    let point = image_point(observation, *x, *y)?;
                    focus(&target, root, guard)?;
                    hit_check(&target, root, point, guard)?;
                    super::cursor::show(point, guard);
                    let source = event_source()?;
                    let event = CGEvent::new_scroll_wheel_event2(
                        Some(&source),
                        CGScrollEventUnit::Pixel,
                        2,
                        -delta_y.round() as i32,
                        -delta_x.round() as i32,
                        0,
                    )
                    .ok_or_else(stale)?;
                    CGEvent::set_location(
                        Some(&event),
                        CGPoint {
                            x: point.x,
                            y: point.y,
                        },
                    );
                    post(target.app.process_id, &event, guard)
                }
                ComputerAction::Drag { from, to } => {
                    let from = image_point(observation, from.x, from.y)?;
                    let to = image_point(observation, to.x, to.y)?;
                    focus(&target, root, guard)?;
                    hit_check(&target, root, from, guard)?;
                    hit_check(&target, root, to, guard)?;
                    super::cursor::show(from, guard);
                    let source = event_source()?;
                    let down = mouse_event(&source, CGEventType::LeftMouseDown, from)?;
                    let up = mouse_event(&source, CGEventType::LeftMouseUp, from)?;
                    let mut last = from;
                    guard.before_dispatch()?;
                    CGEvent::post_to_pid(target.app.process_id, Some(&down));
                    let result: Result<(), AgentError> = (|| {
                        guard.check()?;
                        for step in 1..=16 {
                            let point = Point {
                                x: from.x + (to.x - from.x) * f64::from(step) / 16.0,
                                y: from.y + (to.y - from.y) * f64::from(step) / 16.0,
                            };
                            hit_check(&target, root, point, guard)?;
                            super::cursor::show(point, guard);
                            let event = mouse_event(&source, CGEventType::LeftMouseDragged, point)?;
                            guard.before_dispatch()?;
                            CGEvent::post_to_pid(target.app.process_id, Some(&event));
                            last = point;
                            guard.check()?;
                            thread::sleep(Duration::from_millis(10));
                        }
                        Ok(())
                    })();
                    CGEvent::set_location(
                        Some(&up),
                        CGPoint {
                            x: last.x,
                            y: last.y,
                        },
                    );
                    release(target.app.process_id, &up, guard);
                    result?;
                    guard.check()
                }
            }
        })
    }

    fn request_permission(
        &self,
        permission: ComputerPermission,
    ) -> Result<ComputerCapabilities, AgentError> {
        autoreleasepool(|_| {
            match permission {
                ComputerPermission::Accessibility => {
                    let key = CFString::from_str("AXTrustedCheckOptionPrompt");
                    let value = unsafe { kCFBooleanTrue }.ok_or_else(stale)?;
                    let options =
                        CFDictionary::<CFString, CFBoolean>::from_slices(&[&key], &[value]);
                    // This explicit user settings action alone permits showing the OS prompt.
                    unsafe { AXIsProcessTrustedWithOptions(Some(options.cast_unchecked())) };
                }
                ComputerPermission::ScreenCapture => {
                    CGRequestScreenCaptureAccess();
                }
                ComputerPermission::InputPosting => {
                    CGRequestPostEventAccess();
                }
            }
            let capabilities = self.capabilities()?;
            let granted = match permission {
                ComputerPermission::Accessibility => capabilities.accessibility,
                ComputerPermission::ScreenCapture => capabilities.screen_capture,
                ComputerPermission::InputPosting => capabilities.input_posting,
            };
            if !granted {
                // Previously denied requests may not show another prompt; open the relevant pane.
                let url =
                    NSURL::URLWithString(&NSString::from_str(permission_settings_url(permission)))
                        .ok_or_else(stale)?;
                if !NSWorkspace::sharedWorkspace().openURL(&url) {
                    return Err(error("computer_permission_settings", "Abra Ajustes do Sistema → Privacidade e Segurança para permitir o acesso do Jarvis."));
                }
            }
            Ok(capabilities)
        })
    }
}

fn permission_settings_url(permission: ComputerPermission) -> &'static str {
    match permission {
        ComputerPermission::ScreenCapture => "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_ScreenCapture",
        ComputerPermission::Accessibility | ComputerPermission::InputPosting => "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessibility_focus_only_failure_does_not_record_input() {
        for mutation in [AxMutation::Raise, AxMutation::Focus] {
            let guard = OperationGuard::new(Instant::now() + Duration::from_secs(1));
            let result: Result<(), AgentError> = (|| {
                mutation.before_dispatch(&guard)?;
                Err(stale())
            })();
            assert_eq!(result.unwrap_err().code, "computer_stale_target");
            assert!(!guard.dispatched.load(Ordering::SeqCst));
            assert!(!guard.cancel());
        }
    }

    #[test]
    fn cancelled_accessibility_focus_refuses_the_native_side_effect() {
        for mutation in [AxMutation::Raise, AxMutation::Focus] {
            let guard = OperationGuard::new(Instant::now() + Duration::from_secs(1));
            assert!(!guard.cancel());
            let result = mutation.before_dispatch(&guard);
            assert_eq!(result.unwrap_err().code, "computer_stopped");
            assert!(!guard.dispatched.load(Ordering::SeqCst));
        }
    }

    #[test]
    fn failed_accessibility_input_keeps_dispatch_for_recovery() {
        for mutation in [AxMutation::Press, AxMutation::SelectedText("Input".into())] {
            let guard = OperationGuard::new(Instant::now() + Duration::from_secs(1));
            let result: Result<(), AgentError> = (|| {
                mutation.before_dispatch(&guard)?;
                Err(stale())
            })();
            assert_eq!(result.unwrap_err().code, "computer_stale_target");
            assert!(guard.dispatched.load(Ordering::SeqCst));
            assert!(guard.cancel());
            assert_eq!(guard.check().unwrap_err().code, "computer_stopped");
        }
    }

    #[test]
    fn empty_accessibility_children_do_not_request_an_invalid_index() {
        let children = ax_child_page(0, 64, |_| -> Result<Vec<u8>, AgentError> {
            panic!("Index zero is invalid for an empty AX array")
        })
        .unwrap();
        assert!(children.is_empty());
    }

    #[test]
    fn accessibility_children_requests_respect_available_count_and_limit() {
        let children = ax_child_page(1, 64, |count| Ok(vec![count])).unwrap();
        assert_eq!(children, [1]);
        let children = ax_child_page(100, 64, |count| Ok(vec![count])).unwrap();
        assert_eq!(children, [64]);
        let result = ax_child_page(-1, 64, |_| -> Result<Vec<u8>, AgentError> {
            panic!("Invalid AX counts must not be dispatched")
        });
        assert_eq!(result.unwrap_err().code, "computer_stale_target");
    }

    #[test]
    fn self_input_waits_for_main_and_expires_without_dispatch_on_a_worker() {
        use std::sync::atomic::AtomicUsize;

        let guard = OperationGuard::new(Instant::now() + Duration::from_millis(25));
        let inputs = Arc::new(AtomicUsize::new(0));
        let calls = inputs.clone();
        let result = ax_input(std::process::id() as i32, &guard, move |guard| {
            guard.before_dispatch()?;
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        assert_eq!(result.unwrap_err().code, "computer_stopped");
        assert_eq!(inputs.load(Ordering::SeqCst), 0);
        assert!(!guard.dispatched.load(Ordering::SeqCst));
    }

    #[test]
    fn external_input_stays_on_the_worker_and_records_a_single_dispatch() {
        use std::sync::atomic::AtomicUsize;

        let guard = OperationGuard::new(Instant::now() + Duration::from_secs(1));
        let inputs = Arc::new(AtomicUsize::new(0));
        let calls = inputs.clone();
        let worker = thread::current().id();
        ax_input(-1, &guard, move |guard| {
            guard.before_dispatch()?;
            assert_eq!(thread::current().id(), worker);
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
        assert_eq!(inputs.load(Ordering::SeqCst), 1);
        assert!(guard.dispatched.load(Ordering::SeqCst));
    }

    #[test]
    fn focus_queries_return_the_native_state_without_recording_input() {
        let guard = OperationGuard::new(Instant::now() + Duration::from_secs(1));
        let worker = thread::current().id();
        let focused = ax_input(-1, &guard, move |_| {
            assert_eq!(thread::current().id(), worker);
            Ok(true)
        })
        .unwrap();
        assert!(focused);
        assert!(!guard.dispatched.load(Ordering::SeqCst));
        assert!(!guard.cancel());
        let result = ax_input(-1, &guard, |_| Ok(false));
        assert_eq!(result.unwrap_err().code, "computer_stopped");
    }

    #[test]
    fn self_accessibility_reads_wait_for_main_without_becoming_input() {
        use std::sync::atomic::AtomicUsize;

        let guard = OperationGuard::new(Instant::now() + Duration::from_millis(25));
        let app = unsafe { AXUIElement::new_application(std::process::id() as i32) };
        let calls = Arc::new(AtomicUsize::new(0));
        let queries = calls.clone();
        let result = ax_query(&app, &guard, move |_, _| {
            queries.fetch_add(1, Ordering::SeqCst);
            Ok(true)
        });
        assert_eq!(result.unwrap_err().code, "computer_stopped");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(!guard.dispatched.load(Ordering::SeqCst));
    }

    #[test]
    fn cancelling_self_input_while_queued_does_not_send_a_late_action() {
        use std::sync::atomic::AtomicUsize;

        let guard = OperationGuard::new(Instant::now() + Duration::from_secs(1));
        let cancelled = guard.clone();
        let stop = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            cancelled.cancel()
        });
        let inputs = Arc::new(AtomicUsize::new(0));
        let calls = inputs.clone();
        let result = ax_input(std::process::id() as i32, &guard, move |guard| {
            guard.before_dispatch()?;
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        assert!(!stop.join().unwrap());
        assert_eq!(result.unwrap_err().code, "computer_stopped");
        assert_eq!(inputs.load(Ordering::SeqCst), 0);
        assert!(!guard.dispatched.load(Ordering::SeqCst));
    }

    #[test]
    fn stopped_input_never_dispatches_even_when_the_app_is_jarvis() {
        let guard = OperationGuard::new(Instant::now() + Duration::from_secs(1));
        assert!(!guard.cancel());
        let result = ax_input(
            std::process::id() as i32,
            &guard,
            |_| -> Result<(), AgentError> {
                panic!("Stopped operations must not enter native input")
            },
        );
        assert_eq!(result.unwrap_err().code, "computer_stopped");
        assert!(!guard.dispatched.load(Ordering::SeqCst));
    }

    #[test]
    fn permission_shortcuts_open_the_relevant_privacy_pane() {
        assert!(permission_settings_url(ComputerPermission::Accessibility)
            .ends_with("Privacy_Accessibility"));
        assert_eq!(
            permission_settings_url(ComputerPermission::InputPosting),
            permission_settings_url(ComputerPermission::Accessibility)
        );
        assert!(permission_settings_url(ComputerPermission::ScreenCapture)
            .ends_with("Privacy_ScreenCapture"));
    }

    fn observation() -> Observation {
        Observation {
            id: "fixture".into(),
            target_id: "target".into(),
            generation: 1,
            revision: 1,
            window: ComputerWindow {
                id: 1,
                title: "Fixture".into(),
                bounds: Bounds {
                    x: -1920.0,
                    y: -400.0,
                    width: 800.0,
                    height: 600.0,
                },
            },
            elements: Vec::new(),
            truncated: false,
            capture: Some(CaptureGeometry {
                pixel_width: 1600,
                pixel_height: 1200,
                scale_x: 2.0,
                scale_y: 2.0,
            }),
            errors: Vec::new(),
        }
    }

    #[test]
    fn retina_pixels_map_to_negative_global_monitor_coordinates() {
        let point = image_point(&observation(), 800.0, 400.0).unwrap();
        assert_eq!((point.x, point.y), (-1520.0, -200.0));
    }

    #[test]
    fn image_coordinates_reject_edges_nonfinite_values_and_missing_capture() {
        let mut observation = observation();
        for (x, y) in [(1600.0, 0.0), (-1.0, 0.0), (0.0, 1200.0), (f64::NAN, 1.0)] {
            assert!(image_point(&observation, x, y).is_err());
        }
        observation.capture = None;
        assert!(image_point(&observation, 1.0, 1.0).is_err());
    }

    #[test]
    fn selected_text_insertion_uses_utf16_and_rejects_split_surrogates() {
        assert_eq!(
            replace_selection(
                "A😀Z",
                CFRange {
                    location: 1,
                    length: 2
                },
                "á"
            ),
            Some("AáZ".into())
        );
        assert!(replace_selection(
            "A😀Z",
            CFRange {
                location: 2,
                length: 0
            },
            "á"
        )
        .is_none());
        assert!(replace_selection(
            "abc",
            CFRange {
                location: -1,
                length: 1
            },
            "x"
        )
        .is_none());
    }

    #[test]
    fn key_combinations_allow_known_modifiers_and_reject_arbitrary_keys() {
        assert_eq!(
            key_combo("Cmd+Shift+A").unwrap(),
            (0, CGEventFlags::MaskCommand | CGEventFlags::MaskShift)
        );
        assert!(key_combo("Cmd+Cmd+A").is_err());
        assert!(key_combo("Fn+launch-other-app").is_err());
    }

    #[test]
    fn capture_geometry_uses_actual_dimensions_and_enforces_pixel_budget() {
        let bounds = observation().window.bounds;
        let geometry = capture_geometry(bounds, 800, 600).unwrap();
        assert_eq!((geometry.scale_x, geometry.scale_y), (1.0, 1.0));
        assert!(capture_geometry(bounds, 20_000, 20_000).is_err());
        assert!(capture_geometry(bounds, 0, 600).is_err());
    }

    #[test]
    fn unavailable_requires_complete_inventory_and_absent_window_identity() {
        let missing = WindowInventory {
            records: Vec::new(),
            identities: Vec::new(),
            complete: true,
        };
        assert_eq!(
            inventory_window(missing, 42, 1).unwrap_err().code,
            "computer_target_unavailable"
        );
        for (identities, complete) in [(Vec::new(), false), (vec![(42, 1)], true)] {
            let unreadable = WindowInventory {
                records: Vec::new(),
                identities,
                complete,
            };
            assert_eq!(
                inventory_window(unreadable, 42, 1).unwrap_err().code,
                "computer_stale_target"
            );
        }
        let valid = WindowInventory {
            records: vec![WindowRecord {
                pid: 42,
                layer: 0,
                window: observation().window,
            }],
            identities: vec![(42, 1)],
            complete: true,
        };
        assert_eq!(inventory_window(valid, 42, 1).unwrap().id, 1);
    }

    #[test]
    fn failed_child_reads_preserve_partial_observation_and_propagate_cancellation() {
        let fixture = observation();
        let bounds = fixture.window.bounds;
        let mut observed = NativeObservation {
            window: fixture.window,
            elements: vec![ComputerElement {
                id: "fixture:0".into(),
                role: "AXButton".into(),
                title: "Fixture button".into(),
                value: None,
                bounds: Some(bounds),
                enabled: true,
                protected: false,
                actions: vec!["AXPress".into()],
            }],
            truncated: false,
            png: Some(vec![1, 2, 3]),
            capture: fixture.capture,
            errors: Vec::new(),
        };
        let guard = OperationGuard::new(Instant::now() + Duration::from_secs(1));
        let result = partial_children(
            Err(error(
                "computer_accessibility_failed",
                "Children could not be read",
            )),
            &mut observed,
            &guard,
        );
        assert!(result.unwrap().is_empty());
        assert!(observed.truncated);
        assert_eq!(observed.errors, ["Children could not be read"]);
        assert_eq!(observed.elements[0].id, "fixture:0");
        assert_eq!(observed.png, Some(vec![1, 2, 3]));
        assert!(observed.capture.is_some());

        assert!(!guard.cancel());
        assert_eq!(
            partial_children(Ok(Vec::new()), &mut observed, &guard)
                .err()
                .unwrap()
                .code,
            "computer_stopped"
        );
    }

    #[test]
    fn visual_snapshot_without_ax_root_reports_input_unavailable() {
        let window = observation().window;
        let snapshot = Snapshot {
            target: NativeTarget {
                app: ComputerApp {
                    app_id: "fixture-app".into(),
                    name: "Fixture".into(),
                    identity: AppIdentity {
                        bundle_id: "test.fixture".into(),
                        bundle_path: "/fixture.app".into(),
                        executable_path: "/fixture.app/fixture".into(),
                    },
                    process_id: 42,
                    launch_id: "fixture-launch".into(),
                    windows: vec![window.clone()],
                },
                window,
            },
            root: None,
            nodes: HashMap::new(),
            created: Instant::now(),
        };
        let cause = input_root(&snapshot).err().unwrap();
        assert_eq!(cause.code, "computer_input_unavailable");
        assert!(cause.message().contains("somente para leitura"));
    }

    #[test]
    #[ignore = "Reads live macOS permissions and inventory; never prompts or sends input"]
    fn native_inventory_permission_smoke() {
        let backend = MacBackend::default();
        let capabilities = backend.capabilities().unwrap();
        println!(
            "platform={} supported={} accessibility={} screen_capture={} input_posting={} capture_supported={} session_status={}",
            capabilities.platform,
            capabilities.supported,
            capabilities.accessibility,
            capabilities.screen_capture,
            capabilities.input_posting,
            capabilities.capture_supported,
            capabilities.session_status,
        );
        println!("reason={}", capabilities.reason.as_deref().unwrap_or(""));
        let guard = OperationGuard::new(Instant::now() + Duration::from_secs(10));
        let apps = backend.apps(&guard).unwrap();
        println!(
            "inventory_apps={} inventory_windows={} input_dispatched={}",
            apps.len(),
            apps.iter().map(|app| app.windows.len()).sum::<usize>(),
            guard.dispatched.load(std::sync::atomic::Ordering::Acquire),
        );
        assert!(!guard.dispatched.load(std::sync::atomic::Ordering::Acquire));
    }
}
