use bytemuck::{Pod, Zeroable};
use gloss_core::{Border, Color, Edge, Fill, GradientKind, Radius, Shadow, Specular};

pub(crate) const KIND_SHAPE: u32 = 0;
pub(crate) const KIND_OUTER_SHADOW: u32 = 1;
pub(crate) const KIND_INNER_SHADOW: u32 = 2;
pub(crate) const KIND_GLASS: u32 = 3;

/// One 16-byte slot in the per-frame storage-buffer arena.
pub(crate) type GpuVec4 = [f32; 4];

/// A single GPU instance for the shape and glass pipelines.
///
/// Eight `vec4`s = 128 bytes. Everything that does not fit lives in the
/// per-frame arena, addressed by `params.w` (aux) and `gradient.z` (stops).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct GpuInstance {
    pub rect: [f32; 4],
    pub radii: [f32; 4],
    pub fill: [f32; 4],
    pub border: [f32; 4],
    pub spec: [f32; 4],
    pub params: [f32; 4],
    pub gradient: [f32; 4],
    pub clip: [f32; 4],
}

const _: () = assert!(std::mem::size_of::<GpuInstance>() == 128);

pub(crate) fn radii_array(radius: Radius) -> [f32; 4] {
    [
        radius.top_left,
        radius.top_right,
        radius.bottom_right,
        radius.bottom_left,
    ]
}

/// Per-draw context shared by the instance builders.
pub(crate) struct DrawCtx<'a> {
    pub arena: &'a mut Vec<GpuVec4>,
    /// Clip rectangle as `min_x, min_y, max_x, max_y` in pixels.
    pub clip: [f32; 4],
    pub clip_radius: f32,
    /// True when the clip is the whole viewport and can be skipped.
    pub no_clip: bool,
}

impl DrawCtx<'_> {
    fn push_aux(&mut self, first: GpuVec4, second: GpuVec4) -> f32 {
        let base = (self.arena.len() / 2) as f32;
        self.arena.push(first);
        self.arena.push(second);
        base
    }
}

fn pack_kind(kind: u32, edge: Edge, no_clip: bool) -> f32 {
    let mut flags = kind & 0x3;
    if edge.top > 0.0 {
        flags |= 1 << 2;
    }
    if edge.right > 0.0 {
        flags |= 1 << 3;
    }
    if edge.bottom > 0.0 {
        flags |= 1 << 4;
    }
    if edge.left > 0.0 {
        flags |= 1 << 5;
    }
    if no_clip {
        flags |= 1 << 6;
    }
    flags as f32
}

impl GpuInstance {
    fn base(ctx: &DrawCtx<'_>, rect: [f32; 4], radius: Radius, kind: u32, edge: Edge) -> Self {
        Self {
            rect,
            radii: radii_array(radius),
            fill: [0.0; 4],
            border: [0.0; 4],
            spec: [0.0; 4],
            params: [
                radius.smoothing,
                1.0,
                pack_kind(kind, edge, ctx.no_clip),
                0.0,
            ],
            gradient: [0.0, 90.0, 0.0, 0.0],
            clip: ctx.clip,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn shape(
        ctx: &mut DrawCtx<'_>,
        rect: [f32; 4],
        radius: Radius,
        fill_color: Color,
        gradient: [f32; 4],
        border: Option<Border>,
        specular: Option<Specular>,
        opacity: f32,
    ) -> Self {
        let edge = specular.map(|spec| spec.edge).unwrap_or_default();
        let mut out = Self::base(ctx, rect, radius, KIND_SHAPE, edge);
        let spec_width = specular.map(|spec| spec.width).unwrap_or(0.0);
        out.params[1] = opacity;
        out.params[3] = ctx.push_aux([spec_width, ctx.clip_radius, 0.0, 0.0], [0.0; 4]);
        out.fill = fill_color.to_array();
        out.gradient = gradient;
        if let Some(border) = border {
            out.border = [border.color.r, border.color.g, border.color.b, border.width];
        }
        if let Some(spec) = specular {
            out.spec = [spec.color.r, spec.color.g, spec.color.b, spec.intensity];
        }
        out
    }

    pub(crate) fn shadow(
        ctx: &mut DrawCtx<'_>,
        rect: [f32; 4],
        radius: Radius,
        shadow: Shadow,
        opacity: f32,
    ) -> Self {
        let kind = if shadow.inset {
            KIND_INNER_SHADOW
        } else {
            KIND_OUTER_SHADOW
        };
        let mut out = Self::base(ctx, rect, radius, kind, Edge::default());
        out.params[1] = opacity;
        out.params[3] = ctx.push_aux(
            [0.0, ctx.clip_radius, shadow.blur.max(0.0), shadow.spread],
            [0.0, 0.0, shadow.offset_x, shadow.offset_y],
        );
        out.fill = shadow.color.to_array();
        out
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn glass(
        ctx: &mut DrawCtx<'_>,
        rect: [f32; 4],
        radius: Radius,
        tint: Color,
        border: Option<Border>,
        specular: Option<Specular>,
        blur: Option<gloss_core::Blur>,
        refract: f32,
        dispersion: f32,
        opacity: f32,
    ) -> Self {
        let edge = specular.map(|spec| spec.edge).unwrap_or_default();
        let mut out = Self::base(ctx, rect, radius, KIND_GLASS, edge);
        let spec_width = specular.map(|spec| spec.width).unwrap_or(0.0);
        let (blur_radius, saturation) = blur
            .map(|blur| (blur.radius, blur.saturation))
            .unwrap_or((0.0, 1.0));
        out.params[1] = opacity;
        out.params[3] = ctx.push_aux(
            [spec_width, ctx.clip_radius, blur_radius, saturation],
            [refract, dispersion, 0.0, 0.0],
        );
        out.fill = tint.to_array();
        if let Some(border) = border {
            out.border = [border.color.r, border.color.g, border.color.b, border.width];
        }
        if let Some(spec) = specular {
            out.spec = [spec.color.r, spec.color.g, spec.color.b, spec.intensity];
        }
        out
    }
}

/// Append a fill's gradient stops to the arena and return
/// `[kind, angle, stop_base, stop_count]`.
pub(crate) fn gradient_array(fill: Fill, arena: &mut Vec<GpuVec4>) -> [f32; 4] {
    let Some(gradient) = fill.gradient else {
        return [0.0, 90.0, 0.0, 0.0];
    };
    let count = gradient.stop_count().min(4);
    if count == 0 {
        return [0.0, 90.0, 0.0, 0.0];
    }
    let base = (arena.len() / 2) as f32;
    for stop in &gradient.stops[..count] {
        arena.push(stop.color.to_array());
        arena.push([stop.offset, 0.0, 0.0, 0.0]);
    }
    let kind = match gradient.kind {
        GradientKind::Linear => 1.0,
        GradientKind::Radial => 2.0,
    };
    [kind, gradient.angle, base, count as f32]
}

/// A text atlas quad.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct GpuTextInstance {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
    pub color: [f32; 4],
    pub clip: [f32; 4],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aux_indices_are_pair_based() {
        let mut arena = Vec::new();
        let mut ctx = DrawCtx {
            arena: &mut arena,
            clip: [0.0; 4],
            clip_radius: 0.0,
            no_clip: true,
        };
        let first = ctx.push_aux([1.0; 4], [2.0; 4]);
        let second = ctx.push_aux([3.0; 4], [4.0; 4]);
        assert_eq!(first, 0.0);
        assert_eq!(second, 1.0);
        assert_eq!(ctx.arena[second as usize * 2], [3.0; 4]);
        assert_eq!(ctx.arena[second as usize * 2 + 1], [4.0; 4]);
    }
}
