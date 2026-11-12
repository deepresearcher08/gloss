//! Gloss app: winit event loop, window and GPU surface management.

use std::marker::PhantomData;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gloss_core::alloc;
use gloss_core::{Color, Radius, TextStyle};
use gloss_render::{Quad, Renderer, TimingSource};
use gloss_text::TextEngine;
use gloss_widgets::{Document, LAYOUT};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState};
use winit::window::{Window, WindowId};

/// A keyboard key, re-exported for [`App::on_key`] handlers.
pub use winit::keyboard::Key as KeyEvent;
/// Named keyboard keys, re-exported for [`App::on_key`] handlers.
pub use winit::keyboard::NamedKey;

/// Error returned while loading or reloading a hot-reload library.
pub use hot_lib_reloader::HotReloaderError as HotReloadError;

/// How often an idle app checks the watched library for changes.
const HOT_RELOAD_POLL: Duration = Duration::from_millis(200);

/// A handle to a reloadable styles library (M9).
///
/// `T` must be the plain-data type returned by the exported symbol. Instantiate
/// it with a type from the same crate that exports the symbol so the host and
/// the `cdylib` agree on the layout.
pub struct HotReload<T> {
    loader: hot_lib_reloader::LibReloader,
    symbol: Vec<u8>,
    _marker: PhantomData<fn() -> T>,
}

impl<T: 'static> HotReload<T> {
    /// Watches `lib_name` inside `lib_dir` and resolves `symbol`.
    ///
    /// `lib_name` is the crate name (for example `"gloss_styles"`), not the
    /// platform-specific file name.
    pub fn new(
        lib_dir: impl AsRef<Path>,
        lib_name: impl AsRef<str>,
        symbol: impl AsRef<str>,
    ) -> Result<Self, HotReloadError> {
        let loader =
            hot_lib_reloader::LibReloader::new(lib_dir, lib_name, Some(HOT_RELOAD_POLL), None)?;
        Ok(Self {
            loader,
            symbol: symbol.as_ref().as_bytes().to_vec(),
            _marker: PhantomData,
        })
    }

    /// Reloads the library if it changed and returns the freshly built value.
    pub fn reload(&mut self) -> Result<Option<T>, HotReloadError> {
        if !self.loader.update()? {
            return Ok(None);
        }
        // SAFETY: `T` is instantiated from the same crate that exports `symbol`
        // with the signature `fn() -> T`; the caller upholds that contract.
        let build = unsafe { self.loader.get_symbol::<fn() -> T>(&self.symbol)? };
        Ok(Some(build()))
    }
}

/// macOS behind-window vibrancy materials.
///
/// On platforms without native vibrancy support these requests are ignored and
/// the window keeps its solid fallback background.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vibrancy {
    /// A sidebar material.
    Sidebar,
    /// A heads-up-display window material.
    HudWindow,
    /// A popover material.
    Popover,
    /// A menu material.
    Menu,
    /// A titlebar material.
    Titlebar,
    /// A window background material.
    WindowBackground,
    /// The material behind the window background.
    UnderWindowBackground,
}

type ViewFn = Box<dyn FnOnce(&mut Document)>;
type KeyFn = Box<dyn FnMut(Key)>;
type FrameFn = Box<dyn FnMut(f32) -> bool>;
type HotFn = Box<dyn FnMut(&mut Document) -> bool>;

/// Builder for a Gloss application.
pub struct App {
    title: String,
    width: f32,
    height: f32,
    transparent: bool,
    vibrancy: Option<Vibrancy>,
    debug: bool,
    view: Option<ViewFn>,
    on_key: Option<KeyFn>,
    on_frame: Option<FrameFn>,
    hot_reload: Option<HotFn>,
}

impl App {
    /// Creates an app with a default 960x640 window.
    pub fn new() -> Self {
        Self {
            title: "Gloss".to_string(),
            width: 960.0,
            height: 640.0,
            transparent: false,
            vibrancy: None,
            debug: false,
            view: None,
            on_key: None,
            on_frame: None,
            hot_reload: None,
        }
    }

    /// Sets the window title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Sets the initial logical window size.
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Enables a transparent window (required for vibrancy).
    ///
    /// On Windows this also applies an acrylic backdrop; on macOS use
    /// [`vibrancy`](Self::vibrancy) for a native material. The surface is
    /// configured with a premultiplied alpha mode when the backend supports it.
    pub fn transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    /// Requests macOS behind-window vibrancy.
    ///
    /// This also enables a transparent window. Other platforms ignore the
    /// request and keep the solid fallback background.
    pub fn vibrancy(mut self, vibrancy: Vibrancy) -> Self {
        self.transparent = true;
        self.vibrancy = Some(vibrancy);
        self
    }

    /// Shows the debug overlay at startup. F1 toggles it at runtime.
    pub fn debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }

    /// Registers a handler for key presses.
    pub fn on_key(mut self, handler: impl FnMut(Key) + 'static) -> Self {
        self.on_key = Some(Box::new(handler));
        self
    }

    /// Registers a per-frame callback.
    ///
    /// `dt` is the time in seconds since the previous frame. Return `true` to
    /// request another frame, which keeps continuous animations running.
    pub fn on_frame(mut self, handler: impl FnMut(f32) -> bool + 'static) -> Self {
        self.on_frame = Some(Box::new(handler));
        self
    }

    /// Watches a reloadable styles library and applies new values in place.
    ///
    /// `apply` receives the running [`Document`] and the freshly built value, so
    /// it can restyle existing nodes without rebuilding the tree. Signal state
    /// and node identities are preserved across a reload.
    pub fn hot_reload<T: 'static>(
        mut self,
        mut reloader: HotReload<T>,
        mut apply: impl FnMut(&mut Document, T) + 'static,
    ) -> Self {
        self.hot_reload = Some(Box::new(move |doc| match reloader.reload() {
            Ok(Some(value)) => {
                apply(doc, value);
                true
            }
            Ok(None) => false,
            Err(error) => {
                log::error!("hot reload failed: {error}");
                false
            }
        }));
        self
    }

    /// Runs the app. The view function runs once and builds the retained tree.
    pub fn run(mut self, view: impl FnOnce(&mut Document) + 'static) {
        let _ = env_logger::try_init();
        self.view = Some(Box::new(view));
        let event_loop = EventLoop::new().expect("create event loop");
        event_loop.set_control_flow(ControlFlow::Wait);
        let mut runner = Runner {
            app: Some(self),
            state: None,
        };
        event_loop.run_app(&mut runner).expect("run event loop");
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

struct Runner {
    app: Option<App>,
    state: Option<State>,
}

impl ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let Some(app) = self.app.take() else {
            return;
        };
        let attributes = Window::default_attributes()
            .with_title(app.title.clone())
            .with_inner_size(LogicalSize::new(app.width, app.height))
            .with_transparent(app.transparent);
        let window = Arc::new(event_loop.create_window(attributes).expect("create window"));
        if let Some(monitor) = event_loop.primary_monitor() {
            let screen = monitor.size();
            let outer = window.outer_size();
            if outer.width > 0 && outer.height > 0 {
                let x = screen.width.saturating_sub(outer.width) / 2;
                let y = screen.height.saturating_sub(outer.height) / 2;
                window.set_outer_position(winit::dpi::PhysicalPosition::new(x as i32, y as i32));
            }
        }

        #[cfg(target_os = "macos")]
        match app.vibrancy {
            Some(vibrancy) => {
                let _ = window_vibrancy::apply_vibrancy(
                    window.as_ref(),
                    vibrancy_material(vibrancy),
                    None,
                    Some(12.0),
                );
            }
            None if app.transparent => {
                let _ = window_vibrancy::apply_vibrancy(
                    window.as_ref(),
                    window_vibrancy::NSVisualEffectMaterial::HudWindow,
                    None,
                    Some(12.0),
                );
            }
            None => {}
        }
        #[cfg(target_os = "windows")]
        if app.transparent {
            if let Err(error) =
                window_vibrancy::apply_acrylic(window.as_ref(), Some((18, 18, 24, 160)))
            {
                log::debug!("acrylic backdrop unavailable: {error}");
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let _ = app.vibrancy;

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");
        let adapter = select_adapter(&instance, &surface, app.transparent);
        let required_features = adapter.features()
            & (wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features,
            ..Default::default()
        }))
        .expect("create device");
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(capabilities.formats[0]);
        let alpha_mode = if app.transparent {
            [
                wgpu::CompositeAlphaMode::PreMultiplied,
                wgpu::CompositeAlphaMode::PostMultiplied,
                wgpu::CompositeAlphaMode::Inherit,
            ]
            .into_iter()
            .find(|mode| capabilities.alpha_modes.contains(mode))
            .unwrap_or(capabilities.alpha_modes[0])
        } else {
            capabilities.alpha_modes[0]
        };
        let info = adapter.get_info();
        log::debug!(
            "adapter {} ({:?}), format {format:?}, alpha {alpha_mode:?}",
            info.name,
            info.backend
        );
        if app.transparent && alpha_mode == wgpu::CompositeAlphaMode::Opaque {
            log::warn!(
                "transparent window requested, but {} ({:?}) only supports opaque surfaces",
                info.name,
                info.backend
            );
        }
        let physical = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: physical.width.max(1),
            height: physical.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let mut renderer = Renderer::new(&device, &queue, format);
        renderer.resize(config.width, config.height);
        let mut doc = Document::new();
        if let Some(view) = app.view {
            view(&mut doc);
        }
        let scale = window.scale_factor() as f32;
        renderer.set_scale(scale);
        let mut text = TextEngine::new();
        let logical = window.inner_size().to_logical::<f32>(scale as f64);
        let _ = doc.layout(&mut text, logical.width, logical.height);

        self.state = Some(State {
            window,
            _instance: instance,
            surface,
            device,
            queue,
            config,
            renderer,
            doc,
            text,
            scale,
            last_frame: Instant::now(),
            frame_ms: 0.0,
            cpu_ms: 0.0,
            frames: 0,
            pointer: (0.0, 0.0),
            modifiers: ModifiersState::default(),
            show_debug: app.debug,
            on_key: app.on_key,
            on_frame: app.on_frame,
            hot_reload: app.hot_reload,
        });
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                state.resize(physical_size(size.width, size.height));
                state.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                state.scale = scale_factor as f32;
                state.renderer.set_scale(state.scale);
                let size = state.window.inner_size();
                state.resize(physical_size(size.width, size.height));
                state.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                state.redraw();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                state.modifiers = modifiers.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let logical = position.to_logical::<f32>(state.scale as f64);
                state.pointer = (logical.x, logical.y);
                if state.doc.pointer_move(logical.x, logical.y) {
                    state.window.request_redraw();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                if state.doc.pointer_leave() {
                    state.window.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let position = state.pointer;
                if state.doc.pointer_down(position.0, position.1) {
                    state.sync();
                    state.window.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                let position = state.pointer;
                if state.doc.pointer_up(position.0, position.1) {
                    state.sync();
                    state.window.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let amount = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * 40.0,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32,
                };
                let (x, y) = state.pointer;
                if let Some(scroll) = state.doc.scroll_at(x, y) {
                    if state.doc.scroll_by(scroll, -amount) {
                        state.window.request_redraw();
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if let Some(handler) = &mut state.on_key {
                    handler(event.logical_key.clone());
                    state.sync();
                    state.window.request_redraw();
                }
                match event.logical_key {
                    Key::Named(NamedKey::Tab) => {
                        let backwards = state.modifiers.shift_key();
                        state.doc.focus_next(backwards);
                        state.window.request_redraw();
                    }
                    Key::Named(NamedKey::F1) => {
                        state.show_debug = !state.show_debug;
                        state.window.request_redraw();
                    }
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(state) = &mut self.state else {
            return;
        };
        let State {
            doc,
            window,
            hot_reload,
            ..
        } = state;
        let mut reloaded = false;
        if let Some(hot) = hot_reload {
            reloaded = hot(doc);
            if reloaded {
                let root = doc.root();
                doc.mark_dirty(root, LAYOUT);
            }
        }
        if doc.dirty() != 0 || gloss_core::effects_pending() || reloaded {
            window.request_redraw();
        }
        if hot_reload.is_some() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + HOT_RELOAD_POLL));
        }
    }
}

#[cfg(target_os = "macos")]
fn vibrancy_material(vibrancy: Vibrancy) -> window_vibrancy::NSVisualEffectMaterial {
    use window_vibrancy::NSVisualEffectMaterial;
    match vibrancy {
        Vibrancy::Sidebar => NSVisualEffectMaterial::Sidebar,
        Vibrancy::HudWindow => NSVisualEffectMaterial::HudWindow,
        Vibrancy::Popover => NSVisualEffectMaterial::Popover,
        Vibrancy::Menu => NSVisualEffectMaterial::Menu,
        Vibrancy::Titlebar => NSVisualEffectMaterial::Titlebar,
        Vibrancy::WindowBackground => NSVisualEffectMaterial::WindowBackground,
        Vibrancy::UnderWindowBackground => NSVisualEffectMaterial::UnderWindowBackground,
    }
}

fn select_adapter(
    instance: &wgpu::Instance,
    surface: &wgpu::Surface<'static>,
    transparent: bool,
) -> wgpu::Adapter {
    if transparent {
        let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
        let mut best: Option<(i32, wgpu::Adapter)> = None;
        for adapter in adapters {
            let capabilities = surface.get_capabilities(&adapter);
            if capabilities.formats.is_empty() {
                continue;
            }
            let alpha = capabilities.alpha_modes.iter().any(|mode| {
                matches!(
                    mode,
                    wgpu::CompositeAlphaMode::PreMultiplied
                        | wgpu::CompositeAlphaMode::PostMultiplied
                        | wgpu::CompositeAlphaMode::Inherit
                )
            });
            let info = adapter.get_info();
            let score = (alpha as i32) * 2 + (info.device_type != wgpu::DeviceType::Cpu) as i32;
            if best
                .as_ref()
                .is_none_or(|(best_score, _)| score > *best_score)
            {
                best = Some((score, adapter));
            }
        }
        if let Some((_, adapter)) = best {
            return adapter;
        }
    }
    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: Some(surface),
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("no suitable GPU adapter")
}

fn physical_size(width: u32, height: u32) -> winit::dpi::PhysicalSize<u32> {
    winit::dpi::PhysicalSize::new(width.max(1), height.max(1))
}

struct State {
    window: Arc<Window>,
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    doc: Document,
    text: TextEngine,
    scale: f32,
    last_frame: Instant,
    frame_ms: f32,
    cpu_ms: f32,
    frames: u32,
    pointer: (f32, f32),
    modifiers: ModifiersState,
    show_debug: bool,
    on_key: Option<KeyFn>,
    on_frame: Option<FrameFn>,
    hot_reload: Option<HotFn>,
}

impl State {
    fn sync(&mut self) {
        if self.doc.sync() > 0 {
            self.doc.mark_dirty(self.doc.root(), LAYOUT);
        }
    }

    fn resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        self.renderer.resize(size.width, size.height);
        let logical = size.to_logical::<f32>(self.scale as f64);
        self.doc.mark_dirty(self.doc.root(), LAYOUT);
        let _ = self
            .doc
            .layout(&mut self.text, logical.width, logical.height);
        self.window.request_redraw();
    }

    fn paint_debug(&mut self) {
        let stats = self.renderer.stats();
        let source = match self.renderer.timing_source() {
            TimingSource::Gpu => "gpu",
            TimingSource::Cpu => "cpu",
            TimingSource::Unavailable => "n/a",
        };
        let info = format!(
            "Gloss debug  (F1 to hide)\n\
             frame {:.2} ms | cpu {:.2} ms | gpu {:.2} ms ({})\n\
             draws {} | instances {} | glass {} | blur {}\n\
             atlas {:.1}% ({} glyphs) | allocs/frame {}\n\
             dirty 0x{:02x}",
            self.frame_ms,
            self.cpu_ms,
            self.renderer.gpu_ms(),
            source,
            stats.draw_calls,
            stats.instances,
            stats.glass_layers,
            stats.blur_passes,
            stats.atlas_occupancy * 100.0,
            stats.atlas_glyphs,
            alloc::allocations(),
            self.doc.dirty(),
        );
        self.renderer.quad(
            Quad::filled(12.0, 12.0, 372.0, 132.0, Color::rgba(0x101014, 0.88))
                .radius(Radius::circular(10.0)),
        );
        let style = TextStyle::new(12.0).color(Color::rgb(0xe8e8f0));
        let layout = self.text.create_layout(&info, &style, Some(352.0));
        self.renderer.text(&mut self.text, &layout, 22.0, 22.0);
    }

    fn redraw(&mut self) {
        let started = Instant::now();
        alloc::reset_allocations();
        let dt = started
            .duration_since(self.last_frame)
            .as_secs_f32()
            .min(0.1);
        self.last_frame = started;
        let frame_animating = self.on_frame.as_mut().is_some_and(|handler| handler(dt));
        let animating = self.doc.advance(dt) || frame_animating;
        self.sync();
        if self.doc.dirty() & LAYOUT != 0 {
            let logical = self
                .window
                .inner_size()
                .to_logical::<f32>(self.scale as f64);
            if let Err(error) = self
                .doc
                .layout(&mut self.text, logical.width, logical.height)
            {
                log::error!("layout failed: {error}");
            }
        }

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                self.surface.configure(&self.device, &self.config);
                frame
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(frame)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
                    other => {
                        log::warn!("surface still unavailable: {other:?}");
                        return;
                    }
                }
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            other => {
                log::warn!("surface unavailable: {other:?}");
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.renderer.begin();
        self.doc.paint(&mut self.renderer, &mut self.text);
        if self.show_debug {
            self.paint_debug();
        }
        self.renderer.render(&view);
        self.queue.present(frame);
        self.doc.clear_dirty();
        if animating {
            self.window.request_redraw();
        }

        let cpu = started.elapsed().as_secs_f32() * 1000.0;
        self.cpu_ms = self.cpu_ms * 0.9 + cpu * 0.1;
        self.frame_ms = self.frame_ms * 0.9 + dt * 1000.0 * 0.1;
        self.frames += 1;
        if self.frames % 120 == 0 {
            log::debug!("frame: {:.2} ms", self.frame_ms);
        }
    }
}
