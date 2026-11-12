# Decisions

## Seeded decisions (spec section 11)

- **D1** Retained tree + signals, view function runs once.
- **D2** Hover/pressed/focused/disabled are sparse `StyleDelta`s, closures evaluated at build time.
- **D3** `cosmic-text` is the text stack; no `parley` in v0.1.
- **D4** Custom SDF pipeline for boxes; no `vello` in v0.1.
- **D5** Scene capture defaults to `Rgba8UnormSrgb` + dithering; `Rgba16Float` opt-in.
- **D6** Scaled subtrees draw text from the nearest atlas size with scaled quads.
- **D7** Golden images are stored per `(os, backend)`; software adapters are the default.
- **D8** Performance numbers are targets; actuals are reported in `BENCH.md`.
- **D9** Zero-allocation rule applies only to Gloss-owned code, measured with a scoped guard.
- **D10** sRGB only in v0.1; wide-gamut later behind a feature.

## Toolchain and pinned versions

- Rust edition 2024, stable toolchain.
- MSRV: **1.85** (from `workspace.package.rust-version`).
- Resolved versions in `Cargo.lock` at the time of writing:
  - `winit` 0.30.13
  - `wgpu` 30.0.1
  - `taffy` 0.15.1
  - `cosmic-text` 0.19.0
  - `window-vibrancy` 0.8.1
  - `raw-window-handle` 0.6.2
  - `glam` 0.34.3
  - `bytemuck` 1.25.2
  - `pollster` 1.0.1
  - `image` 0.25.10
  - `thiserror` 2.0.21
- The winit 0.30 / wgpu 30 / window-vibrancy 0.8 combination agrees on
  `raw-window-handle` 0.6.2.

## Implementation decisions

- **D11 Widget construction API.** Widgets are plain [`Node`] constructors that
  return a node value; the application inserts them with `Document::add`. Widget
  state that participates in accessibility or interaction (`checked`, `value`,
  `min_value`, `max_value`) lives directly on `Node`, and optional handlers live
  in `Handlers`. This keeps the retained tree inspectable and avoids hidden
  widget objects.
- **D12 Accessibility hooks.** Every node has `role: Role` and
  `label: Option<Arc<str>>`. Labels, buttons and toggles populate the label from
  their text automatically; `.with_label(...)` overrides or supplies one.
- **D13 Toggle/slider painting.** Controls skip the generic base quad. The
  toggle draws a rounded track plus knob and places its label after a fixed left
  pad. The slider draws a neutral track, an accent progress fill and a knob.
  Accent color comes from the resolved fill; the neutral track is derived from
  the accent's luminance. All control quads reuse the node's inherited clip and
  rounded clip radius.
- **D14 Vibrancy fallback.** `App::vibrancy` always enables a transparent
  window. On macOS the requested material is mapped to
  `window_vibrancy::NSVisualEffectMaterial`; on other platforms the request is a
  no-op and the solid window background remains the fallback. `transparent(true)`
  without a vibrancy request keeps the previous macOS `HudWindow` behavior.
- **D15 Examples live at `examples/` in the workspace root.** They are declared
  as path-based `[[example]]` targets on the `gloss` facade package so
  `cargo check --workspace --all-targets` and `cargo run --example <name>` build
  them against the real public API.
- **D16 Golden test adapter selection.** Without `--features gpu-tests`, the
  harness prefers a fallback/software adapter and falls back to the default
  adapter when none exists. With `gpu-tests`, the default adapter is used. The
  chosen backend is written into the golden path, so a machine that switches
  backends gets a missing-golden failure rather than a false mismatch.
- **D17 Hot reload (M9).** Implemented with `hot-lib-reloader` 0.8.2.
  `crates/gloss-styles` builds as both `rlib` and `cdylib`: the host depends on
  the `rlib` for the `StyleSet` type and `gloss-app::HotReload<T>` loads the
  `cdylib` and resolves the exported `gloss_styles` symbol. `App::hot_reload`
  polls the watched library every 200 ms from `about_to_wait` (switching the
  loop to `ControlFlow::WaitUntil` while active) and hands the freshly built
  `StyleSet` to an apply closure together with the running `Document`. Applying
  through `Document::set_style` and `Node::text_style` replaces styles in place,
  so signal values and node identities survive a reload. A rebuild watcher
  (`cargo watch -w crates/gloss-styles -x "build -p gloss-styles"`) produces the
  dylib; `subsecond` was rejected because it targets Dioxus component patching
  rather than a standalone styles crate. The style ABI is the `fn() -> StyleSet`
  symbol plus the `gloss-core` layout shared by the host and the library.
- **D18 Glyph atlas eviction.** The atlas is shelf-packed into a 2048x2048 R8
  texture and keyed by `(font, size, glyph, subpixel bucket)`. Each entry records
  the frame it was last used and the rasterized coverage. When an insert cannot
  fit, the oldest 25% of entries (at least one) are dropped and the atlas is
  fully repacked in one pass. This keeps the hot set resident without per-frame
  allocation or a second texture.
- **D19 Structural bindings.** `Dynamic<T>` is either a plain value or a derived
  closure tracked by the signal runtime. `Document::show` owns a conditional
  subtree: it builds on first truth, keeps the subtree alive while true and
  disposes it (including subscriptions) when false. `Document::for_each` owns a
  parent's child order and reconciles keyed rows with a `HashMap`, reusing nodes
  and their bindings when the key is unchanged.
- **D20 Timing and stats.** `Renderer` requests timestamp-query features when
  the adapter exposes them and records begin/end timestamps around the render
  pass, resolving them asynchronously. When timestamps are unavailable it
  reports CPU submit-to-done time from `queue.on_submitted_work_done`. The same
  pass counts draw calls, instances, glass layers and blur passes into
  `RenderStats`.
- **D21 Debug overlay and allocation counting.** `App::debug(true)` (or F1 at
  runtime) paints a small panel with frame/CPU/GPU milliseconds, the timing
  source, draw/instance/glass/blur counts, atlas occupancy, glyph count and
  process-wide allocations. `gloss_core::alloc::CountingAllocator` is a
  ready-to-install `GlobalAlloc` wrapper; examples install it so the overlay can
  show allocations without affecting library code.

