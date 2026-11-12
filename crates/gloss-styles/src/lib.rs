//! Hot-reloadable styles for Gloss.
//!
//! This crate is built both as an `rlib` (for the host to share the [`StyleSet`]
//! type) and as a `cdylib` (the library `hot-lib-reloader` watches at runtime).
//! Edit this file while `hot_reload` is running to see the window restyle in
//! place. The exported [`gloss_styles`] symbol is the only ABI surface: its
//! signature and the layout of [`StyleSet`] must stay compatible with the host.

use gloss_core::{
    Blur, Color, Edge, Edges, FlexDirection, Radius, Style, TextStyle, Theme, black, rgb, soft,
    white,
};

/// Every style the hot-reload demo reads from the reloadable library.
///
/// `Style` and `Theme` are plain data with no heap allocation, so the value can
/// cross the `cdylib` boundary by value.
#[derive(Clone, Debug)]
pub struct StyleSet {
    /// The palette.
    pub theme: Theme,
    /// Window background style.
    pub background: Style,
    /// The main glass card.
    pub card: Style,
    /// Large heading text.
    pub title: TextStyle,
    /// Secondary body text.
    pub body: TextStyle,
    /// Accent fill used by the counter pill.
    pub accent: Color,
    /// The accent pill behind the counter.
    pub pill: Style,
}

/// Build the current style set.
pub fn styles() -> StyleSet {
    let theme = Theme::dark();
    StyleSet {
        theme,
        background: Style::new().fill(theme.background),
        card: Style::new()
            .radius(Radius::continuous(30.0))
            .backdrop(Blur::dual_kawase(28.0).saturate(1.7))
            .tint(white(0.10))
            .border(1.0, white(0.24))
            .specular(Edge::top(), white(0.5))
            .padding(Edges::all(30.0))
            .gap(16.0)
            .flex_direction(FlexDirection::Column)
            .shadow([
                soft(0.0, 18.0, 42.0, black(0.35)),
                soft(0.0, 2.0, 6.0, black(0.30)),
            ]),
        title: TextStyle::new(30.0)
            .line_height(36.0)
            .weight(700)
            .color(theme.text),
        body: TextStyle::new(15.0).line_height(22.0).color(theme.text_dim),
        accent: rgb(0x8B7CFF),
        pill: Style::new()
            .radius(Radius::circular(999.0))
            .fill(rgb(0x8B7CFF))
            .padding(Edges::symmetric(20.0, 10.0))
            .text(TextStyle::new(15.0).weight(600).color(Color::WHITE)),
    }
}

/// The reloadable ABI entry point. The host looks this symbol up by name.
#[unsafe(no_mangle)]
pub fn gloss_styles() -> StyleSet {
    styles()
}
