//! Verifies that building and resolving styles performs no heap allocation.
//!
//! This lives in its own test binary so the counting allocator is not polluted
//! by unrelated tests running in parallel.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};

use gloss_core::{
    Ease, Prop, Radius, Shadow, Style, TextStyle, Theme, WidgetState, black, ms, soft,
};

struct Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: this allocator delegates every operation to `System` unchanged and
// only increments an atomic counter, so all `GlobalAlloc` contracts are upheld.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        // SAFETY: `layout` is forwarded unchanged to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` and `layout` come from the system allocator.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        // SAFETY: `ptr`, `layout` and `new_size` are forwarded unchanged.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        // SAFETY: `layout` is forwarded unchanged.
        unsafe { System.alloc_zeroed(layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn build_style(theme: &Theme) -> Style {
    Style::new()
        .radius(Radius::continuous(24.0))
        .tint(gloss_core::white(0.12))
        .border(1.0, gloss_core::white(0.25))
        .shadow([
            soft(0.0, 8.0, 24.0, black(0.25)),
            soft(0.0, 1.0, 2.0, black(0.4)),
            Shadow::NONE,
            Shadow::NONE,
        ])
        .text(TextStyle::new(theme.font_size))
        .hover(|s| s.lift(4.0).brighten(0.05))
        .pressed(|s| s.scale(0.98))
        .transition(Prop::SHADOW | Prop::TINT, ms(150.0), Ease::out())
}

#[test]
fn style_construction_and_resolution_allocate_nothing() {
    let theme = Theme::dark();

    // Warm up any lazily-initialised machinery before measuring.
    black_box(build_style(&theme).resolve(WidgetState::idle()));
    black_box(build_style(&theme).resolve(WidgetState {
        hovered: true,
        pressed: true,
        focused: true,
        disabled: false,
    }));

    let before = ALLOCATIONS.load(Ordering::SeqCst);
    for _ in 0..1_000 {
        let style = build_style(&theme);
        black_box(&style);
        black_box(style.resolve(WidgetState::idle()));
        black_box(style.resolve(WidgetState {
            hovered: true,
            pressed: false,
            focused: true,
            disabled: false,
        }));
    }
    let after = ALLOCATIONS.load(Ordering::SeqCst);
    assert_eq!(after, before, "style construction must not allocate");
}
