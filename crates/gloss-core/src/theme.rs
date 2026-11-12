//! Theme tokens shared by presets and applications.

use crate::color::{Color, black, rgb, white};

/// A palette plus a handful of design decisions used by presets.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Theme {
    /// Primary accent.
    pub accent: Color,
    /// Color used on top of the accent.
    pub on_accent: Color,
    /// Window/background color.
    pub background: Color,
    /// Surface color for cards and panels.
    pub surface: Color,
    /// Elevated surface color.
    pub surface_elevated: Color,
    /// Primary text color.
    pub text: Color,
    /// Secondary text color.
    pub text_dim: Color,
    /// Default border color.
    pub border: Color,
    /// Large radius.
    pub radius_lg: f32,
    /// Medium radius.
    pub radius_md: f32,
    /// Small radius.
    pub radius_sm: f32,
    /// Body font size.
    pub font_size: f32,
    /// Body line height.
    pub line_height: f32,
    /// Light direction used by specular highlights (normalised at draw time).
    pub light_dir: [f32; 2],
    /// Default backdrop blur radius.
    pub blur: f32,
    /// Default backdrop saturation.
    pub saturation: f32,
    /// Whether this is a dark theme.
    pub dark: bool,
}

impl Theme {
    /// The default dark theme (the primary Gloss look).
    pub const fn dark() -> Self {
        Self {
            accent: rgb(0x6C5CE7),
            on_accent: Color::WHITE,
            background: rgb(0x0E0F13),
            surface: rgb(0x16181F),
            surface_elevated: rgb(0x1E212B),
            text: rgb(0xF4F5F8),
            text_dim: rgb(0x9AA0AE),
            border: white(0.10),
            radius_lg: crate::tokens::RADIUS_LG,
            radius_md: crate::tokens::RADIUS_MD,
            radius_sm: crate::tokens::RADIUS_SM,
            font_size: crate::tokens::FONT_SIZE,
            line_height: crate::tokens::LINE_HEIGHT,
            light_dir: [-0.55, 0.83],
            blur: crate::tokens::BLUR_RADIUS,
            saturation: crate::tokens::BLUR_SATURATION,
            dark: true,
        }
    }

    /// A light theme.
    pub const fn light() -> Self {
        Self {
            accent: rgb(0x5B4BD6),
            on_accent: Color::WHITE,
            background: rgb(0xEEF0F5),
            surface: white(0.86),
            surface_elevated: Color::WHITE,
            text: rgb(0x1B1D24),
            text_dim: rgb(0x646B7A),
            border: black(0.08),
            radius_lg: crate::tokens::RADIUS_LG,
            radius_md: crate::tokens::RADIUS_MD,
            radius_sm: crate::tokens::RADIUS_SM,
            font_size: crate::tokens::FONT_SIZE,
            line_height: crate::tokens::LINE_HEIGHT,
            light_dir: [-0.55, 0.83],
            blur: crate::tokens::BLUR_RADIUS,
            saturation: crate::tokens::BLUR_SATURATION,
            dark: false,
        }
    }

    /// Text style at the theme's body size.
    #[inline]
    pub const fn body_text(&self) -> crate::style::TextStyle {
        crate::style::TextStyle::new(self.font_size)
            .line_height(self.line_height)
            .color(self.text)
    }

    /// Dimmed text style.
    #[inline]
    pub const fn dim_text(&self) -> crate::style::TextStyle {
        crate::style::TextStyle::new(self.font_size)
            .line_height(self.line_height)
            .color(self.text_dim)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}
