use std::alloc::Layout;
use std::marker::PhantomData;
use std::mem::MaybeUninit;

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct ArenaHandle<T>(usize, PhantomData<T>);

pub struct Arena<T> {
    content: Vec<Chunk<T>>
}

struct Chunk<T> {
    used: u64,
    content: Box<[MaybeUninit<T>; 64]>
}

impl <T> Arena<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            content: vec![Chunk::new()],
        }
    }

    pub fn insert(&mut self, item: T) -> ArenaHandle<T> {
        for (i, chunk) in &mut self.content.iter_mut().enumerate() {
            let slot = chunk.used.trailing_ones() as usize;
            if let Some(slot_ref) = chunk.content.get_mut(slot) {
                chunk.used |= 1 << slot;
                slot_ref.write(item);
                return ArenaHandle(i * 64 | slot, PhantomData::default())
            }
        }
        let mut new_chunk = Chunk::new();
        new_chunk.used |= 1;
        new_chunk.content[0].write(item);
        self.content.push(new_chunk);
        ArenaHandle((self.content.len() - 1) * 64 | 0b1, PhantomData::default())
    }

    #[must_use]
    fn get(&self, handle: ArenaHandle<T>) -> Option<&T> {
        let chunk = self.content.get(handle.0 / 64)?;
        let chunk_idx = handle.0  % 64;
        if (chunk.used & (1 << chunk_idx)) > 0 {
            Some(unsafe { chunk.content[chunk_idx].assume_init_ref() })
        } else {
            None
        }
    }

    #[must_use]
    fn get_mut(&mut self, handle: ArenaHandle<T>) ->  Option<&mut T> {
        let chunk = self.content.get_mut(handle.0 / 64)?;
        let chunk_idx = handle.0  % 64;
        if (chunk.used & (1 << chunk_idx)) > 0 {
            Some(unsafe { chunk.content[chunk_idx].assume_init_mut() })
        } else {
            None
        }
    }
}

impl <T> Chunk<T> {
    fn new() -> Self {
        let content = Box::new(unsafe { MaybeUninit::<[MaybeUninit<T>; 64]>::uninit().assume_init() });
        let content = unsafe {
            let layout = Layout::new::<[MaybeUninit<T>; 64]>();
            let allocated = std::alloc::alloc(layout);
            let allocated = allocated as *mut [MaybeUninit<T>; 64];
            Box::from_raw(allocated)
        };
        Chunk { used: 0, content }
    }
}