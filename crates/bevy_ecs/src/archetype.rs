//! Types for defining [`Archetype`]s, collections of entities that have the same set of
//! components.
//!
//! An archetype uniquely describes a group of entities that share the same components:
//! a world only has one archetype for each unique combination of components, and all
//! entities that have those components and only those components belong to that
//! archetype.
//!
//! Archetypes are not to be confused with [`Table`]s. Each archetype stores its table
//! components in one table, and each archetype uniquely points to one table, but multiple
//! archetypes may store their table components in the same table. These archetypes
//! differ only by the [`SparseSet`] components.
//!
//! Like tables, archetypes can be created but are never cleaned up. Empty archetypes are
//! not removed, and persist until the world is dropped.
//!
//! Archetypes can be fetched from [`Archetypes`], which is accessible via [`World::archetypes`].
//!
//! [`Table`]: crate::storage::Table
//! [`World::archetypes`]: crate::world::World::archetypes

use crate::{
    bundle::{BundleId, BundleInfo},
    component::{ComponentArchetypeInfo, ComponentId, ComponentTableInfo, Components, StorageType},
    entity::{Entities, Entity, EntityLocation},
    query::DebugCheckedUnwrap,
    storage::{
        ImmutableSparseSet, SlotMap, SparseSet, SparseSetIndex, Storages, Table, TableColumn,
        TableId, TableRow,
    },
};

use bevy_utils::{default, HashMap, HashSet};

use std::{
    hash::Hash,
    ops::{Index, IndexMut},
};

use fixedbitset::FixedBitSet;

/// A unique ID of an [`Archetype`].
///
/// **Note:** `ArchetypeId` values are not globally unique and should only be considered valid in
/// their original world. Assume that an `ArchetypeId` (that isn't a constant like [`EMPTY`]) can
/// never refer to the same archetype in any other world.
///
/// [`World`]: crate::world::World
/// [`EMPTY`]: crate::archetype::ArchetypeId::EMPTY
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
// SAFETY: Must be repr(transparent) due to the safety requirements on EntityLocation
#[repr(transparent)]
pub struct ArchetypeId(u32);

impl ArchetypeId {
    /// The ID for the [`Archetype`] with no components.
    pub const EMPTY: ArchetypeId = ArchetypeId(0);
    /// # Safety
    ///
    /// This must always have an all-1s bit pattern to ensure the soundness of
    /// fast [`Entity`] space allocation.
    pub const INVALID: ArchetypeId = ArchetypeId(u32::MAX);

    #[inline]
    pub(crate) const fn new(index: usize) -> Self {
        assert!(index <= u32::MAX as usize);
        ArchetypeId(index as u32)
    }

    #[inline]
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

impl SparseSetIndex for ArchetypeId {
    fn sparse_set_index(&self) -> usize {
        self.0 as usize
    }

    fn get_sparse_set_index(value: usize) -> Self {
        Self(value as u32)
    }
}

/// A unique ID of the [`Component`] data in one [`Archetype`].
///
/// In database terms, these are composite keys representing a [many-to-many relationship] between
/// archetypes and components. A component type has only one [`ComponentId`] and an archetype has
/// only one [`ArchetypeId`], but many archetypes can have similar components, so each occurence
/// is given a unique `ArchetypeComponentId`. This information can be leveraged by system schedulers
/// to opportunistically run systems in parallel. For example, `Query<&mut A, With<B>>` and
/// `Query<&mut A, Without<B>>` can run at the same time because, even though both mutably borrow
/// `A`, they will always borrow from different archetypes.
///
/// To make the access model simpler, every [`Resource`] type is also assigned an
/// `ArchetypeComponentId` (but only one because resource types are not actually included in
/// archetypes).
///
/// **Note:** `ArchetypeComponentId` values are not globally unique and should only be considered
/// valid in their original world. Assume that an `ArchetypeComponentId` from one world can never
/// refer to the same one in any other world.
///
/// [`Component`]: crate::component::Component
/// [`World`]: crate::world::World
/// [`Resource`]: crate::system::Resource
/// [many-to-many relationship]: https://en.wikipedia.org/wiki/Many-to-many_(data_model)
#[derive(Clone, Copy, Debug, Hash, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct ArchetypeComponentId(usize);

impl ArchetypeComponentId {
    #[inline]
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }

    /// Gets the index of the row.
    #[inline]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl SparseSetIndex for ArchetypeComponentId {
    #[inline]
    fn sparse_set_index(&self) -> usize {
        self.0
    }

    fn get_sparse_set_index(value: usize) -> Self {
        Self(value)
    }
}

/// Metadata for a [`Component`] in an [`Archetype`].
///
/// [`Component`]: crate::component::Component
struct ArchetypeComponentInfo {
    storage_type: StorageType,
    archetype_component_id: ArchetypeComponentId,
}

/// The sort order within an [`Archetype`].
///
/// This can be used in conjunction with [`ArchetypeId`] to find the exact location of an
/// [`Entity`]'s data in the [`World`]. An entity's archetype and row can be retrieved via
/// [`Entities::get`].
///
/// [`World`]: crate::world::World
/// [`Entities::get`]: crate::entity::Entities
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
// SAFETY: Must be repr(transparent) due to the safety requirements on EntityLocation
#[repr(transparent)]
pub struct ArchetypeRow(u32);

impl ArchetypeRow {
    /// # Safety:
    ///
    /// This must always have an all-1s bit pattern to ensure soundness in fast [`Entity`] ID space
    /// allocation.
    pub const INVALID: ArchetypeRow = ArchetypeRow(u32::MAX);

    /// Creates a `ArchetypeRow`.
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

/// Metadata used to locate the data of a particular [`Entity`] in an [`Archetype`].
pub struct ArchetypeEntity {
    entity: Entity,
    table_row: TableRow,
}

impl ArchetypeEntity {
    /// The ID of the entity.
    #[inline]
    pub const fn entity(&self) -> Entity {
        self.entity
    }

    /// The row in the [`Table`] where the entity's components are stored.
    ///
    /// [`Table`]: crate::storage::Table
    #[inline]
    pub const fn table_row(&self) -> TableRow {
        self.table_row
    }
}

pub(crate) struct ArchetypeSwapRemoveResult {
    pub(crate) swapped_entity: Option<Entity>,
    pub(crate) table_row: TableRow,
}

/// A hashable representation of the set of components that uniquely define an [`Archetype`].
#[derive(Clone, Hash, PartialEq, Eq)]
struct ArchetypeDef {
    table_components: Box<[ComponentId]>,
    sparse_set_components: Box<[ComponentId]>,
}

/// Information about a group of entities sharing the same set of components in a [`World`].
///
/// For more information, see the [module-level documentation].
///
/// [`World`]: crate::world::World
/// [module-level documentation]: crate::archetype
pub struct Archetype {
    id: ArchetypeId,
    table_id: TableId,
    entities: Vec<ArchetypeEntity>,
    components: ImmutableSparseSet<ComponentId, ArchetypeComponentInfo>,
    edges: ArchetypeEdges,
}

impl Archetype {
    pub(crate) fn new(
        id: ArchetypeId,
        table_id: TableId,
        table_components: impl Iterator<Item = (ComponentId, ArchetypeComponentId)>,
        sparse_set_components: impl Iterator<Item = (ComponentId, ArchetypeComponentId)>,
    ) -> Self {
        // TODO: debug_assert! that the IDs are sorted
        let (min_table, _) = table_components.size_hint();
        let (min_sparse, _) = sparse_set_components.size_hint();
        let mut components = SparseSet::with_capacity(min_table + min_sparse);

        for (component_id, archetype_component_id) in table_components {
            components.insert(
                component_id,
                ArchetypeComponentInfo {
                    storage_type: StorageType::Table,
                    archetype_component_id,
                },
            );
        }

        for (component_id, archetype_component_id) in sparse_set_components {
            components.insert(
                component_id,
                ArchetypeComponentInfo {
                    storage_type: StorageType::SparseSet,
                    archetype_component_id,
                },
            );
        }
        Self {
            id,
            table_id,
            entities: Vec::new(),
            components: components.into_immutable(),
            edges: Default::default(),
        }
    }

    /// Returns the [`ArchetypeId`] of the archetype.
    #[inline]
    pub fn id(&self) -> ArchetypeId {
        self.id
    }

    /// Returns the [`TableId`] of the [`Table`] that holds the component data of the entities
    /// in the archetype.
    #[inline]
    pub fn table_id(&self) -> TableId {
        self.table_id
    }

    /// Returns the number of entities with the archetype.
    #[inline]
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Returns `true` if there are no entities with the archetype.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    #[inline]
    pub fn entities(&self) -> &[ArchetypeEntity] {
        &self.entities
    }

    /// Returns an iterator over the components in the archetype.
    ///
    /// **Note:** The iterator will never yield more than one of each [`ComponentId`].
    #[inline]
    pub fn components(&self) -> impl Iterator<Item = ComponentId> + '_ {
        self.components.indices()
    }

    /// Returns an iterator over the [`Table`](StorageType::Table) components
    /// in the archetype.
    ///
    /// **Note:** The iterator will never yield more than one of each [`ComponentId`].
    #[inline]
    pub fn table_components(&self) -> impl Iterator<Item = ComponentId> + '_ {
        self.components
            .iter()
            .filter(|(_, component)| component.storage_type == StorageType::Table)
            .map(|(id, _)| *id)
    }

    /// Returns an iterator over the [`SparseSet`](StorageType::SparseSet) components
    /// in the archetype.
    ///
    /// **Note:** The iterator will never yield more than one of each [`ComponentId`].
    #[inline]
    pub fn sparse_set_components(&self) -> impl Iterator<Item = ComponentId> + '_ {
        self.components
            .iter()
            .filter(|(_, component)| component.storage_type == StorageType::SparseSet)
            .map(|(id, _)| *id)
    }

    /// Returns a reference to the cached edges of the archetype.
    #[inline]
    pub(crate) fn edges(&self) -> &ArchetypeEdges {
        &self.edges
    }

    /// Returns a mutable reference to the cached edges of the archetype.
    #[inline]
    pub(crate) fn edges_mut(&mut self) -> &mut ArchetypeEdges {
        &mut self.edges
    }

    /// Returns the row in the [`Table`] where the components of the entity at `row`
    /// is stored.
    ///
    /// An entity's archetype row  can be fetched from [`archetype_row`], which
    /// can be retrieved from [`get`].
    ///
    /// # Panics
    /// This function will panic if `row >= self.len()`.
    ///
    /// [`Table`]: crate::storage::Table
    /// [`archetype_row`]: crate::entity::EntityLocation::archetype_row
    /// [`get`]: crate::entity::Entities::get
    #[inline]
    pub fn entity_table_row(&self, row: ArchetypeRow) -> TableRow {
        self.entities[row.index()].table_row
    }

    /// Updates if the components for the entity at `index` can be found in the corresponding table.
    ///
    /// # Panics
    /// This function will panic if `index >= self.len()`.
    #[inline]
    pub(crate) fn set_entity_table_row(&mut self, row: ArchetypeRow, table_row: TableRow) {
        self.entities[row.index()].table_row = table_row;
    }

    /// Add an entity to the archetype.
    ///
    /// # Safety
    /// - Valid component values must be immediately written to the relevant storages.
    /// - `table_row` must be valid.
    pub(crate) unsafe fn allocate(
        &mut self,
        entity: Entity,
        table_row: TableRow,
    ) -> EntityLocation {
        let archetype_row = ArchetypeRow::new(self.entities.len());
        self.entities.push(ArchetypeEntity { entity, table_row });

        EntityLocation {
            archetype_id: self.id,
            archetype_row,
            table_id: self.table_id,
            table_row,
        }
    }

    /// Reserves capacity for at least `additional` more entities to be inserted in the given
    /// [`Archetype`]. The collection may reserve more space to speculatively avoid frequent
    /// reallocations. After calling `reserve`, capacity will be greater than or equal to
    /// `self.len() + additional`. Does nothing if capacity is already sufficient.
    pub(crate) fn reserve(&mut self, additional: usize) {
        self.entities.reserve(additional);
    }

    /// Removes the entity at `row` and returns the [`TableRow`] where its component data lives.
    ///
    /// The removed entity is replaced by the last entity in the archetype.
    ///
    /// # Panics
    ///
    /// Panics if `row` is out of bounds.
    pub(crate) fn swap_remove(&mut self, row: ArchetypeRow) -> ArchetypeSwapRemoveResult {
        let is_last = row.index() == self.entities.len() - 1;
        let entity = self.entities.swap_remove(row.index());
        ArchetypeSwapRemoveResult {
            swapped_entity: if is_last {
                None
            } else {
                Some(self.entities[row.index()].entity)
            },
            table_row: entity.table_row,
        }
    }

    /// Returns `true` if the archetype includes the given component.
    #[inline]
    pub fn contains(&self, component_id: ComponentId) -> bool {
        self.components.contains(component_id)
    }

    /// Returns the [`StorageType`] of the given component.
    ///
    /// Returns `None` if the component is not present in the archetype.
    #[inline]
    pub fn get_storage_type(&self, component_id: ComponentId) -> Option<StorageType> {
        self.components
            .get(component_id)
            .map(|info| info.storage_type)
    }

    /// Returns the corresponding [`ArchetypeComponentId`] for a component in the archetype.
    ///
    /// Returns `None` if the component is not present in the archetype.
    #[inline]
    pub fn get_archetype_component_id(
        &self,
        component_id: ComponentId,
    ) -> Option<ArchetypeComponentId> {
        self.components
            .get(component_id)
            .map(|info| info.archetype_component_id)
    }

    /// Removes all entities from the archetype.
    pub(crate) fn clear_entities(&mut self) {
        self.entities.clear();
    }
}

/// A count that increments every time an [`Archetype`] is created or destroyed.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
#[repr(transparent)]
pub struct ArchetypeGeneration(usize);

impl ArchetypeGeneration {
    #[inline]
    pub(crate) const fn initial() -> Self {
        ArchetypeGeneration(0)
    }

    #[inline]
    pub(crate) fn value(self) -> usize {
        self.0
    }
}

impl Default for ArchetypeGeneration {
    fn default() -> Self {
        Self::initial()
    }
}

/// Stores metadata for every [`Archetype`] in the [`World`].
///
/// For more information, see the [module-level documentation].
///
/// [`World`]: crate::world::World
/// [*module-level documentation]: crate::archetype
#[derive(Default)]
pub struct Archetypes {
    pub(crate) archetypes: SlotMap<Archetype>,
    pub(crate) edges: SlotMap<ArchetypeEdge>,
    archetype_ids: HashMap<ArchetypeDef, ArchetypeId>,
    generation: ArchetypeGeneration,
    archetype_component_count: usize,
}

impl Archetypes {
    pub(crate) fn new(components: &mut Components) -> Self {
        let mut archetypes = Archetypes {
            archetypes: SlotMap::with_capacity(1),
            edges: SlotMap::new(),
            archetype_ids: default(),
            generation: default(),
            archetype_component_count: 0,
        };
        // add empty archetype
        archetypes.get_id_or_insert(components, TableId::empty(), Vec::new(), Vec::new());
        archetypes
    }

    /// Returns the current generation number, which increments each time an archetype is created or
    /// destroyed.
    #[inline]
    pub fn generation(&self) -> ArchetypeGeneration {
        self.generation
    }

    /// Returns the current number of archetypes.
    #[inline]
    pub fn len(&self) -> usize {
        self.archetypes.len()
    }

    /// Returns the current number of edges between archetypes.
    #[inline]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Returns the current number of [`ArchetypeComponentId`].
    #[inline]
    pub fn archetype_component_count(&self) -> usize {
        self.archetype_component_count
    }

    /// TODO
    pub(crate) fn next_archetype_component(&mut self) -> ArchetypeComponentId {
        let n = self.archetype_component_count;
        self.archetype_component_count = self.archetype_component_count.checked_add(1).unwrap();
        ArchetypeComponentId(n)
    }

    /// Remove all entities from all archetypes.
    pub(crate) fn clear_entities(&mut self) {
        for archetype in self.archetypes.values_mut() {
            archetype.clear_entities();
        }
    }

    /// Returns a reference to the [`Archetype`] with no components.
    #[inline]
    pub fn empty(&self) -> &Archetype {
        // SAFETY: empty archetype always exists
        unsafe { self.get(ArchetypeId::EMPTY).unwrap_unchecked() }
    }

    /// Returns a mutable reference to the [`Archetype`] with no components.
    #[inline]
    pub(crate) fn empty_mut(&mut self) -> &mut Archetype {
        // SAFETY: empty archetype always exists
        unsafe { self.get_mut(ArchetypeId::EMPTY).unwrap_unchecked() }
    }

    /// Returns a reference to the [`Archetype`], if it exists.
    #[inline]
    pub fn get(&self, id: ArchetypeId) -> Option<&Archetype> {
        self.archetypes.get_unknown_version(id.index())
    }

    /// Returns a mutable reference to the [`Archetype`], if it exists.
    #[inline]
    pub fn get_mut(&mut self, id: ArchetypeId) -> Option<&mut Archetype> {
        self.archetypes.get_unknown_version_mut(id.index())
    }

    #[inline]
    pub(crate) unsafe fn get_many_unchecked_mut<const N: usize>(
        &mut self,
        ids: [ArchetypeId; N],
    ) -> [&'_ mut Archetype; N] {
        let mut indices = [usize::MAX; N];
        ids.iter()
            .enumerate()
            .for_each(|(i, id)| indices[i] = id.index());
        self.archetypes
            .get_unknown_version_disjoint_unchecked_mut(indices)
    }

    #[inline]
    pub(crate) fn get_many_mut<const N: usize>(
        &mut self,
        ids: [ArchetypeId; N],
    ) -> Option<[&'_ mut Archetype; N]> {
        let mut indices = [usize::MAX; N];
        ids.iter()
            .enumerate()
            .for_each(|(i, id)| indices[i] = id.index());
        self.archetypes.get_unknown_version_disjoint_mut(indices)
    }

    /// Returns the [`ArchetypeId`] of the set of components, creating one if it doesn't exist.
    ///
    /// # Safety
    /// - The given table must exist.
    /// - `table_components` and `sparse_set_components` must be sorted.
    pub(crate) fn get_id_or_insert(
        &mut self,
        components: &mut Components,
        table_id: TableId,
        table_components: Vec<ComponentId>,
        sparse_set_components: Vec<ComponentId>,
    ) -> ArchetypeId {
        let archetype_def = ArchetypeDef {
            sparse_set_components: sparse_set_components.clone().into_boxed_slice(),
            table_components: table_components.clone().into_boxed_slice(),
        };

        let archetypes = &mut self.archetypes;

        *self
            .archetype_ids
            .entry(archetype_def)
            .or_insert_with(move || {
                let archetype_id = ArchetypeId::new(archetypes.next_free_index());

                let mut table_archetype_components = Vec::with_capacity(table_components.len());
                let mut sparse_set_archetype_components =
                    Vec::with_capacity(sparse_set_components.len());

                for (i, component_id) in table_components.iter().cloned().enumerate() {
                    let archetype_component_id =
                        ArchetypeComponentId(self.next_archetype_component());

                    // update index
                    let record = &mut components.location_index[component_id.index()];
                    record.archetypes.insert(
                        archetype_id,
                        ComponentArchetypeInfo {
                            id: archetype_component_id,
                            column: i,
                        },
                    );
                    record
                        .tables
                        .insert(table_id, ComponentTableInfo { column: i });

                    table_archetype_components.push(archetype_component_id);
                }

                for (i, component_id) in sparse_set_components.iter().cloned().enumerate() {
                    let archetype_component_id =
                        ArchetypeComponentId(self.next_archetype_component());

                    // update index
                    let record = &mut components.location_index[component_id.index()];
                    record.archetypes.insert(
                        archetype_id,
                        ComponentArchetypeInfo {
                            id: archetype_component_id,
                            column: table_components.len() + i,
                        },
                    );

                    sparse_set_archetype_components.push(archetype_component_id);
                }

                archetypes.insert(Archetype::new(
                    archetype_id,
                    table_id,
                    table_components.into_iter().zip(table_archetype_components),
                    sparse_set_components
                        .into_iter()
                        .zip(sparse_set_archetype_components),
                ));

                archetype_id
            })
    }

    fn insert_edge(
        &mut self,
        src: ArchetypeId,
        dst: ArchetypeId,
        contents: ArchetypeEdgeContents,
        action: ArchetypeEdgeAction,
    ) {
        let is_bundle = matches!(contents, ArchetypeEdgeContents::Bundle(_));
        let is_insert = matches!(action, ArchetypeEdgeAction::Insert(_));

        let mut new_edge = ArchetypeEdge {
            src,
            dst,
            id: ArchetypeEdgeId(self.edges.next_free_index() as u32),
            contents,
            action,
            prev_incoming: None,
            next_incoming: None,
        };

        // add to the source archetype's set of outgoing edges
        let mut src_archetype = self
            .archetypes
            .get_unknown_version_mut(src.index())
            .unwrap();

        if is_insert {
            src_archetype.edges.outgoing_insert.insert(new_edge.id);
            if is_bundle {
                src_archetype
                    .edges
                    .outgoing_insert_bundle_index
                    .insert(bundle.id, new_edge.id);
            } else {
                src_archetype
                    .edges
                    .outgoing_insert_index
                    .insert(component_id, new_edge.id);
            }
        } else {
            src_archetype.edges.outgoing_remove.insert(new_edge.id);
            if is_bundle {
                src_archetype
                    .edges
                    .outgoing_remove_bundle_index
                    .insert(bundle.id, new_edge.id);
            } else {
                src_archetype
                    .edges
                    .outgoing_remove_index
                    .insert(component_id, new_edge.id);
            }
        }

        // add to the destination archetype's linked list of incoming edges
        let mut dst_archetype = self
            .archetypes
            .get_unknown_version_mut(dst.index())
            .unwrap();

        let head = if is_insert {
            &mut dst_archetype.edges.incoming_insert
        } else {
            &mut dst_archetype.edges.incoming_remove
        };

        new_edge.next_incoming = *head;
        if let Some(edge_id) = head {
            let head_edge = self.edges.get_unknown_version_mut(edge_id.index()).unwrap();
            debug_assert!(head_edge.prev_incoming.is_none());
            head_edge.prev_incoming = Some(new_edge.id);
        }
        *head = Some(new_edge.id);

        self.edges.insert(new_edge);
    }

    /// Returns a reference to the [`ArchetypeEdge`] that was cached after adding
    /// the [`Component`] with `component_id` to the [`Archetype`] with `archetype_id`.
    pub(crate) fn get_edge_for_insert(
        &self,
        archetype_id: ArchetypeId,
        component_id: ComponentId,
    ) -> Option<&ArchetypeEdge> {
        if let Some(archetype) = self.archetypes.get_unknown_version(archetype_id.index()) {
            if let Some(edge_id) = archetype.edges.get_edge_for_insert(component_id) {
                return self.edges.get_unknown_version(edge_id.index());
            }
        }

        None
    }

    /// Returns a reference to the [`ArchetypeEdge`] that was cached after removing
    /// the [`Component`] with `component_id` from the [`Archetype`] with `archetype_id`.
    pub(crate) fn get_edge_for_remove(
        &self,
        archetype_id: ArchetypeId,
        component_id: ComponentId,
    ) -> Option<&ArchetypeEdge> {
        if let Some(archetype) = self.archetypes.get_unknown_version(archetype_id.index()) {
            if let Some(edge_id) = archetype.edges.get_edge_for_remove(component_id) {
                return self.edges.get_unknown_version(edge_id.index());
            }
        }

        None
    }

    /// Returns a reference to the [`ArchetypeEdge`] that was cached after adding
    /// the [`Bundle`] with `bundle_id` to the [`Archetype`] with `archetype_id`.
    pub(crate) fn get_edge_for_insert_bundle(
        &self,
        archetype_id: ArchetypeId,
        bundle_id: BundleId,
    ) -> Option<&ArchetypeEdge> {
        if let Some(archetype) = self.archetypes.get_unknown_version(archetype_id.index()) {
            if let Some(edge_id) = archetype.edges.get_edge_for_insert_bundle(bundle_id) {
                return self.edges.get_unknown_version(edge_id.index());
            }
        }

        None
    }

    /// Returns a reference to the [`ArchetypeEdge`] that was cached after removing
    /// the [`Bundle`] with `bundle_id` from the [`Archetype`] with `archetype_id`.
    pub(crate) fn get_edge_for_remove_bundle(
        &self,
        archetype_id: ArchetypeId,
        bundle_id: BundleId,
    ) -> Option<&ArchetypeEdge> {
        if let Some(archetype) = self.archetypes.get_unknown_version(archetype_id.index()) {
            if let Some(edge_id) = archetype.edges.get_edge_for_remove_bundle(bundle_id) {
                return self.edges.get_unknown_version(edge_id.index());
            }
        }

        None
    }

    fn delete_edge(&mut self, id: ArchetypeEdgeId) {
        remove_edge_from_graph(id, &mut self.archetypes, &mut self.edges);
    }

    /// Returns the [`ArchetypeId`] of the new [`Archetype`] that would be reached by adding the
    /// [`Component`](crate::component::Component) to the given [`Archetype`], and a [`bool`] that
    /// is `true` if the given [`Archetype`] did *not* already have the [`Component`].
    pub(crate) fn insert_component_into_archetype(
        &mut self,
        component_id: ComponentId,
        archetype_id: ArchetypeId,
        components: &mut Components,
        storages: &mut Storages,
        cache_edge: bool,
    ) -> (ArchetypeId, bool) {
        // check cache
        if let Some(edge) = self.get_edge_for_insert(archetype_id, component_id) {
            let new_archetype_id = edge.dst;
            let ArchetypeEdgeAction::Insert(has) = edge.action else {
                unreachable!()
            };
            return (new_archetype_id, has);
        }

        // SAFETY: component has been initialized
        let component_info = unsafe { components.get_info_unchecked(component_id) };
        let component_storage = component_info.storage_type();

        let current_archetype = self.get(archetype_id).unwrap();
        let (new_archetype_id, has) = if current_archetype.contains(component_id) {
            // adding this component does not change the archetype
            (archetype_id, true)
        } else {
            // adding this component changes the archetype
            let mut new_table_components = current_archetype.table_components().collect::<Vec<_>>();
            let mut new_sparse_set_components = current_archetype
                .sparse_set_components()
                .collect::<Vec<_>>();

            let new_table_id = match component_storage {
                StorageType::Table => {
                    new_table_components.push(component_id);
                    new_table_components.sort();

                    // SAFETY: components have all been initialized
                    unsafe {
                        storages
                            .tables
                            .get_id_or_insert(&new_table_components, components)
                    }
                }
                StorageType::SparseSet => {
                    new_sparse_set_components.push(component_id);
                    new_sparse_set_components.sort();
                    current_archetype.table_id()
                }
            };

            (
                self.get_id_or_insert(
                    components,
                    new_table_id,
                    new_table_components,
                    new_sparse_set_components,
                ),
                false,
            )
        };

        if cache_edge {
            // remember this traversal
            self.insert_edge(
                archetype_id,
                new_archetype_id,
                ArchetypeEdgeContents::Component(component_id),
                ArchetypeEdgeAction::Insert(has),
            );
        }

        (new_archetype_id, has)
    }

    /// Returns the [`ArchetypeId`] of the new [`Archetype`] that would be reached by removing the
    /// [`Component`] from the given [`Archetype`], and a [`bool`] that is `true` if the given
    /// [`Archetype`] had the [`Component`].
    ///
    /// [`Component`]: crate::component::Component
    pub(crate) fn remove_component_from_archetype(
        &mut self,
        component_id: ComponentId,
        archetype_id: ArchetypeId,
        components: &mut Components,
        storages: &mut Storages,
        cache_edge: bool,
    ) -> (ArchetypeId, bool) {
        // check cache
        if let Some(edge) = self.get_edge_for_remove(archetype_id, component_id) {
            let new_archetype_id = edge.dst;
            let ArchetypeEdgeAction::Remove(has) = edge.action else {
                unreachable!()
            };
            return (new_archetype_id, has);
        }

        // SAFETY: component has been initialized
        let component_info = unsafe { components.get_info_unchecked(component_id) };
        let component_storage = component_info.storage_type();

        let current_archetype = self.get(archetype_id).unwrap();
        let (new_archetype_id, has) = if current_archetype.contains(component_id) {
            // removing this component changes the archetype
            let mut new_table_components = current_archetype.table_components().collect::<Vec<_>>();
            let mut new_sparse_set_components = current_archetype
                .sparse_set_components()
                .collect::<Vec<_>>();

            let new_table_id = match component_storage {
                StorageType::Table => {
                    // SAFETY: IDs are already sorted, ID must be present
                    let index = unsafe {
                        new_table_components
                            .iter()
                            .position(|&id| id == component_id)
                            .debug_checked_unwrap()
                    };
                    new_table_components.remove(index);

                    // SAFETY: components have all been initialized
                    unsafe {
                        storages
                            .tables
                            .get_id_or_insert(&new_table_components, components)
                    }
                }
                StorageType::SparseSet => {
                    // SAFETY: IDs are already sorted, ID must be present
                    let index = unsafe {
                        new_sparse_set_components
                            .iter()
                            .position(|&id| id == component_id)
                            .debug_checked_unwrap()
                    };
                    new_sparse_set_components.remove(index);

                    current_archetype.table_id()
                }
            };

            (
                self.get_id_or_insert(
                    components,
                    new_table_id,
                    new_table_components,
                    new_sparse_set_components,
                ),
                true,
            )
        } else {
            // removing this component does not change the archetype
            (archetype_id, false)
        };

        if cache_edge {
            // remember this traversal
            self.insert_edge(
                archetype_id,
                new_archetype_id,
                ArchetypeEdgeContents::Component(component_id),
                ArchetypeEdgeAction::Remove(has),
            );
        }

        (new_archetype_id, has)
    }

    /// Returns the [`ArchetypeId`] of the new [`Archetype`] that would be reached by adding the
    /// [`Bundle`] to the given [`Archetype`].
    ///
    /// [`Bundle`]: crate::bundle::Bundle
    pub(crate) fn insert_bundle_into_archetype(
        &mut self,
        bundle: &BundleInfo,
        archetype_id: ArchetypeId,
        components: &mut Components,
        storages: &mut Storages,
        cache_edges: bool,
    ) -> ArchetypeId {
        // check cache
        if let Some(edge) = self.get_edge_for_insert_bundle(archetype_id, bundle.id) {
            let new_archetype_id = edge.dst;
            return new_archetype_id;
        }

        let mut changed = FixedBitSet::with_capacity(bundle.component_ids.len());
        let mut new_archetype_id = archetype_id;
        for (i, component_id) in bundle.component_ids.iter().cloned().enumerate() {
            let (next, has) = self.insert_component_into_archetype(
                component_id,
                new_archetype_id,
                components,
                storages,
                cache_edges,
            );

            new_archetype_id = next;
            changed[i] = has;
        }

        if cache_edges {
            self.insert_edge(
                archetype_id,
                new_archetype_id,
                ArchetypeEdgeContents::Bundle(bundle.clone()),
                ArchetypeEdgeAction::InsertBundle(changed),
            );
        }

        new_archetype_id
    }

    /// Returns the [`ArchetypeId`] of the new [`Archetype`] that would be reached by removing the
    /// [`Bundle`] from the given [`Archetype`], and a [`bool`] that is `true` if the given
    /// [`Archetype`] had all components in the [`Bundle`].
    ///
    /// [`Bundle`]: crate::bundle::Bundle
    pub(crate) fn remove_bundle_from_archetype(
        &mut self,
        bundle: &BundleInfo,
        archetype_id: ArchetypeId,
        components: &mut Components,
        storages: &mut Storages,
        cache_edges: bool,
    ) -> (ArchetypeId, bool) {
        // check cache
        if let Some(edge) = self.get_edge_for_remove_bundle(archetype_id, bundle.id) {
            let new_archetype_id = edge.dst;
            let ArchetypeEdgeAction::RemoveBundle(has_all) = edge.action else {
                unreachable!()
            };
            return (new_archetype_id, has_all);
        }

        let mut new_archetype_id = archetype_id;
        let mut has_all = true;
        for component_id in bundle.component_ids.iter().cloned() {
            let (next, has) = self.remove_component_from_archetype(
                component_id,
                new_archetype_id,
                components,
                storages,
                cache_edges,
            );

            new_archetype_id = next;
            has_all &= has;
        }

        if cache_edges {
            self.insert_edge(
                archetype_id,
                new_archetype_id,
                ArchetypeEdgeContents::Bundle(bundle.clone()),
                ArchetypeEdgeAction::RemoveBundle(has_all),
            );
        }

        (new_archetype_id, has_all)
    }

    /// Moves an entity from one archetype to another and returns its new location.
    ///
    /// # Safety
    ///
    /// - The `location` given must be the true location of `entity`.
    /// - If `action` is [`Insert`](Move::Insert):
    ///   - The new archetype must have a superset of the components present in `entity`'s current
    /// archetype.
    ///   - The caller must ensure the new table row is written with valid data immediately.
    /// - If `action` is [`Take`](Move::Take):
    ///   - The caller assumes responsibility for dropping the components not present in the new
    /// archetype.
    /// - If `action` is [`Remove`](Move::Remove):
    ///   - Nothing else to uphold.
    pub(crate) unsafe fn move_entity(
        &mut self,
        action: Move,
        entity: Entity,
        location: EntityLocation,
        new_archetype_id: ArchetypeId,
        entities: &mut Entities,
        storages: &mut Storages,
    ) -> EntityLocation {
        if location.archetype_id == new_archetype_id {
            return location;
        }

        let ptr: *mut Self = self;

        let [old_archetype, new_archetype] = self
            .get_many_mut([location.archetype_id, new_archetype_id])
            .debug_checked_unwrap();

        let old_table_id = old_archetype.table_id();
        let new_table_id = new_archetype.table_id();

        if new_table_id == old_table_id {
            move_entity_diff_archetype_same_table(
                entity,
                location,
                entities,
                old_archetype,
                new_archetype,
            )
        } else {
            let (old_table, new_table) = storages.tables.get_2_mut(old_table_id, new_table_id);
            move_entity_diff_archetype_diff_table(
                action,
                entity,
                location,
                entities,
                ptr,
                old_archetype,
                new_archetype,
                old_table,
                new_table,
            )
        }
    }

    /// Returns an iterator over all archetypes.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &Archetype> {
        self.archetypes.values()
    }

    /// Returns a mutable iterator over all archetypes.
    #[inline]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Archetype> {
        self.archetypes.values_mut()
    }
}

/// Used to configure [`move_entity`](Archetypes::move_entity).
pub(crate) enum Move {
    /// Component data will be inserted.
    Insert,
    /// Component data will be removed and dropped.
    Remove,
    /// Component data will be removed and forgotten.
    Take,
}

/// # Safety
///
///
pub(crate) unsafe fn move_entity_diff_archetype_same_table(
    entity: Entity,
    location: EntityLocation,
    entities: &mut Entities,
    old_archetype: &mut Archetype,
    new_archetype: &mut Archetype,
) -> EntityLocation {
    debug_assert_ne!(old_archetype.id, new_archetype.id);
    debug_assert_eq!(old_archetype.table_id, new_archetype.table_id);

    // remove `entity` from its old archetype
    let result = old_archetype.swap_remove(location.archetype_row);

    // if removing this entity from the archetype causes another entity to move,
    // we have to update that entity's archetype row
    if let Some(swapped_entity) = result.swapped_entity {
        let mut swapped_location = entities.get(swapped_entity).unwrap();
        swapped_location.archetype_row = location.archetype_row;
        // SAFETY: only the archetype row has changed (to row where `entity` was)
        unsafe {
            entities.set(swapped_entity.index(), swapped_location);
        }
    }

    let new_location = unsafe { new_archetype.allocate(entity, result.table_row) };

    new_location
}

/// # Safety
///
///
pub(crate) unsafe fn move_entity_diff_archetype_diff_table(
    action: Move,
    entity: Entity,
    location: EntityLocation,
    entities: &mut Entities,
    archetypes: *mut Archetypes,
    old_archetype: &mut Archetype,
    new_archetype: &mut Archetype,
    old_table: &mut Table,
    new_table: &mut Table,
) -> EntityLocation {
    debug_assert_ne!(old_archetype.id, new_archetype.id);
    debug_assert_ne!(old_archetype.table_id, new_archetype.table_id);

    // remove `entity` from its old archetype
    let archetype_move = old_archetype.swap_remove(location.archetype_row);

    // if removing this entity from the archetype causes another entity to move,
    // we have to update that entity's archetype row
    if let Some(swapped_entity) = archetype_move.swapped_entity {
        let mut entity_location = entities.get(swapped_entity).debug_checked_unwrap();
        entity_location.archetype_row = location.archetype_row;
        // SAFETY: only the archetype row has changed (to row where `entity` was)
        unsafe {
            entities.set(swapped_entity.index(), entity_location);
        }
    }

    // move `entity` from the old table to the new table
    // SAFETY: `archetype_move.table_row` is valid
    let table_move = unsafe {
        match action {
            Move::Insert => {
                old_table.move_to_superset_unchecked(archetype_move.table_row, new_table)
            }
            Move::Remove => {
                old_table.move_to_and_drop_missing_unchecked(archetype_move.table_row, new_table)
            }
            Move::Take => {
                // (caller responsible for dropping the forgetten component values)
                old_table.move_to_and_forget_missing_unchecked(archetype_move.table_row, new_table)
            }
        }
    };

    // SAFETY: `table_move.table_row` is valid
    let new_location = unsafe { new_archetype.allocate(entity, table_move.table_row) };

    // if removing this entity from the table causes another entity to move,
    // we have to update that entity's table row
    if let Some(swapped_entity) = table_move.swapped_entity {
        let mut swapped_location = entities.get(swapped_entity).debug_checked_unwrap();
        swapped_location.table_row = location.table_row;
        // SAFETY: only the table row has changed (to row where `entity` was)
        unsafe {
            entities.set(swapped_entity.index(), swapped_location);
        }

        // TODO: safety
        let entity_archetype = unsafe { *archetypes }
            .get_mut(swapped_location.archetype_id)
            .debug_checked_unwrap();

        entity_archetype
            .set_entity_table_row(swapped_location.archetype_row, swapped_location.table_row);
    }

    // SAFETY: `new_location` is legitimately where `entity`'s data lives
    unsafe {
        entities.set(entity.index(), new_location);
    }

    new_location
}

/// A unique ID of an [`ArchetypeEdge`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd)]
struct ArchetypeEdgeId(u32);

impl ArchetypeEdgeId {
    #[inline]
    pub(crate) const fn new(index: usize) -> Self {
        Self(index as u32)
    }

    #[inline]
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

enum ArchetypeEdgeContents {
    /// This edge inserts or removes a component.
    Component(ComponentId),
    /// This edge inserts or removes a component bundle. This is a shortcut we can take with
    /// compile-time knowledge.
    Bundle(BundleInfo),
}

/// Metadata that indicates whether an edge inserts or removes components.
pub(crate) enum ArchetypeEdgeAction {
    /// A component will be inserted.
    ///
    /// The [`bool`] is `true` if the component was present in the source archetype.
    Insert(bool),
    /// A component will be removed.
    ///
    /// The [`bool`] is `true` if the component is present in the source archetype.
    Remove(bool),
    /// A bundle will be inserted.
    ///
    /// A bit in the bitset is `true` if the component was present in the source archetype.
    InsertBundle(FixedBitSet),
    /// A bundle will be removed.
    ///
    /// The [`bool`] is `true` if all components in the bundle were present in the source archetype.
    RemoveBundle(bool),
}

/// The details of a previous move from one [`Archetype`] to another. We save this "manifest" to
/// minimize the cost of doing the same move again later.
pub(crate) struct ArchetypeEdge {
    /// The source archetype.
    src: ArchetypeId,
    /// The destination archetype.
    dst: ArchetypeId,
    /// The ID of this edge.
    pub(crate) id: ArchetypeEdgeId,
    /// The ID(s) of the component(s) that was added or removed by this edge.
    contents: ArchetypeEdgeContents,
    /// Whether the bundle was added or removed.
    pub(crate) action: ArchetypeEdgeAction,
    /// The previous incoming edge in the destination archetype's linked list.
    prev_incoming: Option<ArchetypeEdgeId>,
    /// The next incoming edge in the destination archetype's linked list.
    next_incoming: Option<ArchetypeEdgeId>,
}

/// All known edges connecting an [`Archetype`] to others.
///
/// Adding or removing a [`Component`](crate::component::Component) will move an [`Entity`] from
/// one [`Archetype`] to another. [`ArchetypeEdges`] caches the results of these moves.
///
/// **Note:** This structure only caches moves that have actually happened (and are still possible
/// to do). A method returning `None` does not mean that inserting or removing a component is
/// invalid, only that such insertion or removal has not happened yet.
#[derive(Default)]
pub(crate) struct ArchetypeEdges {
    /// Set of outgoing edges that insert components.
    outgoing_insert: HashSet<ArchetypeEdgeId>,
    /// Set of outgoing edges that remove components.
    outgoing_remove: HashSet<ArchetypeEdgeId>,
    /// Lookup edge that inserts a component.
    outgoing_insert_index: HashMap<ComponentId, ArchetypeEdgeId>,
    /// Lookup edge that removes a component.
    outgoing_remove_index: HashMap<ComponentId, ArchetypeEdgeId>,
    /// Lookup edge that inserts a bundle.
    outgoing_insert_bundle_index: HashMap<BundleId, ArchetypeEdgeId>,
    /// Lookup edge that removes a bundle.
    outgoing_remove_bundle_index: HashMap<BundleId, ArchetypeEdgeId>,
    /// Linked list of incoming edges that insert components.
    incoming_insert: Option<ArchetypeEdgeId>,
    /// Linked list of incoming edges that remove components.
    incoming_remove: Option<ArchetypeEdgeId>,
}

impl ArchetypeEdges {
    fn get_edge_for_insert(&self, component: ComponentId) -> Option<ArchetypeEdgeId> {
        self.outgoing_insert_index.get(&component).cloned()
    }

    fn get_edge_for_remove(&self, component: ComponentId) -> Option<ArchetypeEdgeId> {
        self.outgoing_remove_index.get(&component).cloned()
    }

    fn get_edge_for_insert_bundle(&self, bundle: BundleId) -> Option<ArchetypeEdgeId> {
        self.outgoing_insert_bundle_index.get(&bundle).cloned()
    }

    fn get_edge_for_remove_bundle(&self, bundle: BundleId) -> Option<ArchetypeEdgeId> {
        self.outgoing_remove_bundle_index.get(&bundle).cloned()
    }
}

fn remove_archetype_from_graph(
    archetype_id: ArchetypeId,
    archetypes: &mut SlotMap<Archetype>,
    edges: &mut SlotMap<ArchetypeEdge>,
) -> Archetype {
    // remove the node from the graph
    let src_archetype = archetypes
        .remove_unknown_version(archetype_id.index())
        .unwrap();

    // remove its incoming insert edges
    let mut next = src_archetype.edges.incoming_insert;
    while let Some(edge_id) = next {
        let edge = remove_edge_from_graph(edge_id, archetypes, edges);
        next = edge.next_incoming;
    }

    // remove its incoming remove edges
    let mut next = src_archetype.edges.incoming_remove;
    while let Some(edge_id) = next {
        let edge = remove_edge_from_graph(edge_id, archetypes, edges);
        next = edge.next_incoming;
    }

    // remove its outgoing insert edges
    for edge_id in src_archetype.edges.outgoing_insert.into_iter() {
        remove_edge_from_graph(edge_id, archetypes, edges);
    }

    // remove its outgoing remove edges
    for edge_id in src_archetype.edges.outgoing_remove.into_iter() {
        remove_edge_from_graph(edge_id, archetypes, edges);
    }

    src_archetype
}

fn remove_edge_from_graph(
    edge_id: ArchetypeEdgeId,
    archetypes: &mut SlotMap<Archetype>,
    edges: &mut SlotMap<ArchetypeEdge>,
) -> ArchetypeEdge {
    // remove the edge from the graph
    let edge = edges.remove_unknown_version(edge_id.index()).unwrap();

    // repair the linked list it was in
    if let Some(n) = edge.next_incoming {
        let next = edges.get_unknown_version_mut(n.index()).unwrap();
        next.prev_incoming = edge.prev_incoming;
    }

    if let Some(p) = edge.prev_incoming {
        let prev = edges.get_unknown_version_mut(p.index()).unwrap();
        prev.next_incoming = edge.next_incoming;
    } else {
        // edge was head of the list
        // make destination node forget about it
        if let Some(dst) = archetypes.get_unknown_version_mut(edge.dst.index()) {
            match edge.action {
                ArchetypeEdgeAction::Insert(_) | ArchetypeEdgeAction::InsertBundle(_) => {
                    dst.edges.incoming_insert = edge.next_incoming;
                }
                ArchetypeEdgeAction::Remove(_) | ArchetypeEdgeAction::RemoveBundle(_) => {
                    dst.edges.incoming_remove = edge.next_incoming;
                }
            }
        }
    }

    // if source is not being removed, remove this edge from it
    if let Some(src) = archetypes.get_unknown_version_mut(edge.src.index()) {
        match edge.contents {
            ArchetypeEdgeContents::Component(component_id) => match edge.action {
                ArchetypeEdgeAction::Insert(_) => {
                    src.edges.outgoing_insert.remove(&edge.id);
                    src.edges.outgoing_insert_index.remove(&component_id);
                }
                ArchetypeEdgeAction::Remove(_) => {
                    src.edges.outgoing_remove.remove(&edge.id);
                    src.edges.outgoing_remove_index.remove(&component_id);
                }
                _ => unreachable!(),
            },
            ArchetypeEdgeContents::Bundle(bundle) => match edge.action {
                ArchetypeEdgeAction::InsertBundle(_) => {
                    src.edges.outgoing_insert.remove(&edge.id);
                    src.edges.outgoing_insert_index.remove(&bundle.id);
                }
                ArchetypeEdgeAction::RemoveBundle(_) => {
                    src.edges.outgoing_remove.remove(&edge.id);
                    src.edges.outgoing_remove_index.remove(&bundle.id);
                }
                _ => unreachable!(),
            },
        }
    }

    edge
}

impl Index<ArchetypeId> for Archetypes {
    type Output = Archetype;

    #[inline]
    fn index(&self, id: ArchetypeId) -> &Self::Output {
        &self.archetypes.get_unknown_version(id.index()).unwrap()
    }
}

impl IndexMut<ArchetypeId> for Archetypes {
    #[inline]
    fn index_mut(&mut self, id: ArchetypeId) -> &mut Self::Output {
        &mut self.archetypes.get_unknown_version_mut(id.index()).unwrap()
    }
}

// fn sorted_remove<T: Eq + Ord + Copy>(source: &mut Vec<T>, remove: &[T]) {
//     let mut i = 0;
//     source.retain(|&value| {
//         while i < remove.len() && value > remove[i] {
//             i += 1;
//         }

//         if i < remove.len() {
//             value != remove[i]
//         } else {
//             true
//         }
//     });
// }

// #[cfg(test)]
// mod tests {
//     #[test]
//     fn sorted_remove() {
//         let mut a = vec![1, 2, 3, 4, 5, 6, 7];
//         let b = vec![1, 2, 3, 5, 7];
//         super::sorted_remove(&mut a, &b);

//         assert_eq!(a, vec![4, 6]);

//         let mut a = vec![1];
//         let b = vec![1];
//         super::sorted_remove(&mut a, &b);

//         assert_eq!(a, vec![]);

//         let mut a = vec![1];
//         let b = vec![2];
//         super::sorted_remove(&mut a, &b);

//         assert_eq!(a, vec![1]);
//     }
// }
