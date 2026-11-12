//! Property bitflags, easing, springs and style transitions.

use crate::style::ResolvedStyle;
use std::ops::BitOr;

/// A set of animatable style properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Props(u32);

/// Alias kept for the v1 name.
pub type Prop = Props;

impl Props {
    /// The empty set.
    pub const NONE: Self = Self(0);
    /// The fill.
    pub const FILL: Self = Self(1 << 0);
    /// The tint.
    pub const TINT: Self = Self(1 << 1);
    /// The border.
    pub const BORDER: Self = Self(1 << 2);
    /// The shadow stack.
    pub const SHADOW: Self = Self(1 << 3);
    /// Corner radii.
    pub const RADIUS: Self = Self(1 << 4);
    /// Opacity.
    pub const OPACITY: Self = Self(1 << 5);
    /// Geometric transform.
    pub const TRANSFORM: Self = Self(1 << 6);
    /// Text style.
    pub const TEXT: Self = Self(1 << 7);
    /// Backdrop blur.
    pub const BACKDROP: Self = Self(1 << 8);
    /// Refraction and dispersion.
    pub const REFRACT: Self = Self(1 << 9);
    /// Uniform scale.
    pub const SCALE: Self = Self(1 << 10);
    /// Vertical translation.
    pub const OFFSET_Y: Self = Self(1 << 11);
    /// Horizontal translation.
    pub const OFFSET_X: Self = Self(1 << 12);
    /// Specular rim.
    pub const SPECULAR: Self = Self(1 << 13);
    /// All animatable properties.
    pub const ALL: Self = Self((1 << 14) - 1);

    /// Whether `self` contains `other`.
    #[inline]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether the set is empty.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Union of two sets.
    #[inline]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Intersection of two sets.
    #[inline]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

impl BitOr for Props {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

/// Convert a millisecond duration to seconds.
#[inline]
pub const fn ms(milliseconds: f32) -> f32 {
    milliseconds * 0.001
}

/// An easing curve.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Ease {
    /// Linear.
    Linear,
    /// Slow start (cubic).
    In,
    /// Slow end (cubic).
    Out,
    /// Slow start and end (cubic).
    InOut,
    /// A cubic Bezier curve.
    Bezier {
        /// First control point x.
        x1: f32,
        /// First control point y.
        y1: f32,
        /// Second control point x.
        x2: f32,
        /// Second control point y.
        y2: f32,
    },
}

impl Ease {
    /// The standard ease-out curve.
    #[inline]
    pub const fn out() -> Self {
        Self::Out
    }

    /// The standard ease-in curve.
    #[inline]
    pub const fn ease_in() -> Self {
        Self::In
    }

    /// Evaluate the curve at `t` in 0..=1.
    pub fn eval(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::In => t * t * t,
            Self::Out => {
                let u = 1.0 - t;
                1.0 - u * u * u
            }
            Self::InOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let u = -2.0 * t + 2.0;
                    1.0 - u * u * u / 2.0
                }
            }
            Self::Bezier { x1, y1, x2, y2 } => cubic_bezier(x1, y1, x2, y2, t),
        }
    }
}

fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    // Newton-Raphson to invert the x polynomial, with a bisection fallback.
    let sample_x = |t: f32| {
        let u = 1.0 - t;
        3.0 * u * u * t * x1 + 3.0 * u * t * t * x2 + t * t * t
    };
    let sample_y = |t: f32| {
        let u = 1.0 - t;
        3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t
    };
    let derivative_x = |t: f32| {
        let u = 1.0 - t;
        3.0 * u * u * x1 + 6.0 * u * t * (x2 - x1) + 3.0 * t * t * (1.0 - x2)
    };

    let mut t = x;
    for _ in 0..8 {
        let error = sample_x(t) - x;
        if error.abs() < 1e-5 {
            return sample_y(t);
        }
        let d = derivative_x(t);
        if d.abs() < 1e-6 {
            break;
        }
        t -= error / d;
        t = t.clamp(0.0, 1.0);
    }

    let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
    for _ in 0..24 {
        t = 0.5 * (lo + hi);
        let sx = sample_x(t);
        if (sx - x).abs() < 1e-5 {
            return sample_y(t);
        }
        if sx < x {
            lo = t;
        } else {
            hi = t;
        }
    }
    sample_y(t)
}

/// A damped spring.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Spring {
    /// Spring stiffness.
    pub stiffness: f32,
    /// Damping coefficient.
    pub damping: f32,
    /// Mass.
    pub mass: f32,
}

impl Spring {
    /// Create a spring.
    #[inline]
    pub const fn new(stiffness: f32, damping: f32) -> Self {
        Self {
            stiffness,
            damping,
            mass: 1.0,
        }
    }

    /// A gentle spring, good for hover lift.
    #[inline]
    pub const fn gentle() -> Self {
        Self::new(170.0, 20.0)
    }

    /// A snappy spring, good for press feedback.
    #[inline]
    pub const fn snappy() -> Self {
        Self::new(420.0, 30.0)
    }

    /// A bouncy spring.
    #[inline]
    pub const fn bouncy() -> Self {
        Self::new(300.0, 12.0)
    }

    /// Set the mass.
    #[inline]
    pub const fn mass(mut self, mass: f32) -> Self {
        self.mass = mass;
        self
    }

    /// Integrate one step. Returns the new `(value, velocity)`.
    pub fn update(&self, value: f32, velocity: f32, target: f32, dt: f32) -> (f32, f32) {
        let dt = dt.clamp(0.0, 1.0 / 30.0);
        let mass = self.mass.max(0.0001);
        let acceleration = (-self.stiffness * (value - target) - self.damping * velocity) / mass;
        let velocity = velocity + acceleration * dt;
        let value = value + velocity * dt;
        (value, velocity)
    }
}

impl Default for Spring {
    fn default() -> Self {
        Self::gentle()
    }
}

/// A scalar animated by a spring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringValue {
    /// Current value.
    pub value: f32,
    /// Current velocity.
    pub velocity: f32,
    /// Target value.
    pub target: f32,
    /// Spring parameters.
    pub spring: Spring,
}

impl SpringValue {
    /// Create a spring value at rest.
    #[inline]
    pub fn new(value: f32) -> Self {
        Self {
            value,
            velocity: 0.0,
            target: value,
            spring: Spring::gentle(),
        }
    }

    /// Retarget the value.
    #[inline]
    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    /// Step the spring, returning `true` while it is still moving.
    pub fn update(&mut self, dt: f32) -> bool {
        let (value, velocity) = self
            .spring
            .update(self.value, self.velocity, self.target, dt);
        self.value = value;
        self.velocity = velocity;
        if (self.value - self.target).abs() < 1e-3 && self.velocity.abs() < 1e-3 {
            self.value = self.target;
            self.velocity = 0.0;
            false
        } else {
            true
        }
    }

    /// Whether the spring has settled.
    #[inline]
    pub fn is_settled(&self) -> bool {
        (self.value - self.target).abs() < 1e-3 && self.velocity.abs() < 1e-3
    }
}

/// How a style transitions between states.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Transition {
    /// Properties that transition; others snap.
    pub props: Props,
    /// Duration in seconds.
    pub duration: f32,
    /// Easing curve.
    pub ease: Ease,
    /// Optional spring; overrides `duration`/`ease` when present.
    pub spring: Option<Spring>,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            props: Props::NONE,
            duration: 0.15,
            ease: Ease::Out,
            spring: None,
        }
    }
}

/// Animates a [`ResolvedStyle`] towards a target style.
#[derive(Clone, Debug)]
pub struct StyleAnimator {
    current: ResolvedStyle,
    from: ResolvedStyle,
    to: ResolvedStyle,
    elapsed: f32,
    duration: f32,
    ease: Ease,
    spring: Option<Spring>,
    spring_t: SpringValue,
    props: Props,
    active: bool,
}

impl StyleAnimator {
    /// Create an animator already at rest at `style`.
    pub fn new(style: ResolvedStyle) -> Self {
        Self {
            current: style,
            from: style,
            to: style,
            elapsed: 0.0,
            duration: 0.0,
            ease: Ease::Linear,
            spring: None,
            spring_t: SpringValue {
                value: 1.0,
                velocity: 0.0,
                target: 1.0,
                spring: Spring::gentle(),
            },
            props: Props::NONE,
            active: false,
        }
    }

    /// The current interpolated style.
    #[inline]
    pub fn current(&self) -> &ResolvedStyle {
        &self.current
    }

    /// Whether a transition is in flight.
    #[inline]
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Immediately jump to a target.
    pub fn snap_to(&mut self, target: ResolvedStyle) {
        self.current = target;
        self.from = target;
        self.to = target;
        self.active = false;
        self.elapsed = 0.0;
        self.props = Props::NONE;
    }

    /// Move towards `target`. If the style declares no transition the change is
    /// applied immediately.
    pub fn set_target(&mut self, target: ResolvedStyle, transition: Transition) {
        if target == self.to {
            return;
        }
        let animated = !transition.props.is_empty()
            && (transition.spring.is_some() || transition.duration > 0.0);
        if !animated {
            self.snap_to(target);
            return;
        }
        self.from = self.current;
        self.to = target;
        self.elapsed = 0.0;
        self.duration = transition.duration.max(0.0001);
        self.ease = transition.ease;
        self.spring = transition.spring;
        self.props = transition.props;
        if let Some(spring) = transition.spring {
            self.spring_t = SpringValue {
                value: 0.0,
                velocity: 0.0,
                target: 1.0,
                spring,
            };
        }
        self.active = true;
    }

    /// Advance the transition by `dt` seconds. Returns `true` if still active.
    pub fn tick(&mut self, dt: f32) -> bool {
        if !self.active {
            return false;
        }
        self.elapsed += dt.max(0.0);
        let t = if self.spring.is_some() {
            self.spring_t.update(dt);
            self.spring_t.value.clamp(0.0, 1.0)
        } else {
            (self.elapsed / self.duration).clamp(0.0, 1.0)
        };
        if t >= 1.0 {
            self.current = self.to;
            self.active = false;
            return false;
        }
        let eased = if self.spring.is_some() {
            t
        } else {
            self.ease.eval(t)
        };
        self.current = self.masked(self.from, self.to, eased);
        true
    }

    fn masked(&self, from: ResolvedStyle, to: ResolvedStyle, t: f32) -> ResolvedStyle {
        let mut next = ResolvedStyle::lerp(&from, &to, t);
        let props = self.props;
        if !props.contains(Props::FILL) {
            next.fill = to.fill;
        }
        if !props.contains(Props::TINT) {
            next.tint = to.tint;
        }
        if !props.contains(Props::BORDER) {
            next.border = to.border;
        }
        if !props.contains(Props::SHADOW) {
            next.shadows = to.shadows;
        }
        if !props.contains(Props::RADIUS) {
            next.radius = to.radius;
        }
        if !props.contains(Props::OPACITY) {
            next.opacity = to.opacity;
        }
        if !props.contains(Props::TRANSFORM) {
            if !props.contains(Props::OFFSET_X) {
                next.transform.translate_x = to.transform.translate_x;
            }
            if !props.contains(Props::OFFSET_Y) {
                next.transform.translate_y = to.transform.translate_y;
            }
            if !props.contains(Props::SCALE) {
                next.transform.scale = to.transform.scale;
            }
        }
        if !props.contains(Props::TEXT) {
            next.text = to.text;
        }
        if !props.contains(Props::BACKDROP) {
            next.backdrop = to.backdrop;
        }
        if !props.contains(Props::REFRACT) {
            next.refract = to.refract;
            next.dispersion = to.dispersion;
        }
        if !props.contains(Props::SPECULAR) {
            next.specular = to.specular;
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Style, WidgetState};

    #[test]
    fn animator_only_interpolates_declared_props() {
        let from = Style::new()
            .opacity(1.0)
            .fill(crate::color::Color::rgb(0x000000))
            .resolve(WidgetState::idle());
        let to = Style::new()
            .opacity(0.0)
            .fill(crate::color::Color::rgb(0xffffff))
            .resolve(WidgetState::idle());
        let mut animator = StyleAnimator::new(from);
        animator.set_target(
            to,
            Transition {
                props: Props::OPACITY,
                duration: 1.0,
                ease: Ease::Linear,
                spring: None,
            },
        );
        animator.tick(0.5);
        let current = animator.current();
        assert!((current.opacity - 0.5).abs() < 1e-3);
        assert_eq!(current.fill, to.fill);
    }

    #[test]
    fn prop_bits() {
        let p = Props::SHADOW | Props::TINT;
        assert!(p.contains(Props::SHADOW));
        assert!(p.contains(Props::TINT));
        assert!(!p.contains(Props::FILL));
        assert!(Props::ALL.contains(Props::REFRACT));
        assert!(Props::ALL.contains(Props::SPECULAR));
    }

    #[test]
    fn easing_endpoints() {
        for ease in [
            Ease::Linear,
            Ease::In,
            Ease::Out,
            Ease::InOut,
            Ease::Bezier {
                x1: 0.25,
                y1: 0.1,
                x2: 0.25,
                y2: 1.0,
            },
        ] {
            assert!((ease.eval(0.0) - 0.0).abs() < 1e-4);
            assert!((ease.eval(1.0) - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn spring_settles() {
        let mut s = SpringValue::new(0.0);
        s.set_target(1.0);
        let mut steps = 0;
        while s.update(1.0 / 120.0) {
            steps += 1;
            assert!(steps < 10_000);
        }
        assert!((s.value - 1.0).abs() < 1e-2);
    }
}
