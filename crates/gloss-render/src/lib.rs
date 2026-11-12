//! Gloss renderer: wgpu pipelines for SDF boxes, text, blur and glass.
//!
//! The renderer draws into an `Rgba8UnormSrgb` offscreen scene with dithering so
//! that colors stay band-free and glass panels can sample what was already
//! drawn. Glass panels are handled in draw order: the scene is snapshotted, run
//! through a dual-Kawase style blur pyramid, and then sampled by the glass
//! shader.

mod atlas;
mod gpu;

use std::borrow::Cow;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use glam::Vec2;
use gloss_core::{Blur, Border, Color, Fill, Radius, ResolvedStyle, Shadow, ShadowStack, Specular};
use gloss_text::{TextEngine, TextLayout};
use wgpu::util::DeviceExt;

use atlas::Atlas;
use gpu::{DrawCtx, GpuInstance, GpuTextInstance};

const BLUR_LEVELS: usize = 3;
const MAX_ARENA_VEC4: usize = 8192;
const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// A rectangle to draw, with optional styling.
#[derive(Clone, Copy, Debug, Default)]
pub struct Quad {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub radius: Radius,
    pub fill: Option<Fill>,
    pub backdrop: Option<Blur>,
    pub tint: Option<Color>,
    pub border: Option<Border>,
    pub specular: Option<Specular>,
    pub shadows: ShadowStack,
    pub refract: f32,
    pub dispersion: f32,
    pub opacity: f32,
    /// Optional clip rectangle as `min_x, min_y, max_x, max_y`.
    pub clip: Option<[f32; 4]>,
    /// Radius of the rounded clip region.
    pub clip_radius: f32,
}

impl Quad {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            radius: Radius::default(),
            fill: None,
            backdrop: None,
            tint: None,
            border: None,
            specular: None,
            shadows: ShadowStack::new(),
            refract: 0.0,
            dispersion: 0.0,
            opacity: 1.0,
            clip: None,
            clip_radius: 0.0,
        }
    }

    pub fn filled(x: f32, y: f32, width: f32, height: f32, color: Color) -> Self {
        let mut quad = Self::new(x, y, width, height);
        quad.fill = Some(Fill::solid(color));
        quad
    }

    pub fn from_resolved(style: &ResolvedStyle, x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            radius: style.radius,
            fill: style.fill,
            backdrop: style.backdrop,
            tint: style.tint,
            border: style.border,
            specular: style.specular,
            shadows: style.shadows,
            refract: style.refract,
            dispersion: style.dispersion,
            opacity: style.opacity,
            clip: None,
            clip_radius: 0.0,
        }
    }

    pub fn radius(mut self, radius: Radius) -> Self {
        self.radius = radius;
        self
    }

    pub fn fill(mut self, fill: Fill) -> Self {
        self.fill = Some(fill);
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.fill = Some(Fill::solid(color));
        self
    }

    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadows.push(shadow);
        self
    }

    pub fn shadows(mut self, shadows: ShadowStack) -> Self {
        self.shadows = shadows;
        self
    }

    pub fn backdrop(mut self, blur: Blur) -> Self {
        self.backdrop = Some(blur);
        self
    }

    pub fn tint(mut self, color: Color) -> Self {
        self.tint = Some(color);
        self
    }

    pub fn border(mut self, border: Border) -> Self {
        self.border = Some(border);
        self
    }

    pub fn specular(mut self, specular: Specular) -> Self {
        self.specular = Some(specular);
        self
    }

    pub fn refract(mut self, strength: f32) -> Self {
        self.refract = strength;
        self
    }

    pub fn dispersion(mut self, strength: f32) -> Self {
        self.dispersion = strength;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Clip this quad to an axis-aligned rectangle.
    pub fn clip(mut self, x: f32, y: f32, width: f32, height: f32) -> Self {
        self.clip = Some([x, y, x + width, y + height]);
        self
    }

    /// Give the clip region rounded corners.
    pub fn clip_radius(mut self, radius: f32) -> Self {
        self.clip_radius = radius.max(0.0);
        self
    }

    fn is_glass(&self) -> bool {
        self.backdrop.is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Shape,
    Glass,
    Text,
}

#[derive(Clone, Copy, Debug)]
struct Batch {
    kind: Kind,
    start: usize,
    end: usize,
}

enum Op {
    Shape(GpuInstance),
    Glass(GpuInstance),
    Text(GpuTextInstance),
}

struct Targets {
    scene: wgpu::Texture,
    scene_view: wgpu::TextureView,
    snapshot: wgpu::Texture,
    snapshot_view: wgpu::TextureView,
    blur_levels: Vec<(wgpu::Texture, wgpu::TextureView)>,
    _blur_out: wgpu::Texture,
    blur_out_view: wgpu::TextureView,
}

/// Per-frame renderer statistics.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderStats {
    /// Draw calls issued this frame.
    pub draw_calls: u32,
    /// Shape, glass and text instances drawn this frame.
    pub instances: u32,
    /// Glass panels drawn this frame.
    pub glass_layers: u32,
    /// Blur passes recorded this frame.
    pub blur_passes: u32,
    /// Glyphs cached in the atlas.
    pub atlas_glyphs: u32,
    /// Fraction of atlas area occupied by glyphs.
    pub atlas_occupancy: f32,
}

/// Where the reported GPU time came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimingSource {
    /// GPU timestamp queries.
    Gpu,
    /// CPU submit-to-completion timing fallback.
    Cpu,
    /// No measurement is available yet.
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimerState {
    Idle,
    Pending,
}

struct GpuTimer {
    query_set: Option<wgpu::QuerySet>,
    resolve: Option<wgpu::Buffer>,
    readback: Option<wgpu::Buffer>,
    ready: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    cpu_ms: Arc<Mutex<Option<f32>>>,
    state: TimerState,
    last_ms: f32,
    source: TimingSource,
    enabled: bool,
}

impl GpuTimer {
    fn new(device: &wgpu::Device) -> Self {
        let required =
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        let enabled = device.features().contains(required);
        let mut timer = Self {
            query_set: None,
            resolve: None,
            readback: None,
            ready: Arc::new(AtomicBool::new(false)),
            failed: Arc::new(AtomicBool::new(false)),
            cpu_ms: Arc::new(Mutex::new(None)),
            state: TimerState::Idle,
            last_ms: 0.0,
            source: TimingSource::Unavailable,
            enabled,
        };
        if !enabled {
            timer.source = TimingSource::Cpu;
            return timer;
        }
        timer.query_set = Some(device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("gloss gpu timer"),
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        }));
        timer.resolve = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gloss gpu timer resolve"),
            size: 16,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        }));
        timer.readback = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gloss gpu timer readback"),
            size: 16,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        timer
    }

    fn begin(&mut self, encoder: &mut wgpu::CommandEncoder) -> bool {
        if !self.enabled || self.state != TimerState::Idle {
            return false;
        }
        let Some(query_set) = &self.query_set else {
            return false;
        };
        encoder.write_timestamp(query_set, 0);
        true
    }

    fn end(&mut self, encoder: &mut wgpu::CommandEncoder) -> bool {
        if !self.enabled || self.state != TimerState::Idle {
            return false;
        }
        let (Some(query_set), Some(resolve), Some(readback)) =
            (&self.query_set, &self.resolve, &self.readback)
        else {
            return false;
        };
        encoder.write_timestamp(query_set, 1);
        encoder.resolve_query_set(query_set, 0..2, resolve, 0);
        encoder.copy_buffer_to_buffer(resolve, 0, readback, 0, 16);
        true
    }

    fn after_submit(&mut self, queue: &wgpu::Queue, started: bool) {
        if !started {
            if self.enabled {
                return;
            }
            let shared = self.cpu_ms.clone();
            let start = Instant::now();
            queue.on_submitted_work_done(move || {
                if let Ok(mut slot) = shared.lock() {
                    *slot = Some(start.elapsed().as_secs_f32() * 1000.0);
                }
            });
            return;
        }
        let Some(readback) = &self.readback else {
            return;
        };
        self.ready.store(false, Ordering::Relaxed);
        self.failed.store(false, Ordering::Relaxed);
        let ready = self.ready.clone();
        let failed = self.failed.clone();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                if result.is_ok() {
                    ready.store(true, Ordering::Relaxed);
                } else {
                    failed.store(true, Ordering::Relaxed);
                }
            });
        self.state = TimerState::Pending;
    }

    fn poll(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let _ = device.poll(wgpu::PollType::Poll);
        if let Ok(mut slot) = self.cpu_ms.lock() {
            if let Some(ms) = slot.take() {
                self.last_ms = ms;
                self.source = TimingSource::Cpu;
            }
        }
        if self.state != TimerState::Pending {
            return;
        }
        if self.failed.load(Ordering::Relaxed) {
            self.state = TimerState::Idle;
            self.source = TimingSource::Cpu;
            return;
        }
        if !self.ready.load(Ordering::Relaxed) {
            return;
        }
        let Some(readback) = &self.readback else {
            self.state = TimerState::Idle;
            return;
        };
        {
            let Ok(data) = readback.slice(..).get_mapped_range() else {
                self.state = TimerState::Idle;
                return;
            };
            let stamps: &[u64] = bytemuck::cast_slice(&data);
            if stamps.len() >= 2 {
                let delta = stamps[1].saturating_sub(stamps[0]);
                self.last_ms = delta as f32 * queue.get_timestamp_period() / 1_000_000.0;
                self.source = TimingSource::Gpu;
            }
        }
        readback.unmap();
        self.state = TimerState::Idle;
    }
}

/// GPU renderer for Gloss primitives.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    scale: f32,

    pipeline_shape: wgpu::RenderPipeline,
    pipeline_glass: wgpu::RenderPipeline,
    pipeline_text: wgpu::RenderPipeline,
    pipeline_blur_down: wgpu::RenderPipeline,
    pipeline_blur_up: wgpu::RenderPipeline,
    pipeline_composite: wgpu::RenderPipeline,

    bgl_textures: wgpu::BindGroupLayout,
    bgl_atlas: wgpu::BindGroupLayout,
    bgl_globals: wgpu::BindGroupLayout,

    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    arena_buffer: wgpu::Buffer,

    sampler: wgpu::Sampler,
    glass_bind_group: Option<wgpu::BindGroup>,
    composite_bind_group: Option<wgpu::BindGroup>,
    targets: Option<Targets>,

    atlas: Atlas,

    ops: Vec<Op>,
    arena: Vec<gpu::GpuVec4>,
    instances: Vec<GpuInstance>,
    texts: Vec<GpuTextInstance>,
    batches: Vec<Batch>,
    instance_buffer: Option<(wgpu::Buffer, u64)>,
    text_buffer: Option<(wgpu::Buffer, u64)>,
    text_bind_group: Option<(u64, wgpu::BindGroup)>,
    glass_dirty: bool,
    glass_region: Option<[u32; 4]>,
    glass_iterations: u32,
    stats: RenderStats,
    timer: GpuTimer,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct GlobalsGpu {
    viewport: [f32; 2],
    _pad: [f32; 2],
}

impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let device = device.clone();
        let queue = queue.clone();

        let bgl_globals = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gloss globals layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bgl_textures = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gloss glass textures layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let bgl_atlas = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gloss atlas layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gloss globals"),
            contents: bytemuck::bytes_of(&GlobalsGpu {
                viewport: [1.0, 1.0],
                _pad: [0.0, 0.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let arena_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gloss instance arena"),
            size: (MAX_ARENA_VEC4 * std::mem::size_of::<[f32; 4]>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gloss globals bind group"),
            layout: &bgl_globals,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: arena_buffer.as_entire_binding(),
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("gloss linear sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let shape_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gloss shape shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("../shaders/shape.wgsl"))),
        });
        let glass_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gloss glass shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("../shaders/glass.wgsl"))),
        });
        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gloss text shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("../shaders/text.wgsl"))),
        });
        let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gloss blur shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("../shaders/blur.wgsl"))),
        });
        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gloss composite shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "../shaders/composite.wgsl"
            ))),
        });

        let layout_shape = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gloss shape pipeline layout"),
            bind_group_layouts: &[Some(&bgl_globals)],
            immediate_size: 0,
        });
        let layout_glass = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gloss glass pipeline layout"),
            bind_group_layouts: &[Some(&bgl_globals), Some(&bgl_textures)],
            immediate_size: 0,
        });
        let layout_text = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gloss text pipeline layout"),
            bind_group_layouts: &[Some(&bgl_globals), Some(&bgl_atlas)],
            immediate_size: 0,
        });
        let layout_blur = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gloss blur pipeline layout"),
            bind_group_layouts: &[Some(&bgl_textures)],
            immediate_size: 0,
        });

        let scene_target = SCENE_FORMAT;
        let scene_blend = Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        let scene_targets = [Some(wgpu::ColorTargetState {
            format: scene_target,
            blend: scene_blend,
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let instance_attrs = [
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 16,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 32,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 48,
                shader_location: 3,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 64,
                shader_location: 4,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 80,
                shader_location: 5,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 96,
                shader_location: 6,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 112,
                shader_location: 7,
                format: wgpu::VertexFormat::Float32x4,
            },
        ];
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &instance_attrs,
        };
        let text_attrs = [
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 16,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 32,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 48,
                shader_location: 3,
                format: wgpu::VertexFormat::Float32x4,
            },
        ];
        let text_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuTextInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &text_attrs,
        };

        let pipeline_shape = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gloss shape pipeline"),
            layout: Some(&layout_shape),
            vertex: wgpu::VertexState {
                module: &shape_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(instance_layout.clone())],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shape_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &scene_targets,
            }),
            multiview_mask: None,
            cache: None,
        });

        let pipeline_glass = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gloss glass pipeline"),
            layout: Some(&layout_glass),
            vertex: wgpu::VertexState {
                module: &glass_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(instance_layout.clone())],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &glass_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &scene_targets,
            }),
            multiview_mask: None,
            cache: None,
        });

        let pipeline_text = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gloss text pipeline"),
            layout: Some(&layout_text),
            vertex: wgpu::VertexState {
                module: &text_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(text_layout)],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &text_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &scene_targets,
            }),
            multiview_mask: None,
            cache: None,
        });

        let make_blur_pipeline = |label: &'static str, entry: &'static str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout_blur),
                vertex: wgpu::VertexState {
                    module: &blur_shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &blur_shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &scene_targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipeline_blur_down = make_blur_pipeline("gloss blur down", "down");
        let pipeline_blur_up = make_blur_pipeline("gloss blur up", "up");

        let composite_targets = [Some(wgpu::ColorTargetState {
            format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let pipeline_composite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gloss composite pipeline"),
            layout: Some(&layout_blur),
            vertex: wgpu::VertexState {
                module: &composite_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &composite_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &composite_targets,
            }),
            multiview_mask: None,
            cache: None,
        });

        let timer = GpuTimer::new(&device);
        Self {
            atlas: Atlas::new(&device),
            device,
            queue,
            format,
            width: 0,
            height: 0,
            scale: 1.0,
            pipeline_shape,
            pipeline_glass,
            pipeline_text,
            pipeline_blur_down,
            pipeline_blur_up,
            pipeline_composite,
            bgl_textures,
            bgl_atlas,
            bgl_globals,
            globals_buffer,
            globals_bind_group,
            arena_buffer,
            sampler,
            glass_bind_group: None,
            composite_bind_group: None,
            targets: None,
            ops: Vec::new(),
            arena: Vec::new(),
            instances: Vec::new(),
            texts: Vec::new(),
            batches: Vec::new(),
            instance_buffer: None,
            text_buffer: None,
            text_bind_group: None,
            glass_dirty: true,
            glass_region: None,
            glass_iterations: 1,
            stats: RenderStats::default(),
            timer,
        }
    }

    /// The target format this renderer composites into.
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Sets the logical-to-physical pixel scale used at scene build.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale.max(0.1);
    }

    /// The logical-to-physical pixel scale used at scene build.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Statistics from the most recent frame.
    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    /// GPU frame time in milliseconds, or the CPU fallback when timestamps are
    /// unavailable.
    pub fn gpu_ms(&self) -> f32 {
        self.timer.last_ms
    }

    /// Where [`gpu_ms`](Self::gpu_ms) came from.
    pub fn timing_source(&self) -> TimingSource {
        self.timer.source
    }

    /// Recreates offscreen targets for a new viewport size.
    pub fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if self.width == width && self.height == height && self.targets.is_some() {
            return;
        }
        self.width = width;
        self.height = height;

        self.queue.write_buffer(
            &self.globals_buffer,
            0,
            bytemuck::bytes_of(&GlobalsGpu {
                viewport: [width as f32, height as f32],
                _pad: [0.0, 0.0],
            }),
        );

        let targets = create_targets(&self.device, width, height);
        self.composite_bind_group =
            Some(self.create_texture_bind_group("gloss composite bind group", &targets.scene_view));
        self.glass_bind_group = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gloss glass bind group"),
            layout: &self.bgl_textures,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&targets.snapshot_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&targets.blur_out_view),
                },
            ],
        }));
        self.targets = Some(targets);
    }

    /// Starts a new frame, discarding queued primitives.
    pub fn begin(&mut self) {
        self.ops.clear();
        self.arena.clear();
        self.glass_dirty = true;
        self.glass_region = None;
        self.glass_iterations = 1;
        self.atlas.begin_frame(&self.device, &self.queue);
    }

    /// Queues a styled rectangle.
    pub fn quad(&mut self, quad: Quad) {
        if quad.width <= 0.0 || quad.height <= 0.0 {
            return;
        }
        let scale = self.scale;
        let rect = [
            quad.x * scale,
            quad.y * scale,
            quad.width * scale,
            quad.height * scale,
        ];
        let opacity = quad.opacity.clamp(0.0, 1.0);
        let clip = quad
            .clip
            .map(|clip| {
                [
                    clip[0] * scale,
                    clip[1] * scale,
                    clip[2] * scale,
                    clip[3] * scale,
                ]
            })
            .unwrap_or([
                -2.0,
                -2.0,
                self.width as f32 + 2.0,
                self.height as f32 + 2.0,
            ]);
        let radius = Radius {
            top_left: quad.radius.top_left * scale,
            top_right: quad.radius.top_right * scale,
            bottom_right: quad.radius.bottom_right * scale,
            bottom_left: quad.radius.bottom_left * scale,
            smoothing: quad.radius.smoothing,
        };
        let border = quad.border.map(|border| Border {
            width: border.width * scale,
            color: border.color,
        });
        let specular = quad.specular.map(|specular| Specular {
            width: specular.width * scale,
            ..specular
        });
        let gradient = quad
            .fill
            .map(|fill| gpu::gradient_array(fill, &mut self.arena))
            .unwrap_or([0.0, 90.0, 0.0, 0.0]);
        let mut ctx = DrawCtx {
            arena: &mut self.arena,
            clip,
            clip_radius: quad.clip_radius * scale,
            no_clip: quad.clip.is_none(),
        };

        let shadows = quad.shadows.layers();
        self.glass_dirty |= !shadows.is_empty();
        for shadow in shadows {
            let scaled = Shadow {
                offset_x: shadow.offset_x * scale,
                offset_y: shadow.offset_y * scale,
                blur: shadow.blur * scale,
                spread: shadow.spread * scale,
                ..*shadow
            };
            self.ops.push(Op::Shape(GpuInstance::shadow(
                &mut ctx, rect, radius, scaled, opacity,
            )));
        }

        if quad.is_glass() {
            let tint = quad
                .tint
                .or_else(|| quad.fill.map(|fill| fill.color))
                .unwrap_or(Color::TRANSPARENT);
            let backdrop = quad.backdrop.map(|blur| Blur {
                radius: blur.radius * scale,
                ..blur
            });
            if let Some(blur) = backdrop {
                self.glass_iterations = self
                    .glass_iterations
                    .max(blur.resolved_iterations())
                    .min(BLUR_LEVELS as u32);
            }
            self.ops.push(Op::Glass(GpuInstance::glass(
                &mut ctx,
                rect,
                radius,
                tint,
                border,
                specular,
                backdrop,
                quad.refract,
                quad.dispersion,
                opacity,
            )));
        } else {
            self.glass_dirty = true;
            let fill_color = quad
                .fill
                .map(|fill| fill.color)
                .unwrap_or(Color::TRANSPARENT);
            self.ops.push(Op::Shape(GpuInstance::shape(
                &mut ctx, rect, radius, fill_color, gradient, border, specular, opacity,
            )));
        }
    }

    /// Queues a shaped text layout at the given top-left position.
    pub fn text(&mut self, engine: &mut TextEngine, layout: &TextLayout, x: f32, y: f32) {
        self.text_clipped(engine, layout, x, y, None);
    }

    /// Queues a shaped text layout clipped to `clip` (`min_x, min_y, max_x, max_y`).
    pub fn text_clipped(
        &mut self,
        engine: &mut TextEngine,
        layout: &TextLayout,
        x: f32,
        y: f32,
        clip: Option<[f32; 4]>,
    ) {
        self.glass_dirty = true;
        let scale = self.scale;
        engine.set_scale(scale);
        let clip = clip
            .map(|clip| {
                [
                    clip[0] * scale,
                    clip[1] * scale,
                    clip[2] * scale,
                    clip[3] * scale,
                ]
            })
            .unwrap_or([
                -2.0,
                -2.0,
                self.width as f32 + 2.0,
                self.height as f32 + 2.0,
            ]);
        let bitmaps = layout.rasterize(engine);
        for bitmap in &bitmaps {
            let Some(uv) = self.atlas.insert(&self.device, &self.queue, bitmap) else {
                continue;
            };
            self.ops.push(Op::Text(GpuTextInstance {
                rect: [
                    x * scale + bitmap.x as f32,
                    y * scale + bitmap.y as f32,
                    bitmap.width as f32,
                    bitmap.height as f32,
                ],
                uv,
                color: bitmap.color.to_array(),
                clip,
            }));
        }
    }

    /// Renders queued primitives into `target`.
    pub fn render(&mut self, target: &wgpu::TextureView) {
        self.timer.poll(&self.device, &self.queue);
        let Some(targets) = self.targets.as_ref() else {
            return;
        };
        let width = self.width;
        let height = self.height;
        let mut draw_calls = 0u32;
        let mut glass_layers = 0u32;
        let mut blur_passes = 0u32;

        let mut instances = std::mem::take(&mut self.instances);
        let mut texts = std::mem::take(&mut self.texts);
        let mut batches = std::mem::take(&mut self.batches);
        instances.clear();
        texts.clear();
        batches.clear();
        for op in &self.ops {
            let (kind, index) = match op {
                Op::Shape(instance) => {
                    let index = instances.len();
                    instances.push(*instance);
                    (Kind::Shape, index)
                }
                Op::Glass(instance) => {
                    let index = instances.len();
                    instances.push(*instance);
                    (Kind::Glass, index)
                }
                Op::Text(instance) => {
                    let index = texts.len();
                    texts.push(*instance);
                    (Kind::Text, index)
                }
            };
            match batches.last_mut() {
                Some(last) if last.kind == kind && last.end == index => last.end += 1,
                _ => batches.push(Batch {
                    kind,
                    start: index,
                    end: index + 1,
                }),
            }
        }

        let arena_bytes = self.arena.len() * std::mem::size_of::<gpu::GpuVec4>();
        if arena_bytes > self.arena_buffer.size() as usize {
            let size = arena_bytes.next_power_of_two() as u64;
            self.arena_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gloss instance arena"),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.globals_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gloss globals bind group"),
                layout: &self.bgl_globals,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.globals_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.arena_buffer.as_entire_binding(),
                    },
                ],
            });
        }
        if !self.arena.is_empty() {
            self.queue
                .write_buffer(&self.arena_buffer, 0, bytemuck::cast_slice(&self.arena));
        }

        let instance_buffer = write_vertex_buffer(
            &self.device,
            &self.queue,
            &mut self.instance_buffer,
            "gloss instances",
            bytemuck::cast_slice(&instances),
        );
        let text_buffer = write_vertex_buffer(
            &self.device,
            &self.queue,
            &mut self.text_buffer,
            "gloss text instances",
            bytemuck::cast_slice(&texts),
        );

        let text_bind_group = if texts.is_empty() {
            None
        } else {
            let generation = self.atlas.generation();
            if self
                .text_bind_group
                .as_ref()
                .is_none_or(|(cached, _)| *cached != generation)
            {
                let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("gloss atlas bind group"),
                    layout: &self.bgl_atlas,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(self.atlas.view()),
                        },
                    ],
                });
                self.text_bind_group = Some((generation, bind_group));
            }
            self.text_bind_group
                .as_ref()
                .map(|(_, bind_group)| bind_group.clone())
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gloss frame encoder"),
            });
        let timing_started = self.timer.begin(&mut encoder);

        let clear = wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT);
        let mut first_pass = true;

        for batch in &batches {
            if batch.kind == Kind::Glass {
                glass_layers += (batch.end - batch.start) as u32;
                let region = glass_region(&instances, batch, width, height);
                let reusable = !self.glass_dirty
                    && self.glass_region.is_some_and(|cached| {
                        cached[0] <= region[0]
                            && cached[1] <= region[1]
                            && cached[2] >= region[2]
                            && cached[3] >= region[3]
                    });
                if !reusable {
                    if region[2] > region[0] && region[3] > region[1] {
                        encoder.copy_texture_to_texture(
                            wgpu::TexelCopyTextureInfo {
                                texture: &targets.scene,
                                mip_level: 0,
                                origin: wgpu::Origin3d {
                                    x: region[0],
                                    y: region[1],
                                    z: 0,
                                },
                                aspect: wgpu::TextureAspect::All,
                            },
                            wgpu::TexelCopyTextureInfo {
                                texture: &targets.snapshot,
                                mip_level: 0,
                                origin: wgpu::Origin3d {
                                    x: region[0],
                                    y: region[1],
                                    z: 0,
                                },
                                aspect: wgpu::TextureAspect::All,
                            },
                            wgpu::Extent3d {
                                width: region[2] - region[0],
                                height: region[3] - region[1],
                                depth_or_array_layers: 1,
                            },
                        );
                        self.record_blur(
                            &mut encoder,
                            targets,
                            region,
                            width,
                            height,
                            self.glass_iterations as usize,
                        );
                        blur_passes += 2 * self.glass_iterations.clamp(1, BLUR_LEVELS as u32);
                    }
                    self.glass_region = Some(region);
                    self.glass_dirty = false;
                }
            }

            let load = if first_pass {
                clear
            } else {
                wgpu::LoadOp::Load
            };
            first_pass = false;
            let mut pass = begin_pass(&mut encoder, &targets.scene_view, load);
            match batch.kind {
                Kind::Shape => {
                    if let Some(buffer) = &instance_buffer {
                        pass.set_pipeline(&self.pipeline_shape);
                        pass.set_bind_group(0, &self.globals_bind_group, &[]);
                        pass.set_vertex_buffer(0, buffer.slice(..));
                        pass.draw(0..6, batch.start as u32..batch.end as u32);
                        draw_calls += 1;
                    }
                }
                Kind::Glass => {
                    if let (Some(buffer), Some(bind_group)) =
                        (&instance_buffer, &self.glass_bind_group)
                    {
                        pass.set_pipeline(&self.pipeline_glass);
                        pass.set_bind_group(0, &self.globals_bind_group, &[]);
                        pass.set_bind_group(1, bind_group, &[]);
                        pass.set_vertex_buffer(0, buffer.slice(..));
                        pass.draw(0..6, batch.start as u32..batch.end as u32);
                        draw_calls += 1;
                    }
                }
                Kind::Text => {
                    if let (Some(buffer), Some(bind_group)) = (&text_buffer, &text_bind_group) {
                        pass.set_pipeline(&self.pipeline_text);
                        pass.set_bind_group(0, &self.globals_bind_group, &[]);
                        pass.set_bind_group(1, bind_group, &[]);
                        pass.set_vertex_buffer(0, buffer.slice(..));
                        pass.draw(0..6, batch.start as u32..batch.end as u32);
                        draw_calls += 1;
                    }
                }
            }
        }

        if first_pass {
            let _ = begin_pass(&mut encoder, &targets.scene_view, clear);
        }

        if let Some(bind_group) = &self.composite_bind_group {
            let mut pass = begin_pass(&mut encoder, target, clear);
            pass.set_pipeline(&self.pipeline_composite);
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(0..3, 0..1);
            draw_calls += 1;
        }

        self.timer.end(&mut encoder);
        self.queue.submit(Some(encoder.finish()));
        self.timer.after_submit(&self.queue, timing_started);
        self.stats = RenderStats {
            draw_calls,
            instances: (instances.len() + texts.len()) as u32,
            glass_layers,
            blur_passes,
            atlas_glyphs: self.atlas.len() as u32,
            atlas_occupancy: self.atlas.occupancy(),
        };
        self.ops.clear();
        self.arena.clear();
        self.instances = instances;
        self.texts = texts;
        self.batches = batches;
    }

    fn create_texture_bind_group(&self, label: &str, view: &wgpu::TextureView) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.bgl_textures,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(view),
                },
            ],
        })
    }

    fn record_blur(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        targets: &Targets,
        region: [u32; 4],
        width: u32,
        height: u32,
        iterations: usize,
    ) {
        let level_count = iterations.clamp(1, targets.blur_levels.len());
        let levels = &targets.blur_levels[..level_count];
        let mut source = &targets.snapshot_view;
        for (texture, view) in levels {
            let scissor = scaled_scissor(
                region,
                texture.width() as f32 / width.max(1) as f32,
                texture.height() as f32 / height.max(1) as f32,
                texture.width(),
                texture.height(),
            );
            self.blur_pass(encoder, &self.pipeline_blur_down, source, view, scissor);
            source = view;
        }

        let mut source = &levels[levels.len() - 1].1;
        for index in (0..levels.len() - 1).rev() {
            let dest = &levels[index].1;
            let scissor = scaled_scissor(
                region,
                levels[index].0.width() as f32 / width.max(1) as f32,
                levels[index].0.height() as f32 / height.max(1) as f32,
                levels[index].0.width(),
                levels[index].0.height(),
            );
            self.blur_pass(encoder, &self.pipeline_blur_up, source, dest, scissor);
            source = dest;
        }
        self.blur_pass(
            encoder,
            &self.pipeline_blur_up,
            source,
            &targets.blur_out_view,
            scaled_scissor(region, 1.0, 1.0, width, height),
        );
    }

    fn blur_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::RenderPipeline,
        source: &wgpu::TextureView,
        dest: &wgpu::TextureView,
        scissor: [u32; 4],
    ) {
        let bind_group = self.create_texture_bind_group("gloss blur bind group", source);
        let mut pass = begin_pass(encoder, dest, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT));
        pass.set_scissor_rect(scissor[0], scissor[1], scissor[2], scissor[3]);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn scaled_scissor(
    region: [u32; 4],
    scale_x: f32,
    scale_y: f32,
    dest_width: u32,
    dest_height: u32,
) -> [u32; 4] {
    let x0 = ((region[0] as f32 * scale_x).floor() as u32).min(dest_width);
    let y0 = ((region[1] as f32 * scale_y).floor() as u32).min(dest_height);
    let x1 = ((region[2] as f32 * scale_x).ceil() as u32).min(dest_width);
    let y1 = ((region[3] as f32 * scale_y).ceil() as u32).min(dest_height);
    [x0, y0, (x1 - x0).max(1), (y1 - y0).max(1)]
}

fn write_vertex_buffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    slot: &mut Option<(wgpu::Buffer, u64)>,
    label: &str,
    data: &[u8],
) -> Option<wgpu::Buffer> {
    if data.is_empty() {
        return None;
    }
    let needed = data.len() as u64;
    let recreate = slot.as_ref().is_none_or(|(_, capacity)| *capacity < needed);
    if recreate {
        let size = needed.next_power_of_two().max(256);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        *slot = Some((buffer, size));
    }
    let buffer = slot.as_ref().map(|(buffer, _)| buffer.clone())?;
    queue.write_buffer(&buffer, 0, data);
    Some(buffer)
}

fn glass_region(instances: &[GpuInstance], batch: &Batch, width: u32, height: u32) -> [u32; 4] {
    let mut min_x = width as f32;
    let mut min_y = height as f32;
    let mut max_x = 0.0f32;
    let mut max_y = 0.0f32;
    for instance in &instances[batch.start..batch.end] {
        min_x = min_x.min(instance.rect[0]);
        min_y = min_y.min(instance.rect[1]);
        max_x = max_x.max(instance.rect[0] + instance.rect[2]);
        max_y = max_y.max(instance.rect[1] + instance.rect[3]);
    }
    const PAD: f32 = 96.0;
    let clamp = |value: f32, max: u32| value.clamp(0.0, max as f32) as u32;
    [
        clamp(min_x - PAD, width),
        clamp(min_y - PAD, height),
        clamp(max_x + PAD, width),
        clamp(max_y + PAD, height),
    ]
}

fn begin_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    view: &'a wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("gloss pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

fn create_texture(
    device: &wgpu::Device,
    label: &str,
    width: u32,
    height: u32,
    usage: wgpu::TextureUsages,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SCENE_FORMAT,
        usage,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_targets(device: &wgpu::Device, width: u32, height: u32) -> Targets {
    let scene_usage = wgpu::TextureUsages::RENDER_ATTACHMENT
        | wgpu::TextureUsages::TEXTURE_BINDING
        | wgpu::TextureUsages::COPY_SRC;
    let (scene, scene_view) = create_texture(device, "gloss scene", width, height, scene_usage);
    let (snapshot, snapshot_view) = create_texture(
        device,
        "gloss glass snapshot",
        width,
        height,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    );

    let mut blur_levels = Vec::with_capacity(BLUR_LEVELS);
    let mut w = width;
    let mut h = height;
    for level in 0..BLUR_LEVELS {
        w = (w / 2).max(1);
        h = (h / 2).max(1);
        let (texture, view) = create_texture(
            device,
            &format!("gloss blur level {level}"),
            w,
            h,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        blur_levels.push((texture, view));
    }

    let (blur_out, blur_out_view) = create_texture(
        device,
        "gloss blur output",
        width,
        height,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    );

    Targets {
        scene,
        scene_view,
        snapshot,
        snapshot_view,
        blur_levels,
        _blur_out: blur_out,
        blur_out_view,
    }
}

/// Reads an RGBA8 texture back to the CPU. Intended for tests and screenshots.
pub fn read_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let bytes_per_pixel = 4u32;
    let unpadded = width * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded = unpadded.div_ceil(align) * align;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("gloss readback"),
        size: (padded * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("gloss readback encoder"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let _ = receiver.recv();

    let data = slice.get_mapped_range().expect("map readback buffer");
    let mut out = Vec::with_capacity((unpadded * height) as usize);
    for row in 0..height {
        let start = (row * padded) as usize;
        out.extend_from_slice(&data[start..start + unpadded as usize]);
    }
    drop(data);
    buffer.unmap();
    out
}

/// World-space position from layout offsets.
pub fn position(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}
