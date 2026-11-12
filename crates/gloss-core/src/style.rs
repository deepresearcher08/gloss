//! Styles: plain, copyable data built with a fluent builder.

use crate::animation::{Ease, Props, Transition};
use crate::color::Color;
use crate::inline_vec::InlineVec;

/// Maximum number of shadow layers stored inline per box.
pub const MAX_SHADOWS: usize = 4;
/// Maximum number of gradient stops stored inline.
pub const MAX_STOPS: usize = 4;

/// Corner radii and continuous-corner smoothing.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Radius {
    /// Top-left radius.
    pub top_left: f32,
    /// Top-right radius.
    pub top_right: f32,
    /// Bottom-right radius.
    pub bottom_right: f32,
    /// Bottom-left radius.
    pub bottom_left: f32,
    /// Continuous-corner smoothing, 0 = circular, ~0.6 = Apple-like squircle.
    pub smoothing: f32,
}

impl Radius {
    /// A uniform circular radius.
    #[inline]
    pub const fn circular(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
            smoothing: 0.0,
        }
    }

    /// A uniform continuous (squircle) radius with the default smoothing.
    #[inline]
    pub const fn continuous(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
            smoothing: crate::tokens::CORNER_SMOOTHING,
        }
    }

    /// Per-corner radii.
    #[inline]
    pub const fn per_corner(
        top_left: f32,
        top_right: f32,
        bottom_right: f32,
        bottom_left: f32,
    ) -> Self {
        Self {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
            smoothing: 0.0,
        }
    }

    /// Override the smoothing parameter.
    #[inline]
    pub const fn smoothing(mut self, smoothing: f32) -> Self {
        self.smoothing = smoothing;
        self
    }

    /// Linear interpolation between two radii.
    pub fn lerp(a: Self, b: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            top_left: lerp(a.top_left, b.top_left, t),
            top_right: lerp(a.top_right, b.top_right, t),
            bottom_right: lerp(a.bottom_right, b.bottom_right, t),
            bottom_left: lerp(a.bottom_left, b.bottom_left, t),
            smoothing: lerp(a.smoothing, b.smoothing, t),
        }
    }
}

impl Default for Radius {
    fn default() -> Self {
        Self::circular(0.0)
    }
}

/// A single soft shadow layer.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Shadow {
    /// Horizontal offset in pixels.
    pub offset_x: f32,
    /// Vertical offset in pixels.
    pub offset_y: f32,
    /// Blur radius in pixels.
    pub blur: f32,
    /// Spread in pixels.
    pub spread: f32,
    /// Shadow color.
    pub color: Color,
    /// Draw as an inner shadow clipped to the box.
    pub inset: bool,
}

impl Shadow {
    /// The empty shadow.
    pub const NONE: Self = Self {
        offset_x: 0.0,
        offset_y: 0.0,
        blur: 0.0,
        spread: 0.0,
        color: Color::TRANSPARENT,
        inset: false,
    };

    /// Create a shadow layer.
    #[inline]
    pub const fn new(offset_x: f32, offset_y: f32, blur: f32, spread: f32, color: Color) -> Self {
        Self {
            offset_x,
            offset_y,
            blur,
            spread,
            color,
            inset: false,
        }
    }

    /// Mark the shadow as inset.
    #[inline]
    pub const fn inset(mut self) -> Self {
        self.inset = true;
        self
    }

    fn lerp(a: Self, b: Self, t: f32) -> Self {
        Self {
            offset_x: lerp(a.offset_x, b.offset_x, t),
            offset_y: lerp(a.offset_y, b.offset_y, t),
            blur: lerp(a.blur, b.blur, t),
            spread: lerp(a.spread, b.spread, t),
            color: a.color.mix(b.color, t),
            inset: b.inset,
        }
    }
}

/// A soft drop shadow.
#[inline]
pub const fn soft(offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Shadow {
    Shadow::new(offset_x, offset_y, blur, 0.0, color)
}

/// A soft inner shadow.
#[inline]
pub const fn inner(offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Shadow {
    Shadow::new(offset_x, offset_y, blur, 0.0, color).inset()
}

/// Fixed-capacity inline shadow list.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ShadowStack {
    layers: [Shadow; MAX_SHADOWS],
    len: u8,
}

impl ShadowStack {
    /// An empty stack.
    #[inline]
    pub const fn new() -> Self {
        Self {
            layers: [Shadow::NONE; MAX_SHADOWS],
            len: 0,
        }
    }

    /// Remove all layers.
    #[inline]
    pub fn clear(&mut self) {
        self.layers = [Shadow::NONE; MAX_SHADOWS];
        self.len = 0;
    }

    /// Push a layer, ignoring extras past [`MAX_SHADOWS`].
    #[inline]
    pub fn push(&mut self, shadow: Shadow) {
        let index = self.len as usize;
        if index < MAX_SHADOWS {
            self.layers[index] = shadow;
            self.len += 1;
        }
    }

    /// Number of active layers.
    #[inline]
    pub const fn count(&self) -> usize {
        self.len as usize
    }

    /// Whether the stack has no layers.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Active layers.
    #[inline]
    pub fn layers(&self) -> &[Shadow] {
        &self.layers[..self.len as usize]
    }

    /// Active layers, mutably.
    #[inline]
    pub fn layers_mut(&mut self) -> &mut [Shadow] {
        &mut self.layers[..self.len as usize]
    }

    fn lerp(a: Self, b: Self, t: f32) -> Self {
        let mut out = Self::new();
        let count = a.len.max(b.len) as usize;
        for i in 0..count {
            let sa = if (i as u8) < a.len {
                a.layers[i]
            } else {
                Shadow::NONE
            };
            let sb = if (i as u8) < b.len {
                b.layers[i]
            } else {
                Shadow::NONE
            };
            out.push(Shadow::lerp(sa, sb, t));
        }
        out
    }
}

impl Default for ShadowStack {
    fn default() -> Self {
        Self::new()
    }
}

/// A gradient stop.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Stop {
    /// Position along the gradient, 0..1.
    pub offset: f32,
    /// Stop color.
    pub color: Color,
}

/// A gradient stop constructor.
#[inline]
pub const fn stop(offset: f32, color: Color) -> Stop {
    Stop { offset, color }
}

/// Gradient kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GradientKind {
    /// Linear gradient along `angle` (degrees).
    Linear,
    /// Radial gradient centred in the box.
    Radial,
}

/// An inline gradient with up to [`MAX_STOPS`] stops.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Gradient {
    /// Gradient kind.
    pub kind: GradientKind,
    /// Angle in degrees, used by linear gradients.
    pub angle: f32,
    /// Stops.
    pub stops: [Stop; MAX_STOPS],
    /// Number of active stops.
    pub len: u8,
}

impl Gradient {
    /// An empty gradient of the given kind.
    #[inline]
    pub const fn new(kind: GradientKind) -> Self {
        Self {
            kind,
            angle: 90.0,
            stops: [Stop {
                offset: 0.0,
                color: Color::TRANSPARENT,
            }; MAX_STOPS],
            len: 0,
        }
    }

    /// Build a linear gradient from an iterator of stops.
    pub fn linear(stops: impl IntoIterator<Item = Stop>) -> Self {
        let mut g = Self::new(GradientKind::Linear);
        for s in stops {
            g.push(s);
        }
        g
    }

    /// Build a radial gradient from an iterator of stops.
    pub fn radial(stops: impl IntoIterator<Item = Stop>) -> Self {
        let mut g = Self::new(GradientKind::Radial);
        for s in stops {
            g.push(s);
        }
        g
    }

    /// Append a stop, ignoring extras past [`MAX_STOPS`].
    #[inline]
    pub fn push(&mut self, s: Stop) {
        let index = self.len as usize;
        if index < MAX_STOPS {
            self.stops[index] = s;
            self.len += 1;
        }
    }

    /// Set the linear gradient angle in degrees.
    #[inline]
    pub const fn angle(mut self, angle: f32) -> Self {
        self.angle = angle;
        self
    }

    /// Number of active stops.
    #[inline]
    pub const fn stop_count(&self) -> usize {
        self.len as usize
    }
}

/// A box fill: a solid color plus an optional gradient.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Fill {
    /// Solid base color (also used as the gradient fallback).
    pub color: Color,
    /// Optional gradient overlay.
    pub gradient: Option<Gradient>,
}

impl Fill {
    /// A solid fill.
    #[inline]
    pub const fn solid(color: Color) -> Self {
        Self {
            color,
            gradient: None,
        }
    }

    /// A gradient fill.
    #[inline]
    pub const fn gradient(gradient: Gradient) -> Self {
        Self {
            color: Color::TRANSPARENT,
            gradient: Some(gradient),
        }
    }
}

/// Backdrop blur parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Blur {
    /// Requested blur radius in pixels.
    pub radius: f32,
    /// Saturation applied to the blurred backdrop.
    pub saturation: f32,
    /// Number of dual-Kawase iterations; 0 selects automatically from `radius`.
    pub iterations: u32,
}

impl Blur {
    /// A dual-Kawase backdrop blur.
    #[inline]
    pub const fn dual_kawase(radius: f32) -> Self {
        Self {
            radius,
            saturation: 1.0,
            iterations: 0,
        }
    }

    /// Set backdrop saturation.
    #[inline]
    pub const fn saturate(mut self, saturation: f32) -> Self {
        self.saturation = saturation;
        self
    }

    /// Override the iteration count.
    #[inline]
    pub const fn iterations(mut self, iterations: u32) -> Self {
        self.iterations = iterations;
        self
    }

    /// Resolved iteration count for the requested radius.
    #[inline]
    pub fn resolved_iterations(&self) -> u32 {
        if self.iterations > 0 {
            self.iterations
        } else {
            ((self.radius / 6.0).round() as u32).clamp(1, 6)
        }
    }
}

/// A box border.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Border {
    /// Border width in pixels.
    pub width: f32,
    /// Border color.
    pub color: Color,
}

impl Border {
    /// Create a border.
    #[inline]
    pub const fn new(width: f32, color: Color) -> Self {
        Self { width, color }
    }
}

/// Per-edge weights used by specular highlights.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Edge {
    /// Top edge weight, 0..1.
    pub top: f32,
    /// Right edge weight, 0..1.
    pub right: f32,
    /// Bottom edge weight, 0..1.
    pub bottom: f32,
    /// Left edge weight, 0..1.
    pub left: f32,
}

impl Edge {
    /// A top-only highlight (the classic glass rim).
    #[inline]
    pub const fn top() -> Self {
        Self {
            top: 1.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }

    /// All edges equally.
    #[inline]
    pub const fn all() -> Self {
        Self {
            top: 1.0,
            right: 1.0,
            bottom: 1.0,
            left: 1.0,
        }
    }

    /// Top and left edges.
    #[inline]
    pub const fn top_left() -> Self {
        Self {
            top: 1.0,
            right: 0.0,
            bottom: 0.0,
            left: 1.0,
        }
    }
}

/// A specular rim highlight.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Specular {
    /// Edge weights.
    pub edge: Edge,
    /// Highlight color.
    pub color: Color,
    /// Highlight width in pixels.
    pub width: f32,
    /// Highlight intensity multiplier.
    pub intensity: f32,
}

/// Text styling.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextStyle {
    /// Font size in pixels.
    pub size: f32,
    /// Line height in pixels.
    pub line_height: f32,
    /// Text color.
    pub color: Color,
    /// Font weight (100..900).
    pub weight: u16,
    /// Additional letter tracking in pixels.
    pub tracking: f32,
}

impl TextStyle {
    /// Body text style.
    #[inline]
    pub const fn new(size: f32) -> Self {
        Self {
            size,
            line_height: size * 1.4,
            color: Color::WHITE,
            weight: 400,
            tracking: 0.0,
        }
    }

    /// Set the font size (line height is left untouched).
    #[inline]
    pub const fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Set the line height.
    #[inline]
    pub const fn line_height(mut self, line_height: f32) -> Self {
        self.line_height = line_height;
        self
    }

    /// Set the color.
    #[inline]
    pub const fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set the weight.
    #[inline]
    pub const fn weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }

    /// Set letter tracking.
    #[inline]
    pub const fn tracking(mut self, tracking: f32) -> Self {
        self.tracking = tracking;
        self
    }

    /// The default text style.
    #[inline]
    pub const fn body() -> Self {
        Self::new(crate::tokens::FONT_SIZE)
    }

    fn lerp(a: Self, b: Self, t: f32) -> Self {
        Self {
            size: lerp(a.size, b.size, t),
            line_height: lerp(a.line_height, b.line_height, t),
            color: a.color.mix(b.color, t),
            weight: b.weight,
            tracking: lerp(a.tracking, b.tracking, t),
        }
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self::body()
    }
}

/// Box edges (padding, margin, insets).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Edges {
    /// Top edge.
    pub top: f32,
    /// Right edge.
    pub right: f32,
    /// Bottom edge.
    pub bottom: f32,
    /// Left edge.
    pub left: f32,
}

impl Edges {
    /// The same value on all edges.
    #[inline]
    pub const fn all(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    /// Vertical and horizontal values.
    #[inline]
    pub const fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    /// Explicit per-edge values.
    #[inline]
    pub const fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
}

/// Flex main-axis direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FlexDirection {
    /// Left to right.
    Row,
    /// Top to bottom.
    Column,
}

/// Cross/main-axis alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Align {
    /// Start of the axis.
    Start,
    /// Centred.
    Center,
    /// End of the axis.
    End,
    /// Fill the axis.
    Stretch,
}

/// Positioning scheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Position {
    /// Normal flow.
    Relative,
    /// Out of flow, positioned against the parent.
    Absolute,
}

/// Layout properties forwarded to taffy.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LayoutStyle {
    /// Padding.
    pub padding: Edges,
    /// Margin.
    pub margin: Edges,
    /// Gap between children.
    pub gap: f32,
    /// Preferred width.
    pub width: Option<f32>,
    /// Preferred height.
    pub height: Option<f32>,
    /// Minimum width.
    pub min_width: Option<f32>,
    /// Minimum height.
    pub min_height: Option<f32>,
    /// Maximum width.
    pub max_width: Option<f32>,
    /// Maximum height.
    pub max_height: Option<f32>,
    /// Flex grow factor.
    pub flex_grow: f32,
    /// Flex shrink factor.
    pub flex_shrink: f32,
    /// Main-axis direction for children.
    pub flex_direction: FlexDirection,
    /// Cross-axis alignment for children.
    pub align_items: Align,
    /// Main-axis distribution for children.
    pub justify_content: Align,
    /// Positioning scheme.
    pub position: Position,
    /// Left inset for absolute positioning.
    pub left: Option<f32>,
    /// Top inset for absolute positioning.
    pub top: Option<f32>,
    /// Right inset for absolute positioning.
    pub right: Option<f32>,
    /// Bottom inset for absolute positioning.
    pub bottom: Option<f32>,
}

impl Default for LayoutStyle {
    fn default() -> Self {
        Self {
            padding: Edges::default(),
            margin: Edges::default(),
            gap: 0.0,
            width: None,
            height: None,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_direction: FlexDirection::Row,
            align_items: Align::Stretch,
            justify_content: Align::Start,
            position: Position::Relative,
            left: None,
            top: None,
            right: None,
            bottom: None,
        }
    }
}

/// Interaction state used to resolve a style.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WidgetState {
    /// Pointer is over the widget.
    pub hovered: bool,
    /// Pointer is pressed on the widget.
    pub pressed: bool,
    /// Widget owns keyboard focus.
    pub focused: bool,
    /// Widget is disabled.
    pub disabled: bool,
}

impl WidgetState {
    /// The idle state.
    #[inline]
    pub const fn idle() -> Self {
        Self {
            hovered: false,
            pressed: false,
            focused: false,
            disabled: false,
        }
    }

    /// Whether any interaction flag is active.
    #[inline]
    pub const fn is_idle(&self) -> bool {
        !self.hovered && !self.pressed && !self.focused && !self.disabled
    }
}

/// Sparse, inline state overrides.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StyleDelta {
    /// Properties this delta sets.
    pub props: Props,
    /// Replacement corner radii.
    pub radius: Option<Radius>,
    /// Replacement fill.
    pub fill: Option<Fill>,
    /// Replacement backdrop blur.
    pub backdrop: Option<Blur>,
    /// Replacement tint.
    pub tint: Option<Color>,
    /// Replacement border.
    pub border: Option<Border>,
    /// Replacement specular rim.
    pub specular: Option<Specular>,
    /// Replacement refraction strength.
    pub refract: Option<f32>,
    /// Replacement dispersion strength.
    pub dispersion: Option<f32>,
    /// Replacement shadow layers.
    pub shadows: Option<ShadowStack>,
    /// Replacement text style.
    pub text: Option<TextStyle>,
    /// Opacity multiplier.
    pub opacity: Option<f32>,
    /// Horizontal translation in pixels (added).
    pub translate_x: Option<f32>,
    /// Vertical translation in pixels (added; negative is up).
    pub translate_y: Option<f32>,
    /// Scale multiplier.
    pub scale: Option<f32>,
    /// Brighten all foreground colors.
    pub brighten: Option<f32>,
    /// Darken all foreground colors.
    pub darken: Option<f32>,
    /// Scales shadow offsets and blur.
    pub shadow_scale: Option<f32>,
}

/// Backwards-compatible name for [`StyleDelta`].
pub type StateStyle = StyleDelta;

impl StyleDelta {
    /// An empty delta.
    #[inline]
    pub const fn new() -> Self {
        Self {
            props: Props::NONE,
            radius: None,
            fill: None,
            backdrop: None,
            tint: None,
            border: None,
            specular: None,
            refract: None,
            dispersion: None,
            shadows: None,
            text: None,
            opacity: None,
            translate_x: None,
            translate_y: None,
            scale: None,
            brighten: None,
            darken: None,
            shadow_scale: None,
        }
    }

    /// Set the corner radii.
    #[inline]
    pub fn radius(mut self, radius: Radius) -> Self {
        self.radius = Some(radius);
        self.props = self.props.union(Props::RADIUS);
        self
    }

    /// Replace the fill.
    #[inline]
    pub fn fill(mut self, fill: Fill) -> Self {
        self.fill = Some(fill);
        self.props = self.props.union(Props::FILL);
        self
    }

    /// Replace the fill with a solid color.
    #[inline]
    pub fn color(self, color: Color) -> Self {
        self.fill(Fill::solid(color))
    }

    /// Replace the backdrop blur.
    #[inline]
    pub fn backdrop(mut self, blur: Blur) -> Self {
        self.backdrop = Some(blur);
        self.props = self.props.union(Props::BACKDROP);
        self
    }

    /// Replace the tint.
    #[inline]
    pub fn tint(mut self, color: Color) -> Self {
        self.tint = Some(color);
        self.props = self.props.union(Props::TINT);
        self
    }

    /// Replace the border.
    #[inline]
    pub fn border(mut self, border: Border) -> Self {
        self.border = Some(border);
        self.props = self.props.union(Props::BORDER);
        self
    }

    /// Replace the border with a 1px solid color.
    #[inline]
    pub fn border_color(self, color: Color) -> Self {
        self.border(Border::new(1.0, color))
    }

    /// Replace the specular rim.
    #[inline]
    pub fn specular(mut self, specular: Specular) -> Self {
        self.specular = Some(specular);
        self.props = self.props.union(Props::SPECULAR);
        self
    }

    /// Replace the shadow layers.
    #[inline]
    pub fn shadows(mut self, shadows: ShadowStack) -> Self {
        self.shadows = Some(shadows);
        self.props = self.props.union(Props::SHADOW);
        self
    }

    /// Replace the text style.
    #[inline]
    pub fn text(mut self, text: TextStyle) -> Self {
        self.text = Some(text);
        self.props = self.props.union(Props::TEXT);
        self
    }

    /// Move the widget up by `pixels`.
    #[inline]
    pub fn lift(mut self, pixels: f32) -> Self {
        self.translate_y = Some(-pixels);
        self.props = self.props.union(Props::OFFSET_Y);
        self
    }

    /// Move the widget down by `pixels`.
    #[inline]
    pub fn sink(mut self, pixels: f32) -> Self {
        self.translate_y = Some(pixels);
        self.props = self.props.union(Props::OFFSET_Y);
        self
    }

    /// Translate on both axes.
    #[inline]
    pub fn translate(mut self, x: f32, y: f32) -> Self {
        self.translate_x = Some(x);
        self.translate_y = Some(y);
        self.props = self.props.union(Props::OFFSET_X).union(Props::OFFSET_Y);
        self
    }

    /// Scale the widget.
    #[inline]
    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = Some(scale);
        self.props = self.props.union(Props::SCALE);
        self
    }

    /// Brighten foreground colors.
    #[inline]
    pub fn brighten(mut self, amount: f32) -> Self {
        self.brighten = Some(amount);
        self
    }

    /// Darken foreground colors.
    #[inline]
    pub fn darken(mut self, amount: f32) -> Self {
        self.darken = Some(amount);
        self
    }

    /// Multiply opacity.
    #[inline]
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = Some(opacity);
        self.props = self.props.union(Props::OPACITY);
        self
    }

    /// Scale drop shadows.
    #[inline]
    pub fn shadow_scale(mut self, scale: f32) -> Self {
        self.shadow_scale = Some(scale);
        self.props = self.props.union(Props::SHADOW);
        self
    }

    /// Set the Liquid Glass refraction strength.
    #[inline]
    pub fn refract(mut self, strength: f32) -> Self {
        self.refract = Some(strength);
        self.props = self.props.union(Props::REFRACT);
        self
    }

    /// Set the chromatic dispersion strength.
    #[inline]
    pub fn dispersion(mut self, strength: f32) -> Self {
        self.dispersion = Some(strength);
        self.props = self.props.union(Props::REFRACT);
        self
    }

    /// Merge another delta, with `other` taking precedence.
    pub fn merge(mut self, other: Self) -> Self {
        self.props = self.props.union(other.props);
        macro_rules! merge_fields {
            ($($field:ident),* $(,)?) => {
                $(
                    if other.$field.is_some() {
                        self.$field = other.$field;
                    }
                )*
            };
        }
        merge_fields!(
            radius,
            fill,
            backdrop,
            tint,
            border,
            specular,
            refract,
            dispersion,
            shadows,
            text,
            opacity,
            translate_x,
            translate_y,
            scale,
            brighten,
            darken,
            shadow_scale,
        );
        self
    }
}

/// A 2D affine transform applied at draw time.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Transform {
    /// Horizontal translation.
    pub translate_x: f32,
    /// Vertical translation.
    pub translate_y: f32,
    /// Uniform scale.
    pub scale: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translate_x: 0.0,
            translate_y: 0.0,
            scale: 1.0,
        }
    }
}

/// A style with state fully resolved for one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedStyle {
    /// Corner radii.
    pub radius: Radius,
    /// Box fill.
    pub fill: Option<Fill>,
    /// Backdrop blur.
    pub backdrop: Option<Blur>,
    /// Tint applied over the backdrop.
    pub tint: Option<Color>,
    /// Border.
    pub border: Option<Border>,
    /// Specular rim.
    pub specular: Option<Specular>,
    /// Liquid-glass refraction strength.
    pub refract: f32,
    /// Chromatic dispersion strength.
    pub dispersion: f32,
    /// Shadow layers.
    pub shadows: ShadowStack,
    /// Text style.
    pub text: Option<TextStyle>,
    /// Opacity.
    pub opacity: f32,
    /// Transform.
    pub transform: Transform,
    /// Layout properties (not animated).
    pub layout: LayoutStyle,
}

impl ResolvedStyle {
    /// Linear interpolation between two resolved styles.
    pub fn lerp(a: &Self, b: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            radius: Radius::lerp(a.radius, b.radius, t),
            fill: lerp_fill(a.fill, b.fill, t),
            backdrop: match (a.backdrop, b.backdrop) {
                (Some(a), Some(b)) => Some(Blur {
                    radius: lerp(a.radius, b.radius, t),
                    saturation: lerp(a.saturation, b.saturation, t),
                    iterations: if t < 0.5 { a.iterations } else { b.iterations },
                }),
                (None, Some(b)) => Some(b),
                (Some(a), None) => Some(a),
                (None, None) => None,
            },
            tint: lerp_color(a.tint, b.tint, t),
            border: match (a.border, b.border) {
                (Some(a), Some(b)) => Some(Border {
                    width: lerp(a.width, b.width, t),
                    color: a.color.mix(b.color, t),
                }),
                (None, Some(b)) => Some(Border {
                    width: b.width * t,
                    color: b.color,
                }),
                (Some(a), None) => Some(Border {
                    width: a.width * (1.0 - t),
                    color: a.color,
                }),
                (None, None) => None,
            },
            specular: lerp_specular(a.specular, b.specular, t),
            refract: lerp(a.refract, b.refract, t),
            dispersion: lerp(a.dispersion, b.dispersion, t),
            shadows: ShadowStack::lerp(a.shadows, b.shadows, t),
            text: match (a.text, b.text) {
                (Some(a), Some(b)) => Some(TextStyle::lerp(a, b, t)),
                (None, Some(b)) => Some(b),
                (Some(a), None) => Some(a),
                (None, None) => None,
            },
            opacity: lerp(a.opacity, b.opacity, t),
            transform: Transform {
                translate_x: lerp(a.transform.translate_x, b.transform.translate_x, t),
                translate_y: lerp(a.transform.translate_y, b.transform.translate_y, t),
                scale: lerp(a.transform.scale, b.transform.scale, t),
            },
            layout: b.layout,
        }
    }
}

/// Concrete visual properties shared by every state.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StyleData {
    /// Corner radii.
    pub radius: Radius,
    /// Box fill.
    pub fill: Option<Fill>,
    /// Backdrop blur.
    pub backdrop: Option<Blur>,
    /// Tint applied over the backdrop.
    pub tint: Option<Color>,
    /// Border.
    pub border: Option<Border>,
    /// Specular rim.
    pub specular: Option<Specular>,
    /// Liquid-glass refraction strength.
    pub refract: f32,
    /// Chromatic dispersion strength.
    pub dispersion: f32,
    /// Shadow layers.
    pub shadows: ShadowStack,
    /// Text style.
    pub text: Option<TextStyle>,
    /// Opacity.
    pub opacity: f32,
    /// Layout properties.
    pub layout: LayoutStyle,
}

impl Default for StyleData {
    fn default() -> Self {
        Self {
            radius: Radius::circular(0.0),
            fill: None,
            backdrop: None,
            tint: None,
            border: None,
            specular: None,
            refract: 0.0,
            dispersion: 0.0,
            shadows: ShadowStack::new(),
            text: None,
            opacity: 1.0,
            layout: LayoutStyle::default(),
        }
    }
}

/// The main style type: concrete data plus inline state overrides.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// Base visual properties.
    pub base: StyleData,
    /// Hover overrides.
    pub hover: StyleDelta,
    /// Pressed overrides.
    pub pressed: StyleDelta,
    /// Focused overrides.
    pub focused: StyleDelta,
    /// Disabled overrides.
    pub disabled: StyleDelta,
    /// Property transitions.
    pub transitions: InlineVec<Transition, 8>,
}

impl Style {
    /// An empty style.
    #[inline]
    pub const fn new() -> Self {
        Self {
            base: StyleData {
                radius: Radius::circular(0.0),
                fill: None,
                backdrop: None,
                tint: None,
                border: None,
                specular: None,
                refract: 0.0,
                dispersion: 0.0,
                shadows: ShadowStack::new(),
                text: None,
                opacity: 1.0,
                layout: LayoutStyle {
                    padding: Edges::all(0.0),
                    margin: Edges::all(0.0),
                    gap: 0.0,
                    width: None,
                    height: None,
                    min_width: None,
                    min_height: None,
                    max_width: None,
                    max_height: None,
                    flex_grow: 0.0,
                    flex_shrink: 1.0,
                    flex_direction: FlexDirection::Row,
                    align_items: Align::Stretch,
                    justify_content: Align::Start,
                    position: Position::Relative,
                    left: None,
                    top: None,
                    right: None,
                    bottom: None,
                },
            },
            hover: StyleDelta::new(),
            pressed: StyleDelta::new(),
            focused: StyleDelta::new(),
            disabled: StyleDelta::new(),
            transitions: InlineVec::new(),
        }
    }

    /// Set the corner radii.
    #[inline]
    pub const fn radius(mut self, radius: Radius) -> Self {
        self.base.radius = radius;
        self
    }

    /// Set a solid fill color.
    #[inline]
    pub const fn fill(mut self, color: Color) -> Self {
        self.base.fill = Some(Fill::solid(color));
        self
    }

    /// Set an arbitrary fill.
    #[inline]
    pub const fn with_fill(mut self, fill: Fill) -> Self {
        self.base.fill = Some(fill);
        self
    }

    /// Set a gradient fill.
    #[inline]
    pub const fn gradient(mut self, gradient: Gradient) -> Self {
        self.base.fill = Some(Fill::gradient(gradient));
        self
    }

    /// Enable a backdrop blur.
    #[inline]
    pub const fn backdrop(mut self, blur: Blur) -> Self {
        self.base.backdrop = Some(blur);
        self
    }

    /// Set the tint color.
    #[inline]
    pub const fn tint(mut self, color: Color) -> Self {
        self.base.tint = Some(color);
        self
    }

    /// Set the border.
    #[inline]
    pub const fn border(mut self, width: f32, color: Color) -> Self {
        self.base.border = Some(Border::new(width, color));
        self
    }

    /// Set a specular rim highlight.
    #[inline]
    pub const fn specular(mut self, edge: Edge, color: Color) -> Self {
        self.base.specular = Some(Specular {
            edge,
            color,
            width: 1.0,
            intensity: 1.0,
        });
        self
    }

    /// Set the specular highlight width.
    #[inline]
    pub const fn specular_width(mut self, width: f32) -> Self {
        if let Some(mut specular) = self.base.specular {
            specular.width = width;
            self.base.specular = Some(specular);
        }
        self
    }

    /// Set the specular highlight intensity.
    #[inline]
    pub const fn specular_intensity(mut self, intensity: f32) -> Self {
        if let Some(mut specular) = self.base.specular {
            specular.intensity = intensity;
            self.base.specular = Some(specular);
        }
        self
    }

    /// Set Liquid Glass refraction strength.
    #[inline]
    pub const fn refract(mut self, strength: f32) -> Self {
        self.base.refract = strength;
        self
    }

    /// Set chromatic dispersion strength.
    #[inline]
    pub const fn dispersion(mut self, strength: f32) -> Self {
        self.base.dispersion = strength;
        self
    }

    /// Replace the shadow layers.
    #[inline]
    pub fn shadow(mut self, shadows: impl IntoIterator<Item = Shadow>) -> Self {
        self.base.shadows.clear();
        for shadow in shadows {
            self.base.shadows.push(shadow);
        }
        self
    }

    /// Append one shadow layer.
    #[inline]
    pub fn add_shadow(mut self, shadow: Shadow) -> Self {
        self.base.shadows.push(shadow);
        self
    }

    /// Set the text style.
    #[inline]
    pub const fn text(mut self, text: TextStyle) -> Self {
        self.base.text = Some(text);
        self
    }

    /// Set the opacity.
    #[inline]
    pub const fn opacity(mut self, opacity: f32) -> Self {
        self.base.opacity = opacity;
        self
    }

    /// Set padding.
    #[inline]
    pub const fn padding(mut self, padding: Edges) -> Self {
        self.base.layout.padding = padding;
        self
    }

    /// Set margin.
    #[inline]
    pub const fn margin(mut self, margin: Edges) -> Self {
        self.base.layout.margin = margin;
        self
    }

    /// Set the child gap.
    #[inline]
    pub const fn gap(mut self, gap: f32) -> Self {
        self.base.layout.gap = gap;
        self
    }

    /// Set both width and height.
    #[inline]
    pub const fn size(mut self, width: f32, height: f32) -> Self {
        self.base.layout.width = Some(width);
        self.base.layout.height = Some(height);
        self
    }

    /// Set the preferred width.
    #[inline]
    pub const fn width(mut self, width: f32) -> Self {
        self.base.layout.width = Some(width);
        self
    }

    /// Set the preferred height.
    #[inline]
    pub const fn height(mut self, height: f32) -> Self {
        self.base.layout.height = Some(height);
        self
    }

    /// Set the maximum width.
    #[inline]
    pub const fn max_width(mut self, width: f32) -> Self {
        self.base.layout.max_width = Some(width);
        self
    }

    /// Set the maximum height.
    #[inline]
    pub const fn max_height(mut self, height: f32) -> Self {
        self.base.layout.max_height = Some(height);
        self
    }

    /// Set the flex grow factor.
    #[inline]
    pub const fn flex_grow(mut self, grow: f32) -> Self {
        self.base.layout.flex_grow = grow;
        self
    }

    /// Set the main-axis direction.
    #[inline]
    pub const fn flex_direction(mut self, direction: FlexDirection) -> Self {
        self.base.layout.flex_direction = direction;
        self
    }

    /// Set cross-axis alignment.
    #[inline]
    pub const fn align_items(mut self, align: Align) -> Self {
        self.base.layout.align_items = align;
        self
    }

    /// Set main-axis distribution.
    #[inline]
    pub const fn justify_content(mut self, align: Align) -> Self {
        self.base.layout.justify_content = align;
        self
    }

    /// Use absolute positioning.
    #[inline]
    pub const fn absolute(mut self) -> Self {
        self.base.layout.position = Position::Absolute;
        self
    }

    /// Set absolute insets.
    #[inline]
    pub const fn inset(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.base.layout.left = Some(left);
        self.base.layout.top = Some(top);
        self.base.layout.right = Some(right);
        self.base.layout.bottom = Some(bottom);
        self
    }

    /// Set hover overrides, evaluated eagerly.
    #[inline]
    pub fn hover(mut self, f: impl FnOnce(StyleDelta) -> StyleDelta) -> Self {
        self.hover = f(self.hover);
        self
    }

    /// Set pressed overrides, evaluated eagerly.
    #[inline]
    pub fn pressed(mut self, f: impl FnOnce(StyleDelta) -> StyleDelta) -> Self {
        self.pressed = f(self.pressed);
        self
    }

    /// Set focused overrides, evaluated eagerly.
    #[inline]
    pub fn focused(mut self, f: impl FnOnce(StyleDelta) -> StyleDelta) -> Self {
        self.focused = f(self.focused);
        self
    }

    /// Set disabled overrides, evaluated eagerly.
    #[inline]
    pub fn disabled(mut self, f: impl FnOnce(StyleDelta) -> StyleDelta) -> Self {
        self.disabled = f(self.disabled);
        self
    }

    /// Add a property transition.
    #[inline]
    pub fn transition(mut self, props: Props, duration: f32, ease: Ease) -> Self {
        self.transitions.push(Transition {
            props,
            duration,
            ease,
            spring: None,
        });
        self
    }

    /// Add a spring transition.
    #[inline]
    pub fn spring(mut self, props: Props, spring: crate::animation::Spring) -> Self {
        self.transitions.push(Transition {
            props,
            duration: 0.0,
            ease: Ease::Linear,
            spring: Some(spring),
        });
        self
    }

    /// Resolve the style for a given interaction state.
    pub fn resolve(&self, state: WidgetState) -> ResolvedStyle {
        let base = &self.base;
        let mut resolved = ResolvedStyle {
            radius: base.radius,
            fill: base.fill,
            backdrop: base.backdrop,
            tint: base.tint,
            border: base.border,
            specular: base.specular,
            refract: base.refract,
            dispersion: base.dispersion,
            shadows: base.shadows,
            text: base.text,
            opacity: base.opacity,
            transform: Transform::default(),
            layout: base.layout,
        };

        if state.disabled {
            apply_state(&mut resolved, &self.disabled);
            resolved.opacity *= 0.55;
            return resolved;
        }
        if state.hovered {
            apply_state(&mut resolved, &self.hover);
        }
        if state.pressed {
            apply_state(&mut resolved, &self.pressed);
        }
        if state.focused {
            apply_state(&mut resolved, &self.focused);
        }
        resolved
    }
}

impl Default for Style {
    fn default() -> Self {
        Self::new()
    }
}

fn apply_state(resolved: &mut ResolvedStyle, patch: &StyleDelta) {
    if let Some(radius) = patch.radius {
        resolved.radius = radius;
    }
    if let Some(fill) = patch.fill {
        resolved.fill = Some(fill);
    }
    if let Some(backdrop) = patch.backdrop {
        resolved.backdrop = Some(backdrop);
    }
    if let Some(tint) = patch.tint {
        resolved.tint = Some(tint);
    }
    if let Some(border) = patch.border {
        resolved.border = Some(border);
    }
    if let Some(specular) = patch.specular {
        resolved.specular = Some(specular);
    }
    if let Some(refract) = patch.refract {
        resolved.refract = refract;
    }
    if let Some(dispersion) = patch.dispersion {
        resolved.dispersion = dispersion;
    }
    if let Some(shadows) = patch.shadows {
        resolved.shadows = shadows;
    }
    if let Some(text) = patch.text {
        resolved.text = Some(text);
    }
    if let Some(opacity) = patch.opacity {
        resolved.opacity *= opacity;
    }
    if let Some(x) = patch.translate_x {
        resolved.transform.translate_x += x;
    }
    if let Some(y) = patch.translate_y {
        resolved.transform.translate_y += y;
    }
    if let Some(scale) = patch.scale {
        resolved.transform.scale *= scale;
    }
    if let Some(scale) = patch.shadow_scale {
        for shadow in resolved.shadows.layers_mut() {
            shadow.offset_x *= scale;
            shadow.offset_y *= scale;
            shadow.blur *= scale;
            shadow.spread *= scale;
        }
    }
    if let Some(amount) = patch.brighten {
        brighten_all(resolved, amount);
    }
    if let Some(amount) = patch.darken {
        darken_all(resolved, amount);
    }
}

fn brighten_all(resolved: &mut ResolvedStyle, amount: f32) {
    if let Some(fill) = resolved.fill.as_mut() {
        fill.color = fill.color.brighten(amount);
        if let Some(gradient) = fill.gradient.as_mut() {
            for stop in &mut gradient.stops[..gradient.len as usize] {
                stop.color = stop.color.brighten(amount);
            }
        }
    }
    if let Some(tint) = resolved.tint.as_mut() {
        *tint = tint.brighten(amount);
    }
    if let Some(border) = resolved.border.as_mut() {
        border.color = border.color.brighten(amount);
    }
    if let Some(text) = resolved.text.as_mut() {
        text.color = text.color.brighten(amount);
    }
}

fn darken_all(resolved: &mut ResolvedStyle, amount: f32) {
    if let Some(fill) = resolved.fill.as_mut() {
        fill.color = fill.color.darken(amount);
        if let Some(gradient) = fill.gradient.as_mut() {
            for stop in &mut gradient.stops[..gradient.len as usize] {
                stop.color = stop.color.darken(amount);
            }
        }
    }
    if let Some(tint) = resolved.tint.as_mut() {
        *tint = tint.darken(amount);
    }
    if let Some(border) = resolved.border.as_mut() {
        border.color = border.color.darken(amount);
    }
    if let Some(text) = resolved.text.as_mut() {
        text.color = text.color.darken(amount);
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_color(a: Option<Color>, b: Option<Color>, t: f32) -> Option<Color> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.mix(b, t)),
        (None, Some(b)) => Some(b.with_alpha(b.a * t)),
        (Some(a), None) => Some(a.with_alpha(a.a * (1.0 - t))),
        (None, None) => None,
    }
}

fn lerp_fill(a: Option<Fill>, b: Option<Fill>, t: f32) -> Option<Fill> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let gradient = if t < 0.5 { a.gradient } else { b.gradient };
            Some(Fill {
                color: a.color.mix(b.color, t),
                gradient,
            })
        }
        (None, Some(b)) => Some(Fill {
            color: b.color.with_alpha(b.color.a * t),
            gradient: b.gradient,
        }),
        (Some(a), None) => Some(Fill {
            color: a.color.with_alpha(a.color.a * (1.0 - t)),
            gradient: a.gradient,
        }),
        (None, None) => None,
    }
}

fn lerp_specular(a: Option<Specular>, b: Option<Specular>, t: f32) -> Option<Specular> {
    match (a, b) {
        (Some(a), Some(b)) => Some(Specular {
            edge: b.edge,
            color: a.color.mix(b.color, t),
            width: lerp(a.width, b.width, t),
            intensity: lerp(a.intensity, b.intensity, t),
        }),
        (None, Some(b)) => Some(Specular {
            edge: b.edge,
            color: b.color,
            width: b.width,
            intensity: b.intensity * t,
        }),
        (Some(a), None) => Some(Specular {
            edge: a.edge,
            color: a.color,
            width: a.width,
            intensity: a.intensity * (1.0 - t),
        }),
        (None, None) => None,
    }
}
