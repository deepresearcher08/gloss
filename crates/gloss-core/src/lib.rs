//! Gloss core: styles, tokens, themes, color and animation.
//!
//! Everything here is plain data and pure functions. Styles are built with a
//! fluent builder, contain inline fixed-capacity storage for shadows and
//! gradients, and resolve to a [`ResolvedStyle`] for a given [`WidgetState`].

pub mod alloc;
pub mod animation;
pub mod arena;
pub mod color;
pub mod dynamic;
pub mod inline_vec;
pub mod presets;
pub mod signal;
pub mod style;
pub mod theme;
pub mod tokens;

pub use animation::{Ease, Prop, Props, Spring, SpringValue, StyleAnimator, Transition, ms};
pub use arena::{Arena, NodeId};
pub use color::{
    Color, black, linear_to_srgb, rgb, rgba, srgb_to_linear_approx, srgb_to_linear_exact, white,
};
pub use dynamic::{Derived, Dynamic, derived};
pub use inline_vec::InlineVec;
pub use presets::{
    ButtonVariant, button, button_style, card, clay, frosted_sidebar, glass, glass_panel,
    liquid_glass, neumorphic, vibrant_toolbar,
};
pub use signal::{Effect, Signal, effect, effects_pending, flush_effects, signal};
pub use style::{
    Align, Blur, Border, Edge, Edges, Fill, FlexDirection, Gradient, GradientKind, LayoutStyle,
    MAX_SHADOWS, MAX_STOPS, Position, Radius, ResolvedStyle, Shadow, ShadowStack, Specular,
    StateStyle, Stop, Style, StyleData, StyleDelta, TextStyle, Transform, WidgetState, inner, soft,
    stop,
};
pub use theme::Theme;

/// The crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
