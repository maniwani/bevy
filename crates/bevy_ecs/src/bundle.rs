//! Types for handling [`Bundle`]s.
//!
//! This module contains the [`Bundle`] trait and some other helper types.

pub use bevy_ecs_macros::Bundle;
use bevy_utils::{HashMap, HashSet};
use fixedbitset::FixedBitSet;

use crate::archetype::{
    move_entity_diff_archetype_diff_table, move_entity_diff_archetype_same_table, Move, ArchetypeEdgeAction,
};
use crate::query::DebugCheckedUnwrap;
use crate::{
    archetype::{Archetype, ArchetypeId, Archetypes},
    component::{Component, ComponentId, ComponentStorage, Components, StorageType, Tick},
    entity::{Entities, Entity, EntityLocation},
    removal_detection::RemovedComponentEvents,
    storage::{SparseSetIndex, SparseSets, Storages, Table, TableRow},
    TypeIdMap,
};
use bevy_ptr::OwningPtr;
use bevy_utils::all_tuples;
use std::any::TypeId;

/// The `Bundle` trait enables insertion and removal of [`Component`]s from an entity.
///
/// Implementors of the `Bundle` trait are called 'bundles'.
///
/// Each bundle represents a static set of [`Component`] types.
/// Currently, bundles can only contain one of each [`Component`], and will
/// panic once initialised if this is not met.
///
/// ## Insertion
///
/// The primary use for bundles is to add a useful collection of components to an entity.
///
/// Adding a value of bundle to an entity will add the components from the set it
/// represents to the entity.
/// The values of these components are taken from the bundle.
/// If an entity already had one of these components, the entity's original component value
/// will be overwritten.
///
/// Importantly, bundles are only their constituent set of components.
/// You **should not** use bundles as a unit of behavior.
/// The behavior of your app can only be considered in terms of components, as systems,
/// which drive the behavior of a `bevy` application, operate on combinations of
/// components.
///
/// This rule is also important because multiple bundles may contain the same component type,
/// calculated in different ways &mdash; adding both of these bundles to one entity
/// would create incoherent behavior.
/// This would be unexpected if bundles were treated as an abstraction boundary, as
/// the abstraction would be unmaintainable for these cases.
/// For example, both `Camera3dBundle` and `Camera2dBundle` contain the `CameraRenderGraph`
/// component, but specifying different render graphs to use.
/// If the bundles were both added to the same entity, only one of these two bundles would work.
///
/// For this reason, there is intentionally no [`Query`] to match whether an entity
/// contains the components of a bundle.
/// Queries should instead only select the components they logically operate on.
///
/// ## Removal
///
/// Bundles are also used when removing components from an entity.
///
/// Removing a bundle from an entity will remove any of its components attached
/// to the entity from the entity.
/// That is, if the entity does not have all the components of the bundle, those
/// which are present will be removed.
///
/// # Implementors
///
/// Every type which implements [`Component`] also implements `Bundle`, since
/// [`Component`] types can be added to or removed from an entity.
///
/// Additionally, [Tuples](`tuple`) of bundles are also [`Bundle`] (with up to 15 bundles).
/// These bundles contain the items of the 'inner' bundles.
/// This is a convenient shorthand which is primarily used when spawning entities.
/// For example, spawning an entity using the bundle `(SpriteBundle {...}, PlayerMarker)`
/// will spawn an entity with components required for a 2d sprite, and the `PlayerMarker` component.
///
/// [`unit`], otherwise known as [`()`](`unit`), is a [`Bundle`] containing no components (since it
/// can also be considered as the empty tuple).
/// This can be useful for spawning large numbers of empty entities using
/// [`World::spawn_batch`](crate::world::World::spawn_batch).
///
/// Tuple bundles can be nested, which can be used to create an anonymous bundle with more than
/// 15 items.
/// However, in most cases where this is required, the derive macro [`derive@Bundle`] should be
/// used instead.
/// The derived `Bundle` implementation contains the items of its fields, which all must
/// implement `Bundle`.
/// As explained above, this includes any [`Component`] type, and other derived bundles.
///
/// If you want to add `PhantomData` to your `Bundle` you have to mark it with `#[bundle(ignore)]`.
/// ```
/// # use std::marker::PhantomData;
/// use bevy_ecs::{component::Component, bundle::Bundle};
///
/// #[derive(Component)]
/// struct XPosition(i32);
/// #[derive(Component)]
/// struct YPosition(i32);
///
/// #[derive(Bundle)]
/// struct PositionBundle {
///     // A bundle can contain components
///     x: XPosition,
///     y: YPosition,
/// }
///
/// // You have to implement `Default` for ignored field types in bundle structs.
/// #[derive(Default)]
/// struct Other(f32);
///
/// #[derive(Bundle)]
/// struct NamedPointBundle<T: Send + Sync + 'static> {
///     // Or other bundles
///     a: PositionBundle,
///     // In addition to more components
///     z: PointName,
///
///     // when you need to use `PhantomData` you have to mark it as ignored
///     #[bundle(ignore)]
///     _phantom_data: PhantomData<T>
/// }
///
/// #[derive(Component)]
/// struct PointName(String);
/// ```
///
/// # Safety
///
/// Manual implementation of this trait is not supported.
/// That is, there is no safe way to implement this trait, and you must not do so.
/// If you want a type to implement [`Bundle`], you must use [`#[derive(Bundle)]`](derive@Bundle).
///
/// [`Query`]: crate::system::Query
// Some safety points:
// - [`Bundle::component_ids`] must return the [`ComponentId`] for each component type in the
// bundle, in the _exact_ order that [`Bundle::map_components`] is called.
// - [`Bundle::read_components`] must call `func` exactly once for each [`ComponentId`] returned by
//   [`Bundle::component_ids`].
pub unsafe trait Bundle: DynamicBundle + Send + Sync + 'static {
    /// Calls `f` with the [`ComponentId`] of each component, in the order they appear
    /// in the [`Bundle`].
    #[doc(hidden)]
    fn component_ids(
        components: &mut Components,
        storages: &mut Storages,
        f: &mut impl FnMut(ComponentId),
    );

    /// Calls `f`, which reads the data of each component in the bundle, in the order they appear
    /// in the [`Bundle`].
    ///
    /// # Safety
    ///
    /// The caller must iterate components in the order returned by [`component_ids`](Bundle::component_ids).
    #[doc(hidden)]
    unsafe fn read_components<T, F>(context: &mut T, f: &mut F) -> Self
    where
        F: for<'a> FnMut(&'a mut T) -> OwningPtr<'a>,
        Self: Sized;
}

/// The parts from [`Bundle`] that don't require statically knowing the components of the bundle.
pub trait DynamicBundle {
    /// Calls `f` on each component value, in the order they appear in the [`Bundle`].
    ///
    /// # Safety
    ///
    /// The caller must:
    /// - Iterate components in the order returned by [`component_ids`](Bundle::component_ids).
    /// - Supply the correct [`StorageType`] value for each component.
    #[doc(hidden)]
    unsafe fn map_components(self, f: &mut impl FnMut(StorageType, OwningPtr<'_>));
}

// SAFETY:
// - `Bundle::component_ids` returns the `ComponentId` of `C`.
// - `Bundle::read_components` calls `f` once the ID returned by `Bundle::component_ids`.
// - `Bundle::map_components` calls `f` once with the value returned by `Bundle::read_components`.
unsafe impl<C: Component> Bundle for C {
    fn component_ids(
        components: &mut Components,
        storages: &mut Storages,
        f: &mut impl FnMut(ComponentId),
    ) {
        f(components.init_component::<C>(storages));
    }

    unsafe fn read_components<T, F>(context: &mut T, f: &mut F) -> Self
    where
        F: for<'a> FnMut(&'a mut T) -> OwningPtr<'a>,
        Self: Sized,
    {
        // SAFETY: The id given in `component_ids` is for `Self`
        f(context).read()
    }
}

impl<C: Component> DynamicBundle for C {
    #[inline]
    unsafe fn map_components(self, f: &mut impl FnMut(StorageType, OwningPtr<'_>)) {
        OwningPtr::make(self, |ptr| f(C::Storage::STORAGE_TYPE, ptr));
    }
}

macro_rules! tuple_impl {
    ($($name: ident),*) => {
        // SAFETY:
        // - `Bundle::component_ids` returns `ComponentId` in the order the types appear in the tuple.
        // - `Bundle::read_components` calls `f` once with each ID returned by `Bundle::component_ids`.
        // - `Bundle::map_components` calls `f` once with each value returned by `Bundle::read_components`.
        unsafe impl<$($name: Bundle),*> Bundle for ($($name,)*) {
            #[allow(unused_variables)]
            fn component_ids(components: &mut Components, storages: &mut Storages, f: &mut impl FnMut(ComponentId)){
                $(<$name as Bundle>::component_ids(components, storages, f);)*
            }

            #[allow(unused_variables, unused_mut)]
            #[allow(clippy::unused_unit)]
            unsafe fn read_components<T, F>(context: &mut T, f: &mut F) -> Self
            where
                F: FnMut(&mut T) -> OwningPtr<'_>
            {
                // Rust guarantees that tuple calls are evaluated 'left to right'.
                // https://doc.rust-lang.org/reference/expressions.html#evaluation-order-of-operands
                ($(<$name as Bundle>::read_components(context, f),)*)
            }
        }

        impl<$($name: Bundle),*> DynamicBundle for ($($name,)*) {
            #[allow(unused_variables, unused_mut)]
            #[inline(always)]
            unsafe fn map_components(self, f: &mut impl FnMut(StorageType, OwningPtr<'_>)) {
                #[allow(non_snake_case)]
                let ($(mut $name,)*) = self;
                $(
                    $name.map_components(&mut *f);
                )*
            }
        }
    }
}

all_tuples!(tuple_impl, 0, 15, B);

/// An opaque, unique key representing a [`Bundle`].
///
/// **Note:** `BundleId` values are not globally unique and should only be considered
/// valid in their original world. Assume that a `BundleId` from one world will never
/// refer to the same bundle in any other world.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct BundleId(usize);

impl BundleId {
    /// Returns the index of the associated [`Bundle`] type.
    ///
    /// Note that this is unique per-world, and should not be reused across them.
    #[inline]
    pub fn index(self) -> usize {
        self.0
    }
}

impl SparseSetIndex for BundleId {
    #[inline]
    fn sparse_set_index(&self) -> usize {
        self.index()
    }

    #[inline]
    fn get_sparse_set_index(value: usize) -> Self {
        Self(value)
    }
}

/// The [`BundleId`] of the [`Bundle`] and the [`ComponentId`] of each [`Component`] in it.
#[derive(Clone)]
pub struct BundleInfo {
    pub(crate) id: BundleId,
    pub(crate) component_ids: Box<[ComponentId]>,
}

impl BundleInfo {
    /// Create a new [`BundleInfo`].
    ///
    /// # Safety
    ///
    // The IDs in `component_ids` must be valid, must have their storage initialized (i.e. the
    // table column or sparse set), and must appear in the same order as the tuple itself.
    unsafe fn new(
        bundle_type_name: &'static str,
        components: &Components,
        component_ids: Vec<ComponentId>,
        id: BundleId,
    ) -> BundleInfo {
        let mut deduped = component_ids.clone();
        deduped.sort();
        deduped.dedup();

        if deduped.len() != component_ids.len() {
            // TODO: Replace with `Vec::partition_dedup` when stabilized
            // https://github.com/rust-lang/rust/issues/54279
            let mut seen = HashSet::new();
            let mut dups = Vec::new();
            for id in component_ids {
                if !seen.insert(id) {
                    dups.push(id);
                }
            }

            let names = dups
                .into_iter()
                .map(|id| {
                    // SAFETY: the caller ensures component_id is valid.
                    unsafe { components.get_info_unchecked(id).name() }
                })
                .collect::<Vec<_>>()
                .join(", ");

            panic!("Bundle {bundle_type_name} has duplicate components: {names}");
        }

        // SAFETY: The caller ensures that component_ids:
        // - is valid for the associated world
        // - has had its storage initialized
        // - is in the same order as the source bundle type
        BundleInfo { id, component_ids: component_ids.into_boxed_slice() }
    }

    /// Returns a value identifying the associated [`Bundle`] type.
    #[inline]
    pub const fn id(&self) -> BundleId {
        self.id
    }

    /// Returns the [ID](ComponentId) of each component stored in this bundle.
    #[inline]
    pub fn components(&self) -> &[ComponentId] {
        &self.component_ids
    }

    /// Returns a [`BundleInserter`] that can add the components in the [`Bundle`] to
    /// entities in the archetype given by the [`ArchetypeId`].
    pub(crate) fn make_bundle_inserter<'bundle, 'world>(
        &'bundle self,
        entities: &'world mut Entities,
        archetypes: &'world mut Archetypes,
        components: &mut Components,
        storages: &'world mut Storages,
        archetype_id: ArchetypeId,
        change_tick: Tick,
    ) -> BundleInserter<'bundle, 'world> {
        let new_archetype_id = archetypes.insert_bundle_into_archetype(
            &self,
            archetype_id,
            components,
            storages,
            true,
        );

        let edge = archetypes.get_edge_for_insert_bundle(archetype_id, self.id).unwrap();
        let ArchetypeEdgeAction::InsertBundle(changed) = &edge.action else { unreachable!() };
        // TODO: actual safety lol
        let changed = unsafe { &*(changed as *const _) };

        let archetypes_ptr: *mut Archetypes = archetypes;
    
        let [archetype, new_archetype] = archetypes.get_many_mut([archetype_id, new_archetype_id]).unwrap();
        let table_id = archetype.table_id();
        let new_table_id = new_archetype.table_id();
        let (table, new_table) = storages.tables.get_2_mut(table_id, new_table_id);

        let move_result = if archetype_id == new_archetype_id {
            MoveResult::SameArchetype
        } else {
            if table_id == new_table_id {
                MoveResult::NewArchetypeSameTable { new_archetype }
            } else {
                MoveResult::NewArchetypeNewTable {
                    archetypes: archetypes_ptr,
                    new_archetype,
                    new_table,
                }
            }
        };

        BundleInserter {
            archetype,
            bundle_info: self,
            table,
            entities,
            sparse_sets: &mut storages.sparse_sets,
            change_tick,
            changed,
            move_result,
        }
    }

    /// Returns a [`BundleSpawner`] that can add the components in the [`Bundle`] to
    /// entities in the archetype.
    pub(crate) fn make_bundle_spawner<'bundle, 'world>(
        &'bundle self,
        entities: &'world mut Entities,
        archetypes: &'world mut Archetypes,
        components: &mut Components,
        storages: &'world mut Storages,
        change_tick: Tick,
    ) -> BundleSpawner<'bundle, 'world> {
        let new_archetype_id = archetypes.insert_bundle_into_archetype(
            &self,
            ArchetypeId::EMPTY,
            components,
            storages,
            true,
        );

        let edge = archetypes.get_edge_for_insert_bundle(ArchetypeId::EMPTY, self.id).unwrap();
        let ArchetypeEdgeAction::InsertBundle(changed) = &edge.action else { unreachable!() };
        // TODO: actual safety lol
        let changed = unsafe { &*(changed as *const _) };

        let archetype = archetypes.get_mut(new_archetype_id).unwrap();
        let table = storages.tables.get_mut(archetype.table_id()).unwrap();

        BundleSpawner {
            archetype,
            bundle_info: self,
            table,
            entities,
            sparse_sets: &mut storages.sparse_sets,
            change_tick,
            changed,
        }
    }
}

pub(crate) struct BundleInserter<'bundle, 'world> {
    bundle_info: &'bundle BundleInfo,
    pub(crate) entities: &'world mut Entities,
    pub(crate) archetype: &'world mut Archetype,
    table: &'world mut Table,
    sparse_sets: &'world mut SparseSets,
    change_tick: Tick,
    changed: &'world FixedBitSet,
    move_result: MoveResult<'world>,
}

pub(crate) struct BundleSpawner<'bundle, 'world> {
    bundle_info: &'bundle BundleInfo,
    pub(crate) entities: &'world mut Entities,
    pub(crate) archetype: &'world mut Archetype,
    table: &'world mut Table,
    sparse_sets: &'world mut SparseSets,
    change_tick: Tick,
    changed: &'world FixedBitSet,
}

pub(crate) enum MoveResult<'a> {
    SameArchetype,
    NewArchetypeSameTable {
        new_archetype: &'a mut Archetype,
    },
    NewArchetypeNewTable {
        archetypes: *mut Archetypes,
        new_archetype: &'a mut Archetype,
        new_table: &'a mut Table,
    },
}

impl BundleInserter<'_, '_> {
    /// # Safety
    /// - `entity` must currently exist in the source archetype for this inserter.
    /// - `archetype_row` must be `entity`'s location in the archetype.
    /// - `T` must match this [`BundleInfo`]'s type
    #[inline]
    pub unsafe fn insert<T: DynamicBundle>(
        &mut self,
        entity: Entity,
        location: EntityLocation,
        bundle: T,
    ) -> EntityLocation {
        // move entity
        let (new_location, new_table) = match self.move_result {
            MoveResult::SameArchetype => (location, self.table),
            MoveResult::NewArchetypeSameTable { new_archetype } => {
                let new_location = move_entity_diff_archetype_same_table(
                    entity,
                    location,
                    self.entities,
                    self.archetype,
                    new_archetype,
                );

                (new_location, self.table)
            }
            MoveResult::NewArchetypeNewTable {
                archetypes,
                new_archetype,
                new_table,
            } => {
                let new_location = move_entity_diff_archetype_diff_table(
                    Move::Insert,
                    entity,
                    location,
                    self.entities,
                    archetypes,
                    self.archetype,
                    new_archetype,
                    self.table,
                    new_table,
                );

                (new_location, new_table)
            }
        };

        // write data
        write_components(
            entity,
            bundle,
            &self.bundle_info,
            self.changed,
            self.change_tick,
            new_table,
            new_location.table_row,
            self.sparse_sets,
        );

        new_location
    }
}

impl BundleSpawner<'_, '_> {
    pub fn reserve_storage(&mut self, additional: usize) {
        self.archetype.reserve(additional);
        self.table.reserve(additional);
    }

    /// # Safety
    /// - `entity` must be allocated (with no storage yet)
    /// - `T` must be the [`Bundle`] type from the [`BundleInfo`]
    #[inline]
    pub unsafe fn spawn_non_existent<T: DynamicBundle>(
        &mut self,
        entity: Entity,
        bundle: T,
    ) -> EntityLocation {
        let table_row = self.table.allocate(entity);
        let location = self.archetype.allocate(entity, table_row);
        write_components(
            entity,
            bundle,
            self.bundle_info,
            self.changed,
            self.change_tick,
            self.table,
            table_row,
            self.sparse_sets,
        );
        self.entities.set(entity.index(), location);

        location
    }

    /// # Safety
    /// - `T` must be the [`Bundle`] type from the [`BundleInfo`]
    #[inline]
    pub unsafe fn spawn<T: Bundle>(&mut self, bundle: T) -> Entity {
        let entity = self.entities.alloc();
        // SAFETY: `entity` is allocated (with no storage yet), `BundleInfo` is for `T`
        self.spawn_non_existent(entity, bundle);
        entity
    }
}

/// Writes components values from a [`Bundle`] to an entity.
///
/// # Safety
///
/// `bundle_component_status` must return the "correct" [`ComponentStatus`] for each component
/// in the [`Bundle`], with respect to the entity's original archetype (prior to the bundle being added)
/// For example, if the original archetype already has `ComponentA` and `T` also has `ComponentA`, the status
/// should be `Mutated`. If the original archetype does not have `ComponentA`, the status should be `Added`.
/// When "inserting" a bundle into an existing entity, [`AddBundle`](crate::archetype::AddBundle)
/// should be used, which will report `Added` vs `Mutated` status based on the current archetype's structure.
///
/// When spawning a bundle, [`SpawnBundleStatus`] can be used instead, which removes the need
/// to look up the [`AddBundle`](crate::archetype::AddBundle) in the archetype graph, which requires
/// ownership of the entity's current archetype.
///
/// - `table` must be the "new" table for `entity`.
/// - `table_row` must have space allocated for the `entity`
/// - `bundle` must match this [`BundleInfo`]'s type
#[inline]
#[allow(clippy::too_many_arguments)]
pub(crate) unsafe fn write_components<T: DynamicBundle>(
    entity: Entity,
    bundle: T,
    info: &BundleInfo,
    changed: &FixedBitSet,
    change_tick: Tick,
    table: &mut Table,
    table_row: TableRow,
    sparse_sets: &mut SparseSets,
) {
    // NOTE: `self.component_ids` matches the order returned by `T::component_ids`
    let mut bundle_component_index = 0;
    bundle.map_components(&mut |component_storage_type, component_ptr| {
        // SAFETY: `bundle_component_index` is always within bounds
        let component_id = *info.component_ids.get_unchecked(bundle_component_index);
        match component_storage_type {
            StorageType::Table => {
                let column = table.get_column_mut(component_id).debug_checked_unwrap();
                // SAFETY: bundle_component is a valid index for this bundle
                if changed[bundle_component_index] {
                    column.replace(table_row, component_ptr, change_tick);
                } else {
                    column.initialize(table_row, component_ptr, change_tick);
                }
            }
            StorageType::SparseSet => {
                sparse_sets
                    .get_mut(component_id)
                    .debug_checked_unwrap()
                    .insert(entity, component_ptr, change_tick);
            }
        }

        bundle_component_index += 1;
    });
}

/// Takes ownership of stored component data.
///
/// This function leaves the memory unchanged, but the component behind the returned pointer is
/// now owned by the caller, who assumes responsibility for dropping it. The caller must copy the
/// data to a new location and then ensure the original location is forgotten.
///
///  # Safety
///
/// - All arguments must come from the same [`World`].
/// - `component_id` must be valid.
/// - `location.table_row` must be a valid index into the relevant table column.
/// - The relevant table row **must be removed** by the caller once all components are taken, without dropping the values.
#[inline]
pub(crate) unsafe fn take_component<'a>(
    entity: Entity,
    location: EntityLocation,
    component_id: ComponentId,
    components: &Components,
    storages: &'a mut Storages,
    removed_components: &mut RemovedComponentEvents,
) -> OwningPtr<'a> {
    // SAFETY: component_id is valid (as promised by caller)
    let component_info = components.get_info_unchecked(component_id);
    removed_components.send(component_id, entity);
    match component_info.storage_type() {
        StorageType::Table => {
            let table = &mut storages.tables[location.table_id];
            // TODO: dense table column index
            let components = table.get_column_mut(component_id).debug_checked_unwrap();
            // SAFETY:
            // - archetypes never have invalid table rows
            // - index is in bounds (as promised by caller)
            // - promote is safe because the caller promises to remove the table row without
            // dropping its values immediately after this
            components
                .get_data_unchecked_mut(location.table_row)
                .promote()
        }
        StorageType::SparseSet => storages
            .sparse_sets
            .get_mut(component_id)
            .debug_checked_unwrap()
            .remove_and_forget(entity)
            .debug_checked_unwrap(),
    }
}

/// Stores metadata for every known [`Bundle`].
#[derive(Default)]
pub struct Bundles {
    bundle_infos: Vec<BundleInfo>,
    /// Cache static [`BundleId`]
    bundle_ids: TypeIdMap<BundleId>,
    /// Cache dynamic [`BundleId`] with multiple components
    dynamic_bundle_ids: HashMap<Box<[ComponentId]>, (BundleId, Box<[StorageType]>)>,
    /// Cache optimized dynamic [`BundleId`] with single component
    dynamic_component_bundle_ids: HashMap<ComponentId, (BundleId, StorageType)>,
}

impl Bundles {
    /// Gets the metadata associated with a specific type of bundle.
    /// Returns `None` if the bundle is not registered with the world.
    #[inline]
    pub fn get(&self, bundle_id: BundleId) -> Option<&BundleInfo> {
        self.bundle_infos.get(bundle_id.index())
    }

    /// Gets the value identifying a specific type of bundle.
    /// Returns `None` if the bundle does not exist in the world,
    /// or if `type_id` does not correspond to a type of bundle.
    #[inline]
    pub fn get_id(&self, type_id: TypeId) -> Option<BundleId> {
        self.bundle_ids.get(&type_id).cloned()
    }

    /// Initializes a new [`BundleInfo`] for a statically known type.
    pub(crate) fn init_info<'a, T: Bundle>(
        &'a mut self,
        components: &mut Components,
        storages: &mut Storages,
    ) -> &'a BundleInfo {
        let id = self.bundle_ids.entry(TypeId::of::<T>()).or_insert_with(|| {
            let mut component_ids = Vec::new();
            T::component_ids(components, storages, &mut |id| component_ids.push(id));

            let id = BundleId(self.bundle_infos.len());
            let bundle_info =
                // SAFETY: `T::component_ids` ensures:
                // - the `ComponentInfo` of each component in the bundle exists
                // - the storage for each component in the bundle exists
                // - the components appear in the same order as they appear in `T`
                unsafe { 
                    BundleInfo::new(
                        std::any::type_name::<T>(),
                        components,
                        component_ids,
                        id
                    )
                };

            self.bundle_infos.push(bundle_info);

            id
        });

        // SAFETY: bundle info must exist (it already existed or we just created it)
        unsafe { self.bundle_infos.get_unchecked(id.0) }
    }

    /// Initializes a new [`BundleInfo`] for a dynamic [`Bundle`].
    ///
    /// # Panics
    ///
    /// Panics if any of the provided [`ComponentId`]s do not exist in the
    /// provided [`Components`].
    pub(crate) fn init_dynamic_info(
        &mut self,
        components: &mut Components,
        component_ids: &[ComponentId],
    ) -> (&BundleInfo, &[StorageType]) {
        let bundle_infos = &mut self.bundle_infos;
        // Use `raw_entry_mut` to avoid cloning `component_ids` to access `Entry`
        let (_, (bundle_id, storage_types)) = self
            .dynamic_bundle_ids
            .raw_entry_mut()
            .from_key(component_ids)
            .or_insert_with(|| {
                (
                    Vec::from(component_ids).into_boxed_slice(),
                    initialize_dynamic_bundle(
                        bundle_infos,
                        components,
                        Vec::from(component_ids),
                    ),
                )
            });

        // SAFETY: index either exists, or was initialized
        let bundle_info = unsafe { bundle_infos.get_unchecked(bundle_id.0) };
        (bundle_info, storage_types)
    }

    /// Initializes a new [`BundleInfo`] for a dynamic [`Bundle`] with only one component.
    ///
    /// # Panics
    ///
    /// Panics if the provided [`ComponentId`] does not exist in the provided [`Components`].
    pub(crate) fn init_component_info(
        &mut self,
        components: &mut Components,
        component_id: ComponentId,
    ) -> (&BundleInfo, StorageType) {
        let (bundle_id, storage_types) = self
            .dynamic_component_bundle_ids
            .entry(component_id)
            .or_insert_with(|| {
                let (id, storage_type) =
                    initialize_dynamic_bundle(&mut self.bundle_infos, components, vec![component_id]);
                // SAFETY: `storage_type` guaranteed to have length 1
                (id, storage_type[0])
            });

        // SAFETY: index either exists, or was initialized
        let bundle_info = unsafe { self.bundle_infos.get_unchecked(bundle_id.0) };

        (bundle_info, *storage_types)
    }
}

/// Asserts that all components are part of [`Components`]
/// and initializes a [`BundleInfo`].
fn initialize_dynamic_bundle(
    bundle_infos: &mut Vec<BundleInfo>,
    components: &Components,
    component_ids: Vec<ComponentId>,
) -> (BundleId, Box<[StorageType]>) {
    // Assert component existence
    let storage_types = component_ids.iter().map(|&id| {
        components.get_info(id).unwrap_or_else(|| {
            panic!(
                "`init_dynamic_info` called with component id {id:?} which doesn't exist in this world"
            )
        }).storage_type()
    }).collect::<Vec<_>>().into_boxed_slice();

    let id = BundleId(bundle_infos.len());
    let bundle_info =
        // SAFETY: `component_ids` are valid as they were just checked
        unsafe { BundleInfo::new("<dynamic bundle>", components, component_ids, id) };
    bundle_infos.push(bundle_info);

    (id, storage_types)
}
