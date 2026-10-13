//! A bounded native preview of the selected window, independent of agent input.

use super::{Bounds, NativeTarget, Point};
use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchQueueAttr, DispatchRetained, DispatchTime};
use objc2::{
    define_class, extern_protocol, msg_send,
    rc::{autoreleasepool, Allocated, Retained},
    runtime::{AnyClass, AnyObject},
    AnyThread, DefinedClass, Encode, Encoding, MainThreadMarker, MainThreadOnly, RefEncode,
};
use objc2_app_kit::{
    NSBackingStoreType, NSBezierPath, NSColor, NSCompositingOperation, NSEvent, NSFont,
    NSGraphicsContext, NSImage, NSPanel, NSScreen, NSTextField, NSView, NSWindowCollectionBehavior,
    NSWindowStyleMask, NSWorkspace,
};
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::CGImage;
use objc2_foundation::{
    ns_string, NSArray, NSData, NSError, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString,
};
use std::{
    cell::RefCell,
    ptr::NonNull,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

#[link(name = "ScreenCaptureKit", kind = "framework")]
unsafe extern "C" {}
#[link(name = "CoreImage", kind = "framework")]
unsafe extern "C" {}
#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    fn CMSampleBufferIsValid(buffer: *const SampleBuffer) -> u8;
    fn CMSampleBufferDataIsReady(buffer: *const SampleBuffer) -> u8;
    fn CMSampleBufferGetImageBuffer(buffer: *const SampleBuffer) -> *mut PixelBuffer;
}

#[repr(C)]
struct SampleBuffer {
    _opaque: [u8; 0],
}
#[repr(C)]
struct PixelBuffer {
    _opaque: [u8; 0],
}
// SAFETY: These opaque reference encodings match CMSampleBuffer.h and CVPixelBuffer.h.
unsafe impl RefEncode for SampleBuffer {
    const ENCODING_REF: Encoding =
        Encoding::Pointer(&Encoding::Struct("opaqueCMSampleBuffer", &[]));
}
unsafe impl RefEncode for PixelBuffer {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("__CVBuffer", &[]));
}

const PANEL_SIZE: NSSize = NSSize {
    width: 336.0,
    height: 254.0,
};
const VIEWPORT: NSRect = NSRect {
    origin: NSPoint { x: 12.0, y: 54.0 },
    size: NSSize {
        width: 312.0,
        height: 176.0,
    },
};
static REQUEST: AtomicU64 = AtomicU64::new(0);
thread_local! {
    static ACTIVE: RefCell<Option<Preview>> = const { RefCell::new(None) };
    static IMAGE_CONTEXT: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FrameInterval {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}
// SAFETY: The public CMTime layout and Objective-C encoding come from CoreMedia's CMTime.h.
unsafe impl Encode for FrameInterval {
    const ENCODING: Encoding = Encoding::Struct(
        "CMTime",
        &[i64::ENCODING, i32::ENCODING, u32::ENCODING, i64::ENCODING],
    );
}

struct Frame(CFRetained<CGImage>);
// SAFETY: CGImage is immutable; only AppKit NSImage creation and drawing stay on the main thread.
unsafe impl Send for Frame {}

#[derive(Default)]
struct Mailbox<T> {
    latest: Option<T>,
    scheduled: bool,
}
impl<T> Mailbox<T> {
    fn put(&mut self, frame: T) -> bool {
        self.latest = Some(frame);
        let schedule = !self.scheduled;
        self.scheduled = true;
        schedule
    }
    fn take(&mut self) -> Option<T> {
        self.scheduled = false;
        self.latest.take()
    }
}

struct Shared {
    cancelled: Arc<AtomicBool>,
    retired: AtomicBool,
    failed: AtomicBool,
    frames: Mutex<Mailbox<Frame>>,
    size: NSSize,
}
impl Shared {
    fn current(&self) -> bool {
        !self.cancelled.load(Ordering::SeqCst) && !self.retired.load(Ordering::SeqCst)
    }
    fn capturing(&self) -> bool {
        self.current() && !self.failed.load(Ordering::SeqCst)
    }
}

extern_protocol!(
    /// # Safety
    /// Implementations must preserve ScreenCaptureKit's callback ABI and sample lifetime.
    // objc2's generated trait triggers this lint despite the safety contract above.
    #[allow(clippy::missing_safety_doc)]
    unsafe trait SCStreamOutput: NSObjectProtocol {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        unsafe fn output(&self, stream: &AnyObject, sample: *const SampleBuffer, kind: isize);
    }
);
extern_protocol!(
    /// # Safety
    /// Implementations must preserve ScreenCaptureKit's delegate ABI and error ownership.
    #[allow(clippy::missing_safety_doc)]
    unsafe trait SCStreamDelegate: NSObjectProtocol {
        #[optional]
        #[unsafe(method(stream:didStopWithError:))]
        fn failed(&self, stream: &AnyObject, error: &NSError);
    }
);

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "JarvisComputerPreviewOutput"]
    #[thread_kind = AnyThread]
    #[ivars = Arc<Shared>]
    struct StreamOutput;
    unsafe impl NSObjectProtocol for StreamOutput {}
    unsafe impl SCStreamOutput for StreamOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        unsafe fn output(&self, _stream: &AnyObject, sample: *const SampleBuffer, kind: isize) {
            let shared = self.ivars();
            if kind != 0 || sample.is_null() || !shared.capturing() { return; }
            // SAFETY: ScreenCaptureKit owns this sample for the duration of its output callback.
            if unsafe { CMSampleBufferIsValid(sample) == 0 || CMSampleBufferDataIsReady(sample) == 0 } { return; }
            let pixel = unsafe { CMSampleBufferGetImageBuffer(sample) };
            if pixel.is_null() { return; }
            let frame = autoreleasepool(|_| image_from_pixel_buffer(pixel));
            let Some(frame) = frame else { fallback(shared.clone()); return; };
            let schedule = shared.frames.lock().ok().map(|mut frames| frames.put(frame)).unwrap_or(false);
            if schedule {
                let shared = shared.clone();
                DispatchQueue::main().exec_async(move || deliver(shared));
            }
        }
    }
    unsafe impl SCStreamDelegate for StreamOutput {
        #[unsafe(method(stream:didStopWithError:))]
        fn failed(&self, _stream: &AnyObject, _error: &NSError) { fallback(self.ivars().clone()); }
    }
);

struct ViewState {
    observed: Retained<NSImage>,
    image: Retained<NSImage>,
    source: Bounds,
    cursor: Option<Point>,
    clicked: Option<Instant>,
    reduced_motion: bool,
}
define_class!(
    #[unsafe(super(NSView))]
    #[name = "JarvisComputerPreviewView"]
    #[thread_kind = MainThreadOnly]
    #[ivars = RefCell<ViewState>]
    struct PreviewView;
    impl PreviewView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool { true }
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool { true }
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> Option<&NSView> {
            let size = self.bounds().size;
            (point.x >= 0.0 && point.y >= 0.0 && point.x <= size.width && point.y <= size.height)
                .then_some(&**self)
        }
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if let Some(window) = self.window() { window.performWindowDragWithEvent(event); }
        }
        #[unsafe(method(drawRect:))]
        fn draw(&self, _rect: NSRect) {
            let surface = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(self.bounds(), 12.0, 12.0);
            NSColor::colorWithSRGBRed_green_blue_alpha(0.13, 0.14, 0.15, 0.96).setFill();
            surface.fill();
            NSColor::colorWithWhite_alpha(1.0, 0.12).setStroke();
            surface.setLineWidth(1.0);
            surface.stroke();
            let state = self.ivars().borrow();
            let rect = letterbox(state.image.size(), VIEWPORT);
            NSGraphicsContext::saveGraphicsState_class();
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(VIEWPORT, 6.0, 6.0).addClip();
            NSColor::blackColor().setFill();
            NSBezierPath::fillRect(VIEWPORT);
            // SAFETY: All NSImage use is main-thread-only; no hints dictionary is passed.
            unsafe { state.image.drawInRect_fromRect_operation_fraction_respectFlipped_hints(rect, NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)), NSCompositingOperation::SourceOver, 1.0, true, None); }
            if let Some(point) = state.cursor.and_then(|p| cursor_point(p, state.source, rect)) {
                if let Some(clicked) = state.clicked.filter(|time| time.elapsed() < click_duration(state.reduced_motion)) {
                    let progress = if state.reduced_motion { 0.0 } else { clicked.elapsed().as_secs_f64() / 0.55 };
                    let radius = if state.reduced_motion { 8.0 } else { 5.0 + progress * 12.0 };
                    NSColor::systemBlueColor().colorWithAlphaComponent(1.0 - progress).setStroke();
                    let ring = NSBezierPath::bezierPathWithOvalInRect(NSRect::new(NSPoint::new(point.x - radius, point.y - radius), NSSize::new(radius * 2.0, radius * 2.0)));
                    ring.setLineWidth(2.0);
                    ring.stroke();
                }
                let cursor = NSBezierPath::bezierPath();
                cursor.moveToPoint(point);
                for (x, y) in [(2.0, 15.0), (6.0, 10.0), (12.0, 9.0)] { cursor.lineToPoint(NSPoint::new(point.x + x, point.y + y)); }
                cursor.closePath();
                NSColor::systemBlueColor().setFill();
                cursor.fill();
                NSColor::whiteColor().setStroke();
                cursor.setLineWidth(1.0);
                cursor.stroke();
            }
            NSGraphicsContext::restoreGraphicsState_class();
        }
    }
);
define_class!(
    #[unsafe(super(NSPanel))]
    #[name = "JarvisComputerPreviewPanel"]
    #[thread_kind = MainThreadOnly]
    struct PreviewPanel;
    impl PreviewPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool { false }
        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main(&self) -> bool { false }
    }
);

struct Preview {
    panel: Retained<PreviewPanel>,
    view: Retained<PreviewView>,
    status: Retained<NSTextField>,
    target: NativeTarget,
    shared: Arc<Shared>,
    stream: Option<Retained<AnyObject>>,
    output: Option<Retained<StreamOutput>>,
    queue: Option<DispatchRetained<DispatchQueue>>,
}
impl Preview {
    fn stop_stream(&mut self) {
        if let Some(stream) = self.stream.take() {
            let output = self.output.take();
            let queue = self.queue.take();
            let stopping = stream.clone();
            let completion = RcBlock::new(move |_error: *mut NSError| {
                let _keep_alive_until_stopped = (&stopping, &output, &queue);
            });
            // SAFETY: SCStream's public asynchronous stop retains its completion block.
            unsafe {
                let _: () = msg_send![&*stream, stopCaptureWithCompletionHandler: &*completion];
            }
        }
        self.queue = None;
    }
    fn retire(&mut self) {
        self.shared.retired.store(true, Ordering::SeqCst);
        self.stop_stream();
        self.panel.orderOut(None);
        if let Ok(mut frames) = self.shared.frames.lock() {
            frames.take();
        }
    }
}

fn letterbox(size: NSSize, viewport: NSRect) -> NSRect {
    if !size.width.is_finite()
        || !size.height.is_finite()
        || size.width <= 0.0
        || size.height <= 0.0
    {
        return viewport;
    }
    let scale = (viewport.size.width / size.width).min(viewport.size.height / size.height);
    let size = NSSize::new(size.width * scale, size.height * scale);
    NSRect::new(
        NSPoint::new(
            viewport.origin.x + (viewport.size.width - size.width) / 2.0,
            viewport.origin.y + (viewport.size.height - size.height) / 2.0,
        ),
        size,
    )
}
fn cursor_point(point: Point, source: Bounds, viewport: NSRect) -> Option<NSPoint> {
    let x = (point.x - source.x) / source.width;
    let y = (point.y - source.y) / source.height;
    (x.is_finite() && y.is_finite() && (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)).then(
        || {
            NSPoint::new(
                viewport.origin.x + x * viewport.size.width,
                viewport.origin.y + y * viewport.size.height,
            )
        },
    )
}
fn stream_size(bounds: Bounds) -> Option<NSSize> {
    if !bounds.x.is_finite()
        || !bounds.y.is_finite()
        || !bounds.width.is_finite()
        || !bounds.height.is_finite()
        || bounds.width <= 0.0
        || bounds.height <= 0.0
    {
        return None;
    }
    let scale = (640.0 / bounds.width).min(420.0 / bounds.height).min(1.0);
    Some(NSSize::new(
        (bounds.width * scale).round().max(1.0),
        (bounds.height * scale).round().max(1.0),
    ))
}
fn click_duration(reduced_motion: bool) -> Duration {
    Duration::from_millis(if reduced_motion { 200 } else { 550 })
}
fn label(text: &str, rect: NSRect, font: f64, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setFrame(rect);
    label.setFont(Some(&NSFont::systemFontOfSize(font)));
    label.setTextColor(Some(&NSColor::colorWithWhite_alpha(1.0, 0.85)));
    label
}
fn panel_origin(target: Bounds, mtm: MainThreadMarker) -> NSPoint {
    let screens = NSScreen::screens(mtm);
    let Some(primary) = screens.firstObject() else {
        return NSPoint::new(16.0, 16.0);
    };
    let primary_top = primary.frame().origin.y + primary.frame().size.height;
    let center = NSPoint::new(
        target.x + target.width / 2.0,
        primary_top - target.y - target.height / 2.0,
    );
    let screen = screens
        .iter()
        .find(|screen| {
            let frame = screen.frame();
            center.x >= frame.origin.x
                && center.x <= frame.origin.x + frame.size.width
                && center.y >= frame.origin.y
                && center.y <= frame.origin.y + frame.size.height
        })
        .unwrap_or(primary);
    let frame = screen.visibleFrame();
    NSPoint::new(
        frame.origin.x + frame.size.width - PANEL_SIZE.width - 16.0,
        frame.origin.y + frame.size.height - PANEL_SIZE.height - 16.0,
    )
}

fn create(
    target: NativeTarget,
    image: Retained<NSImage>,
    shared: Arc<Shared>,
    mtm: MainThreadMarker,
) -> Preview {
    let rect = NSRect::new(NSPoint::new(0.0, 0.0), PANEL_SIZE);
    // SAFETY: All AppKit allocation, ownership, and designated initialization stay on its main thread.
    let panel: Retained<PreviewPanel> = unsafe {
        msg_send![super(PreviewPanel::alloc(mtm).set_ivars(())), initWithContentRect: rect, styleMask: NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel, backing: NSBackingStoreType::Buffered, defer: false]
    };
    panel.setTitle(ns_string!("Visão do Jarvis"));
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(true);
    panel.setMovable(true);
    panel.setMovableByWindowBackground(true);
    panel.setHidesOnDeactivate(false);
    panel.setLevel(28);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    unsafe {
        panel.setReleasedWhenClosed(false);
    }
    let state = RefCell::new(ViewState {
        observed: image.clone(),
        image,
        source: target.window.bounds,
        cursor: None,
        clicked: None,
        reduced_motion: NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion(),
    });
    let view: Retained<PreviewView> =
        unsafe { msg_send![super(PreviewView::alloc(mtm).set_ivars(state)), initWithFrame: rect] };
    view.addSubview(&label(
        "Jarvis está vendo",
        NSRect::new(NSPoint::new(13.0, 9.0), NSSize::new(310.0, 18.0)),
        12.0,
        mtm,
    ));
    view.addSubview(&label(
        &format!("{} · {}", target.app.name, target.window.title),
        NSRect::new(NSPoint::new(13.0, 29.0), NSSize::new(310.0, 16.0)),
        10.0,
        mtm,
    ));
    let status = label(
        "Observado agora",
        NSRect::new(NSPoint::new(13.0, 234.0), NSSize::new(310.0, 16.0)),
        10.0,
        mtm,
    );
    view.addSubview(&status);
    panel.setContentView(Some(&view));
    panel.setFrameOrigin(panel_origin(target.window.bounds, mtm));
    panel.orderFrontRegardless();
    Preview {
        panel,
        view,
        status,
        target,
        shared,
        stream: None,
        output: None,
        queue: None,
    }
}

fn image_from_pixel_buffer(pixel: *mut PixelBuffer) -> Option<Frame> {
    let image_class = AnyClass::get(c"CIImage")?;
    let context_class = AnyClass::get(c"CIContext")?;
    IMAGE_CONTEXT.with_borrow_mut(|slot| {
        if slot.is_none() {
            *slot = unsafe {
                msg_send![context_class, contextWithOptions: std::ptr::null::<AnyObject>()]
            };
        }
        let context = slot.as_ref()?;
        // SAFETY: CoreImage reads this live CVPixelBuffer only inside the ScreenCaptureKit callback.
        let image: Option<Retained<AnyObject>> =
            unsafe { msg_send![image_class, imageWithCVPixelBuffer: pixel] };
        let image = image?;
        let extent: NSRect = unsafe { msg_send![&*image, extent] };
        let result: *mut CGImage =
            unsafe { msg_send![&**context, createCGImage: &*image, fromRect: extent] };
        // SAFETY: createCGImage follows Core Foundation's create rule and returns an immutable image.
        NonNull::new(result).map(|raw| Frame(unsafe { CFRetained::from_raw(raw) }))
    })
}

fn deliver(shared: Arc<Shared>) {
    let frame = shared
        .frames
        .lock()
        .ok()
        .and_then(|mut frames| frames.take());
    let Some(frame) = frame else {
        return;
    };
    if !shared.capturing() {
        return;
    }
    if MainThreadMarker::new().is_none() {
        return;
    }
    ACTIVE.with_borrow(|slot| {
        if let Some(active) = slot
            .as_ref()
            .filter(|active| Arc::ptr_eq(&active.shared, &shared))
        {
            let image = NSImage::initWithCGImage_size(NSImage::alloc(), &frame.0, shared.size);
            active.view.ivars().borrow_mut().image = image;
            active.status.setStringValue(ns_string!("Prévia ao vivo"));
            active.view.setNeedsDisplay(true);
        }
    });
}

fn fallback(shared: Arc<Shared>) {
    if !shared.current() || shared.failed.swap(true, Ordering::SeqCst) {
        return;
    }
    DispatchQueue::main().exec_async(move || {
        if !shared.current() {
            return;
        }
        ACTIVE.with_borrow_mut(|slot| {
            if let Some(active) = slot
                .as_mut()
                .filter(|active| Arc::ptr_eq(&active.shared, &shared))
            {
                active.stop_stream();
                let mut state = active.view.ivars().borrow_mut();
                state.image = state.observed.clone();
                drop(state);
                active.status.setStringValue(ns_string!("Última captura"));
                active.view.setNeedsDisplay(true);
            }
        });
    });
}

fn attach_stream(target: NativeTarget, shared: Arc<Shared>) {
    let Some(content_class) = AnyClass::get(c"SCShareableContent") else {
        fallback(shared);
        return;
    };
    let callback_shared = shared.clone();
    let callback = RcBlock::new(move |content: *mut AnyObject, error: *mut NSError| {
        let shared = callback_shared.clone();
        if !shared.current() {
            return;
        }
        if content.is_null() || !error.is_null() {
            fallback(shared);
            return;
        }
        let window = autoreleasepool(|_| {
            // SAFETY: SCShareableContent owns this window list during its completion callback.
            let windows: Retained<NSArray<AnyObject>> = unsafe { msg_send![content, windows] };
            windows.iter().find(|window| {
                let id: u32 = unsafe { msg_send![&**window, windowID] };
                let owner: Option<Retained<AnyObject>> =
                    unsafe { msg_send![&**window, owningApplication] };
                let pid = owner.map(|owner| unsafe {
                    let pid: i32 = msg_send![&*owner, processID];
                    pid
                });
                id == target.window.id && pid == Some(target.app.process_id)
            })
        });
        let Some(window) = window else {
            fallback(shared);
            return;
        };
        // SCWindow is immutable metadata; retain it across the hop to AppKit's thread.
        let window = SendObject(window);
        DispatchQueue::main().exec_async(move || build_stream(window.into_inner(), shared));
    });
    // SAFETY: This public API retains the callback; enumeration never requests additional consent.
    unsafe {
        let _: () = msg_send![content_class, getShareableContentExcludingDesktopWindows: true, onScreenWindowsOnly: true, completionHandler: &*callback];
    }
}

struct SendObject(Retained<AnyObject>);
// SAFETY: Used only for immutable SCWindow metadata, which ScreenCaptureKit shares across queues.
unsafe impl Send for SendObject {}
impl SendObject {
    fn into_inner(self) -> Retained<AnyObject> {
        self.0
    }
}

fn build_stream(window: Retained<AnyObject>, shared: Arc<Shared>) {
    if !shared.current() {
        return;
    }
    let built = autoreleasepool(|_| {
        let filter_class = AnyClass::get(c"SCContentFilter")?;
        let config_class = AnyClass::get(c"SCStreamConfiguration")?;
        let stream_class = AnyClass::get(c"SCStream")?;
        // SAFETY: The selected SCWindow and the public ScreenCaptureKit designated initializers match the SDK.
        let filter_alloc: Allocated<AnyObject> = unsafe { msg_send![filter_class, alloc] };
        let filter: Option<Retained<AnyObject>> =
            unsafe { msg_send![filter_alloc, initWithDesktopIndependentWindow: &*window] };
        let filter = filter?;
        let config: Retained<AnyObject> = unsafe { msg_send![config_class, new] };
        unsafe {
            let _: () = msg_send![&*config, setWidth: shared.size.width as usize];
            let _: () = msg_send![&*config, setHeight: shared.size.height as usize];
            let _: () = msg_send![&*config, setPixelFormat: 0x4247_5241_u32];
            let _: () = msg_send![&*config, setShowsCursor: false];
            let _: () = msg_send![&*config, setIgnoreShadowsSingleWindow: true];
            let _: () = msg_send![&*config, setScalesToFit: true];
            let _: () = msg_send![&*config, setPreservesAspectRatio: true];
            let _: () = msg_send![&*config, setQueueDepth: 3_isize];
            let _: () = msg_send![&*config, setCapturesAudio: false];
            let _: () = msg_send![&*config, setMinimumFrameInterval: FrameInterval { value: 1, timescale: 60, flags: 1, epoch: 0 }];
        }
        let output: Retained<StreamOutput> =
            unsafe { msg_send![super(StreamOutput::alloc().set_ivars(shared.clone())), init] };
        let stream_alloc: Allocated<AnyObject> = unsafe { msg_send![stream_class, alloc] };
        let stream: Option<Retained<AnyObject>> = unsafe {
            msg_send![stream_alloc, initWithFilter: &*filter, configuration: &*config, delegate: &*output]
        };
        let stream = stream?;
        let queue = DispatchQueue::new(
            "com.foxtag.jarvis.computer-preview",
            DispatchQueueAttr::SERIAL,
        );
        let mut error: *mut NSError = std::ptr::null_mut();
        let queue_ptr = DispatchRetained::as_ptr(&queue)
            .as_ptr()
            .cast::<AnyObject>();
        let added: bool = unsafe {
            msg_send![&*stream, addStreamOutput: &*output, type: 0_isize, sampleHandlerQueue: queue_ptr, error: &mut error]
        };
        added.then_some((stream, output, queue))
    });
    let Some((stream, output, queue)) = built else {
        fallback(shared);
        return;
    };
    ACTIVE.with_borrow_mut(|slot| {
        let Some(active) = slot
            .as_mut()
            .filter(|active| Arc::ptr_eq(&active.shared, &shared))
        else {
            return;
        };
        active.stream = Some(stream.clone());
        active.output = Some(output);
        active.queue = Some(queue);
        let completion = RcBlock::new(move |error: *mut NSError| {
            if !error.is_null() {
                fallback(shared.clone());
            }
        });
        unsafe {
            let _: () = msg_send![&*stream, startCaptureWithCompletionHandler: &*completion];
        }
    });
}

fn watch_cancel(shared: Arc<Shared>) {
    let Ok(when) = DispatchTime::try_from(Duration::from_millis(200)) else {
        return;
    };
    let _ = DispatchQueue::main().after(when, move || {
        if shared.retired.load(Ordering::SeqCst) {
            return;
        }
        if shared.cancelled.load(Ordering::SeqCst) {
            hide_if_cancelled(&shared.cancelled);
        } else {
            watch_cancel(shared);
        }
    });
}

pub(super) fn start(target: NativeTarget, png: Vec<u8>, cancelled: Arc<AtomicBool>) {
    if cancelled.load(Ordering::SeqCst) || png.len() > 20 * 1024 * 1024 {
        return;
    }
    let Some(size) = stream_size(target.window.bounds) else {
        return;
    };
    let request = REQUEST.fetch_add(1, Ordering::SeqCst) + 1;
    DispatchQueue::main().exec_async(move || {
        if REQUEST.load(Ordering::SeqCst) != request || cancelled.load(Ordering::SeqCst) {
            return;
        }
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let data = NSData::with_bytes(&png);
        let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
            return;
        };
        let mut created = None;
        ACTIVE.with_borrow_mut(|slot| {
            if let Some(active) = slot.as_mut().filter(|active| {
                Arc::ptr_eq(&active.shared.cancelled, &cancelled)
                    && active.shared.capturing()
                    && active.target.app.process_id == target.app.process_id
                    && active.target.app.launch_id == target.app.launch_id
                    && active.target.window.id == target.window.id
                    && active.shared.size == size
            }) {
                let mut state = active.view.ivars().borrow_mut();
                state.observed = image.clone();
                state.image = image;
                state.source = target.window.bounds;
                drop(state);
                active.target = target;
                active.status.setStringValue(ns_string!("Observado agora"));
                active.view.setNeedsDisplay(true);
                return;
            }
            let previous_origin = slot.as_ref().map(|active| active.panel.frame().origin);
            if let Some(mut previous) = slot.take() {
                previous.retire();
            }
            let shared = Arc::new(Shared {
                cancelled,
                retired: AtomicBool::new(false),
                failed: AtomicBool::new(false),
                frames: Mutex::new(Mailbox {
                    latest: None,
                    scheduled: false,
                }),
                size,
            });
            created = Some((target.clone(), shared.clone()));
            let preview = create(target, image, shared, mtm);
            if let Some(origin) = previous_origin {
                preview.panel.setFrameOrigin(origin);
            }
            *slot = Some(preview);
        });
        if let Some((target, shared)) = created {
            attach_stream(target, shared.clone());
            watch_cancel(shared);
        }
    });
}

pub(super) fn update_cursor(point: Point, click: bool, cancelled: &Arc<AtomicBool>) {
    if !point.x.is_finite() || !point.y.is_finite() {
        return;
    }
    let cancelled = cancelled.clone();
    DispatchQueue::main().exec_async(move || {
        ACTIVE.with_borrow(|slot| {
            if let Some(active) = slot.as_ref().filter(|active| {
                active.shared.current() && Arc::ptr_eq(&active.shared.cancelled, &cancelled)
            }) {
                let mut state = active.view.ivars().borrow_mut();
                state.cursor = Some(point);
                let clicked = Instant::now();
                let reduced_motion =
                    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion();
                state.reduced_motion = reduced_motion;
                if click {
                    state.clicked = Some(clicked);
                }
                drop(state);
                active.view.setNeedsDisplay(true);
                if click {
                    pulse(active.shared.clone(), clicked, reduced_motion);
                }
            }
        });
    });
}
fn pulse(shared: Arc<Shared>, clicked: Instant, reduced_motion: bool) {
    let delay = if reduced_motion {
        click_duration(true)
    } else {
        Duration::from_millis(16)
    };
    let Ok(when) = DispatchTime::try_from(delay) else {
        return;
    };
    let _ = DispatchQueue::main().after(when, move || {
        if !shared.current() {
            return;
        }
        ACTIVE.with_borrow(|slot| {
            if let Some(active) = slot
                .as_ref()
                .filter(|active| Arc::ptr_eq(&active.shared, &shared))
            {
                if active.view.ivars().borrow().clicked == Some(clicked) {
                    active.view.setNeedsDisplay(true);
                    if !reduced_motion && clicked.elapsed() < click_duration(false) {
                        pulse(shared.clone(), clicked, false);
                    }
                }
            }
        });
    });
}

pub(super) fn hide_if_cancelled(cancelled: &Arc<AtomicBool>) {
    let cancelled = cancelled.clone();
    DispatchQueue::main().exec_async(move || {
        ACTIVE.with_borrow_mut(|slot| {
            if slot.as_ref().is_some_and(|active| {
                Arc::ptr_eq(&active.shared.cancelled, &cancelled)
                    && cancelled.load(Ordering::SeqCst)
            }) {
                if let Some(mut active) = slot.take() {
                    active.retire();
                }
            }
        });
    });
}
pub(super) fn hide() {
    REQUEST.fetch_add(1, Ordering::SeqCst);
    DispatchQueue::main().exec_async(|| {
        ACTIVE.with_borrow_mut(|slot| {
            if let Some(mut active) = slot.take() {
                active.retire();
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mailbox_keeps_only_newest_frame_and_schedules_one_delivery() {
        let mut frames = Mailbox::default();
        assert!(frames.put(1));
        assert!(!frames.put(2));
        assert!(!frames.put(3));
        assert_eq!(frames.take(), Some(3));
        assert!(frames.put(4));
        assert_eq!(frames.take(), Some(4));
        assert_eq!(frames.take(), None);
    }
    #[test]
    fn thumbnail_preserves_aspect_and_maps_global_cursor_into_letterbox() {
        let source = Bounds {
            x: -1800.0,
            y: -600.0,
            width: 1600.0,
            height: 900.0,
        };
        let rect = letterbox(NSSize::new(1600.0, 900.0), VIEWPORT);
        assert!((rect.size.width / rect.size.height - 16.0 / 9.0).abs() < 0.00001);
        let center = cursor_point(
            Point {
                x: -1000.0,
                y: -150.0,
            },
            source,
            rect,
        )
        .unwrap();
        assert!((center.x - 168.0).abs() < 0.00001);
        assert!((center.y - 142.0).abs() < 0.00001);
        assert!(cursor_point(Point { x: 0.0, y: 0.0 }, source, rect).is_none());
        assert!(cursor_point(
            Point {
                x: f64::NAN,
                y: 0.0
            },
            source,
            rect
        )
        .is_none());
    }
    #[test]
    fn streaming_dimensions_are_bounded_and_reject_invalid_geometry() {
        let mut bounds = Bounds {
            x: 0.0,
            y: 0.0,
            width: 3200.0,
            height: 1800.0,
        };
        assert_eq!(stream_size(bounds), Some(NSSize::new(640.0, 360.0)));
        bounds.width = 1000.0;
        bounds.height = 8000.0;
        let size = stream_size(bounds).unwrap();
        assert!(size.width <= 640.0 && size.height <= 420.0);
        bounds.height = 0.0;
        assert!(stream_size(bounds).is_none());
        bounds.height = f64::INFINITY;
        assert!(stream_size(bounds).is_none());
    }
    #[test]
    fn retired_and_cancelled_owners_reject_late_frames() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let shared = Shared {
            cancelled: cancelled.clone(),
            retired: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            frames: Mutex::new(Mailbox {
                latest: None,
                scheduled: false,
            }),
            size: NSSize::new(640.0, 360.0),
        };
        assert!(shared.capturing());
        shared.failed.store(true, Ordering::SeqCst);
        assert!(shared.current());
        assert!(!shared.capturing());
        shared.failed.store(false, Ordering::SeqCst);
        shared.retired.store(true, Ordering::SeqCst);
        assert!(!shared.current());
        shared.retired.store(false, Ordering::SeqCst);
        cancelled.store(true, Ordering::SeqCst);
        assert!(!shared.current());
        assert!(!Arc::ptr_eq(&cancelled, &Arc::new(AtomicBool::new(true))));
    }
    #[test]
    fn reduced_motion_uses_one_short_static_click_indicator() {
        assert_eq!(click_duration(true), Duration::from_millis(200));
        assert_eq!(click_duration(false), Duration::from_millis(550));
    }
}
