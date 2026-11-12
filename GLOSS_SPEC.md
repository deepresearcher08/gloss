# Gloss — Spec for Coding Agent (v2)

Gloss is a Rust GUI framework whose purpose is **macOS-style visual design**: glassmorphism, Liquid Glass (refraction + specular), neumorphism, claymorphism, continuous (squircle) corners, soft multi-layer shadows. Styles are written as **plain Rust modules** (typed, composable, no stylesheet language). Rendering is GPU-first via wgpu. Speed and low overhead are core features.

Gloss is not a general-purpose toolkit clone. Visual quality and styling ergonomics come first. Widget breadth comes second.

**Scope of "web-style":** Gloss implements a *subset of CSS semantics* (box model, flex/grid, gradients, shadows, radii, transforms, filters, backdrop blur, transitions, clipping, text). There is no DOM, no JS, no cascade, no HTML/CSS compatibility goal.

**What changed from v1:** retained-tree + signals architecture is now explicit (new M5); `Style` hover/pressed variants are sparse deltas instead of closures stored in data; text stack is `cosmic-text` (it includes the rasterizer); the dependency list is complete and version-pinned; HiDPI, clipping/scroll, surface recovery, nested glass, GPU-less golden tests, and timestamp-query fallbacks are specified; the zero-allocation rule is scoped to Gloss-owned code; performance numbers are targets to report, not hard gates.

---

## 1. Operating rules for the agent

1. Work **one milestone at a time** (Section 9). Do not start milestone N+1 until N's acceptance criteria pass.
2. After each milestone: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, then commit with message `M<N>: <summary>`.
3. Use only the dependencies in Section 3. If you need another, stop, state the reason in `DECISIONS.md`, then add it.
4. No `unsafe` outside `gloss-render` unless unavoidable. Document every `unsafe` block with a `// SAFETY:` comment.
5. **Allocation rule (scoped):** Gloss-owned code paths must perform zero heap allocations per frame in steady state, meaning no input, no dirty nodes, no active animation. wgpu, winit, and taffy allocate internally and are excluded. Verify with a counting global allocator and a scoped guard (`AllocGuard`) around Gloss's own frame-prep code in tests.
6. Every visual feature needs a **golden-image test** (Section 8.4).
7. If a requirement is ambiguous, pick the simplest option, record it in `DECISIONS.md` (seeded in Section 11), and continue.
8. Keep the public API small. Anything not in Section 5 is `pub(crate)` until needed.
9. Never guess an API from memory. Before using a crate API, read its docs for the pinned version (`cargo doc --open` or docs.rs). Crates here are 0.x and change between releases.

---

## 2. Goals and non-goals

**Goals**
- Glass and "morphism" looks that match or beat native macOS apps.
- Styles as data: a `Style` struct built by a fluent builder, composable by ordinary Rust functions.
- State-driven transitions (hover, pressed, focus, disabled) declared in the style, not in widget code.
- Retained widget tree with fine-grained signals for app state.
- 60 fps minimum, 120 fps target on integrated GPUs (Apple Silicon, Intel Xe, AMD APU).
- Native behind-window vibrancy on macOS in addition to in-app glass.

**Non-goals (v0.1)**
- Web/WASM, mobile, a CSS parser, a cascade/specificity system, rotation transforms, a visual editor.
- Full accessibility. Leave hooks: every node carries `role: Role` and `label: Option<Arc<str>>` fields for later `accesskit` integration.
- Wide-gamut (Display P3) output. Document the sRGB-only decision; add behind a `wide-gamut` feature later.

---

## 3. Stack (versions verified on the crates.io index, 2026-11-11)

| Concern | Crate | Pin |
|---|---|---|
| Windowing / events | `winit` | 0.30.x (stable line; do **not** use 0.31 betas) |
| GPU | `wgpu` | 30.0.x |
| Layout | `taffy` | 0.15.x |
| Text shaping + rasterization | `cosmic-text` (bundles `swash` for rasterization) | 0.19.x |
| Behind-window vibrancy (macOS) | `window-vibrancy` | 0.8.x |
| Window handle traits | `raw-window-handle` | 0.6.x |
| Math | `glam` | 0.34.x |
| GPU struct casting | `bytemuck` (feature `derive`) | 1.25.x |
| Async init for wgpu | `pollster` | 1.0.x |
| Flag sets (`PropSet`) | `bitflags` | 2.x |
| Images (golden tests, dev-dependency) | `image` | 0.25.x |
| Errors | `thiserror` | 2.x |
| Hot reload (M9) | `subsecond` (Dioxus) or `hot-lib-reloader`, evaluate both | — |

Notes:
- **M0 must verify** that the `winit` 0.30 `Window`, `wgpu` 30 surface creation, and `window-vibrancy` 0.8 all agree on `raw-window-handle` 0.6. Record the working combination and exact patch versions in `DECISIONS.md`, and commit `Cargo.lock`.
- `parley` is a possible later replacement for `cosmic-text`, but it only shapes and lays out and does not rasterize. Do not use it in v0.1.
- `accesskit` is not a v0.1 dependency. The role/label hooks are enough.
- Hand-write: the slotmap arena, `InlineVec<T, N>`, the signal runtime, the glyph atlas shelf packer.
- Rust edition 2024, stable toolchain. Record MSRV. Must build on macOS, Linux, Windows. macOS is the primary target.

---

## 4. Workspace layout

```
gloss/
  Cargo.toml                 (workspace)
  Cargo.lock                 (committed)
  DECISIONS.md
  BENCH.md                   (measured numbers, updated per milestone)
  crates/
    gloss-core/              Style, StyleDelta, tokens, color, signals, node arena, animation
    gloss-render/            wgpu renderer, SDF pipeline, text pipeline, blur, glass, render graph
    gloss-layout/            taffy wrapper
    gloss-text/              cosmic-text wrapper, glyph atlas
    gloss-widgets/           Button, Card, Panel, Toggle, Slider, Sidebar, Toolbar, Label, Scroll
    gloss-app/               winit loop, input, window, scale factor, vibrancy, surface recovery
    gloss/                   facade crate re-exporting the public API
  examples/
    glass_card.rs            M4 demo
    counter.rs               M5 demo (signals + events)
    gallery.rs               M8 demo
  tests/golden/<os>-<backend>/
```

---

## 5. Core API (target shape)

### 5.1 Styles

```rust
// Tokens and theme: ordinary Rust items
pub mod tokens {
    pub const ACCENT: Color = rgb(0x6C5CE7);
    pub const RADIUS_LG: f32 = 20.0;
}

// Styles are functions returning plain data
pub fn glass_panel(theme: &Theme) -> Style {
    Style::new()
        .radius(Radius::continuous(24.0))
        .backdrop(Blur::dual_kawase(24.0).saturate(1.6))
        .tint(white(0.12))
        .border(1.0, white(0.25))
        .specular(Edge::top(), white(0.5))
        .refract(0.04)                                   // Liquid Glass (M7)
        .shadow([soft(0, 8, 24, black(0.25)), soft(0, 1, 2, black(0.4))])
        .hover(|d| d.lift(4.0).brighten(0.05))           // closure evaluated EAGERLY into a StyleDelta
        .pressed(|d| d.scale(0.98))
        .transition(Props::SHADOW | Props::TINT, ms(150), Ease::out())
}
```

### 5.2 Style data model (this resolves the closure/recursion problem)

- `Style` is `Clone + Debug + PartialEq` and contains **no heap-allocated fields**. Not required to be `Copy`.
- `Style { base: StyleData, hover: StyleDelta, pressed: StyleDelta, focused: StyleDelta, disabled: StyleDelta, transitions: InlineVec<Transition, 8> }`
- `StyleData`: every visual property as a concrete value (shadows in `InlineVec<Shadow, 4>`).
- `StyleDelta`: the same properties, each `Option<T>`, plus a `PropSet` bitflags mask of which are set. Sparse and inline, no `Box`.
- `.hover(|d| ...)`, `.pressed(...)`, `.focused(...)`, `.disabled(...)` take `FnOnce(StyleDelta) -> StyleDelta` and call it immediately at build time. Closures are never stored.
- `Props` is a `bitflags` type (`SHADOW`, `TINT`, `OPACITY`, `SCALE`, `OFFSET_Y`, `BACKDROP`, ...). `Prop::A | Prop::B` in v1 becomes `Props::A | Props::B`.
- `serde` support behind an off-by-default feature.

### 5.3 Resolution

`resolve(style, interaction_state, active_transitions) -> ResolvedStyle`, cached per node. Recomputed only when the node's interaction state changes, a bound signal changes, or a transition on that node is active. The "nothing changed" check must be **O(1)** (a single global dirty counter or root dirty flag), not a tree walk.

### 5.4 Application and reactivity (retained tree + signals)

The view function runs **once** and builds a retained tree. It does not rerun every frame.

```rust
fn main() {
    gloss::App::new().title("Counter").run(|| {
        let theme = Theme::light();
        let count = signal(0_i32);

        panel(glass_panel(&theme)).children([
            // dynamic text: closure subscribes to `count`, updates only this node
            label(move || format!("Clicked {}", count.get())).style(text_style()),
            button("Go")
                .style(button_style(Variant::Primary))
                .on_click(move |_| count.update(|c| *c += 1)),
            // dynamic structure
            show(move || count.get() > 5, || label("Nice.")),
        ])
    });
}
```

Reactivity requirements (all hand-written in `gloss-core`, single-threaded):
- `Signal<T>` is a `Copy` handle into a thread-local generational arena. `get()` inside a tracking scope subscribes. `set()` / `update()` marks subscribers dirty.
- Bindings: any property taking `impl Into<Dynamic<T>>` (static value or closure). Closure bindings re-run only when their dependencies change, and write only the affected node property, marking the narrowest dirty flag (`PAINT`, `STYLE`, `LAYOUT`, `TEXT`).
- Structural helpers: `show(cond, view)` and `for_each(items, key_fn, view_fn)` with keyed reconciliation. Nodes are created and removed through the arena and old subtrees are dropped, with signals and subscriptions disposed.
- No `Rc<RefCell>` leaks: every subscription is owned by the node that created it and disposed when the node is removed. Test for this.
- `NodeId` is a generational index. Stale ids are detected, never UB.

### 5.5 Frame pipeline

```
events -> handlers -> signal writes -> dirty flags
  -> (if dirty) style resolve -> (if LAYOUT dirty) taffy layout
  -> scene build (instances, text runs, layers) -> render graph -> present
```
Each stage is skipped when its dirty flag is clear. If nothing is dirty and no animation is active, **no frame is produced** (`ControlFlow::Wait`).

---

## 6. Rendering architecture

### 6.1 Units, HiDPI, surface
- Public API uses **logical pixels**. The renderer works in **physical pixels**. Scale factor comes from winit and is applied at scene build.
- Handle `ScaleFactorChanged` and resize: reconfigure the surface, recreate size-dependent offscreen textures, invalidate caches, and relayout.
- Hairline borders: `border(1.0, ...)` is 1 logical px. Provide `Border::hairline()` = exactly 1 physical px.
- Surface errors are handled, not unwrapped: `Lost` / `Outdated` leads to reconfigure and retry, `Timeout` leads to skip the frame, `OutOfMemory` leads to a clean exit with a message. Test via forced reconfiguration.
- Surface format: prefer an sRGB format. Blend in linear space. Premultiplied alpha throughout. Window transparency enabled when vibrancy or translucent windows are requested.

### 6.2 Primitive: SDF rounded rect
One instanced pipeline draws all boxes. `Instance` is `#[repr(C)]`, `bytemuck::Pod`, 16-byte aligned. Target size at most about 128 bytes.

Instance fields: rect, per-corner radii, corner smoothing, fill kind + solid color, gradient/shadow **offsets into a per-frame storage buffer arena**, border width + color, opacity, clip rect, optional clip radius, flags.

- Gradient stops (up to 4) and shadows (up to 4, plus inner shadows) live in the storage-buffer arena, referenced by `(offset, count)` from the instance. This keeps instances small. If a target backend lacks fragment-stage storage buffers, fall back to a uniform array path and record it in `DECISIONS.md`.
- **The instance quad is inflated** by the maximum shadow extent (blur radius + spread + offset) so outer shadows render outside the shape.
- Anti-aliasing from the SDF derivative (`fwidth`), no MSAA.
- Shadows are analytic Gaussian-approximated rounded-rect shadows in the fragment shader, **not** blurred textures. Inner shadows use the inverted shape, clipped to the shape.

**Continuous corners.** A true superellipse has no closed-form SDF. Specified approximation:
- In each corner region, with `p` the offset from the corner circle center normalized by radius `r`, evaluate `f(p) = (|p.x|^n + |p.y|^n)^(1/n) - 1` with `n = lerp(2.0, 5.0, smoothing)` (`smoothing` in 0..1, `0.6` is the Apple-like default; `n = 2` is a circular corner).
- Convert to an approximate distance with first-order normalization `d ≈ f / |∇f|` (gradient by analytic derivative or `dpdx/dpdy`), then scale by `r`.
- Borders, inner shadows, and glass masking all use this same `d`. Test at `smoothing = 0` against the exact rounded-rect SDF (max deviation less than 0.05 px), and test thin borders on high-smoothing corners for visible artifacts.

### 6.3 Text rendering
- `cosmic-text` shapes and lays out. Its `swash` integration rasterizes glyphs into a hand-written **shelf-packed R8 atlas** with LRU eviction and a bounded size.
- Glyph quads draw through a separate instanced pipeline sampling the atlas, premultiplied, with a documented text gamma/contrast adjustment constant to avoid thin or blotchy text on dark and light backgrounds.
- Subpixel positioning: horizontal only, behind a flag, default off. Grayscale AA only (no LCD).
- Text always draws **after** the glass pass of its layer (never blurred as part of the backdrop unless it belongs to a lower layer).

### 6.4 Transforms and clipping
- Supported transforms: translate and uniform scale, applied at scene build to the node's subtree rects. No rotation in v0.1.
- Text inside a scaled subtree draws from the nearest atlas size with scaled quads (small blur at `scale(0.98)` is accepted). Record this as decision D6.
- Clipping: each instance carries an axis-aligned clip rect (intersection of ancestors) plus **one** optional rounded clip radius (the innermost rounded clipper). Scroll containers are clip + translate offset. Nested rounded clips beyond one level are not supported in v0.1 (document it).

### 6.5 Layers and passes
A frame is a list of **layers** drawn back to front. A layer containing glass triggers:

1. Render everything below into an offscreen `scene` texture. **Default `Rgba8UnormSrgb` with dithering noise** (to prevent banding). `Rgba16Float` is opt-in via config because it doubles bandwidth on integrated GPUs.
2. **Dual Kawase blur**: N downsample passes, then N upsample passes, at 1/2 to 1/4 resolution. Map requested radius to pass count and offset.
3. Draw glass instances sampling the blurred texture, masked by the SDF.
4. Draw the layer's content (children, text) above.

**Nested glass.** Glass inside glass requires re-snapshotting the scene after the lower glass layer completes, so each nesting depth costs one more scene copy and blur chain. Cap depth at `MAX_GLASS_DEPTH = 2` (const, documented). Deeper glass degrades to tint-only with a debug warning.

Required optimizations:
- Blur only the **bounding region** behind glass (scissor + padded rect), not the full screen.
- **Cache** the blurred backdrop while nothing beneath changed (damage tracking by per-layer version counters).
- **Batch** all glass instances in a layer into one blur chain and one draw.
- Pool offscreen textures. Never allocate per frame. Recreate only on resize or scale change.

### 6.6 Glass shading (M4)
```
c  = saturate(blurred_backdrop, s)
c  = mix(c, tint.rgb, tint.a)
c += noise(frag_xy) * 0.015              // dither, prevents banding
c += border_highlight(d, angle)          // 1px light edge, brighter top-left
```

### 6.7 Liquid Glass (M7)
- `n = normalize(grad(d))`, analytic or via derivatives of the approximate SDF.
- Refract: `uv' = uv + n * strength * smoothstep(edge_w, 0.0, -d)` when sampling the backdrop.
- Chromatic dispersion: sample R, G, B at slightly different strengths.
- Specular: edge highlight weighted by `dot(n, light_dir)`; configurable light direction (default top-left).
- Parameters exposed via `Style::refract(...)`, `.specular(...)`, `.dispersion(...)`. Refraction samples must be clamped to the padded blur region.

### 6.8 Presets (M8)
In `gloss::presets`: `glass`, `liquid_glass`, `neumorphic` (dual opposite-offset soft shadows on a matching background), `clay` (outer soft shadow + inner highlight + inner shadow), `frosted_sidebar`, `vibrant_toolbar`.

### 6.9 macOS vibrancy
Behind-window blur is **OS-provided**. Expose `Window::vibrancy(Material::Sidebar | HudWindow | ...)` using `window-vibrancy` on a transparent winit window. Other platforms: no-op with a solid tinted fallback background.

---

## 7. Layout, input, animation

- **Layout**: `taffy` flexbox/grid. `Style` exposes padding, margin, gap, size, flex props and forwards them. Relayout only on `LAYOUT`-dirty. Text measurement feeds taffy through a measure function, cached by (text, style, available width).
- **Scroll**: `Scroll` widget with wheel/trackpad input, clamped offset, clip. Momentum and rubber-banding via the spring animator.
- **Input**: hit testing against resolved boxes in reverse paint order, respecting clips and transforms. Events: hover enter/leave, press, release, click, focus, scroll, key, text input. Per-widget state machine `Idle -> Hover -> Pressed` plus orthogonal `Focused` and `Disabled`. Keyboard focus traversal (Tab / Shift-Tab) with a visible focus ring. IME composition is out of scope in v0.1 (document it).
- **Animation**: property transitions on a single frame clock. Easing: linear, ease-in/out, cubic-bezier, and **spring** (stiffness, damping, mass). Animating a property never re-resolves unrelated properties. Redraw is requested only while animations are active.

---

## 8. Cross-cutting requirements

### 8.1 Performance targets (report actuals in `BENCH.md`; these are targets, not hard gates)

| Item | Target |
|---|---|
| Frame time, gallery, integrated GPU | < 8.3 ms (120 fps), floor 16.6 ms |
| Blur chain at 1080p equivalent | ~1 ms, report actual |
| 10,000 boxes (4 shadows each) | ~2 ms GPU, report actual |
| Style resolution, 1,000 nodes, nothing dirty | O(1) check, < 50 µs |
| Steady-state Gloss-owned heap allocations per frame | 0 |
| Idle app | no frames produced |

### 8.2 GPU timing
Use `wgpu::Features::TIMESTAMP_QUERY` when available. If unsupported, fall back to CPU-side submit-to-completion timing (`queue.on_submitted_work_done`) and label the number as such in `BENCH.md`.

### 8.3 Debug overlay
Frame ms, CPU ms, GPU ms (or fallback), draw calls, instance count, glass layers and blur passes, atlas occupancy, allocation counter, dirty flags.

### 8.4 Golden tests
- Headless render of scenes to PNG via an offscreen wgpu target. No window required.
- GPU output differs across vendors and drivers. Goldens are stored per `(os, backend)` in `tests/golden/<os>-<backend>/`.
- CI-style default uses a **software adapter** (lavapipe on Linux, WARP on Windows, Metal on macOS runners). Local real-GPU runs sit behind `--features gpu-tests` with a looser tolerance.
- Comparison: pass if at least 99.9% of pixels are within 2/255 per channel and no pixel differs by more than 16/255. A missing golden fails with a message explaining how to bless one via `GLOSS_BLESS=1`.

---

## 9. Milestones and acceptance criteria

**M0 — Skeleton**
Workspace, winit window, wgpu surface, resize and **scale-factor** handling, surface-error recovery, clear color, `ControlFlow::Wait` loop with on-demand redraw, timing overlay. Verify the winit/wgpu/window-vibrancy handle compatibility and pin versions.
*Accept:* opens on macOS, resizes cleanly, survives forced surface reconfigure, moves between displays of different DPI without blur or size glitches, frame timing prints.

**M1 — SDF box renderer**
Instanced pipeline with storage-buffer arena: fill, per-corner radii, continuous corners (approximation in 6.2), border, multi-layer outer and inner shadows, linear gradient, clip rect + one rounded clip. Headless golden harness.
*Accept:* golden tests for 8+ reference boxes (including `smoothing = 0` vs exact SDF check and thin border on a squircle); 10,000-box scene benchmark recorded.

**M2 — Style, tokens, theme**
`Style` / `StyleDelta` / `StyleData`, `InlineVec`, `Props` bitflags, builder, eager state-delta closures, `Theme`, color utilities (sRGB/linear, alpha, mix), resolution with per-node cache.
*Accept:* unit tests, including delta merge and resolution; `Style::new()` and chaining allocate nothing (allocation-counter test).

**M3 — Node arena, layout, text, clipping, scroll**
Generational node arena, taffy integration, cosmic-text shaping, atlas with LRU eviction, `Label`, `Panel`, `Scroll`.
*Accept:* demo with nested flex layout, wrapped text, and a clipped scrolling list; golden tests; atlas eviction test.

**M4 — Backdrop blur + glass (the pitch demo)**
Render graph, offscreen scene, dual Kawase, region-limited blur, backdrop cache, glass shader, dithering, tint, saturation, border highlight, `MAX_GLASS_DEPTH`. `examples/glass_card.rs`: a glass card over a colorful animated background.
*Accept:* holds 60 fps at 1440p on integrated GPU; blur cost measured and recorded; cache verified (blur skipped when the backdrop is unchanged); nested glass depth 2 works and depth 3 degrades with a warning; visual review against macOS reference screenshots; golden test.

**M5 — Reactivity and events**
Signals runtime, tracked bindings, dirty flags, `show`, keyed `for_each`, subscription disposal, hit testing, click/hover/focus handlers, Tab traversal, `Button`. `examples/counter.rs`.
*Accept:* counter demo works; changing a signal updates only the bound node (verified by dirty-flag assertions); removed nodes dispose subscriptions (leak test); idle app renders nothing.

**M6 — States and animation**
Interaction state machine, `StyleDelta` overrides, transitions, easing and springs, focus ring.
*Accept:* button with hover lift + press scale animates smoothly; interrupting a transition mid-way reverses without a jump; idle CPU/GPU near zero (verified).

**M7 — Liquid Glass**
Refraction, dispersion, specular, edge profile. Parameters in `Style`.
*Accept:* side-by-side with macOS Liquid Glass reference looks comparable; golden tests; still 60 fps.

**M8 — Widgets, presets, vibrancy**
Toggle, Slider, Card, Sidebar, Toolbar plus existing widgets. Presets from 6.8. macOS vibrancy. `examples/gallery.rs`.
*Accept:* gallery shows all morphism presets; vibrancy works on macOS; keyboard focus ring renders; role/label hooks populated on every widget.

**M9 — Hot reload**
Live-reload the styles crate in development. Evaluate `subsecond` vs `hot-lib-reloader`; record the decision.
*Accept:* editing a style file updates the running window in under 2 s with no restart and no loss of signal state.

---

## 10. Definition of done (v0.1)

- M0 to M8 complete, M9 attempted.
- `gallery` shows glass, liquid glass, neumorphic, clay, and frosted sidebar on macOS with vibrancy.
- README with a 20-line "hello glass" example and a screenshot.
- All tests and goldens pass; no clippy warnings; `BENCH.md` has measured numbers for every target in 8.1.
- Known limitations documented: no rotation, one rounded clip level, glass depth cap, no IME, sRGB only, grayscale text AA.

---

## 11. `DECISIONS.md` seed (pre-made decisions; do not re-litigate)

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