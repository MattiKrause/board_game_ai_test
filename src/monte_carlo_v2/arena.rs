use std::alloc::Layout;
use std::hash::Hasher;
use std::marker::PhantomData;
use std::mem::MaybeUninit;

pub struct ArenaHandle<T>(usize, PhantomData<T>);

pub struct Arena<T> {
    content: Vec<Chunk<T>>,
    last_free: usize,
}

struct Chunk<T> {
    used: u64,
    content: Box<[MaybeUninit<T>; 64]>,
}

impl<T> Arena<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            content: vec![Chunk::new()],
            last_free: 0,
        }
    }

    pub fn insert(&mut self, item: T) -> ArenaHandle<T> {
        self.last_free = self.last_free.min(self.content.len());
        for (i, chunk) in self.content[self.last_free..].iter_mut().enumerate() {
            let slot = chunk.used.trailing_ones() as usize;
            if let Some(slot_ref) = chunk.content.get_mut(slot) {
                chunk.used |= 1 << slot;
                slot_ref.write(item);
                self.last_free += i;
                return ArenaHandle::new(self.last_free * 64 | slot);
            }
        }
        let mut new_chunk = Chunk::new();
        new_chunk.used |= 0b1;
        new_chunk.content[0].write(item);
        self.content.push(new_chunk);
        self.last_free = self.content.len() - 1;
        ArenaHandle::new((self.content.len() - 1) * 64)
    }

    #[must_use]
    pub fn get(&self, handle: &ArenaHandle<T>) -> Option<&T> {
        debug_assert!(self.content.len() > 0);
        let chunk_idx = handle.0 / 64;
        let slot_idx = handle.0 % 64;
        let chunk = self.content.get(chunk_idx)?;
        if (chunk.used & (1 << slot_idx)) > 0 {
            Some(unsafe { chunk.content[slot_idx].assume_init_ref() })
        } else {
            None
        }
    }

    #[must_use]
    pub fn get_mut(&mut self, handle: &ArenaHandle<T>) -> Option<&mut T> {
        let chunk_idx = handle.0 / 64;
        let slot_idx = handle.0 % 64;
        let chunk = self.content.get_mut(chunk_idx)?;
        if (chunk.used & (1 << slot_idx)) > 0 {
            Some(unsafe { chunk.content[slot_idx].assume_init_mut() })
        } else {
            None
        }
    }

    pub fn purge(&mut self) {
        for chunk in &mut self.content {
            chunk.clear(|_| ());
        }
        self.last_free = 0;
    }
}

impl<T> Chunk<T> {
    fn new() -> Self {
        let content = unsafe {
            let layout = Layout::new::<[MaybeUninit<T>; 64]>();
            let allocated = std::alloc::alloc(layout);
            let allocated = allocated as *mut [MaybeUninit<T>; 64];
            Box::from_raw(allocated)
        };
        Chunk { used: 0, content }
    }

    fn clear<F: FnMut(T)>(&mut self, mut with: F) {
        for (i, slot) in self.content.iter_mut().enumerate() {
            if (self.used & (1 << i as u64)) > 0 {
                with(unsafe { slot.assume_init_read() });
            }
        }
        self.used = 0;
    }
}

impl<T> Drop for Chunk<T> {
    fn drop(&mut self) {
        self.clear(|_| ());
    }
}

impl<T> ArenaHandle<T> {
    pub fn new(handle: usize) -> Self {
        debug_assert!(handle != usize::MAX);
        Self(handle, PhantomData::default())
    }

    pub const fn invalid() -> Self {
        Self(usize::MAX, PhantomData)
    }
}

impl<T> PartialEq<ArenaHandle<T>> for ArenaHandle<T> {
    fn eq(&self, other: &ArenaHandle<T>) -> bool {
        self.0.eq(&other.0)
    }
}

impl<T> Eq for ArenaHandle<T> {}

impl<T> Copy for ArenaHandle<T> {}

impl<T> Clone for ArenaHandle<T> {
    fn clone(&self) -> Self {
        Self(self.0, self.1)
    }
}

impl<T> std::hash::Hash for ArenaHandle<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

#[cfg(test)]
mod tests {
    use super::Arena;

    #[test]
    fn test() {
        let mut arena: Arena<u64> = Arena::new();
        let numbers: Vec<u64> = std::iter::repeat(1)
            .take(1000)
            .scan(0, |a, b| {
                *a += b;
                Some(*a)
            })
            .collect::<Vec<_>>();
        let handle = numbers.iter().copied()
            .map(|n| arena.insert(n))
            .collect::<Vec<_>>();
        let gathered = handle.iter().clone()
            .filter_map(|handle| arena.get(handle))
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(numbers, gathered);
        let gathered_mut = handle.iter().clone()
            .filter_map(|handle| arena.get_mut(handle).copied())
            .collect::<Vec<_>>();
        assert_eq!(numbers, gathered_mut);
        arena.purge();
        assert_eq!(handle.iter().filter_map(|handle| arena.get(handle)).next(), None);
    }
}