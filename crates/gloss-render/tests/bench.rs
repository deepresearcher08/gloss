//! Manual GPU benchmarks. Run with:
//! `cargo test -p gloss-render --test bench -- --ignored --nocapture`

use std::time::Instant;

use gloss_core::{Blur, Color, Radius, soft};
use gloss_render::{Quad, Renderer};

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

fn gpu() -> Option<Gpu> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .ok()?;
    let features = adapter.features()
        & (wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS);
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: features,
        ..Default::default()
    }))
    .ok()?;
    eprintln!("adapter: {:?}", adapter.get_info());
    Some(Gpu { device, queue })
}

fn target(gpu: &Gpu, width: u32, height: u32) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bench target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn measure(
    renderer: &mut Renderer,
    gpu: &Gpu,
    view: &wgpu::TextureView,
    build: impl Fn(&mut Renderer),
) {
    renderer.begin();
    build(renderer);
    let start = Instant::now();
    renderer.render(view);
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    let wall = start.elapsed().as_secs_f32() * 1000.0;
    let stats = renderer.stats();

    renderer.begin();
    renderer.quad(Quad::filled(0.0, 0.0, 1.0, 1.0, Color::BLACK));
    renderer.render(view);
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());

    eprintln!(
        "wall submit-to-idle: {:.2} ms | reported {:.2} ms ({:?}) | draws {} instances {} glass {} blur {}",
        wall,
        renderer.gpu_ms(),
        renderer.timing_source(),
        stats.draw_calls,
        stats.instances,
        stats.glass_layers,
        stats.blur_passes,
    );
}

#[test]
#[ignore = "manual benchmark"]
fn ten_thousand_boxes() {
    let Some(gpu) = gpu() else {
        eprintln!("no GPU adapter available; skipping");
        return;
    };
    let width = 1920;
    let height = 1080;
    let texture = target(&gpu, width, height);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8UnormSrgb);
    renderer.resize(width, height);
    measure(&mut renderer, &gpu, &view, |renderer| {
        for i in 0..10_000u32 {
            let x = (i % 100) as f32 * 19.0;
            let y = (i / 100) as f32 * 10.0;
            renderer.quad(
                Quad::new(x, y, 18.0, 9.0)
                    .radius(Radius::circular(2.0))
                    .color(Color::rgb(0x3b82f6))
                    .shadow(soft(0.0, 2.0, 4.0, Color::rgba(0x000000, 0.35)))
                    .shadow(soft(0.0, 4.0, 10.0, Color::rgba(0x000000, 0.30)))
                    .shadow(soft(0.0, 8.0, 20.0, Color::rgba(0x000000, 0.20)))
                    .shadow(soft(0.0, 16.0, 36.0, Color::rgba(0x000000, 0.12))),
            );
        }
    });
}

#[test]
#[ignore = "manual benchmark"]
fn fullscreen_glass_blur() {
    let Some(gpu) = gpu() else {
        eprintln!("no GPU adapter available; skipping");
        return;
    };
    let width = 1920;
    let height = 1080;
    let texture = target(&gpu, width, height);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8UnormSrgb);
    renderer.resize(width, height);
    measure(&mut renderer, &gpu, &view, |renderer| {
        renderer.quad(Quad::filled(0.0, 0.0, 1920.0, 1080.0, Color::rgb(0x0b1220)));
        for i in 0..40u32 {
            let x = (i % 8) as f32 * 240.0;
            let y = (i / 8) as f32 * 200.0;
            renderer.quad(
                Quad::new(x + 8.0, y + 8.0, 224.0, 184.0)
                    .radius(Radius::circular(24.0))
                    .backdrop(Blur::dual_kawase(20.0))
                    .tint(Color::rgba(0xffffff, 0.12)),
            );
        }
    });
}
