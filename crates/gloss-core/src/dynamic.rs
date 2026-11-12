//! Dynamic values: either a static value or a tracked closure.
//!
//! [`Dynamic`] lets property bindings accept both plain values and reactive
//! closures without forcing every call site to wrap values in a closure.

use std::rc::Rc;

/// A closure whose result is tracked against Gloss signals when evaluated
/// inside an [`Effect`](crate::Effect).
#[derive(Clone)]
pub struct Derived<T> {
    run: Rc<dyn Fn() -> T>,
}

impl<T> Derived<T> {
    /// Evaluates the closure.
    #[inline]
    pub fn get(&self) -> T {
        (self.run)()
    }
}

impl<T> std::fmt::Debug for Derived<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Derived(..)")
    }
}

/// Creates a [`Derived`] value from a closure.
pub fn derived<T>(run: impl Fn() -> T + 'static) -> Derived<T> {
    Derived { run: Rc::new(run) }
}

/// A value that is either fixed or produced by a tracked closure.
pub enum Dynamic<T> {
    /// A fixed value.
    Static(T),
    /// A closure re-evaluated when its signals change.
    Derived(Derived<T>),
}

impl<T> Dynamic<T> {
    /// A fixed value.
    #[inline]
    pub const fn value(value: T) -> Self {
        Self::Static(value)
    }

    /// A tracked closure.
    pub fn derive(run: impl Fn() -> T + 'static) -> Self {
        Self::Derived(derived(run))
    }

    /// Reads the current value.
    #[inline]
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        match self {
            Self::Static(value) => value.clone(),
            Self::Derived(run) => run.get(),
        }
    }

    /// Whether this value is backed by a closure.
    #[inline]
    pub fn is_dynamic(&self) -> bool {
        matches!(self, Self::Derived(_))
    }

    /// The fixed value, when this is not derived.
    #[inline]
    pub fn as_static(&self) -> Option<&T> {
        match self {
            Self::Static(value) => Some(value),
            Self::Derived(_) => None,
        }
    }
}

impl<T> Clone for Dynamic<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        match self {
            Self::Static(value) => Self::Static(value.clone()),
            Self::Derived(run) => Self::Derived(run.clone()),
        }
    }
}

impl<T> std::fmt::Debug for Dynamic<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Static(value) => f.debug_tuple("Static").field(value).finish(),
            Self::Derived(_) => f.write_str("Derived(..)"),
        }
    }
}

impl<T> From<T> for Dynamic<T> {
    fn from(value: T) -> Self {
        Self::Static(value)
    }
}

impl<T> From<Derived<T>> for Dynamic<T> {
    fn from(value: Derived<T>) -> Self {
        Self::Derived(value)
    }
}

impl From<&str> for Dynamic<String> {
    fn from(value: &str) -> Self {
        Self::Static(value.to_owned())
    }
}

impl From<&String> for Dynamic<String> {
    fn from(value: &String) -> Self {
        Self::Static(value.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Effect, flush_effects, signal};

    #[test]
    fn static_values_read_back() {
        let value: Dynamic<u32> = 7.into();
        assert!(!value.is_dynamic());
        assert_eq!(value.get(), 7);
        assert_eq!(value.as_static(), Some(&7));
    }

    #[test]
    fn derived_values_track_signals() {
        let count = signal(1u32);
        let value = Dynamic::derive(move || count.get() * 2);
        assert!(value.is_dynamic());
        assert_eq!(value.get(), 2);

        let seen = std::rc::Rc::new(std::cell::Cell::new(0));
        let seen_effect = seen.clone();
        let _effect = Effect::new(move || seen_effect.set(value.get()));
        assert_eq!(seen.get(), 2);

        count.set(5);
        flush_effects();
        assert_eq!(seen.get(), 10);
    }
}
