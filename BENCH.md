# Benchmarks

Performance numbers below are **targets**, not gates (D8). Measurements are
added as they are taken; anything not yet measured is marked explicitly.

| Item | Target | Status |
|---|---|---|
| Frame time, gallery, integrated GPU | < 8.3 ms (120 fps), floor 16.6 ms | not yet measured in the gallery on integrated GPU; GPU timer + overlay implemented |
| Blur chain at 1080p equivalent | ~1 ms | measured 0.92 ms GPU for 40 glass panels / 6 blur passes on RTX 3050 Laptop (Vulkan, release and debug) |
| 10,000 boxes (4 shadows each) | ~2 ms GPU | measured 8.84–9.70 ms GPU / 50,000 instances on RTX 3050 Laptop (Vulkan, release); above target (fill/overdraw bound, see notes) |
| Style resolution, 1,000 nodes, nothing dirty | O(1) check, < 50 µs | measured 2.41 µs clean `sync` per call; `dirty()` check 0.001 µs (debug) |
| Steady-state Gloss-owned heap allocations per frame | 0 | not yet measured per frame; `CountingAllocator` and scoped allocation test exist |
| Idle app | no frames produced | not yet measured on hardware; event loop waits and only redraws on dirty/effects |

## Data points

| Date | Measurement | Value | Notes |
|---|---|---|---|
| 2026-11-11 | `cargo test -p gloss-render --test golden` (headless 640x420 scene, software/default adapter, includes readback and PNG comparison) | ~1.1 s wall time | debug build; not a GPU frame-time benchmark |
| 2026-11-11 | same run with `GLOSS_BLESS=1` (render + PNG write) | ~3.2 s wall time | first run includes adapter/device setup |
| 2026-11-11 | `cargo test --workspace --quiet` (warm build) | ~4.0 s wall time | includes 18 core, 4 layout, 2 render unit, 1 golden, 2 readback, 4 text and 12 widget tests |
| 2026-11-11 | `bench::ten_thousand_boxes` at 1920x1080, 4 shadow layers per box | 8.84–9.70 ms GPU, 15.4–18.9 ms submit-to-idle, 50,000 instances | RTX 3050 6GB Laptop, Vulkan, release build; above the ~2 ms target |
| 2026-11-11 | `bench::ten_thousand_boxes` (debug) | 8.85–9.66 ms GPU | same GPU, debug build; GPU-bound, so debug and release are within noise |
| 2026-11-11 | `bench::fullscreen_glass_blur` at 1920x1080 | 0.92 ms GPU, 5.9–8.6 ms submit-to-idle, 40 glass panels, 6 blur passes | RTX 3050 6GB Laptop, Vulkan, release and debug |
| 2026-11-11 | `bench::clean_sync_thousand_nodes` | 2.41 µs per clean `sync`; 0.001 µs per `dirty()` check | 1,000 nodes, no bindings, debug build |

### 10,000-box notes

The earlier 3.61 ms figure was invalid: `DrawCtx::push_aux` returned a vec4
index while `shape.wgsl` and `glass.wgsl` read `arena[aux_index * 2u]`, so all
aux data after the first entry was misread. Shadows rendered nearly sharp
(blur read as ~0) and glass lost its blur/refraction parameters. The golden
image had been blessed with that broken output.

After fixing the index, the four shadow layers really do cover their inflated
quads (the largest is ~90x81 px per 18x9 box, `blur = 36`), producing ~117M
shadow fragments plus ~1.6M fill fragments at 1080p. That is ~14x screen
overdraw, so the benchmark is fill/bandwidth bound, not CPU bound. The
per-frame allocation work (reused instance/text/batch buffers, cached text
bind group, no per-node `String`/`Vec` clones, clipped-quad skip) is still in
place and keeps the CPU side cheap; it just cannot move a GPU fill-bound
number. A single-pass merged shadow stack was tried and rejected: it reduced
fragments by ~38% but the per-fragment 4-layer SDF loop cost more than the
saved bandwidth (~12 ms vs ~9 ms).

To approach ~2 ms would require a different strategy (e.g. rendering shadows
at half resolution into a scratch target and compositing, or a baked/analytic
falloff), which is not implemented.

## Reproduce

```sh
cargo test -p gloss-render --test bench -- --ignored --nocapture
cargo test -p gloss-render --test bench --release -- --ignored --nocapture
cargo test -p gloss-widgets --test bench -- --ignored --nocapture
```

The GPU benchmarks request timestamp-query features when the adapter exposes
them and print both the reported GPU time and the submit-to-idle wall time.
