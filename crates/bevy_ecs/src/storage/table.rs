use crate::{
    component::{ComponentId, ComponentInfo, ComponentTicks, Components, Tick, TickCells},
    entity::Entity,
    query::DebugCheckedUnwrap,
    storage::{blob_vec::BlobVec, ImmutableSparseSet, SparseSet, SparseSetIndex},
};
use bevy_ptr::{OwningPtr, Ptr, PtrMut, UnsafeCellDeref};
use bevy_utils::HashMap;
use std::alloc::Layout;
use std::{
    cell::UnsafeCell,
    ops::{Index, IndexMut},
};

/// An opaque unique ID for a [`Table`] within a [`World`].
///
/// Use with [`Tables::get`] to access the corresponding table.
///
/// Each [`Archetype`] always points to a table via [`Archetype::table_id`].
/// Multiple archetypes can point to the same table so long as the components
/// stored in the table are identical, but do not share the same sparse set
/// components.
///
/// [`World`]: crate::world::World
/// [`Archetype`]: crate::archetype::Archetype
/// [`Archetype::table_id`]: crate::archetype::Archetype::table_id
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
// SAFETY: Must be repr(transparent) due to the safety requirements on EntityLocation
#[repr(transparent)]
pub struct TableId(u32);

impl TableId {
    pub(crate) const INVALID: TableId = TableId(u32::MAX);

    /// Creates a new [`TableId`].
    ///
    /// `index` *must* be retrieved from calling [`TableId::index`] on a `TableId` you got
    /// from a table of a given [`World`] or the created ID may be invalid.
    ///
    /// [`World`]: crate::world::World
    #[inline]
    pub fn new(index: usize) -> Self {
        TableId(index as u32)
    }

    /// Gets the underlying table index from the ID.
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }

    /// The [`TableId`] of the [`Table`] without any components.
    #[inline]
    pub const fn empty() -> TableId {
        TableId(0)
    }
}

impl SparseSetIndex for TableId {
    fn sparse_set_index(&self) -> usize {
        self.0 as usize
    }

    fn get_sparse_set_index(value: usize) -> Self {
        Self(value as u32)
    }
}

/// A column in a [`Table`].
///
/// This is the index of the (conceptual) [`Vec<T>`] that stores all data for a specific component
/// in the [`Table`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct TableColumn(pub(crate) u32);

/// The row index where an entity's data is stored in a [`Table`]. An entity's data is stored at
/// the same row in each [`Column`].
///
/// An entity's table row can be acquired from [`Archetype::entity_table_row`] and can be used
/// alongside [`Archetype::table_id`] to locate its data.
///
/// When components are added to or removed from an entity, that entity will be moved to another
/// [`Table`]. This move will usually invalidate another row in the initial [`Table`], so don't
/// cache this value. Always get the current location of an entity from its [`Archetype`] before
/// attempting to access its components.
///
/// [`Archetype`]: crate::archetype::Archetype
/// [`Archetype::entity_table_row`]: crate::archetype::Archetype::entity_table_row
/// [`Archetype::table_id`]: crate::archetype::Archetype::table_id
/// [`Entity`]: crate::entity::Entity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// SAFETY: Must be repr(transparent) due to the safety requirements on EntityLocation
#[repr(transparent)]
pub struct TableRow(u32);

impl TableRow {
    pub(crate) const INVALID: TableRow = TableRow(u32::MAX);

    /// Creates a `TableRow`.
    #[inline]
    pub const fn new(index: usize) -> Self {
        Self(index as u32)
    }

    /// Gets the index of the row.
    #[inline]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A contiguous array of homogenous data.
/// Essentially a `Vec<T>` whose type information has been elided.
///
/// [`Column`] also stores two additional arrays containing the [`Added`] and [`Changed`] ticks of
/// each component value. The three arrays are aligned such that a particular element has the same
/// index in all three.
///
/// Immutable slices over these arrays can be acquired using [`Column::get_data_slice`],
/// [`Column::get_added_ticks_slice`], and [`Column::get_changed_ticks_slice`].
///
/// Like many other low-level storage types, [`Column`] has a highly `unsafe` interface. You are
/// advised against interacting with [`Column`] directly.
///
/// [`Added`]: crate::query::Added
/// [`Changed`]: crate::query::Changed
#[derive(Debug)]
pub struct Column {
    data: BlobVec,
    added_ticks: Vec<UnsafeCell<Tick>>,
    changed_ticks: Vec<UnsafeCell<Tick>>,
}

impl Column {
    /// Constructs a new [`Column`], configured with a component's layout and an initial `capacity`.
    #[inline]
    pub(crate) fn with_capacity(component_info: &ComponentInfo, capacity: usize) -> Self {
        Column {
            // SAFETY: component_info.drop() is valid for the types that will be inserted.
            data: unsafe { BlobVec::new(component_info.layout(), component_info.drop(), capacity) },
            added_ticks: Vec::with_capacity(capacity),
            changed_ticks: Vec::with_capacity(capacity),
        }
    }

    /// Returns the [`Layout`] for the underlying type.
    #[inline]
    pub fn item_layout(&self) -> Layout {
        self.data.layout()
    }

    /// Returns the current number of rows in the column.
    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns `true` if there are no rows.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Clears the column, removing all values.
    ///
    /// Note that this function has no effect on the allocated capacity of the [`Column`]>
    pub fn clear(&mut self) {
        self.data.clear();
        self.added_ticks.clear();
        self.changed_ticks.clear();
    }

    /// Appends a new value to the end of the [`Column`].
    ///
    /// # Safety
    /// `ptr` must point to valid data of this column's component type
    pub(crate) unsafe fn push(&mut self, ptr: OwningPtr<'_>, ticks: ComponentTicks) {
        self.data.push(ptr);
        self.added_ticks.push(UnsafeCell::new(ticks.added));
        self.changed_ticks.push(UnsafeCell::new(ticks.changed));
    }

    #[inline]
    pub(crate) fn reserve_exact(&mut self, additional: usize) {
        self.data.reserve_exact(additional);
        self.added_ticks.reserve_exact(additional);
        self.changed_ticks.reserve_exact(additional);
    }

    /// Writes component data to the column at given row.
    /// Assumes the slot is uninitialized, drop is not called.
    /// To overwrite existing initialized value, use `replace` instead.
    ///
    /// # Safety
    /// Assumes data has already been allocated for the given row.
    #[inline]
    pub(crate) unsafe fn initialize(&mut self, row: TableRow, data: OwningPtr<'_>, tick: Tick) {
        debug_assert!(row.index() < self.len());
        self.data.initialize_unchecked(row.index(), data);
        *self.added_ticks.get_unchecked_mut(row.index()).get_mut() = tick;
        *self.changed_ticks.get_unchecked_mut(row.index()).get_mut() = tick;
    }

    /// Writes component data to the column at given row.
    /// Assumes the slot is initialized, calls drop.
    ///
    /// # Safety
    /// Assumes data has already been allocated for the given row.
    #[inline]
    pub(crate) unsafe fn replace(&mut self, row: TableRow, data: OwningPtr<'_>, change_tick: Tick) {
        debug_assert!(row.index() < self.len());
        self.data.replace_unchecked(row.index(), data);
        *self.changed_ticks.get_unchecked_mut(row.index()).get_mut() = change_tick;
    }

    /// Writes component data to the column at given row.
    /// Assumes the slot is initialized, calls drop.
    /// Does not update the Component's ticks.
    ///
    /// # Safety
    /// Assumes data has already been allocated for the given row.
    #[inline]
    pub(crate) unsafe fn replace_untracked(&mut self, row: TableRow, data: OwningPtr<'_>) {
        debug_assert!(row.index() < self.len());
        self.data.replace_unchecked(row.index(), data);
    }

    /// Removes an element from the [`Column`].
    ///
    /// - The value will be dropped if it implements [`Drop`].
    /// - This does not preserve ordering, but is O(1).
    /// - This does not do any bounds checking.
    /// - The element is replaced with the last element in the [`Column`].
    ///
    /// # Safety
    /// `row` must be within the range `[0, self.len())`.
    ///
    /// [`Drop`]: std::ops::Drop
    #[inline]
    pub(crate) unsafe fn swap_remove_unchecked(&mut self, row: TableRow) {
        self.data.swap_remove_and_drop_unchecked(row.index());
        self.added_ticks.swap_remove(row.index());
        self.changed_ticks.swap_remove(row.index());
    }

    /// Removes an element from the [`Column`] and returns it and its change detection ticks.
    /// This does not preserve ordering, but is O(1).
    ///
    /// The element is replaced with the last element in the [`Column`].
    ///
    /// It is the caller's responsibility to ensure that the removed value is dropped or used.
    /// Failure to do so may result in resources not being released (i.e. files handles not being
    /// released, memory leaks, etc.)
    ///
    /// Returns `None` if `row` is out of bounds.
    #[inline]
    #[must_use = "The returned pointer should be used to drop the removed component"]
    pub(crate) fn swap_remove_and_forget(
        &mut self,
        row: TableRow,
    ) -> Option<(OwningPtr<'_>, ComponentTicks)> {
        (row.index() < self.data.len()).then(|| {
            // SAFETY: The row was length checked before this.
            let data = unsafe { self.data.swap_remove_and_forget_unchecked(row.index()) };
            let added = self.added_ticks.swap_remove(row.index()).into_inner();
            let changed = self.changed_ticks.swap_remove(row.index()).into_inner();
            (data, ComponentTicks { added, changed })
        })
    }

    /// Removes an element from the [`Column`] and returns it and its change detection ticks.
    /// This does not preserve ordering, but is O(1). Unlike [`Column::swap_remove_and_forget`]
    /// this does not do any bounds checking.
    ///
    /// The element is replaced with the last element in the [`Column`].
    ///
    /// It's the caller's responsibility to ensure that the removed value is dropped or used.
    /// Failure to do so may result in resources not being released (i.e. files handles not being
    /// released, memory leaks, etc.)
    ///
    /// # Safety
    /// `row` must be within the range `[0, self.len())`.
    #[inline]
    #[must_use = "The returned pointer should be used to dropped the removed component"]
    pub(crate) unsafe fn swap_remove_and_forget_unchecked(
        &mut self,
        row: TableRow,
    ) -> (OwningPtr<'_>, ComponentTicks) {
        let data = self.data.swap_remove_and_forget_unchecked(row.index());
        let added = self.added_ticks.swap_remove(row.index()).into_inner();
        let changed = self.changed_ticks.swap_remove(row.index()).into_inner();
        (data, ComponentTicks { added, changed })
    }

    /// Removes the element from `other` at `src_row` and inserts it
    /// into the current column to initialize the values at `dst_row`.
    /// Does not do any bounds checking.
    ///
    /// # Safety
    ///
    ///  - `other` must have the same data layout as `self`
    ///  - `src_row` must be in bounds for `other`
    ///  - `dst_row` must be in bounds for `self`
    ///  - `other[src_row]` must be initialized to a valid value.
    ///  - `self[dst_row]` must not be initialized yet.
    #[inline]
    pub(crate) unsafe fn initialize_from_unchecked(
        &mut self,
        dst_row: TableRow,
        src_column: &mut Column,
        src_row: TableRow,
    ) {
        debug_assert!(self.data.layout() == src_column.data.layout());
        let ptr = self.data.get_unchecked_mut(dst_row.index());
        src_column.data.swap_remove_unchecked(src_row.index(), ptr);

        *self.added_ticks.get_unchecked_mut(dst_row.index()) =
            src_column.added_ticks.swap_remove(src_row.index());

        *self.changed_ticks.get_unchecked_mut(dst_row.index()) =
            src_column.changed_ticks.swap_remove(src_row.index());
    }

    /// Returns a pointer to the first element of the [`Column`].
    ///
    /// The pointer is type erased, so using this function to fetch anything
    /// other than the first element will require computing the offset using
    /// [`Column::item_layout`].
    #[inline]
    pub fn get_data_ptr(&self) -> Ptr<'_> {
        self.data.get_ptr()
    }

    /// Returns the slice to the [`Column`]'s data cast to the given type.
    ///
    /// **Note:** The values stored within are [`UnsafeCell`].
    /// The user must ensure that each access to the individual elements
    /// adheres to the safety invariants of [`UnsafeCell`].
    ///
    /// # Safety
    /// The type `T` must be the type of the items in this column.
    ///
    /// [`UnsafeCell`]: std::cell::UnsafeCell
    pub unsafe fn get_data_slice<T>(&self) -> &[UnsafeCell<T>] {
        self.data.get_slice()
    }

    /// Returns the slice to the [`Column`]'s "added" change detection ticks.
    ///
    /// Note: The values stored within are [`UnsafeCell`].
    /// The user must ensure that each access to the individual elements
    /// adheres to the safety invariants of [`UnsafeCell`].
    ///
    /// [`UnsafeCell`]: std::cell::UnsafeCell
    #[inline]
    pub fn get_added_ticks_slice(&self) -> &[UnsafeCell<Tick>] {
        &self.added_ticks
    }

    /// Returns the slice to the [`Column`]'s "changed" change detection ticks.
    ///
    /// Note: The values stored within are [`UnsafeCell`].
    /// The user must ensure that each access to the individual elements
    /// adheres to the safety invariants of [`UnsafeCell`].
    ///
    /// [`UnsafeCell`]: std::cell::UnsafeCell
    #[inline]
    pub fn get_changed_ticks_slice(&self) -> &[UnsafeCell<Tick>] {
        &self.changed_ticks
    }

    /// Returns a reference to the data and change detection ticks at `row`.
    ///
    /// Returns `None` if `row` is out of bounds.
    #[inline]
    pub fn get(&self, row: TableRow) -> Option<(Ptr<'_>, TickCells<'_>)> {
        (row.index() < self.data.len())
            // SAFETY: The row is length checked before fetching the pointer. This is being
            // accessed through a read-only reference to the column.
            .then(|| unsafe {
                (
                    self.data.get_unchecked(row.index()),
                    TickCells {
                        added: self.added_ticks.get_unchecked(row.index()),
                        changed: self.changed_ticks.get_unchecked(row.index()),
                    },
                )
            })
    }

    /// Returns a read-only reference to the data at `row`.
    ///
    /// Returns `None` if `row` is out of bounds.
    #[inline]
    pub fn get_data(&self, row: TableRow) -> Option<Ptr<'_>> {
        // SAFETY: The row is length checked before fetching the pointer. This is being
        // accessed through a read-only reference to the column.
        (row.index() < self.data.len()).then(|| unsafe { self.data.get_unchecked(row.index()) })
    }

    /// Returns a read-only reference to the data at `row`. Unlike [`Column::get`] this does not
    /// do any bounds checking.
    ///
    /// # Safety
    /// - `row` must be within the range `[0, self.len())`.
    /// - no other mutable reference to the data of the same row can exist at the same time
    #[inline]
    pub unsafe fn get_data_unchecked(&self, row: TableRow) -> Ptr<'_> {
        debug_assert!(row.index() < self.data.len());
        self.data.get_unchecked(row.index())
    }

    /// Returns a mutable reference to the data at `row`.
    ///
    /// Returns `None` if `row` is out of bounds.
    #[inline]
    pub fn get_data_mut(&mut self, row: TableRow) -> Option<PtrMut<'_>> {
        // SAFETY: The row is length checked before fetching the pointer. This is being
        // accessed through an exclusive reference to the column.
        (row.index() < self.data.len()).then(|| unsafe { self.data.get_unchecked_mut(row.index()) })
    }

    /// Returns a mutable reference to the data at `row`. Unlike [`Column::get_data_mut`] this does not
    /// do any bounds checking.
    ///
    /// # Safety
    /// - index must be in-bounds
    /// - no other reference to the data of the same row can exist at the same time
    #[inline]
    pub(crate) unsafe fn get_data_unchecked_mut(&mut self, row: TableRow) -> PtrMut<'_> {
        debug_assert!(row.index() < self.data.len());
        self.data.get_unchecked_mut(row.index())
    }

    /// Returns the "added" change detection ticks for the value at `row`.
    ///
    /// Returns `None` if `row` is out of bounds.
    ///
    /// Note: The values stored within are [`UnsafeCell`].
    /// The user must ensure that each access to the individual elements
    /// adheres to the safety invariants of [`UnsafeCell`].
    ///
    /// [`UnsafeCell`]: std::cell::UnsafeCell
    #[inline]
    pub fn get_added_ticks(&self, row: TableRow) -> Option<&UnsafeCell<Tick>> {
        self.added_ticks.get(row.index())
    }

    /// Returns the "changed" change detection ticks for the value at `row`.
    ///
    /// Returns `None` if `row` is out of bounds.
    ///
    /// Note: The values stored within are [`UnsafeCell`].
    /// The user must ensure that each access to the individual elements
    /// adheres to the safety invariants of [`UnsafeCell`].
    ///
    /// [`UnsafeCell`]: std::cell::UnsafeCell
    #[inline]
    pub fn get_changed_ticks(&self, row: TableRow) -> Option<&UnsafeCell<Tick>> {
        self.changed_ticks.get(row.index())
    }

    /// Returns the change detection ticks for the value at `row`.
    ///
    /// Returns `None` if `row` is out of bounds.
    #[inline]
    pub fn get_ticks(&self, row: TableRow) -> Option<ComponentTicks> {
        if row.index() < self.data.len() {
            // SAFETY: The size of the column has already been checked.
            Some(unsafe { self.get_ticks_unchecked(row) })
        } else {
            None
        }
    }

    /// Returns the "added" change detection ticks for the value at `row`. Unlike [`Column::get_added_ticks`]
    /// this function does not do any bounds checking.
    ///
    /// # Safety
    /// `row` must be within the range `[0, self.len())`.
    #[inline]
    pub unsafe fn get_added_ticks_unchecked(&self, row: TableRow) -> &UnsafeCell<Tick> {
        debug_assert!(row.index() < self.added_ticks.len());
        self.added_ticks.get_unchecked(row.index())
    }

    /// Returns the "changed" change detection ticks for the value at `row`. Unlike [`Column::get_changed_ticks`]
    /// this function does not do any bounds checking.
    ///
    /// # Safety
    /// `row` must be within the range `[0, self.len())`.
    #[inline]
    pub unsafe fn get_changed_ticks_unchecked(&self, row: TableRow) -> &UnsafeCell<Tick> {
        debug_assert!(row.index() < self.changed_ticks.len());
        self.changed_ticks.get_unchecked(row.index())
    }

    /// Returns the change detection ticks for the value at `row`. Unlike [`Column::get_ticks`]
    /// this function does not do any bounds checking.
    ///
    /// # Safety
    /// `row` must be within the range `[0, self.len())`.
    #[inline]
    pub unsafe fn get_ticks_unchecked(&self, row: TableRow) -> ComponentTicks {
        debug_assert!(row.index() < self.added_ticks.len());
        debug_assert!(row.index() < self.changed_ticks.len());
        ComponentTicks {
            added: self.added_ticks.get_unchecked(row.index()).read(),
            changed: self.changed_ticks.get_unchecked(row.index()).read(),
        }
    }

    #[inline]
    pub(crate) fn check_change_ticks(&mut self, change_tick: Tick) {
        for component_ticks in &mut self.added_ticks {
            component_ticks.get_mut().check_tick(change_tick);
        }
        for component_ticks in &mut self.changed_ticks {
            component_ticks.get_mut().check_tick(change_tick);
        }
    }
}

/// A builder type for constructing [`Table`]s.
///
///  - Use [`with_capacity`] to initialize the builder.
///  - Repeatedly call [`add_column`] to add columns for components.
///  - Finalize with [`build`] to get the constructed [`Table`].
///
/// [`with_capacity`]: Self::with_capacity
/// [`add_column`]: Self::add_column
/// [`build`]: Self::build
pub(crate) struct TableBuilder {
    columns: SparseSet<ComponentId, Column>,
    capacity: usize,
}

impl TableBuilder {
    /// Creates a blank [`Table`], allocating space for `column_capacity` columns
    /// with the capacity to hold `capacity` entities worth of components each.
    pub fn with_capacity(capacity: usize, column_capacity: usize) -> Self {
        Self {
            columns: SparseSet::with_capacity(column_capacity),
            capacity,
        }
    }

    pub fn add_column(&mut self, component_info: &ComponentInfo) {
        self.columns.insert(
            component_info.id(),
            Column::with_capacity(component_info, self.capacity),
        );
    }

    pub fn build(self) -> Table {
        Table {
            columns: self.columns.into_immutable(),
            rows: Vec::with_capacity(self.capacity),
        }
    }
}

/// An [SoA] structure that stores arrays of [`Component`] data (columns) for multiple entities
/// (rows).
///
/// A table behaves like a `HashMap<ComponentId, Column>`, where each [`Column`] is a [`Vec`]
/// holding the component data. The components of a particular entity are stored at the same index
/// in all columns.
///
/// [SoA]: https://en.wikipedia.org/wiki/AoS_and_SoA#Structure_of_arrays
/// [`Component`]: crate::component::Component
pub struct Table {
    columns: ImmutableSparseSet<ComponentId, Column>,
    rows: Vec<Entity>,
}

impl Table {
    /// Returns the number of components (columns) in the table.
    #[inline]
    pub fn component_count(&self) -> usize {
        self.columns.len()
    }

    /// Returns the number of entities (rows) in the table.
    #[inline]
    pub fn entity_count(&self) -> usize {
        self.rows.len()
    }

    /// Returns the maximum number of entities the table can currently store
    /// without reallocating the underlying memory.
    #[inline]
    pub fn entity_capacity(&self) -> usize {
        self.rows.capacity()
    }

    /// Returns `true` if the table has no entities.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Returns the entities stored in the [`Table`].
    #[inline]
    pub fn entities(&self) -> &[Entity] {
        &self.rows
    }

    /// Clears all of the stored components in the table.
    pub(crate) fn clear(&mut self) {
        self.rows.clear();
        for column in self.columns.values_mut() {
            column.clear();
        }
    }

    /// Swap-removes the data at the given row and returns the [`Entity`] of the entity whose
    /// data was swapped into that position, if one existed.
    ///
    /// # Safety
    /// - `row` must be in-bounds.
    pub(crate) unsafe fn swap_remove_unchecked(&mut self, row: TableRow) -> Option<Entity> {
        debug_assert!(row.index() < self.rows.len());
        let is_last = row.index() == self.rows.len() - 1;
        self.rows.swap_remove(row.index());
        for column in self.columns.values_mut() {
            column.swap_remove_unchecked(row);
        }

        if is_last {
            None
        } else {
            Some(self.rows[row.index()])
        }
    }

    /// # Safety
    ///
    pub(crate) unsafe fn copy_from_unchecked(
        &mut self,
        src_table: &mut Table,
        src_row: TableRow,
    ) -> TableRow {
        let entity = src_table.entities()[src_row.0 as usize];

        let dst_row = self.allocate(entity);
        for (&component_id, dst_column) in self.columns.iter_mut() {
            if let Some(src_column) = src_table.get_column_mut(component_id) {
                dst_column.initialize_from_unchecked(dst_row, src_column, src_row)
            }
        }

        dst_row
    }

    /// Moves the component values at `row` to `new_table` and returns the [`TableMoveResult`].
    ///
    /// Called when components are inserted.
    ///
    /// # Safety
    /// - `row` must be in-bounds.
    /// - `new_table` must have a superset of the components this table has.
    pub(crate) unsafe fn move_to_superset_unchecked(
        &mut self,
        row: TableRow,
        new_table: &mut Table,
    ) -> TableMoveResult {
        debug_assert!(row.index() < self.rows.len());
        let is_last = row.index() == self.rows.len() - 1;
        let new_row = new_table.allocate(self.rows.swap_remove(row.index()));
        for (component_id, column) in self.columns.iter_mut() {
            new_table
                .get_column_mut(*component_id)
                .debug_checked_unwrap()
                .initialize_from_unchecked(new_row, column, row);
        }
        TableMoveResult {
            table_row: new_row,
            swapped_entity: if is_last {
                None
            } else {
                Some(self.rows[row.index()])
            },
        }
    }

    /// Moves the component values at `row` to `new_table`, drops any values of components
    /// that `new_table` doesn't have, and returns the [`TableMoveResult`].
    ///
    /// Called when components are removed.
    ///
    /// # Safety
    /// - `row` must be in-bounds
    pub(crate) unsafe fn move_to_and_drop_missing_unchecked(
        &mut self,
        row: TableRow,
        new_table: &mut Table,
    ) -> TableMoveResult {
        debug_assert!(row.index() < self.rows.len());
        let is_last = row.index() == self.rows.len() - 1;
        let new_row = new_table.allocate(self.rows.swap_remove(row.index()));
        for (component_id, column) in self.columns.iter_mut() {
            if let Some(new_column) = new_table.get_column_mut(*component_id) {
                new_column.initialize_from_unchecked(new_row, column, row);
            } else {
                column.swap_remove_unchecked(row);
            }
        }
        TableMoveResult {
            table_row: new_row,
            swapped_entity: if is_last {
                None
            } else {
                Some(self.rows[row.index()])
            },
        }
    }

    /// Moves the component values at `row` to `new_table`, leaks any values of components
    /// that `new_table` doesn't have, and returns the [`TableMoveResult`].
    ///
    /// Called when components are taken.
    ///
    /// # Safety
    /// - `row` must be in-bounds.
    /// - The caller is responsible for dropping the leaked values.
    pub(crate) unsafe fn move_to_and_forget_missing_unchecked(
        &mut self,
        row: TableRow,
        new_table: &mut Table,
    ) -> TableMoveResult {
        debug_assert!(row.index() < self.rows.len());
        let is_last = row.index() == self.rows.len() - 1;
        let new_row = new_table.allocate(self.rows.swap_remove(row.index()));
        for (component_id, column) in self.columns.iter_mut() {
            if let Some(new_column) = new_table.get_column_mut(*component_id) {
                new_column.initialize_from_unchecked(new_row, column, row);
            } else {
                // SAFETY: It's the caller's responsibility to drop these.
                let (_, _) = column.swap_remove_and_forget_unchecked(row);
            }
        }
        TableMoveResult {
            table_row: new_row,
            swapped_entity: if is_last {
                None
            } else {
                Some(self.rows[row.index()])
            },
        }
    }

    /// Returns `true` if the table contains a [`Column`] for the [`Component`].
    ///
    /// [`Component`]: crate::component::Component
    #[inline]
    pub fn has_column(&self, component_id: ComponentId) -> bool {
        self.columns.contains(component_id)
    }

    /// Returns a reference to the [`Column`] for the [`Component`], if it exists.
    ///
    /// [`Component`]: crate::component::Component
    #[inline]
    pub fn get_column(&self, component_id: ComponentId) -> Option<&Column> {
        self.columns.get(component_id)
    }

    /// Returns a mutable reference to the [`Column`] for the [`Component`], if it exists.
    ///
    /// [`Component`]: crate::component::Component
    #[inline]
    pub(crate) fn get_column_mut(&mut self, component_id: ComponentId) -> Option<&mut Column> {
        self.columns.get_mut(component_id)
    }

    /// Reserves `additional` rows in the table.
    pub(crate) fn reserve(&mut self, additional: usize) {
        if self.rows.capacity() - self.rows.len() < additional {
            self.rows.reserve(additional);

            // have all columns match the entity vector's capacity
            let new_capacity = self.rows.capacity();
            for column in self.columns.values_mut() {
                column.reserve_exact(new_capacity - column.len());
            }
        }
    }

    /// Allocates a row for the entity's data and returns its index.
    ///
    /// # Safety
    /// The caller must immediately write valid values to the columns in the row.
    pub(crate) unsafe fn allocate(&mut self, entity: Entity) -> TableRow {
        self.reserve(1);
        let index = self.rows.len();
        self.rows.push(entity);
        for column in self.columns.values_mut() {
            column.data.set_len(self.rows.len());
            column.added_ticks.push(UnsafeCell::new(Tick::new(0)));
            column.changed_ticks.push(UnsafeCell::new(Tick::new(0)));
        }
        TableRow::new(index)
    }

    pub(crate) fn check_change_ticks(&mut self, change_tick: Tick) {
        for column in self.columns.values_mut() {
            column.check_change_ticks(change_tick);
        }
    }

    /// Iterates over the [`Column`]s of the table.
    pub fn iter(&self) -> impl Iterator<Item = &Column> {
        self.columns.values()
    }
}

/// A collection of [`Table`] storages, indexed by [`TableId`]
///
/// Can be accessed via [`Storages`](crate::storage::Storages)
pub struct Tables {
    tables: Vec<Table>,
    table_ids: HashMap<Vec<ComponentId>, TableId>,
}

impl Default for Tables {
    fn default() -> Self {
        let empty_table = TableBuilder::with_capacity(0, 0).build();
        Tables {
            tables: vec![empty_table],
            table_ids: HashMap::default(),
        }
    }
}

/// The result of transferring an entity from one table to another.
///
/// Has the row index in the destination table, as well as the entity in that table
/// whose data was originally stored in that row, if one existed.
pub(crate) struct TableMoveResult {
    pub table_row: TableRow,
    pub swapped_entity: Option<Entity>,
}

impl Tables {
    /// Returns the number of [`Table`]s this collection contains
    #[inline]
    pub fn len(&self) -> usize {
        self.tables.len()
    }

    /// Returns true if this collection contains no [`Table`]s
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    /// Returns a [`Table`] by its [`TableId`].
    ///
    /// Returns `None` if `id` is invalid.
    #[inline]
    pub fn get(&self, id: TableId) -> Option<&Table> {
        self.tables.get(id.index())
    }

    /// Returns mutable references to two different [`Table`]s.
    ///
    /// # Panics
    ///
    /// Panics if `a` and `b` are equal.
    #[inline]
    pub(crate) fn get_mut(&mut self, id: TableId) -> Option<&mut Table> {
        self.tables.get_mut(id.index())
    }

    #[inline]
    pub(crate) fn remove(&mut self, id: TableId) -> bool {
        todo!()
    }

    #[inline]
    pub(crate) fn get_2_mut(&mut self, a: TableId, b: TableId) -> (&mut Table, &mut Table) {
        if a.index() > b.index() {
            let (b_slice, a_slice) = self.tables.split_at_mut(a.index());
            (&mut a_slice[0], &mut b_slice[b.index()])
        } else {
            let (a_slice, b_slice) = self.tables.split_at_mut(b.index());
            (&mut a_slice[a.index()], &mut b_slice[0])
        }
    }

    /// Attempts to fetch a table based on the provided components,
    /// creating and returning a new [`Table`] if one did not already exist.
    ///
    /// # Safety
    /// `component_ids` must contain components that exist in `components`
    pub(crate) unsafe fn get_id_or_insert(
        &mut self,
        component_ids: &[ComponentId],
        components: &Components,
    ) -> TableId {
        let tables = &mut self.tables;
        let (_key, value) = self
            .table_ids
            .raw_entry_mut()
            .from_key(component_ids)
            .or_insert_with(|| {
                let mut table = TableBuilder::with_capacity(0, component_ids.len());
                for component_id in component_ids {
                    table.add_column(components.get_info_unchecked(*component_id));
                }
                tables.push(table.build());
                (component_ids.to_vec(), TableId::new(tables.len() - 1))
            });

        *value
    }

    /// Iterates through all of the tables stored within in [`TableId`] order.
    pub fn iter(&self) -> std::slice::Iter<'_, Table> {
        self.tables.iter()
    }

    /// Clears all data from all [`Table`]s stored within.
    pub(crate) fn clear(&mut self) {
        for table in &mut self.tables {
            table.clear();
        }
    }

    pub(crate) fn check_change_ticks(&mut self, change_tick: Tick) {
        for table in &mut self.tables {
            table.check_change_ticks(change_tick);
        }
    }
}

impl Index<TableId> for Tables {
    type Output = Table;

    #[inline]
    fn index(&self, index: TableId) -> &Self::Output {
        &self.tables[index.index()]
    }
}

impl IndexMut<TableId> for Tables {
    #[inline]
    fn index_mut(&mut self, index: TableId) -> &mut Self::Output {
        &mut self.tables[index.index()]
    }
}

#[cfg(test)]
mod tests {
    use crate as bevy_ecs;
    use crate::component::Component;
    use crate::ptr::OwningPtr;
    use crate::storage::Storages;
    use crate::{
        component::{Components, Tick},
        entity::Entity,
        storage::{TableBuilder, TableRow},
    };
    #[derive(Component)]
    struct W<T>(T);

    #[test]
    fn table() {
        let mut components = Components::default();
        let mut storages = Storages::default();
        let component_id = components.init_component::<W<TableRow>>(&mut storages);
        let columns = &[component_id];
        let mut builder = TableBuilder::with_capacity(0, columns.len());
        builder.add_column(components.get_info(component_id).unwrap());
        let mut table = builder.build();
        let entities = (0..200).map(Entity::from_raw).collect::<Vec<_>>();
        for entity in &entities {
            // SAFETY: we allocate and immediately set data afterwards
            unsafe {
                let row = table.allocate(*entity);
                let value: W<TableRow> = W(row);
                OwningPtr::make(value, |value_ptr| {
                    table.get_column_mut(component_id).unwrap().initialize(
                        row,
                        value_ptr,
                        Tick::new(0),
                    );
                });
            };
        }

        assert_eq!(table.entity_capacity(), 256);
        assert_eq!(table.entity_count(), 200);
    }
}
