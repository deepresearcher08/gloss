use std::path::{Path, PathBuf};

use gloss_core::{Blur, Border, Color, Edge, Gradient, Radius, Specular, inner, soft, stop};
use gloss_render::{Quad, Renderer, read_texture};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 420;

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    backend: wgpu::Backend,
}

fn request_adapter(instance: &wgpu::Instance, force_fallback: bool) -> Option<wgpu::Adapter> {
    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: force_fallback,
        ..Default::default()
    }))
    .ok()
}

fn gpu() -> Option<Gpu> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = if cfg!(feature = "gpu-tests") {
        request_adapter(&instance, false)
    } else {
        request_adapter(&instance, true).or_else(|| request_adapter(&instance, false))
    }?;
    let backend = adapter.get_info().backend;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
    Some(Gpu {
        device,
        queue,
        backend,
    })
}

fn os_tag() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn golden_path(backend: wgpu::Backend) -> PathBuf {
    let backend = format!("{backend:?}").to_lowercase();
    workspace_root()
        .join("tests/golden")
        .join(format!("{}-{backend}", os_tag()))
        .join("primitives.png")
}

fn save_png(path: &Path, pixels: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create golden directory");
    }
    let image = image::RgbaImage::from_raw(WIDTH, HEIGHT, pixels.to_vec()).expect("image buffer");
    image.save(path).expect("save golden image");
}

fn scene(renderer: &mut Renderer) {
    renderer.quad(Quad::filled(
        -8.0,
        -8.0,
        WIDTH as f32 + 16.0,
        HEIGHT as f32 + 16.0,
        Color::rgb(0x0b1220),
    ));

    renderer.quad(
        Quad::filled(20.0, 20.0, 120.0, 90.0, Color::rgb(0x3b82f6))
            .radius(Radius::circular(20.0).smoothing(0.0)),
    );
    renderer.quad(
        Quad::filled(160.0, 20.0, 120.0, 90.0, Color::rgb(0x14b8a6))
            .radius(Radius::continuous(24.0).smoothing(1.0)),
    );
    renderer.quad(
        Quad::filled(300.0, 20.0, 120.0, 90.0, Color::rgba(0xffffff, 0.04))
            .radius(Radius::continuous(18.0).smoothing(1.0))
            .border(Border::new(1.0, Color::rgba(0xffffff, 0.7))),
    );
    renderer.quad(
        Quad::filled(440.0, 20.0, 160.0, 100.0, Color::rgb(0x8b5cf6))
            .radius(Radius::continuous(22.0))
            .shadow(soft(0.0, 10.0, 24.0, Color::rgba(0x000000, 0.5))),
    );

    renderer.quad(
        Quad::filled(20.0, 140.0, 160.0, 100.0, Color::rgb(0x1e212b))
            .radius(Radius::continuous(18.0))
            .shadow(inner(0.0, 4.0, 14.0, Color::rgba(0x000000, 0.55))),
    );
    renderer.quad(
        Quad::new(200.0, 140.0, 160.0, 100.0)
            .radius(Radius::continuous(14.0))
            .fill(gloss_core::Fill::gradient(Gradient::linear([
                stop(0.0, Color::rgb(0xff0044)),
                stop(1.0, Color::rgb(0x0033ff)),
            ]))),
    );
    renderer.quad(
        Quad::filled(380.0, 140.0, 120.0, 80.0, Color::rgb(0x22c55e))
            .radius(Radius::continuous(10.0))
            .clip(400.0, 150.0, 70.0, 50.0)
            .clip_radius(12.0),
    );
    renderer.quad(
        Quad::filled(520.0, 140.0, 100.0, 80.0, Color::rgb(0xf59e0b))
            .radius(Radius::per_corner(4.0, 16.0, 32.0, 48.0)),
    );

    renderer.quad(
        Quad::new(160.0, 250.0, 340.0, 140.0)
            .radius(Radius::continuous(20.0))
            .fill(gloss_core::Fill::gradient(Gradient::linear([
                stop(0.0, Color::rgb(0x6c5ce7)),
                stop(0.5, Color::rgb(0x00c2ff)),
                stop(1.0, Color::rgb(0xff6b9d)),
            ]))),
    );
    renderer.quad(
        Quad::new(180.0, 260.0, 280.0, 120.0)
            .radius(Radius::continuous(24.0))
            .backdrop(Blur::dual_kawase(20.0))
            .tint(Color::rgba(0xffffff, 0.12))
            .border(Border::new(1.0, Color::rgba(0xffffff, 0.45)))
            .specular(Specular {
                edge: Edge::top(),
                color: Color::rgba(0xffffff, 0.8),
                width: 1.5,
                intensity: 1.0,
            })
            .refract(4.0)
            .dispersion(0.15),
    );
}

fn text_run(renderer: &mut Renderer, text: &mut gloss_text::TextEngine) {
    let style = gloss_core::TextStyle {
        size: 30.0,
        line_height: 40.0,
        color: Color::rgb(0xffffff),
        weight: 600,
        tracking: 0.0,
    };
    let layout = text.create_layout("Gloss", &style, None);
    renderer.text(text, &layout, 24.0, 330.0);
}

#[test]
fn primitives_golden() {
    let Some(gpu) = gpu() else {
        eprintln!("no GPU adapter available; skipping golden test");
        return;
    };
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, format);
    renderer.resize(WIDTH, HEIGHT);
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("golden target"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());

    renderer.begin();
    scene(&mut renderer);
    let mut text = gloss_text::TextEngine::new();
    text_run(&mut renderer, &mut text);
    renderer.render(&view);

    let actual = read_texture(&gpu.device, &gpu.queue, &target, WIDTH, HEIGHT);
    let path = golden_path(gpu.backend);
    let bless = std::env::var_os("GLOSS_BLESS")
        .and_then(|value| value.to_str().map(str::to_owned))
        .as_deref()
        == Some("1");

    if bless {
        save_png(&path, &actual);
        eprintln!("blessed golden at {}", path.display());
        return;
    }

    if !path.exists() {
        panic!(
            "missing golden image at {}; run with GLOSS_BLESS=1 to create it",
            path.display()
        );
    }

    let golden = image::open(&path)
        .unwrap_or_else(|error| panic!("read golden {}: {error}", path.display()))
        .to_rgba8();
    assert_eq!(
        (golden.width(), golden.height()),
        (WIDTH, HEIGHT),
        "golden dimensions"
    );

    let total = (WIDTH * HEIGHT) as usize;
    let mut within = 0usize;
    let mut max_diff = 0u8;
    for (actual_pixel, golden_pixel) in actual.chunks_exact(4).zip(golden.as_raw().chunks_exact(4))
    {
        let mut pixel_ok = true;
        for index in 0..4 {
            let diff = actual_pixel[index].abs_diff(golden_pixel[index]);
            max_diff = max_diff.max(diff);
            if diff > 2 {
                pixel_ok = false;
            }
        }
        if pixel_ok {
            within += 1;
        }
    }

    let ok = within * 1000 >= total * 999 && max_diff <= 16;
    if !ok {
        let mismatch_dir = workspace_root().join("target/golden-mismatch");
        let actual_path =
            mismatch_dir.join(format!("{}-{:?}-primitives.png", os_tag(), gpu.backend));
        save_png(&actual_path, &actual);
        panic!(
            "golden mismatch for {}: {} of {} pixels within tolerance ({:.3}%), max channel diff {}; actual written to {}",
            path.display(),
            within,
            total,
            within as f64 / total as f64 * 100.0,
            max_diff,
            actual_path.display()
        );
    }
}
