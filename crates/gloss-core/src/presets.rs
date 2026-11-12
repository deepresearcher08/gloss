//! Ready-made style presets.

use crate::animation::{Ease, Prop, ms};
use crate::color::{Color, black, white};
use crate::style::{Blur, Edge, Radius, Specular, Style, TextStyle, inner, soft};
use crate::theme::Theme;

fn base_transition(style: Style) -> Style {
    style
        .transition(Prop::ALL, ms(150.0), Ease::out())
        .hover(|s| s.lift(1.0).brighten(0.02))
        .pressed(|s| s.scale(0.985))
        .disabled(|s| s.opacity(0.45))
}

/// The classic glass panel: blurred backdrop, soft tint, light rim.
pub fn glass(theme: &Theme) -> Style {
    base_transition(
        Style::new()
            .radius(Radius::continuous(24.0))
            .backdrop(Blur::dual_kawase(theme.blur).saturate(theme.saturation))
            .tint(white(0.12))
            .border(1.0, white(0.25))
            .specular(Edge::top(), white(0.5))
            .shadow([
                soft(0.0, 8.0, 24.0, black(0.25)),
                soft(0.0, 1.0, 2.0, black(0.4)),
            ]),
    )
}

/// Alias used by the examples.
pub fn glass_panel(theme: &Theme) -> Style {
    glass(theme)
}

/// Liquid Glass: refraction, dispersion and a prominent specular rim.
pub fn liquid_glass(theme: &Theme) -> Style {
    glass(theme)
        .refract(0.04)
        .dispersion(0.012)
        .specular_width(1.25)
        .specular_intensity(1.35)
        .radius(Radius::continuous(28.0))
        .shadow([
            soft(0.0, 12.0, 32.0, black(0.32)),
            soft(0.0, 2.0, 4.0, black(0.28)),
        ])
}

/// Neumorphism: matching fill with opposing soft light and dark shadows.
pub fn neumorphic(theme: &Theme) -> Style {
    base_transition(
        Style::new()
            .radius(Radius::continuous(20.0))
            .fill(theme.surface)
            .shadow([
                soft(-6.0, -6.0, 16.0, white(0.55)),
                soft(6.0, 6.0, 18.0, black(0.18)),
            ])
            .text(theme.body_text()),
    )
}

/// Claymorphism: a puffy solid surface with an inner highlight and inner shade.
pub fn clay(theme: &Theme) -> Style {
    base_transition(
        Style::new()
            .radius(Radius::continuous(28.0))
            .fill(theme.surface_elevated)
            .shadow([
                soft(0.0, 14.0, 30.0, black(0.22)),
                inner(0.0, -3.0, 8.0, white(0.35)),
                inner(0.0, 5.0, 12.0, black(0.16)),
            ])
            .text(theme.body_text()),
    )
}

/// A frosted sidebar surface.
pub fn frosted_sidebar(theme: &Theme) -> Style {
    Style::new()
        .radius(Radius::circular(0.0))
        .backdrop(Blur::dual_kawase(theme.blur * 1.25).saturate(theme.saturation))
        .tint(if theme.dark { white(0.06) } else { white(0.55) })
        .border(1.0, white(if theme.dark { 0.12 } else { 0.7 }))
        .specular(Edge::top_left(), white(0.35))
        .text(theme.body_text())
}

/// A vibrant toolbar surface.
pub fn vibrant_toolbar(theme: &Theme) -> Style {
    Style::new()
        .radius(Radius::continuous(theme.radius_md))
        .backdrop(Blur::dual_kawase(theme.blur * 0.8).saturate(theme.saturation))
        .tint(if theme.dark { white(0.08) } else { white(0.6) })
        .border(1.0, white(if theme.dark { 0.14 } else { 0.8 }))
        .specular(Edge::top(), white(0.4))
        .text(theme.body_text())
}

/// Button variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Filled with the accent color.
    Primary,
    /// A translucent surface.
    Secondary,
    /// Transparent until hovered.
    Ghost,
}

/// A button style.
pub fn button(theme: &Theme, variant: ButtonVariant) -> Style {
    let radius = Radius::continuous(theme.radius_md);
    match variant {
        ButtonVariant::Primary => base_transition(
            Style::new()
                .radius(radius)
                .fill(theme.accent)
                .border(1.0, white(0.18))
                .specular(Edge::top(), white(0.35))
                .shadow([
                    soft(0.0, 6.0, 18.0, theme.accent.fade(0.35)),
                    soft(0.0, 1.0, 3.0, black(0.35)),
                ])
                .text(TextStyle::new(theme.font_size).color(theme.on_accent))
                .hover(|s| s.lift(1.5).brighten(0.04))
                .pressed(|s| s.scale(0.97)),
        ),
        ButtonVariant::Secondary => base_transition(
            Style::new()
                .radius(radius)
                .fill(if theme.dark { white(0.08) } else { white(0.7) })
                .border(1.0, if theme.dark { white(0.16) } else { black(0.10) })
                .specular(Edge::top(), white(0.25))
                .shadow([soft(0.0, 2.0, 8.0, black(0.16))])
                .text(TextStyle::new(theme.font_size).color(theme.text))
                .hover(|s| s.brighten(0.04))
                .pressed(|s| s.scale(0.97)),
        ),
        ButtonVariant::Ghost => base_transition(
            Style::new()
                .radius(radius)
                .fill(Color::TRANSPARENT)
                .text(TextStyle::new(theme.font_size).color(theme.text))
                .hover(|s| s.tint(white(0.06)))
                .pressed(|s| s.scale(0.97)),
        ),
    }
}

/// Alias with the name used in the core API sketch.
pub fn button_style(theme: &Theme, variant: ButtonVariant) -> Style {
    button(theme, variant)
}

/// A card surface style.
pub fn card(theme: &Theme) -> Style {
    Style::new()
        .radius(Radius::continuous(theme.radius_lg))
        .fill(theme.surface)
        .border(1.0, theme.border)
        .shadow([soft(0.0, 10.0, 28.0, black(0.20))])
        .padding(crate::style::Edges::all(crate::tokens::SPACE_6))
        .flex_direction(crate::style::FlexDirection::Column)
        .gap(crate::tokens::SPACE_3)
        .text(theme.body_text())
}

/// The default specular used by most presets.
#[inline]
pub fn glass_specular() -> Specular {
    Specular {
        edge: Edge::top(),
        color: white(0.5),
        width: 1.0,
        intensity: 1.0,
    }
}
