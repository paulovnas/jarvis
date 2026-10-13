//! A click-through native pointer, independent of the user's physical cursor.
use super::{OperationGuard, Point};
use dispatch2::{DispatchQueue, DispatchTime};
use objc2::{define_class, msg_send, rc::Retained, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSBezierPath, NSColor, NSFont, NSPanel, NSScreen, NSTextField, NSView,
    NSWindowCollectionBehavior, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{ns_string, NSPoint, NSRect, NSSize};
use std::{
    cell::{Cell, RefCell},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const SIZE: NSSize = NSSize {
    width: 96.0,
    height: 84.0,
};
const TIP: NSPoint = NSPoint { x: 24.0, y: 24.0 };
const PULSE: Duration = Duration::from_millis(450);
static REVISION: AtomicU64 = AtomicU64::new(0);
thread_local! { static PANEL: RefCell<Option<Panel>> = const { RefCell::new(None) }; }

define_class!(
    #[unsafe(super(NSPanel))]
    #[name = "JarvisAgentCursorPanel"]
    struct CursorPanel;
    impl CursorPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool { false }
        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main(&self) -> bool { false }
    }
);
struct Drawing {
    clicked: Cell<Option<Instant>>,
    reduced_motion: bool,
}

fn pulse_shape(elapsed: Duration, reduced_motion: bool) -> Option<(f64, f64)> {
    let progress = elapsed.as_secs_f64() / PULSE.as_secs_f64();
    (progress < 1.0).then_some({
        if reduced_motion {
            (12.0, 0.6)
        } else {
            (8.0 + 14.0 * progress, 0.7 * (1.0 - progress))
        }
    })
}

define_class!(
    #[unsafe(super(NSView))]
    #[name = "JarvisAgentCursorView"]
    #[ivars = Drawing]
    struct CursorView;
    impl CursorView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool { true }
        #[unsafe(method(drawRect:))]
        fn draw(&self, _rect: NSRect) {
            if let Some((radius, alpha)) = self.ivars().clicked.get().and_then(|time| pulse_shape(time.elapsed(), self.ivars().reduced_motion)) {
                let ring = NSBezierPath::bezierPathWithOvalInRect(NSRect::new(NSPoint::new(TIP.x - radius, TIP.y - radius), NSSize::new(radius * 2.0, radius * 2.0)));
                NSColor::systemBlueColor().colorWithAlphaComponent(alpha).setStroke();
                ring.setLineWidth(2.5);
                ring.stroke();
            }
            let path = NSBezierPath::bezierPath();
            path.moveToPoint(TIP);
            for (x, y) in [(2.0, 26.0), (9.0, 18.0), (17.0, 16.0)] { path.lineToPoint(NSPoint::new(TIP.x + x, TIP.y + y)); }
            path.closePath();
            NSColor::blackColor().colorWithAlphaComponent(0.2).setStroke();
            path.setLineWidth(5.0);
            path.stroke();
            NSColor::systemBlueColor().setFill();
            path.fill();
            NSColor::whiteColor().setStroke();
            path.setLineWidth(1.4);
            path.stroke();
            let badge = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(NSRect::new(NSPoint::new(40.0, 52.0), NSSize::new(44.0, 18.0)), 7.0, 7.0);
            NSColor::systemBlueColor().setFill();
            badge.fill();
        }
    }
);
struct Panel {
    panel: Retained<CursorPanel>,
    view: Retained<CursorView>,
    cancelled: Option<Arc<AtomicBool>>,
}

fn origin(point: Point, primary_top: f64) -> NSPoint {
    NSPoint::new(point.x - TIP.x, primary_top - point.y - SIZE.height + TIP.y)
}
fn create(mtm: MainThreadMarker) -> Panel {
    let rect = NSRect::new(NSPoint::new(0.0, 0.0), SIZE);
    // SAFETY: AppKit allocation, drawing and ownership stay on its main thread.
    let panel: Retained<CursorPanel> = unsafe {
        msg_send![super(CursorPanel::alloc(mtm).set_ivars(())), initWithContentRect: rect, styleMask: NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel, backing: NSBackingStoreType::Buffered, defer: false]
    };
    panel.setTitle(ns_string!("Cursor do Jarvis"));
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(false);
    panel.setIgnoresMouseEvents(true);
    panel.setHidesOnDeactivate(false);
    panel.setLevel(29);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    unsafe {
        panel.setReleasedWhenClosed(false);
    }
    let reduced_motion = NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion();
    let view: Retained<CursorView> = unsafe {
        msg_send![super(CursorView::alloc(mtm).set_ivars(Drawing { clicked: Cell::new(None), reduced_motion })), initWithFrame: rect]
    };
    let label = NSTextField::labelWithString(ns_string!("Jarvis"), mtm);
    label.setFrame(NSRect::new(
        NSPoint::new(46.0, 53.0),
        NSSize::new(37.0, 15.0),
    ));
    label.setFont(Some(&NSFont::systemFontOfSize(10.0)));
    label.setTextColor(Some(&NSColor::whiteColor()));
    view.addSubview(&label);
    panel.setContentView(Some(&view));
    Panel {
        panel,
        view,
        cancelled: None,
    }
}

pub(super) fn show(point: Point, guard: &OperationGuard) {
    indicate(point, false, guard);
}
pub(super) fn click(point: Point, guard: &OperationGuard) {
    indicate(point, true, guard);
}
fn indicate(point: Point, click: bool, guard: &OperationGuard) {
    if !point.x.is_finite() || !point.y.is_finite() || guard.check().is_err() {
        return;
    }
    let revision = REVISION.fetch_add(1, Ordering::SeqCst) + 1;
    let guard = guard.clone();
    DispatchQueue::main().exec_async(move || {
        if REVISION.load(Ordering::SeqCst) != revision
            || guard
                .feedback
                .as_ref()
                .is_some_and(|token| token.load(Ordering::SeqCst))
            || guard.check().is_err() && !guard.dispatched.load(Ordering::SeqCst)
        {
            return;
        }
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let screens = NSScreen::screens(mtm);
        let Some(primary) = screens.firstObject() else {
            return;
        };
        PANEL.with_borrow_mut(|slot| {
            let state = slot.get_or_insert_with(|| create(mtm));
            state.cancelled = guard.feedback.clone();
            state.panel.setFrameOrigin(origin(
                point,
                primary.frame().origin.y + primary.frame().size.height,
            ));
            state.view.ivars().clicked.set(click.then(Instant::now));
            state.view.setNeedsDisplay(true);
            state.panel.orderFrontRegardless();
        });
        if let Some(token) = &guard.feedback {
            super::preview::update_cursor(point, click, token);
        }
        if click {
            animate_pulse(revision);
        }
        // A turn-scoped cursor stays visible between actions; standalone probes have a bounded lifetime.
        if guard.feedback.is_none() {
            if let Ok(when) = DispatchTime::try_from(Duration::from_secs(8)) {
                let _ = DispatchQueue::main().after(when, move || {
                    if REVISION.load(Ordering::SeqCst) == revision {
                        hide_on_main();
                    }
                });
            }
        }
    });
}
fn animate_pulse(revision: u64) {
    let interval = PANEL.with_borrow(|slot| {
        if slot
            .as_ref()
            .is_some_and(|state| state.view.ivars().reduced_motion)
        {
            PULSE
        } else {
            Duration::from_millis(16)
        }
    });
    if let Ok(when) = DispatchTime::try_from(interval) {
        let _ = DispatchQueue::main().after(when, move || {
            if REVISION.load(Ordering::SeqCst) != revision {
                return;
            }
            PANEL.with_borrow(|slot| {
                if let Some(state) = slot {
                    state.view.setNeedsDisplay(true);
                    if state
                        .view
                        .ivars()
                        .clicked
                        .get()
                        .is_some_and(|time| time.elapsed() < PULSE)
                    {
                        animate_pulse(revision);
                    }
                }
            });
        });
    }
}
fn hide_on_main() {
    PANEL.with_borrow(|slot| {
        if let Some(state) = slot {
            state.panel.orderOut(None);
        }
    });
}
pub(super) fn hide_if_cancelled(cancelled: &Arc<AtomicBool>) {
    let cancelled = cancelled.clone();
    DispatchQueue::main().exec_async(move || {
        PANEL.with_borrow(|slot| {
            if slot.as_ref().is_some_and(|state| {
                state
                    .cancelled
                    .as_ref()
                    .is_some_and(|token| Arc::ptr_eq(token, &cancelled))
                    && cancelled.load(Ordering::SeqCst)
            }) {
                REVISION.fetch_add(1, Ordering::SeqCst);
                hide_on_main();
            }
        });
    });
}
pub(super) fn hide() {
    REVISION.fetch_add(1, Ordering::SeqCst);
    DispatchQueue::main().exec_async(hide_on_main);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_tip_matches_quartz_points_on_primary_negative_and_upper_displays() {
        for (point, expected) in [
            (Point { x: 100.0, y: 40.0 }, NSPoint::new(76.0, 800.0)),
            (
                Point {
                    x: -1200.0,
                    y: 30.0,
                },
                NSPoint::new(-1224.0, 810.0),
            ),
            (Point { x: 10.0, y: -400.0 }, NSPoint::new(-14.0, 1240.0)),
        ] {
            assert_eq!(origin(point, 900.0), expected);
        }
    }
    #[test]
    fn click_feedback_expires_and_respects_reduced_motion() {
        let early = pulse_shape(Duration::from_millis(10), false).unwrap();
        let later = pulse_shape(Duration::from_millis(200), false).unwrap();
        assert!(later.0 > early.0 && later.1 < early.1);
        assert_eq!(
            pulse_shape(Duration::from_millis(10), true),
            pulse_shape(Duration::from_millis(200), true)
        );
        assert!(pulse_shape(PULSE, false).is_none());
        assert!(pulse_shape(PULSE, true).is_none());
    }
}
