//! Design tokens.
//!
//! Tokens are plain `const`s so they can be used in `const` style tables and
//! do not allocate or initialise anything at runtime.

use crate::color::{Color, rgb};

/// Primary accent color (indigo).
pub const ACCENT: Color = rgb(0x6C5CE7);
/// Accent used for destructive actions.
pub const DANGER: Color = rgb(0xFF_5A_5A);
/// Accent used for positive actions.
pub const SUCCESS: Color = rgb(0x34_C7_59);
/// Accent used for warnings.
pub const WARNING: Color = rgb(0xFF_9F_0A);

/// Large corner radius.
pub const RADIUS_LG: f32 = 20.0;
/// Extra-large corner radius, used for glass cards.
pub const RADIUS_XL: f32 = 28.0;
/// Medium corner radius.
pub const RADIUS_MD: f32 = 12.0;
/// Small corner radius.
pub const RADIUS_SM: f32 = 8.0;
/// Corner smoothing that approximates Apple's continuous corners.
pub const CORNER_SMOOTHING: f32 = 0.6;

/// Base spacing unit (4 pt).
pub const SPACE_1: f32 = 4.0;
/// 8 pt.
pub const SPACE_2: f32 = 8.0;
/// 12 pt.
pub const SPACE_3: f32 = 12.0;
/// 16 pt.
pub const SPACE_4: f32 = 16.0;
/// 24 pt.
pub const SPACE_6: f32 = 24.0;
/// 32 pt.
pub const SPACE_8: f32 = 32.0;

/// Default body font size.
pub const FONT_SIZE: f32 = 14.0;
/// Default line height.
pub const LINE_HEIGHT: f32 = 20.0;
/// Default animation duration in milliseconds.
pub const DURATION_MS: f32 = 150.0;

/// Default backdrop blur radius for frosted glass.
pub const BLUR_RADIUS: f32 = 24.0;
/// Default backdrop saturation boost.
pub const BLUR_SATURATION: f32 = 1.6;
