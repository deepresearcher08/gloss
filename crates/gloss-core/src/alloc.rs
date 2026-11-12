//! Optional allocation counting.
//!
//! Install [`CountingAllocator`] as a binary's global allocator to make the
//! debug overlay's allocation counter meaningful:
//!
//! ```
//! #[global_allocator]
//! static GLOBAL: gloss_core::alloc::CountingAllocator =
//!     gloss_core::alloc::CountingAllocator;
//! ```
//!
//! The counter is process-global and monotonically increasing; call
//! [`reset_allocations`] to restart it for a measurement window.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

/// A global allocator that forwards to [`System`] while counting allocations.
pub struct CountingAllocator;

// SAFETY: every method forwards directly to `System`, preserving its contract.
// The only added work is an atomic increment, which cannot observe or alter the
// allocated memory.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: `layout` is forwarded unchanged from the caller.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` and `layout` are forwarded unchanged from the caller.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: `layout` is forwarded unchanged from the caller.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: all arguments are forwarded unchanged from the caller.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// Total allocations observed since process start or the last reset.
#[inline]
pub fn allocations() -> usize {
    ALLOCATIONS.load(Ordering::Relaxed)
}

/// Resets the allocation counter to zero.
#[inline]
pub fn reset_allocations() {
    ALLOCATIONS.store(0, Ordering::Relaxed);
}

/// Measures allocations performed inside a scope.
pub struct AllocGuard {
    start: usize,
}

impl AllocGuard {
    /// Starts a measurement.
    #[inline]
    pub fn new() -> Self {
        Self {
            start: allocations(),
        }
    }

    /// Allocations observed since the guard was created.
    #[inline]
    pub fn allocations(&self) -> usize {
        allocations().saturating_sub(self.start)
    }
}

impl Default for AllocGuard {
    fn default() -> Self {
        Self::new()
    }
}
