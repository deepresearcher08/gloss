//! Generational arena used for the retained node tree.
//!
//! Handles carry a generation counter, so a stale [`NodeId`] can never alias a
//! newer node that reuses the same slot.

use std::fmt;

/// A generational handle into an [`Arena`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId {
    index: u32,
    generation: u32,
}

impl NodeId {
    /// Creates a handle from raw parts. Prefer [`Arena::insert`].
    pub const fn from_raw(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// The slot index.
    pub const fn index(self) -> u32 {
        self.index
    }

    /// The generation counter.
    pub const fn generation(self) -> u32 {
        self.generation
    }

    /// Returns a stable numeric key for hashing in external maps.
    pub const fn key(self) -> u64 {
        ((self.generation as u64) << 32) | self.index as u64
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeId({}g{})", self.index, self.generation)
    }
}

struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

/// A slot-reusing arena with generational handles.
pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
    len: usize,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Arena<T> {
    /// Creates an empty arena.
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            len: 0,
        }
    }

    /// Inserts a value and returns its handle.
    pub fn insert(&mut self, value: T) -> NodeId {
        self.len += 1;
        match self.free.pop() {
            Some(index) => {
                let slot = &mut self.slots[index as usize];
                slot.generation = slot.generation.wrapping_add(1);
                slot.value = Some(value);
                NodeId {
                    index,
                    generation: slot.generation,
                }
            }
            None => {
                self.slots.push(Slot {
                    generation: 0,
                    value: Some(value),
                });
                NodeId {
                    index: (self.slots.len() - 1) as u32,
                    generation: 0,
                }
            }
        }
    }

    /// Returns a reference when the handle is live.
    pub fn get(&self, id: NodeId) -> Option<&T> {
        self.slots
            .get(id.index as usize)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.value.as_ref())
    }

    /// Returns a mutable reference when the handle is live.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut T> {
        self.slots
            .get_mut(id.index as usize)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.value.as_mut())
    }

    /// Returns whether the handle still points at a live value.
    pub fn contains(&self, id: NodeId) -> bool {
        self.get(id).is_some()
    }

    /// Removes and returns the value when the handle is live.
    pub fn remove(&mut self, id: NodeId) -> Option<T> {
        let slot = self
            .slots
            .get_mut(id.index as usize)
            .filter(|slot| slot.generation == id.generation)?;
        let value = slot.value.take()?;
        self.len -= 1;
        self.free.push(id.index);
        Some(value)
    }

    /// Number of live values.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns whether the arena holds no live values.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Iterates over live `(id, value)` pairs in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &T)> + '_ {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            slot.value.as_ref().map(|value| {
                (
                    NodeId {
                        index: index as u32,
                        generation: slot.generation,
                    },
                    value,
                )
            })
        })
    }

    /// Iterates mutably over live `(id, value)` pairs in slot order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (NodeId, &mut T)> + '_ {
        self.slots
            .iter_mut()
            .enumerate()
            .filter_map(|(index, slot)| {
                slot.value.as_mut().map(|value| {
                    (
                        NodeId {
                            index: index as u32,
                            generation: slot.generation,
                        },
                        value,
                    )
                })
            })
    }

    /// Drops every value, keeping allocated slots for reuse.
    pub fn clear(&mut self) {
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot.value.take().is_some() {
                slot.generation = slot.generation.wrapping_add(1);
                self.free.push(index as u32);
            }
        }
        self.len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reuses_slots_with_bumped_generations() {
        let mut arena = Arena::new();
        let first = arena.insert("one");
        assert_eq!(arena.get(first), Some(&"one"));
        assert_eq!(arena.remove(first), Some("one"));
        assert!(arena.get(first).is_none());

        let second = arena.insert("two");
        assert_eq!(second.index(), first.index());
        assert_ne!(second.generation(), first.generation());
        assert_eq!(arena.get(second), Some(&"two"));
        assert!(arena.get(first).is_none());
    }

    #[test]
    fn tracks_len_and_clear() {
        let mut arena = Arena::new();
        let a = arena.insert(1);
        let b = arena.insert(2);
        assert_eq!(arena.len(), 2);
        arena.clear();
        assert!(arena.is_empty());
        assert!(arena.get(a).is_none());
        assert!(arena.get(b).is_none());
        let c = arena.insert(3);
        assert_eq!(arena.get(c), Some(&3));
        assert_eq!(arena.len(), 1);
    }
}
