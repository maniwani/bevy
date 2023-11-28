use std::marker::PhantomData;
use std::mem;
use std::ptr;

use crate::storage::SparseSetIndex;

pub trait Index: Copy + Eq {
    fn from_usize(index: usize) -> Self;
    fn to_usize(self) -> usize;
}

impl<T> Index for T
where
    T: Copy + Eq + SparseSetIndex,
{
    fn from_usize(index: usize) -> Self {
        Self::get_sparse_set_index(index)
    }

    fn to_usize(self) -> usize {
        self.sparse_set_index()
    }
}

pub struct SparseVec<I: Index, V> {
    chunks: Vec<Option<Box<[Option<V>]>>>,
    len: usize,
    chunk_len: usize,
    marker: PhantomData<I>,
}

impl<I, V> SparseVec<I, V>
where
    I: Index,
{
    pub const fn new(chunk_len: usize) -> Self {
        Self {
            chunks: Vec::new(),
            len: 0,
            chunk_len: 0,
            marker: PhantomData,
        }
    }

    pub fn with_capacity(chunk_len: usize, capacity: usize) -> Self {
        let num_chunks = capacity / chunk_len;
        let mut chunks = Vec::with_capacity(num_chunks);
        chunks.fill_with(|| Some(Self::make_chunk(chunk_len)));

        Self {
            chunks,
            len: 0,
            chunk_len,
            marker: PhantomData,
        }
    }

    #[inline]
    fn find_chunk(&self, key: I) -> (usize, usize) {
        let index = key.to_usize();
        (index / self.chunk_len, index % self.chunk_len)
    }

    #[inline]
    fn make_chunk(chunk_len: usize) -> Box<[Option<V>]> {
        let mut chunk = Vec::with_capacity(chunk_len).into_boxed_slice();
        // SAFETY: the discriminant of `Option::None` is zero
        unsafe {
            ptr::write_bytes(
                chunk.as_mut_ptr(),
                0,
                mem::size_of::<Option<V>>() * chunk_len,
            );
        }

        chunk
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.chunks.iter().flatten().count() * self.chunk_len
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn clear(&mut self) {
        for chunk in self.chunks.iter_mut().flatten() {
            // SAFETY: the discriminant of `Option::None` is zero
            unsafe {
                ptr::write_bytes(
                    chunk.as_mut_ptr(),
                    0,
                    mem::size_of::<Option<V>>() * self.chunk_len,
                );
            }
        }
    }

    #[inline]
    pub fn contains(&self, key: I) -> bool {
        self.get(key).is_some()
    }

    #[inline]
    pub fn get(&self, key: I) -> Option<&V> {
        let (chunk_index, index_in_chunk) = self.find_chunk(key);
        if let Some(maybe_chunk) = self.chunks.get(chunk_index) {
            if let Some(chunk) = maybe_chunk.as_ref() {
                // SAFETY: `index` is in bounds
                return unsafe { chunk.get_unchecked(index_in_chunk).as_ref() };
            }
        }

        None
    }

    #[inline]
    pub unsafe fn get_unchecked(&self, key: I) -> &V {
        debug_assert!(self.contains(key));
        let (chunk_index, index_in_chunk) = self.find_chunk(key);
        let chunk = self.chunks.get_unchecked(chunk_index).as_ref().unwrap();
        chunk
            .get_unchecked(index_in_chunk)
            .as_ref()
            .unwrap_unchecked()
    }

    #[inline]
    pub fn get_mut(&mut self, key: I) -> Option<&mut V> {
        let (chunk_index, index_in_chunk) = self.find_chunk(key);
        if let Some(maybe_chunk) = self.chunks.get_mut(chunk_index) {
            if let Some(chunk) = maybe_chunk.as_mut() {
                // SAFETY: `index` is in bounds
                return unsafe { chunk.get_unchecked_mut(index_in_chunk).as_mut() };
            }
        }

        None
    }

    #[inline]
    pub unsafe fn get_unchecked_mut(&mut self, key: I) -> &mut V {
        debug_assert!(self.contains(key));
        let (chunk_index, index_in_chunk) = self.find_chunk(key);
        let chunk = self.chunks.get_unchecked_mut(chunk_index).as_mut().unwrap();
        chunk
            .get_unchecked_mut(index_in_chunk)
            .as_mut()
            .unwrap_unchecked()
    }

    #[inline]
    pub fn insert(&mut self, key: I, value: V) -> Option<V> {
        let (chunk_index, index_in_chunk) = self.find_chunk(key);
        if chunk_index >= self.chunks.len() {
            self.chunks.resize_with(chunk_index + 1, || None);
        }

        // SAFETY: `chunk` is in bounds
        let chunk = unsafe {
            self.chunks
                .get_unchecked_mut(chunk_index)
                .get_or_insert_with(|| Self::make_chunk(self.chunk_len))
        };

        // SAFETY: `index` is in bounds
        let prev_value = unsafe { chunk.get_unchecked_mut(index_in_chunk).replace(value) };

        if prev_value.is_none() {
            self.len += 1;
        }

        prev_value
    }

    #[inline]
    pub fn remove(&mut self, key: I) -> Option<V> {
        let (chunk_index, index_in_chunk) = self.find_chunk(key);
        if let Some(maybe_chunk) = self.chunks.get_mut(chunk_index) {
            if let Some(chunk) = maybe_chunk.as_mut() {
                // SAFETY: `index` is in bounds
                let prev_value = unsafe { chunk.get_unchecked_mut(index_in_chunk).take() };

                if prev_value.is_some() {
                    self.len -= 1;
                }

                return prev_value;
            }
        }

        None
    }

    pub fn reserve(&mut self, additional: usize) {
        todo!()
    }

    pub fn reserve_exact(&mut self, additional: usize) {
        todo!()
    }
}

pub struct SparseMap<I: Index, V> {
    sparse: SparseVec<I, I>,
    dense: Vec<I>,
    data: Vec<V>,
}

impl<I, V> SparseMap<I, V>
where
    I: Index,
{
    pub const fn new(chunk_len: usize) -> Self {
        Self {
            sparse: SparseVec::new(chunk_len),
            dense: Vec::new(),
            data: Vec::new(),
        }
    }

    pub fn with_capacity(chunk_len: usize, capacity: usize) -> Self {
        Self {
            sparse: SparseVec::with_capacity(chunk_len, capacity),
            dense: Vec::with_capacity(capacity),
            data: Vec::with_capacity(capacity),
        }
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.dense.capacity()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.dense.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.dense.is_empty()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.sparse.clear();
        self.dense.clear();
        self.data.clear();
    }

    #[inline]
    pub fn contains_key(&self, key: I) -> bool {
        self.sparse.contains(key)
    }

    #[inline]
    pub fn get_dense(&self, key: I) -> Option<&I> {
        self.sparse.get(key)
    }

    #[inline]
    pub fn get(&self, key: I) -> Option<&V> {
        self.sparse.get(key).map(|dense| {
            // SAFETY: `dense` is in bounds
            unsafe { self.data.get_unchecked(dense.to_usize()) }
        })
    }

    #[inline]
    pub unsafe fn get_unchecked(&self, key: I) -> &V {
        debug_assert!(self.contains_key(key));
        let dense = self.sparse.get_unchecked(key);
        self.data.get_unchecked(dense.to_usize())
    }

    #[inline]
    pub fn get_mut(&mut self, key: I) -> Option<&mut V> {
        self.sparse.get(key).map(|dense| {
            // SAFETY: `dense` is in bounds
            unsafe { self.data.get_unchecked_mut(dense.to_usize()) }
        })
    }

    #[inline]
    pub unsafe fn get_unchecked_mut(&mut self, key: I) -> &mut V {
        debug_assert!(self.contains_key(key));
        let dense = self.sparse.get_unchecked(key);
        self.data.get_unchecked_mut(dense.to_usize())
    }

    pub fn get_many_mut<const N: usize>(&mut self, keys: [I; N]) -> Option<[&mut V; N]> {
        todo!()
    }

    unsafe fn get_many_unchecked_mut<const N: usize>(
        &mut self,
        keys: [I; N],
    ) -> Option<[&mut V; N]> {
        todo!()
    }

    #[inline]
    pub fn insert(&mut self, key: I, value: V) -> Option<V> {
        if let Some(dense) = self.sparse.get(key) {
            // SAFETY: `dense` is in bounds
            let slot = unsafe { self.data.get_unchecked_mut(dense.to_usize()) };
            let previous = std::mem::replace(slot, value);
            Some(previous)
        } else {
            let dense = I::from_usize(self.len());
            self.sparse.insert(key, dense);
            self.dense.push(key);
            self.data.push(value);
            None
        }
    }

    #[inline]
    pub fn remove(&mut self, key: I) -> Option<V> {
        if let Some(dense) = self.sparse.remove(key) {
            let index = dense.to_usize();
            let value = self.data.swap_remove(index);
            let swapped_key = self.dense.swap_remove(index);
            *self.sparse.get_mut(swapped_key).unwrap() = dense;
            Some(value)
        } else {
            None
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = I> + '_ {
        self.dense.iter().copied()
    }

    pub fn values(&self) -> impl Iterator<Item = &'_ V> + '_ {
        self.data.iter()
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &'_ mut V> + '_ {
        self.data.iter_mut()
    }

    pub fn iter(&self) -> impl Iterator<Item = (I, &'_ V)> + '_ {
        self.dense.iter().copied().zip(self.data.iter())
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (I, &'_ mut V)> + '_ {
        self.dense.iter().copied().zip(self.data.iter_mut())
    }

    pub fn reserve(&mut self, additional: usize) {
        todo!()
    }

    pub fn reserve_exact(&mut self, additional: usize) {
        todo!()
    }
}

struct OptimizedSparseMap<I: Index, V: Copy> {
    lo: Box<[Option<V>]>,
    hi: SparseMap<I, V>,
}

impl<I: Index, V: Copy> OptimizedSparseMap<I, V> {
    pub fn new(chunk_len: usize) -> Self {
        Self {
            lo: vec![None; 256].into_boxed_slice(),
            hi: SparseMap::new(chunk_len),
        }
    }

    #[inline]
    fn is_low_index(&self, index: I) -> bool {
        index.to_usize() < self.lo.len()
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        let lo_cap = self.lo.len();
        let hi_cap = self.hi.capacity();
        lo_cap + hi_cap
    }

    #[inline]
    pub fn len(&self) -> usize {
        let lo_len = self.lo.iter().flatten().count();
        let hi_len = self.hi.len();
        lo_len + hi_len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn clear(&mut self) {
        self.lo.iter_mut().for_each(|value| *value = None);
        self.hi.clear();
    }

    #[inline]
    pub fn contains_key(&self, key: I) -> bool {
        self.get(key).is_some()
    }

    #[inline]
    pub fn get(&self, key: I) -> Option<&V> {
        if self.is_low_index(key) {
            // SAFETY: `key` is in bounds
            unsafe { self.lo.get_unchecked(key.to_usize()).as_ref() }
        } else {
            self.hi.get(key)
        }
    }

    #[inline]
    pub fn get_mut(&mut self, key: I) -> Option<&mut V> {
        if self.is_low_index(key) {
            // SAFETY: `key` is in bounds
            unsafe { self.lo.get_unchecked_mut(key.to_usize()).as_mut() }
        } else {
            self.hi.get_mut(key)
        }
    }

    pub fn get_many_mut<const N: usize>(&mut self, keys: [I; N]) -> Option<[&mut V; N]> {
        todo!()
    }

    unsafe fn get_many_unchecked_mut<const N: usize>(
        &mut self,
        keys: [I; N],
    ) -> Option<[&mut V; N]> {
        todo!()
    }

    #[inline]
    pub fn insert(&mut self, key: I, value: V) -> Option<V> {
        if self.is_low_index(key) {
            // SAFETY: `key` is in bounds
            unsafe { self.lo.get_unchecked_mut(key.to_usize()).replace(value) }
        } else {
            self.hi.insert(key, value)
        }
    }

    #[inline]
    pub fn remove(&mut self, key: I) -> Option<V> {
        if self.is_low_index(key) {
            // SAFETY: `key` is in bounds
            unsafe { self.lo.get_unchecked_mut(key.to_usize()).take() }
        } else {
            self.hi.remove(key)
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = I> + '_ {
        self.lo
            .iter()
            .enumerate()
            .map(|(i, o)| {
                if o.is_some() {
                    Some(I::from_usize(i))
                } else {
                    None
                }
            })
            .flatten()
            .chain(self.hi.keys())
    }

    pub fn values(&self) -> impl Iterator<Item = &'_ V> + '_ {
        self.lo.iter().flatten().chain(self.hi.values())
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &'_ mut V> + '_ {
        self.lo.iter_mut().flatten().chain(self.hi.values_mut())
    }

    pub fn iter(&self) -> impl Iterator<Item = (I, &'_ V)> + '_ {
        self.lo
            .iter()
            .enumerate()
            .map(|(i, o)| {
                if let Some(v) = o {
                    Some((I::from_usize(i), v))
                } else {
                    None
                }
            })
            .flatten()
            .chain(self.hi.iter())
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (I, &'_ mut V)> + '_ {
        self.lo
            .iter_mut()
            .enumerate()
            .map(|(i, o)| {
                if let Some(v) = o {
                    Some((I::from_usize(i), v))
                } else {
                    None
                }
            })
            .flatten()
            .chain(self.hi.iter_mut())
    }
}
