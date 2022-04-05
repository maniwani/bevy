use crate::storage::SparseSetIndex;
use bevy_utils::HashSet;
use fixedbitset::FixedBitSet;
use std::marker::PhantomData;

/// Tracks read and write access to specific elements in a collection.
///
/// System initialization and execution uses these to ensure soundness.
/// See the [`is_compatible`](Access::is_compatible) and [`get_conflicts`](Access::get_conflicts) functions.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Access<T: SparseSetIndex> {
    /// The accessed elements.
    reads_and_writes: FixedBitSet,
    /// The exclusively-accessed elements.
    writes: FixedBitSet,
    /// Does this have access to all elements in the collection?
    /// This field exists as a performance optimization for `&World` and `&mut World`.
    reads_all: bool,
    /// Does this have exclusive access to all elements in the collection?
    /// This field exists as a performance optimization for `&mut World`.
    pub(crate) writes_all: bool,
    marker: PhantomData<T>,
}

impl<T: SparseSetIndex> Default for Access<T> {
    fn default() -> Self {
        Self {
            reads_and_writes: Default::default(),
            writes: Default::default(),
            reads_all: false,
            writes_all: false,
            marker: PhantomData,
        }
    }
}

impl<T: SparseSetIndex> Access<T> {
    pub fn grow(&mut self, bits: usize) {
        self.reads_and_writes.grow(bits);
        self.writes.grow(bits);
    }

    /// Adds access to the element given by `index`.
    pub fn add_read(&mut self, index: T) {
        self.reads_and_writes.grow(index.sparse_set_index() + 1);
        self.reads_and_writes.insert(index.sparse_set_index());
    }

    /// Adds exclusive access to the element given by `index`.
    pub fn add_write(&mut self, index: T) {
        self.reads_and_writes.grow(index.sparse_set_index() + 1);
        self.reads_and_writes.insert(index.sparse_set_index());
        self.writes.grow(index.sparse_set_index() + 1);
        self.writes.insert(index.sparse_set_index());
    }

    /// Returns `true` if this can access the element given by `index`.
    pub fn has_read(&self, index: T) -> bool {
        if self.reads_all {
            true
        } else {
            self.reads_and_writes.contains(index.sparse_set_index())
        }
    }

    /// Returns `true` if this can exclusively access the element given by `index`.
    pub fn has_write(&self, index: T) -> bool {
        if self.writes_all {
            true
        } else {
            self.writes.contains(index.sparse_set_index())
        }
    }

    /// Sets this as having access to all indexed elements.
    pub(crate) fn read_all(&mut self) {
        self.reads_all = true;
    }

    /// Sets this as having exclusive access to all indexed elements.
    pub(crate) fn write_all(&mut self) {
        self.reads_all = true;
        self.writes_all = true;
    }

    /// Removes all accesses.
    pub fn clear(&mut self) {
        self.reads_all = false;
        self.writes_all = false;
        self.reads_and_writes.clear();
        self.writes.clear();
    }

    /// Adds all access from `other`.
    pub fn extend(&mut self, other: &Access<T>) {
        self.reads_all = self.reads_all || other.reads_all;
        self.writes_all = self.writes_all || other.writes_all;
        self.reads_and_writes.union_with(&other.reads_and_writes);
        self.writes.union_with(&other.writes);
    }

    /// Returns `true` if this and `other` can be active at the same time.
    ///
    /// `Access` instances are incompatible if one or both wants exclusive access to an element they have in common
    /// (data race).
    pub fn is_compatible(&self, other: &Access<T>) -> bool {
        // Only systems with no access are compatible with systems that operate on &mut World
        if self.writes_all {
            return other.reads_and_writes.count_ones(..) == 0;
        }

        if other.writes_all {
            return self.reads_and_writes.count_ones(..) == 0;
        }

        // Only systems that do not write data are compatible with systems that operate on &World
        if self.reads_all {
            return other.writes.count_ones(..) == 0;
        }

        if other.reads_all {
            return self.writes.count_ones(..) == 0;
        }

        self.writes.is_disjoint(&other.reads_and_writes)
            && self.reads_and_writes.is_disjoint(&other.writes)
    }

    /// Returns a vector of elements that this and `other` cannot access at the same time.
    pub fn get_conflicts(&self, other: &Access<T>) -> Vec<T> {
        let mut conflicts = FixedBitSet::default();

        // this also covers the case where two `&mut World` systems conflict
        // it's just the world metadata component
        if self.writes_all {
            conflicts.extend(other.reads_and_writes.ones());
        }

        if other.writes_all {
            conflicts.extend(self.reads_and_writes.ones());
        }

        if !(self.writes_all || other.writes_all) {
            match (self.reads_all, other.reads_all) {
                (false, false) => {
                    conflicts.extend(self.writes.intersection(&other.reads_and_writes));
                    conflicts.extend(self.reads_and_writes.intersection(&other.writes));
                }
                (false, true) => {
                    conflicts.extend(self.writes.ones());
                }
                (true, false) => {
                    conflicts.extend(other.writes.ones());
                }
                (true, true) => (),
            }
        }

        conflicts
            .ones()
            .map(SparseSetIndex::get_sparse_set_index)
            .collect()
    }

    /// Returns the indices of the elements this has access to.
    pub fn reads(&self) -> impl Iterator<Item = T> + '_ {
        self.reads_and_writes.ones().map(T::get_sparse_set_index)
    }

    /// Returns the indices of the elements this has non-exclusive access to.
    pub fn only_reads(&self) -> impl Iterator<Item = T> + '_ {
        self.reads_and_writes
            .difference(&self.writes)
            .map(T::get_sparse_set_index)
    }

    /// Returns the indices of the elements this has exclusive access to.
    pub fn writes(&self) -> impl Iterator<Item = T> + '_ {
        self.writes.ones().map(T::get_sparse_set_index)
    }
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct FilteredAccess<T: SparseSetIndex> {
    access: Access<T>,
    with: FixedBitSet,
    without: FixedBitSet,
}

impl<T: SparseSetIndex> Default for FilteredAccess<T> {
    fn default() -> Self {
        Self {
            access: Access::default(),
            with: Default::default(),
            without: Default::default(),
        }
    }
}

impl<T: SparseSetIndex> From<FilteredAccess<T>> for FilteredAccessSet<T> {
    fn from(filtered_access: FilteredAccess<T>) -> Self {
        let mut base = FilteredAccessSet::<T>::default();
        base.add(filtered_access);
        base
    }
}

impl<T: SparseSetIndex> FilteredAccess<T> {
    #[inline]
    pub fn access(&self) -> &Access<T> {
        &self.access
    }

    pub fn add_read(&mut self, index: T) {
        self.access.add_read(index.clone());
        self.add_with(index);
    }

    pub fn add_write(&mut self, index: T) {
        self.access.add_write(index.clone());
        self.add_with(index);
    }

    pub fn add_with(&mut self, index: T) {
        self.with.grow(index.sparse_set_index() + 1);
        self.with.insert(index.sparse_set_index());
    }

    pub fn add_without(&mut self, index: T) {
        self.without.grow(index.sparse_set_index() + 1);
        self.without.insert(index.sparse_set_index());
    }

    pub fn is_compatible(&self, other: &FilteredAccess<T>) -> bool {
        if self.access.is_compatible(&other.access) {
            true
        } else {
            self.with.intersection(&other.without).next().is_some()
                || self.without.intersection(&other.with).next().is_some()
        }
    }

    pub fn extend(&mut self, access: &FilteredAccess<T>) {
        self.access.extend(&access.access);
        self.with.union_with(&access.with);
        self.without.union_with(&access.without);
    }

    pub(crate) fn read_all(&mut self) {
        self.access.read_all();
    }

    pub(crate) fn write_all(&mut self) {
        self.access.write_all();
    }
}
#[derive(Clone, Debug)]
pub struct FilteredAccessSet<T: SparseSetIndex> {
    combined_access: Access<T>,
    filtered_accesses: Vec<FilteredAccess<T>>,
}

impl<T: SparseSetIndex> FilteredAccessSet<T> {
    #[inline]
    pub fn combined_access(&self) -> &Access<T> {
        &self.combined_access
    }

    #[inline]
    pub fn combined_access_mut(&mut self) -> &mut Access<T> {
        &mut self.combined_access
    }

    pub fn get_conflicts(&self, filtered_access: &FilteredAccess<T>) -> Vec<T> {
        // if combined unfiltered access is incompatible, check each filtered access for
        // compatibility
        let mut conflicts = HashSet::<usize>::default();
        if !filtered_access.access.is_compatible(&self.combined_access) {
            for current_filtered_access in &self.filtered_accesses {
                if !current_filtered_access.is_compatible(filtered_access) {
                    conflicts.extend(
                        current_filtered_access
                            .access
                            .get_conflicts(&filtered_access.access)
                            .iter()
                            .map(|ind| ind.sparse_set_index()),
                    );
                }
            }
        }
        conflicts
            .iter()
            .map(|ind| T::get_sparse_set_index(*ind))
            .collect()
    }

    pub fn add(&mut self, filtered_access: FilteredAccess<T>) {
        self.combined_access.extend(&filtered_access.access);
        self.filtered_accesses.push(filtered_access);
    }

    pub fn extend(&mut self, filtered_access_set: FilteredAccessSet<T>) {
        self.combined_access
            .extend(&filtered_access_set.combined_access);
        self.filtered_accesses
            .extend(filtered_access_set.filtered_accesses);
    }

    pub fn clear(&mut self) {
        self.combined_access.clear();
        self.filtered_accesses.clear();
    }
}

impl<T: SparseSetIndex> Default for FilteredAccessSet<T> {
    fn default() -> Self {
        Self {
            combined_access: Default::default(),
            filtered_accesses: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::query::{Access, FilteredAccess};

    #[test]
    fn access_get_conflicts() {
        let mut access_a = Access::<usize>::default();
        access_a.add_read(0);
        access_a.add_read(1);

        let mut access_b = Access::<usize>::default();
        access_b.add_read(0);
        access_b.add_write(1);

        assert_eq!(access_a.get_conflicts(&access_b), vec![1]);

        let mut access_c = Access::<usize>::default();
        access_c.add_write(0);
        access_c.add_write(1);

        assert_eq!(access_a.get_conflicts(&access_c), vec![0, 1]);
        assert_eq!(access_b.get_conflicts(&access_c), vec![0, 1]);

        let mut access_d = Access::<usize>::default();
        access_d.add_read(0);

        assert_eq!(access_d.get_conflicts(&access_a), vec![]);
        assert_eq!(access_d.get_conflicts(&access_b), vec![]);
        assert_eq!(access_d.get_conflicts(&access_c), vec![0]);
    }

    #[test]
    fn filtered_access_extend() {
        let mut access_a = FilteredAccess::<usize>::default();
        access_a.add_read(0);
        access_a.add_read(1);
        access_a.add_with(2);

        let mut access_b = FilteredAccess::<usize>::default();
        access_b.add_read(0);
        access_b.add_write(3);
        access_b.add_without(4);

        access_a.extend(&access_b);

        let mut expected = FilteredAccess::<usize>::default();
        expected.add_read(0);
        expected.add_read(1);
        expected.add_with(2);
        expected.add_write(3);
        expected.add_without(4);

        assert!(access_a.eq(&expected));
    }
}
