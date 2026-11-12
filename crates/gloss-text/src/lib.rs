//! Text shaping, layout and rasterisation for Gloss.
//!
//! Shaping is handled by [`cosmic_text`] (the spec's sanctioned fallback for
//! parley). Rasterised glyph coverage is produced by swash and copied into
//! CPU-side bitmaps that the renderer can upload into its glyph atlas.

use cosmic_text::{
    Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent, SwashImage,
    Weight,
};
use gloss_core::{Color, TextStyle};

pub use cosmic_text::CacheKey;

/// A CPU-rasterised glyph bitmap.
#[derive(Clone, Debug)]
pub struct GlyphBitmap {
    /// Swash cache key identifying the glyph and its subpixel position.
    pub cache_key: CacheKey,
    /// Left edge relative to the text layout origin.
    pub x: i32,
    /// Top edge relative to the text layout origin.
    pub y: i32,
    /// Bitmap width in pixels.
    pub width: u32,
    /// Bitmap height in pixels.
    pub height: u32,
    /// Pixel data.
    ///
    /// For [`GlyphContent::Mask`] this is one byte of coverage per pixel; for
    /// [`GlyphContent::Color`] it is four bytes of non-premultiplied RGBA.
    pub data: Vec<u8>,
    /// Whether the bitmap is a coverage mask or full colour.
    pub content: GlyphContent,
    /// Colour to tint mask glyphs with.
    pub color: Color,
}

/// The kind of pixel data in a [`GlyphBitmap`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlyphContent {
    /// One byte of alpha coverage per pixel.
    Mask,
    /// Four bytes of RGBA per pixel (emoji and other colour glyphs).
    Color,
}

/// Measured dimensions of a text layout.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextMetrics {
    /// Width of the longest line, in pixels.
    pub width: f32,
    /// Total height of all lines, in pixels.
    pub height: f32,
    /// Number of laid-out lines.
    pub lines: usize,
}

/// An immutable shaped text layout.
#[derive(Debug)]
pub struct TextLayout {
    buffer: Buffer,
    metrics: TextMetrics,
    color: Color,
}

impl TextLayout {
    /// Measured size and line count.
    #[inline]
    pub fn metrics(&self) -> TextMetrics {
        self.metrics
    }

    /// Width of the longest line.
    #[inline]
    pub fn width(&self) -> f32 {
        self.metrics.width
    }

    /// Total height of the layout.
    #[inline]
    pub fn height(&self) -> f32 {
        self.metrics.height
    }

    /// The underlying cosmic-text buffer.
    #[inline]
    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    /// The text colour carried by this layout.
    #[inline]
    pub fn color(&self) -> Color {
        self.color
    }

    /// Visits every glyph pen position, in physical pixels.
    ///
    /// The position is the pen origin; add the glyph image's `placement.left`
    /// and subtract `placement.top` to obtain the bitmap's top-left corner.
    pub fn for_each_glyph<F>(&self, mut f: F)
    where
        F: FnMut(CacheKey, i32, i32),
    {
        for run in self.buffer.layout_runs() {
            for glyph in run.glyphs {
                let physical = glyph.physical((0.0, run.line_y), 1.0);
                f(physical.cache_key, physical.x, physical.y);
            }
        }
    }

    /// Number of glyphs in the layout.
    pub fn glyph_count(&self) -> usize {
        let mut count = 0;
        self.for_each_glyph(|_, _, _| count += 1);
        count
    }

    /// Rasterises every glyph into a CPU bitmap.
    pub fn rasterize(&self, engine: &mut TextEngine) -> Vec<GlyphBitmap> {
        let mut bitmaps = Vec::new();
        let scale = engine.scale;
        for run in self.buffer.layout_runs() {
            for glyph in run.glyphs {
                let physical = glyph.physical((0.0, run.line_y * scale), scale);
                let Some(image) = engine.glyph_image(physical.cache_key) else {
                    continue;
                };
                let (content, data) = match image.content {
                    SwashContent::Mask => (GlyphContent::Mask, image.data.clone()),
                    SwashContent::Color => (GlyphContent::Color, image.data.clone()),
                    _ => continue,
                };
                bitmaps.push(GlyphBitmap {
                    cache_key: physical.cache_key,
                    x: physical.x + image.placement.left,
                    y: physical.y - image.placement.top,
                    width: image.placement.width,
                    height: image.placement.height,
                    data,
                    content,
                    color: self.color,
                });
            }
        }
        bitmaps
    }
}

/// Owns the font database and glyph caches.
#[derive(Debug)]
pub struct TextEngine {
    font_system: FontSystem,
    swash_cache: SwashCache,
    scale: f32,
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEngine {
    /// Creates an engine with the system font database.
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            scale: 1.0,
        }
    }

    /// Sets the physical-pixel scale used when rasterising glyphs.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale.max(0.1);
    }

    /// The physical-pixel scale used when rasterising glyphs.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Shapes `text` with `style`, wrapping at `width` when provided.
    pub fn create_layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        width: Option<f32>,
    ) -> TextLayout {
        let metrics = Metrics::new(style.size.max(1.0), style.line_height.max(1.0));
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(width, None);

        let [r, g, b, a] = style.color.to_srgb8();
        let mut attrs = Attrs::new()
            .family(Family::SansSerif)
            .weight(Weight(style.weight))
            .color(cosmic_text::Color::rgba(r, g, b, a));
        if style.tracking != 0.0 {
            attrs = attrs.letter_spacing(style.tracking);
        }

        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let mut metrics_out = TextMetrics {
            width: 0.0,
            height: 0.0,
            lines: 0,
        };
        for run in buffer.layout_runs() {
            metrics_out.width = metrics_out.width.max(run.line_w);
            metrics_out.height = metrics_out.height.max(run.line_top + run.line_height);
            metrics_out.lines += 1;
        }
        if metrics_out.lines == 0 {
            metrics_out.height = style.line_height.max(1.0);
        }

        TextLayout {
            buffer,
            metrics: metrics_out,
            color: style.color,
        }
    }

    /// Measures `text` without keeping the shaped layout.
    pub fn measure(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> TextMetrics {
        self.create_layout(text, style, width).metrics()
    }

    /// The rasterised image for a glyph, or `None` if it cannot be rendered.
    pub fn glyph_image(&mut self, cache_key: CacheKey) -> Option<&SwashImage> {
        self.swash_cache
            .get_image(&mut self.font_system, cache_key)
            .as_ref()
    }

    /// Direct access to the font system (for registering custom fonts).
    #[inline]
    pub fn font_system(&self) -> &FontSystem {
        &self.font_system
    }

    /// Mutable access to the font system.
    #[inline]
    pub fn font_system_mut(&mut self) -> &mut FontSystem {
        &mut self.font_system
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(size: f32) -> TextStyle {
        TextStyle::new(size).line_height(size * 1.4)
    }

    #[test]
    fn measures_positive_size() {
        let mut engine = TextEngine::new();
        let m = engine.measure("Hello, Gloss", &style(16.0), None);
        assert!(m.width > 0.0, "width should be positive: {m:?}");
        assert!(m.height >= 16.0, "height should cover a line: {m:?}");
        assert_eq!(m.lines, 1);
    }

    #[test]
    fn wraps_at_requested_width() {
        let mut engine = TextEngine::new();
        let text = "The quick brown fox jumps over the lazy dog again and again";
        let wide = engine.measure(text, &style(16.0), None);
        let narrow = engine.measure(text, &style(16.0), Some(120.0));
        assert!(
            narrow.lines > wide.lines,
            "narrow: {narrow:?} wide: {wide:?}"
        );
        assert!(narrow.width <= 121.0, "narrow width: {}", narrow.width);
    }

    #[test]
    fn tracks_glyphs_and_rasterizes() {
        let mut engine = TextEngine::new();
        let layout = engine.create_layout("Ag", &style(32.0), None);
        assert!(layout.glyph_count() >= 2);

        let bitmaps = layout.rasterize(&mut engine);
        assert!(!bitmaps.is_empty());
        assert!(bitmaps.iter().any(|b| {
            b.width > 0
                && b.height > 0
                && b.content == GlyphContent::Mask
                && b.data.iter().any(|&p| p > 0)
        }));
    }

    #[test]
    fn empty_text_still_has_line_height() {
        let mut engine = TextEngine::new();
        let m = engine.measure("", &style(20.0), Some(200.0));
        assert!(m.height > 0.0);
        assert_eq!(m.width, 0.0);
    }
}
