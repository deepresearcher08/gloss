//! Single-threaded reactive signals with tracked effects.
//!
//! Signals are copyable generational handles into a thread-local arena. Reading
//! a signal inside an effect records a dependency; writing it marks dependent
//! effects dirty until [`flush_effects`] runs them.

use std::any::Any;
use std::cell::RefCell;
use std::marker::PhantomData;

thread_local! {
    static RUNTIME: RefCell<Runtime> = RefCell::new(Runtime::default());
}

#[derive(Default)]
struct Runtime {
    slots: Vec<Slot>,
    free: Vec<u32>,
    effects: Vec<EffectSlot>,
    effect_free: Vec<u32>,
    tracking: RefCell<Option<Vec<u32>>>,
    current: Option<u32>,
    dirty: Vec<u32>,
}

struct Slot {
    generation: u32,
    value: Option<Box<dyn Any>>,
    subscribers: Vec<u32>,
}

struct EffectSlot {
    generation: u32,
    alive: bool,
    deps: Vec<u32>,
    run: Option<Box<dyn FnMut()>>,
}

/// A copyable handle to a reactive value.
///
/// Handles are tied to a generational arena slot, so a stale handle is detected
/// instead of aliasing a newer value.
pub struct Signal<T> {
    index: u32,
    generation: u32,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Copy for Signal<T> {}
impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Signal<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Signal")
            .field("index", &self.index)
            .field("generation", &self.generation)
            .finish()
    }
}

impl<T: 'static> Signal<T> {
    /// Creates a signal on the current thread.
    pub fn new(value: T) -> Self {
        let (index, generation) = RUNTIME.with(|runtime| {
            let mut runtime = runtime.borrow_mut();
            let index = match runtime.free.pop() {
                Some(index) => {
                    let slot = &mut runtime.slots[index as usize];
                    slot.generation = slot.generation.wrapping_add(1);
                    slot.value = Some(Box::new(value));
                    slot.subscribers.clear();
                    index
                }
                None => {
                    runtime.slots.push(Slot {
                        generation: 0,
                        value: Some(Box::new(value)),
                        subscribers: Vec::new(),
                    });
                    (runtime.slots.len() - 1) as u32
                }
            };
            let generation = runtime.slots[index as usize].generation;
            (index, generation)
        });
        let signal = Self {
            index,
            generation,
            _marker: PhantomData,
        };
        record_dependency(signal.index);
        signal
    }

    /// Returns whether the handle still points at a live value.
    pub fn is_alive(&self) -> bool {
        RUNTIME.with(|runtime| {
            let runtime = runtime.borrow();
            runtime
                .slots
                .get(self.index as usize)
                .is_some_and(|slot| slot.generation == self.generation && slot.value.is_some())
        })
    }

    /// Reads the value, subscribing the current effect.
    ///
    /// # Panics
    ///
    /// Panics when the handle is stale.
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.with(Clone::clone)
    }

    /// Reads the value through a closure, subscribing the current effect.
    ///
    /// # Panics
    ///
    /// Panics when the handle is stale.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        RUNTIME.with(|runtime| {
            let runtime = runtime.borrow();
            let slot = runtime
                .slots
                .get(self.index as usize)
                .filter(|slot| slot.generation == self.generation && slot.value.is_some())
                .unwrap_or_else(|| panic!("stale signal handle"));
            let value = slot
                .value
                .as_ref()
                .and_then(|value| value.downcast_ref::<T>())
                .unwrap_or_else(|| panic!("signal type mismatch"));
            let output = f(value);
            record_dependency(self.index);
            output
        })
    }

    /// Reads the value, returning `None` when the handle is stale.
    pub fn try_get(&self) -> Option<T>
    where
        T: Clone,
    {
        if !self.is_alive() {
            return None;
        }
        Some(self.get())
    }

    /// Replaces the value and marks subscribers dirty.
    ///
    /// # Panics
    ///
    /// Panics when the handle is stale.
    pub fn set(&self, value: T) {
        self.update(|slot| *slot = value);
    }

    /// Mutates the value and marks subscribers dirty.
    ///
    /// # Panics
    ///
    /// Panics when the handle is stale.
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        RUNTIME.with(|runtime| {
            let mut runtime = runtime.borrow_mut();
            let slot = runtime
                .slots
                .get_mut(self.index as usize)
                .filter(|slot| slot.generation == self.generation && slot.value.is_some())
                .unwrap_or_else(|| panic!("stale signal handle"));
            {
                let value = slot
                    .value
                    .as_mut()
                    .and_then(|value| value.downcast_mut::<T>())
                    .unwrap_or_else(|| panic!("signal type mismatch"));
                f(value);
            }
            mark_subscribers(&mut runtime, self.index);
        });
    }

    /// Drops the value and disposes all subscriptions.
    pub fn dispose(&self) {
        RUNTIME.with(|runtime| {
            let mut runtime = runtime.borrow_mut();
            let Some(slot) = runtime.slots.get_mut(self.index as usize) else {
                return;
            };
            if slot.generation != self.generation || slot.value.is_none() {
                return;
            }
            slot.value = None;
            slot.subscribers.clear();
            slot.generation = slot.generation.wrapping_add(1);
            runtime.free.push(self.index);
        });
    }
}

/// Convenience constructor matching `let count = signal(0);`.
pub fn signal<T: 'static>(value: T) -> Signal<T> {
    Signal::new(value)
}

/// A tracked reactive closure.
///
/// Effects run once at creation, then re-run when any signal they read changes.
/// Dropping an effect unsubscribes it.
pub struct Effect {
    index: u32,
    generation: u32,
}

impl Effect {
    /// Creates and immediately runs a tracked effect.
    pub fn new(run: impl FnMut() + 'static) -> Self {
        RUNTIME.with(|runtime| {
            let mut runtime = runtime.borrow_mut();
            let index = match runtime.effect_free.pop() {
                Some(index) => {
                    let slot = &mut runtime.effects[index as usize];
                    slot.generation = slot.generation.wrapping_add(1);
                    slot.alive = true;
                    slot.deps.clear();
                    slot.run = Some(Box::new(run));
                    index
                }
                None => {
                    runtime.effects.push(EffectSlot {
                        generation: 0,
                        alive: true,
                        deps: Vec::new(),
                        run: Some(Box::new(run)),
                    });
                    (runtime.effects.len() - 1) as u32
                }
            };
            let generation = runtime.effects[index as usize].generation;
            drop(runtime);
            run_effect(index);
            Self { index, generation }
        })
    }

    /// Returns whether the effect is still alive.
    pub fn is_alive(&self) -> bool {
        RUNTIME.with(|runtime| {
            let runtime = runtime.borrow();
            runtime
                .effects
                .get(self.index as usize)
                .is_some_and(|slot| slot.generation == self.generation && slot.alive)
        })
    }
}

impl Drop for Effect {
    fn drop(&mut self) {
        RUNTIME.with(|runtime| {
            let mut runtime = runtime.borrow_mut();
            let Some(slot) = runtime.effects.get_mut(self.index as usize) else {
                return;
            };
            if slot.generation != self.generation || !slot.alive {
                return;
            }
            slot.alive = false;
            slot.run = None;
            detach(&mut runtime, self.index);
            runtime.effect_free.push(self.index);
        });
    }
}

/// Creates a tracked effect (see [`Effect`]).
pub fn effect(run: impl FnMut() + 'static) -> Effect {
    Effect::new(run)
}

/// Runs every effect that was marked dirty by a signal write.
///
/// Returns the number of effect runs performed.
pub fn flush_effects() -> usize {
    let mut runs = 0;
    loop {
        let dirty = RUNTIME.with(|runtime| {
            let mut runtime = runtime.borrow_mut();
            std::mem::take(&mut runtime.dirty)
        });
        if dirty.is_empty() {
            return runs;
        }
        for index in dirty {
            run_effect(index);
            runs += 1;
            if runs > 10_000 {
                panic!("effect flush did not settle");
            }
        }
    }
}

/// Returns whether any effect is waiting to run.
pub fn effects_pending() -> bool {
    RUNTIME.with(|runtime| !runtime.borrow().dirty.is_empty())
}

fn record_dependency(index: u32) {
    RUNTIME.with(|runtime| {
        let runtime = runtime.borrow();
        if let Some(tracking) = runtime.tracking.borrow_mut().as_mut() {
            if !tracking.contains(&index) {
                tracking.push(index);
            }
        }
    });
}

fn mark_subscribers(runtime: &mut Runtime, index: u32) {
    let subscribers = runtime.slots[index as usize].subscribers.clone();
    for subscriber in subscribers {
        if !runtime.dirty.contains(&subscriber) {
            runtime.dirty.push(subscriber);
        }
    }
}

fn detach(runtime: &mut Runtime, effect: u32) {
    let deps = std::mem::take(&mut runtime.effects[effect as usize].deps);
    for dep in deps {
        if let Some(slot) = runtime.slots.get_mut(dep as usize) {
            slot.subscribers.retain(|subscriber| *subscriber != effect);
        }
    }
}

fn run_effect(index: u32) {
    let Some((mut run, outer_tracking, outer_current)) = RUNTIME.with(|runtime| {
        let mut runtime = runtime.borrow_mut();
        if !runtime
            .effects
            .get(index as usize)
            .is_some_and(|slot| slot.alive)
        {
            return None;
        }
        detach(&mut runtime, index);
        let outer_tracking = runtime.tracking.replace(Some(Vec::new()));
        let outer_current = runtime.current.replace(index);
        let run = runtime.effects[index as usize].run.take()?;
        Some((run, outer_tracking, outer_current))
    }) else {
        return;
    };

    run();

    RUNTIME.with(|runtime| {
        let mut runtime = runtime.borrow_mut();
        let deps = runtime.tracking.replace(None).unwrap_or_default();
        runtime.tracking.replace(outer_tracking);
        runtime.current = outer_current;
        let Some(slot) = runtime.effects.get_mut(index as usize) else {
            return;
        };
        slot.run = Some(run);
        slot.deps = deps.clone();
        for dep in deps {
            if let Some(slot) = runtime.slots.get_mut(dep as usize) {
                if !slot.subscribers.contains(&index) {
                    slot.subscribers.push(index);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn effect_tracks_and_reruns() {
        let first = signal(1);
        let second = signal(10);
        let runs = Rc::new(Cell::new(0));
        let total = Rc::new(Cell::new(0));
        let effect = {
            let runs = runs.clone();
            let total = total.clone();
            effect(move || {
                runs.set(runs.get() + 1);
                total.set(first.get() + second.get());
            })
        };
        assert_eq!(runs.get(), 1);
        assert_eq!(total.get(), 11);

        first.set(2);
        assert!(effects_pending());
        assert_eq!(flush_effects(), 1);
        assert_eq!(total.get(), 12);

        second.update(|value| *value += 5);
        flush_effects();
        assert_eq!(total.get(), 17);
        assert_eq!(runs.get(), 3);

        drop(effect);
        first.set(100);
        assert_eq!(flush_effects(), 0);
    }

    #[test]
    fn nested_effects_track_independently() {
        let outer_signal = signal(1);
        let inner_signal = signal(2);
        let outer_seen = Rc::new(Cell::new(0));
        let inner_seen = Rc::new(Cell::new(0));
        let outer_observed = outer_seen.clone();
        let inner_observed = inner_seen.clone();
        let inner = Rc::new(RefCell::new(None));

        {
            let outer_seen = outer_seen.clone();
            let inner_seen = inner_seen.clone();
            let inner = inner.clone();
            let _outer = effect(move || {
                outer_seen.set(outer_signal.get());
                if inner.borrow().is_none() {
                    let inner_seen = inner_seen.clone();
                    *inner.borrow_mut() = Some(effect(move || {
                        inner_seen.set(inner_signal.get());
                    }));
                }
            });

            inner_signal.set(20);
            assert_eq!(flush_effects(), 1);
            assert_eq!(inner_observed.get(), 20);
            assert_eq!(outer_observed.get(), 1);

            outer_signal.set(3);
            flush_effects();
            assert_eq!(outer_observed.get(), 3);
        }
    }

    #[test]
    fn stale_handles_are_detected_after_reuse() {
        let first = signal(1u8);
        assert!(first.is_alive());
        first.dispose();
        assert!(!first.is_alive());
        assert!(first.try_get().is_none());

        let second = signal(9u8);
        assert_eq!(second.get(), 9);
        assert!(!first.is_alive());
        assert!(first.try_get().is_none());
    }

    #[test]
    fn disposing_twice_does_not_alias_slots() {
        let first = signal(1u8);
        first.dispose();
        first.dispose();

        let second = signal(2u8);
        let third = signal(3u8);
        assert_eq!(second.get(), 2);
        assert_eq!(third.get(), 3);
        assert!(!first.is_alive());
    }

    #[test]
    fn disposed_signals_stop_notifying() {
        let count = signal(0);
        let runs = Rc::new(Cell::new(0));
        let guard = {
            let runs = runs.clone();
            effect(move || {
                runs.set(runs.get() + 1);
                let _ = count.get();
            })
        };
        assert_eq!(runs.get(), 1);
        count.dispose();
        assert_eq!(flush_effects(), 0);
        drop(guard);
    }
}
