use std::cmp::max;
use std::marker::PhantomData;

pub struct SliceHandle<T> {
    chunk_idx: usize,
    start_idx: usize,
    len: usize,
    _data: PhantomData<T>
}

pub struct SliceArena<T>(Vec<Vec<T>>);

impl <T> SliceArena<T> {
    pub fn new() -> Self {
        Self(vec![alloc_chunk(0)])
    }

    pub fn insert(&mut self, mut insert: impl Iterator<Item = T>) -> SliceHandle<T> {
        if self.0.is_empty() {
            self.0.push(alloc_chunk(0));
        }
        let mut chunk_ref = self.0.last_mut().unwrap();
        let mut starting_len = chunk_ref.len();

        let min_size = insert.size_hint().0;
        if min_size <= chunk_ref.capacity() - chunk_ref.len(){
            while let Some(next) = insert.next() {
                if chunk_ref.capacity() - chunk_ref.len() > 0 {
                    chunk_ref.push(next);
                } else {
                    let mut new_chunk = alloc_chunk(0);
                    new_chunk.extend(chunk_ref.drain(starting_len..));
                    new_chunk.extend(insert);
                    self.0.push(new_chunk);
                    starting_len = 0;
                    chunk_ref = self.0.last_mut().unwrap();
                    break;
                }
            }
            let len =  chunk_ref.len() - starting_len;
            SliceHandle {
                chunk_idx: self.0.len() - 1,
                start_idx: starting_len,
                len,
                _data: PhantomData,
            }
        } else {
            let mut new_chunk = alloc_chunk(min_size);
            new_chunk.extend(insert);
            let nc_len = new_chunk.len();
            self.0.push(new_chunk);
            SliceHandle {
                chunk_idx: self.0.len() - 1,
                start_idx: 0,
                len: nc_len,
                _data: PhantomData
            }
        }
    }

    pub fn get(&self, handle: &SliceHandle<T>) ->  Option<&[T]> {
        self.0.get(handle.chunk_idx)
            .and_then(|chunk| chunk.get(handle.start_idx..(handle.start_idx + handle.len)))
    }

    pub fn get_mut(&mut self, handle: &SliceHandle<T>) -> Option<&mut [T]> {
        self.0.get_mut(handle.chunk_idx)
            .and_then(|chunk| chunk.get_mut(handle.start_idx..(handle.start_idx + handle.len)))
    }

    pub(crate) fn clear(&mut self) {
        for chunk in &mut self.0 {
            chunk.clear();
        }
    }
}
#[inline(never)]
fn alloc_chunk<T>(required: usize) -> Vec<T> {
    const PAGE_SIZE: usize = 4096;
    let allocated_amount: usize =  PAGE_SIZE / std::mem::size_of::<T>();
    Vec::with_capacity(max(allocated_amount, required))
}

impl <T> SliceHandle<T> {
    pub fn empty() -> Self {
        Self {
            chunk_idx: 0,
            start_idx: 0,
            len: 0,
            _data: Default::default(),
        }
    }
    pub fn len(&self) -> usize {
        self.len
    }
}

#[cfg(test)]
mod test {
    use std::marker::PhantomData;
    use crate::monte_carlo_v2::arena::ArenaHandle;
    use crate::monte_carlo_v2::moves_buffer::{SliceArena, SliceHandle};

    #[test]
    fn test() {
        let mut arena = SliceArena::<u64>::new();
        let handle_1 = arena.insert([1, 2, 3, 4].into_iter());
        let handle_2 = arena.insert([30, 31, 21, 8, 0, 19].into_iter());

        assert_eq!(arena.get(&handle_1), Some([1u64, 2, 3, 4].as_slice()));
        assert_eq!(arena.get(&handle_2), Some([30u64, 31, 21, 8, 0, 19].as_slice()));
        assert_eq!(arena.get(&SliceHandle {
            chunk_idx: 0,
            start_idx: 10,
            len: 10,
            _data: PhantomData,
        }), None);
        assert_eq!(arena.get(&SliceHandle {
            chunk_idx: 0,
            start_idx: 0,
            len: 11,
            _data: PhantomData,
        }), None);
        assert_eq!(arena.get(&SliceHandle {
            chunk_idx: 1,
            start_idx: 0,
            len: 10,
            _data: PhantomData,
        }), None);


        arena.clear();

        assert_eq!(arena.get(&handle_1), None);
        assert_eq!(arena.get(&handle_2), None);

        let handle_3 = arena.insert(vec![0u64; 512].into_iter());

        assert_eq!(handle_3.chunk_idx, 0);
        let handle_4 = arena.insert([1, 2, 3, 4].into_iter());
        assert_eq!(handle_4.chunk_idx,  1);
        assert_eq!(arena.get_mut(&handle_4), Some([1u64, 2, 3, 4].as_mut_slice()));
    }
}