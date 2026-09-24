//! Test-only, thread-local allocation accounting for synchronous preparation.
//! Counts allocation requests and cumulative requested bytes, not retained heap.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Counts {
    pub allocations: u64,
    pub bytes: u64,
}

thread_local! {
    static ACTIVE: Cell<Option<Counts>> = const { Cell::new(None) };
}

struct Allocator;

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

fn record(bytes: usize) {
    // TLS may already be gone when a test thread is being destroyed. Recording
    // never allocates and never changes the allocator's result.
    let _ = ACTIVE.try_with(|active| {
        if let Some(mut counts) = active.get() {
            counts.allocations = counts.allocations.saturating_add(1);
            counts.bytes = counts.bytes.saturating_add(bytes as u64);
            active.set(Some(counts));
        }
    });
}

// SAFETY: every operation forwards the original pointer/layout to System;
// accounting only changes a thread-local Cell and cannot allocate or unwind.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        unsafe { System.realloc(ptr, layout, size) }
    }
}

pub(super) fn measure<T>(operation: impl FnOnce() -> T) -> (T, Counts) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ACTIVE.with(|active| active.set(None));
        }
    }
    ACTIVE.with(|active| {
        assert!(active.get().is_none(), "allocation probes cannot nest");
        active.set(Some(Counts::default()));
    });
    let reset = Reset;
    let value = operation();
    let counts = ACTIVE.with(|active| active.get().unwrap());
    drop(reset);
    (value, counts)
}
