//! A small fixed-capacity vector with no heap allocation.

use std::fmt;

/// Inline vector holding at most `N` values.
pub struct InlineVec<T, const N: usize> {
    items: [Option<T>; N],
    len: usize,
}

impl<T, const N: usize> InlineVec<T, N> {
    /// An empty vector.
    pub const fn new() -> Self {
        Self {
            items: [const { None }; N],
            len: 0,
        }
    }

    /// Number of elements.
    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the vector is empty.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Maximum number of elements.
    #[inline]
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Whether the vector is full.
    #[inline]
    pub const fn is_full(&self) -> bool {
        self.len == N
    }

    /// Remove all elements.
    pub fn clear(&mut self) {
        for slot in &mut self.items[..self.len] {
            *slot = None;
        }
        self.len = 0;
    }

    /// Append a value. Returns `false` (dropping the value) when full.
    pub fn push(&mut self, value: T) -> bool {
        if self.len >= N {
            return false;
        }
        self.items[self.len] = Some(value);
        self.len += 1;
        true
    }

    /// Remove and return the last value.
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        self.items[self.len].take()
    }

    /// Borrow an element by index.
    #[inline]
    pub fn get(&self, index: usize) -> Option<&T> {
        if index < self.len {
            self.items[index].as_ref()
        } else {
            None
        }
    }

    /// Mutably borrow an element by index.
    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if index < self.len {
            self.items[index].as_mut()
        } else {
            None
        }
    }

    /// Iterate over elements.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items[..self.len].iter().filter_map(Option::as_ref)
    }

    /// Iterate mutably over elements.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.items[..self.len].iter_mut().filter_map(Option::as_mut)
    }
}

impl<T: Clone, const N: usize> Clone for InlineVec<T, N> {
    fn clone(&self) -> Self {
        let mut out = Self::new();
        for value in self.iter() {
            out.push(value.clone());
        }
        out
    }
}

impl<T: PartialEq, const N: usize> PartialEq for InlineVec<T, N> {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.iter().zip(other.iter()).all(|(a, b)| a == b)
    }
}

impl<T: fmt::Debug, const N: usize> fmt::Debug for InlineVec<T, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<T, const N: usize> Default for InlineVec<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::InlineVec;

    #[test]
    fn push_pop_and_iterate() {
        let mut vec: InlineVec<u32, 3> = InlineVec::new();
        assert!(vec.is_empty());
        assert!(vec.push(1));
        assert!(vec.push(2));
        assert!(vec.push(3));
        assert!(!vec.push(4));
        assert_eq!(vec.len(), 3);
        assert_eq!(vec.iter().copied().collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(vec.pop(), Some(3));
        assert_eq!(vec.len(), 2);
        vec.clear();
        assert!(vec.is_empty());
    }

    #[test]
    fn supports_non_copy_values() {
        let mut vec: InlineVec<String, 2> = InlineVec::new();
        vec.push("hello".to_string());
        vec.push("world".to_string());
        let joined = vec.iter().cloned().collect::<Vec<_>>().join(" ");
        assert_eq!(joined, "hello world");
        assert_eq!(vec.clone(), vec);
    }
}
