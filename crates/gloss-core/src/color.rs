//! Color utilities.
//!
//! Colors are stored in **linear** space so that blending and interpolation are
//! physically correct. Constructors accept familiar sRGB values; the transfer
//! function used by the `const` constructor is a fast polynomial approximation
//! (maximum error < 0.5/255), which keeps `Color` constructible in `const`
//! contexts such as token tables.

/// A linear-space RGBA color.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Color {
    /// Red channel in linear space.
    pub r: f32,
    /// Green channel in linear space.
    pub g: f32,
    /// Blue channel in linear space.
    pub b: f32,
    /// Alpha, 0..1 (not premultiplied).
    pub a: f32,
}

/// Fast approximation of the sRGB electro-optical transfer function.
///
/// This is a well-known cubic approximation with a maximum error well below a
/// single 8-bit code value. It is `const` so colors can live in token tables.
#[inline]
pub const fn srgb_to_linear_approx(c: f32) -> f32 {
    c * (c * (c * 0.305_306_02 + 0.682_171_1) + 0.012_522_878)
}

/// Exact sRGB electro-optical transfer function.
#[inline]
pub fn srgb_to_linear_exact(c: f32) -> f32 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Exact inverse sRGB transfer function (linear -> sRGB).
#[inline]
pub fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

const fn channel(hex: u32, shift: u32) -> f32 {
    ((hex >> shift) & 0xFF) as f32 / 255.0
}

impl Color {
    /// Transparent black.
    pub const TRANSPARENT: Self = Self::new(0.0, 0.0, 0.0, 0.0);

    /// An opaque white.
    pub const WHITE: Self = Self::new(1.0, 1.0, 1.0, 1.0);

    /// An opaque black.
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0, 1.0);

    /// Build a linear color from raw channel values.
    #[inline]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Build a color from an sRGB hex literal, e.g. `rgb(0x6C5CE7)`.
    #[inline]
    pub const fn rgb(hex: u32) -> Self {
        Self::rgba(hex, 1.0)
    }

    /// Build a color from an sRGB hex literal and an alpha.
    #[inline]
    pub const fn rgba(hex: u32, a: f32) -> Self {
        Self {
            r: srgb_to_linear_approx(channel(hex, 16)),
            g: srgb_to_linear_approx(channel(hex, 8)),
            b: srgb_to_linear_approx(channel(hex, 0)),
            a,
        }
    }

    /// Build a color from raw sRGB components in 0..=1.
    #[inline]
    pub fn from_srgb(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self {
            r: srgb_to_linear_exact(r),
            g: srgb_to_linear_exact(g),
            b: srgb_to_linear_exact(b),
            a,
        }
    }

    /// Build a color from an 8-bit sRGB array.
    #[inline]
    pub fn from_srgb8(rgba: [u8; 4]) -> Self {
        Self::from_srgb(
            rgba[0] as f32 / 255.0,
            rgba[1] as f32 / 255.0,
            rgba[2] as f32 / 255.0,
            rgba[3] as f32 / 255.0,
        )
    }

    /// Return a copy with a new alpha.
    #[inline]
    pub const fn with_alpha(mut self, a: f32) -> Self {
        self.a = a;
        self
    }

    /// Return a copy with the alpha multiplied by `factor`.
    #[inline]
    pub const fn fade(mut self, factor: f32) -> Self {
        self.a *= factor;
        self
    }

    /// Linear interpolation between `self` and `other`.
    #[inline]
    pub fn mix(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    /// Add `amount` to each linear RGB channel, clamping to 1.
    #[inline]
    pub fn brighten(self, amount: f32) -> Self {
        Self {
            r: (self.r + amount).clamp(0.0, 1.0),
            g: (self.g + amount).clamp(0.0, 1.0),
            b: (self.b + amount).clamp(0.0, 1.0),
            a: self.a,
        }
    }

    /// Subtract `amount` from each linear RGB channel, clamping to 0.
    #[inline]
    pub fn darken(self, amount: f32) -> Self {
        Self {
            r: (self.r - amount).clamp(0.0, 1.0),
            g: (self.g - amount).clamp(0.0, 1.0),
            b: (self.b - amount).clamp(0.0, 1.0),
            a: self.a,
        }
    }

    /// Premultiply RGB by alpha.
    #[inline]
    pub const fn premultiplied(self) -> [f32; 4] {
        [self.r * self.a, self.g * self.a, self.b * self.a, self.a]
    }

    /// Straight (non-premultiplied) linear RGBA.
    #[inline]
    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Straight linear RGBA with alpha premultiplied, as expected by the GPU.
    #[inline]
    pub const fn to_premultiplied_array(self) -> [f32; 4] {
        self.premultiplied()
    }

    /// Relative luminance (Rec. 709) of the linear RGB channels.
    #[inline]
    pub fn luminance(self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    /// Convert to an 8-bit sRGB array.
    pub fn to_srgb8(self) -> [u8; 4] {
        [
            to_u8(linear_to_srgb(self.r)),
            to_u8(linear_to_srgb(self.g)),
            to_u8(linear_to_srgb(self.b)),
            to_u8(self.a),
        ]
    }

    /// Convert to an 8-bit sRGB array, blending against an opaque background and
    /// compositing in linear space. Useful for tests and thumbnails.
    pub fn to_srgb8_over(self, background: Color) -> [u8; 4] {
        let c = self.premultiplied();
        let b = background.premultiplied();
        let inv = 1.0 - c[3];
        let out = [
            c[0] + b[0] * inv,
            c[1] + b[1] * inv,
            c[2] + b[2] * inv,
            c[3] + b[3] * inv,
        ];
        [
            to_u8(linear_to_srgb(out[0])),
            to_u8(linear_to_srgb(out[1])),
            to_u8(linear_to_srgb(out[2])),
            to_u8(out[3]),
        ]
    }
}

#[inline]
fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Opaque sRGB color from a hex literal.
#[inline]
pub const fn rgb(hex: u32) -> Color {
    Color::rgb(hex)
}

/// sRGB color with alpha from a hex literal.
#[inline]
pub const fn rgba(hex: u32, a: f32) -> Color {
    Color::rgba(hex, a)
}

/// Opaque white with the given alpha (linear).
#[inline]
pub const fn white(a: f32) -> Color {
    Color::new(1.0, 1.0, 1.0, a)
}

/// Opaque black with the given alpha (linear).
#[inline]
pub const fn black(a: f32) -> Color {
    Color::new(0.0, 0.0, 0.0, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approximation_matches_exact() {
        for i in 0..=255 {
            let c = i as f32 / 255.0;
            let approx = srgb_to_linear_approx(c);
            let exact = srgb_to_linear_exact(c);
            assert!(
                (approx - exact).abs() < 0.005,
                "channel {i}: {approx} vs {exact}"
            );
        }
    }

    #[test]
    fn roundtrips_srgb() {
        let c = rgb(0x6C5CE7);
        let back = Color::from_srgb8(c.to_srgb8());
        assert!((back.r - c.r).abs() < 0.01);
        assert!((back.g - c.g).abs() < 0.01);
        assert!((back.b - c.b).abs() < 0.01);
    }

    #[test]
    fn mixing_hits_endpoints() {
        let a = rgb(0x000000);
        let b = rgb(0xFFFFFF);
        assert_eq!(a.mix(b, 0.0), a);
        assert_eq!(a.mix(b, 1.0), b);
    }
}
