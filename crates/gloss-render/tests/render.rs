use gloss_core::{Blur, Border, Color, Edge, Gradient, Radius, Specular, soft, stop};
use gloss_render::{Quad, Renderer, read_texture};

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

fn gpu() -> Option<Gpu> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .ok()?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
    Some(Gpu { device, queue })
}

#[test]
fn renders_shapes_glass_and_text() {
    let Some(gpu) = gpu() else {
        eprintln!("no GPU adapter available; skipping render test");
        return;
    };
    let width = 360u32;
    let height = 220u32;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;

    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, format);
    renderer.resize(width, height);

    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test target"),
        size: wgpu::Extent3d {
            width,
            height,
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
    renderer.quad(Quad::filled(-8.0, -8.0, 376.0, 236.0, Color::rgb(0x0b1220)));
    renderer.quad(
        Quad::new(30.0, 30.0, 150.0, 90.0)
            .radius(Radius::circular(20.0))
            .color(Color::rgb(0x3b82f6))
            .shadow(soft(0.0, 12.0, 30.0, Color::rgba(0x000000, 0.45))),
    );
    renderer.quad(
        Quad::new(210.0, 10.0, 120.0, 50.0)
            .clip(250.0, 10.0, 40.0, 50.0)
            .fill(gloss_core::Fill::gradient(Gradient::linear([
                stop(0.0, Color::rgb(0xff0044)),
                stop(1.0, Color::rgb(0x0033ff)),
            ]))),
    );
    renderer.quad(
        Quad::new(150.0, 80.0, 180.0, 110.0)
            .radius(Radius::circular(24.0))
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

    let mut text = gloss_text::TextEngine::new();
    let style = gloss_core::TextStyle {
        size: 24.0,
        line_height: 32.0,
        color: Color::rgb(0xffffff),
        weight: 600,
        tracking: 0.0,
    };
    let layout = text.create_layout("Gloss", &style, None);
    renderer.text(&mut text, &layout, 48.0, 150.0);

    renderer.render(&view);
    let error = pollster::block_on(scope.pop());
    assert!(error.is_none(), "wgpu validation error: {error:?}");

    let pixels = read_texture(&gpu.device, &gpu.queue, &target, width, height);
    assert_eq!(pixels.len(), (width * height * 4) as usize);
    assert!(
        pixels.chunks_exact(4).all(|p| p[3] == 255),
        "background should be opaque"
    );

    let unique: std::collections::HashSet<[u8; 3]> =
        pixels.chunks_exact(4).map(|p| [p[0], p[1], p[2]]).collect();
    assert!(unique.len() > 16, "expected a varied image, saw {unique:?}");

    let blue = pixel(&pixels, width, 60, 60);
    assert!(blue[2] > blue[0], "button should be blue, got {blue:?}");

    let glass = pixel(&pixels, width, 240, 135);
    let backdrop = pixel(&pixels, width, 240, 190);
    assert_ne!(glass, backdrop, "glass should alter its backdrop");

    let clipped_out = pixel(&pixels, width, 220, 35);
    assert!(
        clipped_out[0] < 40 && clipped_out[2] < 60,
        "geometry outside the clip should be hidden, got {clipped_out:?}"
    );
    let gradient_top = pixel(&pixels, width, 270, 15);
    let gradient_bottom = pixel(&pixels, width, 270, 55);
    assert_ne!(
        gradient_top, gradient_bottom,
        "gradient should vary across the quad"
    );
    assert!(
        gradient_top[0] > gradient_top[2],
        "gradient should start red at the top, got {gradient_top:?}"
    );
    assert!(
        gradient_bottom[2] > gradient_bottom[0],
        "gradient should end blue at the bottom, got {gradient_bottom:?}"
    );

    let mut brightest = 0u8;
    for y in 150..185 {
        for x in 44..140 {
            brightest = brightest.max(pixel(&pixels, width, x, y)[0]);
        }
    }
    assert!(
        brightest > 60,
        "text should brighten pixels, got {brightest}"
    );

    if let Some(parent) = std::path::Path::new("target").parent() {
        let _ = parent;
    }
    let _ = std::fs::create_dir_all("target");
    let image = image::RgbaImage::from_raw(width, height, pixels).expect("image buffer");
    let _ = image.save("target/golden_render.png");
}

#[test]
fn scale_factor_places_quads_in_physical_pixels() {
    let Some(gpu) = gpu() else {
        eprintln!("no GPU adapter available; skipping render test");
        return;
    };
    let width = 200u32;
    let height = 200u32;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, format);
    renderer.set_scale(2.0);
    renderer.resize(width, height);

    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scale target"),
        size: wgpu::Extent3d {
            width,
            height,
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
    renderer.quad(Quad::filled(10.0, 10.0, 20.0, 20.0, Color::rgb(0xff2200)));
    renderer.render(&view);

    let pixels = read_texture(&gpu.device, &gpu.queue, &target, width, height);
    let inside = pixel(&pixels, width, 50, 50);
    let outside = pixel(&pixels, width, 10, 10);
    assert!(
        inside[0] > 150,
        "scaled quad should cover physical 20..60, got {inside:?}"
    );
    assert!(
        outside[0] < 40,
        "logical padding should stay clear, got {outside:?}"
    );
}

fn pixel(data: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * width + x) * 4) as usize;
    [
        data[index],
        data[index + 1],
        data[index + 2],
        data[index + 3],
    ]
}
