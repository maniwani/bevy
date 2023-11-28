use crate::{
    archetype::Archetype,
    bundle::Bundle,
    change_detection::MutUntyped,
    component::{Component, ComponentId, ComponentTicks},
    entity::{Entity, EntityLocation},
    world::{Mut, World},
};
use bevy_ptr::{OwningPtr, Ptr};
use std::any::TypeId;

use super::{unsafe_world_cell::UnsafeEntityCell, Ref};

/// A reference to an entity and its components.
#[derive(Copy, Clone)]
pub struct EntityRef<'w> {
    world: &'w World,
    entity: Entity,
    location: EntityLocation,
}

impl<'w> EntityRef<'w> {
    /// # Safety
    ///
    ///  - `entity` must be valid for `world`: the generation should match that of the entity at the same index.
    ///  - `location` must be sourced from `world`'s `Entities` and must exactly match the location for `entity`
    ///
    ///  The above is trivially satisfied if `location` was sourced from `world.entities().get(entity)`.
    #[inline]
    pub(crate) unsafe fn new(world: &'w World, entity: Entity, location: EntityLocation) -> Self {
        debug_assert!(world.entities().contains(entity));
        debug_assert_eq!(world.entities().get(entity), Some(location));

        Self {
            world,
            entity,
            location,
        }
    }

    fn as_unsafe_world_cell_readonly(&self) -> UnsafeEntityCell<'w> {
        UnsafeEntityCell::new(
            self.world.as_unsafe_world_cell_readonly(),
            self.entity,
            self.location,
        )
    }

    /// Returns the [ID](Entity) of the current entity.
    #[inline]
    #[must_use = "Omit the .id() call if you do not need to store the `Entity` identifier."]
    pub fn id(&self) -> Entity {
        self.entity
    }

    /// Gets metadata indicating the location where the current entity is stored.
    #[inline]
    pub fn location(&self) -> EntityLocation {
        self.location
    }

    /// Returns the archetype that the current entity belongs to.
    #[inline]
    pub fn archetype(&self) -> &Archetype {
        &self.world.archetypes[self.location.archetype_id]
    }

    /// Gets read-only access to the world that the current entity belongs to.
    #[inline]
    pub fn world(&self) -> &'w World {
        self.world
    }

    /// Returns `true` if the entity has this component.
    #[inline]
    pub fn contains<T: Component>(&self) -> bool {
        self.contains_type_id(TypeId::of::<T>())
    }

    /// Returns `true` if the entity has this component.
    #[inline]
    pub fn contains_id(&self, component_id: ComponentId) -> bool {
        self.as_unsafe_world_cell_readonly()
            .contains_id(component_id)
    }

    /// Returns `true` if the entity has this component.
    #[inline]
    pub fn contains_type_id(&self, type_id: TypeId) -> bool {
        self.as_unsafe_world_cell_readonly()
            .contains_type_id(type_id)
    }

    /// Returns a reference to the entity's component, if it exists.
    #[inline]
    pub fn get<T: Component>(&self) -> Option<&'w T> {
        // SAFETY: &self implies shared access for duration of returned value
        unsafe { self.as_unsafe_world_cell_readonly().get::<T>() }
    }

    /// Gets access to the component of type `T` for the current entity,
    /// including change detection information as a [`Ref`].
    ///
    /// Returns `None` if the entity does not have a component of type `T`.
    #[inline]
    pub fn get_ref<T: Component>(&self) -> Option<Ref<'w, T>> {
        // SAFETY: &self implies shared access for duration of returned value
        unsafe { self.as_unsafe_world_cell_readonly().get_ref::<T>() }
    }

    /// Retrieves the change ticks for the given component. This can be useful for implementing change
    /// detection in custom runtimes.
    #[inline]
    pub fn get_change_ticks<T: Component>(&self) -> Option<ComponentTicks> {
        // SAFETY: &self implies shared access
        unsafe { self.as_unsafe_world_cell_readonly().get_change_ticks::<T>() }
    }

    /// Retrieves the change ticks for the given [`ComponentId`]. This can be useful for implementing change
    /// detection in custom runtimes.
    ///
    /// **You should prefer to use the typed API [`EntityRef::get_change_ticks`] where possible and only
    /// use this in cases where the actual component types are not known at
    /// compile time.**
    #[inline]
    pub fn get_change_ticks_by_id(&self, component_id: ComponentId) -> Option<ComponentTicks> {
        // SAFETY: &self implies shared access
        unsafe {
            self.as_unsafe_world_cell_readonly()
                .get_change_ticks_by_id(component_id)
        }
    }
}

impl<'w> EntityRef<'w> {
    /// Gets the component of the given [`ComponentId`] from the entity.
    ///
    /// **You should prefer to use the typed API where possible and only
    /// use this in cases where the actual component types are not known at
    /// compile time.**
    ///
    /// Unlike [`EntityRef::get`], this returns a raw pointer to the component,
    /// which is only valid while the `'w` borrow of the lifetime is active.
    #[inline]
    pub fn get_by_id(&self, component_id: ComponentId) -> Option<Ptr<'w>> {
        // SAFETY: &self implies shared access for duration of returned value
        unsafe { self.as_unsafe_world_cell_readonly().get_by_id(component_id) }
    }
}

impl<'w> From<EntityMut<'w>> for EntityRef<'w> {
    fn from(entity_mut: EntityMut<'w>) -> EntityRef<'w> {
        // SAFETY: the safety invariants on EntityMut and EntityRef are identical
        // and EntityMut is promised to be valid by construction.
        unsafe { EntityRef::new(entity_mut.world, entity_mut.entity, entity_mut.location) }
    }
}

/// A mutable reference to an entity and its components.
pub struct EntityMut<'w> {
    world: &'w mut World,
    entity: Entity,
    location: EntityLocation,
}

impl<'w> EntityMut<'w> {
    fn as_unsafe_world_cell_readonly(&self) -> UnsafeEntityCell<'_> {
        UnsafeEntityCell::new(
            self.world.as_unsafe_world_cell_readonly(),
            self.entity,
            self.location,
        )
    }
    fn as_unsafe_world_cell(&mut self) -> UnsafeEntityCell<'_> {
        UnsafeEntityCell::new(
            self.world.as_unsafe_world_cell(),
            self.entity,
            self.location,
        )
    }

    /// # Safety
    ///
    /// - The `entity` is alive.
    /// - The `location` belongs to the `entity`.
    #[inline]
    pub(crate) unsafe fn new(
        world: &'w mut World,
        entity: Entity,
        location: EntityLocation,
    ) -> Self {
        debug_assert!(world.entities().contains(entity));
        debug_assert_eq!(world.entities().get(entity), Some(location));

        EntityMut {
            world,
            entity,
            location,
        }
    }

    /// Returns this entity's unique ID.
    #[inline]
    #[must_use = "Omit the .id() call if you do not need to store the `Entity` identifier."]
    pub fn id(&self) -> Entity {
        self.entity
    }

    /// Gets metadata indicating the location where the current entity is stored.
    #[inline]
    pub fn location(&self) -> EntityLocation {
        self.location
    }

    /// Returns the archetype that the current entity belongs to.
    #[inline]
    pub fn archetype(&self) -> &Archetype {
        &self.world.archetypes[self.location.archetype_id]
    }

    /// Returns `true` if the entity has this component.
    #[inline]
    pub fn contains<T: Component>(&self) -> bool {
        self.contains_type_id(TypeId::of::<T>())
    }

    /// Returns `true` if the entity has this component.
    #[inline]
    pub fn contains_id(&self, component_id: ComponentId) -> bool {
        self.as_unsafe_world_cell_readonly()
            .contains_id(component_id)
    }

    /// Returns `true` if the entity has this component.
    #[inline]
    pub fn contains_type_id(&self, type_id: TypeId) -> bool {
        self.as_unsafe_world_cell_readonly()
            .contains_type_id(type_id)
    }

    /// Returns a reference to this the entity has this component.
    #[inline]
    pub fn get<T: Component>(&self) -> Option<&'_ T> {
        // SAFETY: &self implies shared access for duration of returned value
        unsafe { self.as_unsafe_world_cell_readonly().get::<T>() }
    }

    /// Returns a mutable reference to the entity's component, if it exists.
    #[inline]
    pub fn get_mut<T: Component>(&mut self) -> Option<Mut<'_, T>> {
        // SAFETY: &mut self implies exclusive access for duration of returned value
        unsafe { self.as_unsafe_world_cell().get_mut() }
    }

    /// Returns a copy of this component's change ticks.
    #[inline]
    pub fn get_change_ticks<T: Component>(&self) -> Option<ComponentTicks> {
        // SAFETY: &self implies shared access
        unsafe { self.as_unsafe_world_cell_readonly().get_change_ticks::<T>() }
    }

    /// Returns a copy of this component's change ticks.
    ///
    /// Use the strongly-typed [`get_change_ticks`] when possible and only use this
    /// when compile-time type information is not available.
    #[inline]
    pub fn get_change_ticks_by_id(&self, component_id: ComponentId) -> Option<ComponentTicks> {
        // SAFETY: &self implies shared access
        unsafe {
            self.as_unsafe_world_cell_readonly()
                .get_change_ticks_by_id(component_id)
        }
    }

    /// Adds a [`Bundle`] of components to the entity.
    ///
    /// Any components the entity already has will be overwritten with the new values.
    pub fn insert<T: Bundle>(&mut self, bundle: T) -> &mut Self {
        // SAFETY: The entity is alive and we're passing in its actual location.
        self.location = unsafe {
            self.world
                .insert_internal::<T>(self.entity, self.location, bundle)
        };
        self
    }

    /// Adds a dynamic [`Component`] to the entity.
    ///
    /// If the entity already has the component, it will be overwritten with the new value.
    ///
    /// # Safety
    ///
    /// - [`ComponentId`] must be from the same world as [`EntityMut`]
    /// - [`OwningPtr`] must be a valid reference to the type represented by [`ComponentId`]
    pub unsafe fn insert_by_id(
        &mut self,
        component_id: ComponentId,
        component: OwningPtr<'_>,
    ) -> &mut Self {
        todo!()
    }

    /// Adds a dynamic [`Bundle`] of components into the entity.
    ///
    /// Any components the entity already has will be overwritten with the new values.
    ///
    /// # Safety
    /// - Each [`ComponentId`] must be from the same world as [`EntityMut`]
    /// - Each [`OwningPtr`] must be a valid reference to the type represented by [`ComponentId`]
    pub unsafe fn insert_by_ids<'a, I: Iterator<Item = OwningPtr<'a>>>(
        &mut self,
        component_ids: &[ComponentId],
        iter_components: I,
    ) -> &mut Self {
        todo!()
    }

    /// Removes any components in the [`Bundle`] from the entity.
    pub fn remove<T: Bundle>(&mut self) -> &mut Self {
        // SAFETY: The entity is alive and we're passing in its actual location.
        self.location = unsafe { self.world.remove_internal::<T>(self.entity, self.location) };
        self
    }

    /// Removes all components in the [`Bundle`] from the entity and returns their previous values.
    ///
    /// **Note:** This will not remove any components and will return `None` if the entity does not
    /// have all components in the bundle.
    pub fn take<T: Bundle>(&mut self) -> Option<T> {
        let (location, result) =
            // SAFETY: The entity is alive and we're passing in its actual location.
            unsafe { self.world.take_internal::<T>(self.entity, self.location) };
        self.location = location;

        result
    }

    /// Permanently delete the entity and all of its component data.
    pub fn despawn(self) {
        self.world.flush();
        // SAFETY: The entity is alive and we're passing in its actual location.
        unsafe {
            self.world.despawn_internal(self.entity, self.location);
        }
    }

    /// Returns a reference to the [`World`] that owns this entity.
    #[inline]
    pub fn world(&self) -> &World {
        self.world
    }

    /// Returns the underlying mutable reference to the [`World`].
    ///
    /// This function is unsafe because ...
    /// See [`world_scope`] or [`into_world_mut`] for safe alternatives.
    ///
    /// # Safety
    /// Caller must not modify the world in a way that changes the current entity's location
    /// If the caller _does_ do something that could change the location, `self.update_location()`
    /// must be called before using any other methods on this [`EntityMut`].
    #[inline]
    pub unsafe fn world_mut(&mut self) -> &mut World {
        self.world
    }

    /// Consumes the `EntityMut` and returns the underlying exclusive [`World`] reference.
    #[inline]
    pub fn into_world_mut(self) -> &'w mut World {
        self.world
    }

    /// Gives mutable access to this `EntityMut`'s [`World`] in a temporary scope.
    /// This is a safe alternative to using [`Self::world_mut`].
    ///
    /// # Examples
    ///
    /// ```
    /// # use bevy_ecs::prelude::*;
    /// #[derive(Resource, Default, Clone, Copy)]
    /// struct R(u32);
    ///
    /// # let mut world = World::new();
    /// # world.init_resource::<R>();
    /// # let mut entity = world.spawn_empty();
    /// // This closure gives us temporary access to the world.
    /// let new_r = entity.world_scope(|world: &mut World| {
    ///     // Mutate the world while we have access to it.
    ///     let mut r = world.resource_mut::<R>();
    ///     r.0 += 1;
    ///
    ///     // Return a value from the world before giving it back to the `EntityMut`.
    ///     *r
    /// });
    /// # assert_eq!(new_r.0, 1);
    /// ```
    pub fn world_scope<U>(&mut self, f: impl FnOnce(&mut World) -> U) -> U {
        struct Guard<'w, 'a> {
            entity_mut: &'a mut EntityMut<'w>,
        }

        impl Drop for Guard<'_, '_> {
            #[inline]
            fn drop(&mut self) {
                self.entity_mut.update_location();
            }
        }

        // When `guard` is dropped at the end of this scope,
        // it will update the cached `EntityLocation` for this instance.
        // This will run even in case the closure `f` unwinds.
        let guard = Guard { entity_mut: self };
        f(guard.entity_mut.world)
    }

    /// Updates the internal entity location to match the current location in the internal
    /// [`World`].
    ///
    /// This is *only* required when using the unsafe function [`EntityMut::world_mut`],
    /// which enables the location to change.
    pub fn update_location(&mut self) {
        self.location = self.world.entities().get(self.entity).unwrap();
    }
}

impl<'w> EntityMut<'w> {
    /// Gets the component of the given [`ComponentId`] from the entity.
    ///
    /// **You should prefer to use the typed API [`EntityMut::get`] where possible and only
    /// use this in cases where the actual component types are not known at
    /// compile time.**
    ///
    /// Unlike [`EntityMut::get`], this returns a raw pointer to the component,
    /// which is only valid while the [`EntityMut`] is alive.
    #[inline]
    pub fn get_by_id(&self, component_id: ComponentId) -> Option<Ptr<'_>> {
        // SAFETY:
        // - `&self` ensures that no mutable references exist to this entity's components.
        // - `as_unsafe_world_cell_readonly` gives read only permission for all components on this entity
        unsafe { self.as_unsafe_world_cell_readonly().get_by_id(component_id) }
    }

    /// Gets a [`MutUntyped`] of the component of the given [`ComponentId`] from the entity.
    ///
    /// **You should prefer to use the typed API [`EntityMut::get_mut`] where possible and only
    /// use this in cases where the actual component types are not known at
    /// compile time.**
    ///
    /// Unlike [`EntityMut::get_mut`], this returns a raw pointer to the component,
    /// which is only valid while the [`EntityMut`] is alive.
    #[inline]
    pub fn get_mut_by_id(&mut self, component_id: ComponentId) -> Option<MutUntyped<'_>> {
        // SAFETY:
        // - `&mut self` ensures that no references exist to this entity's components.
        // - `as_unsafe_world_cell` gives mutable permission for all components on this entity
        unsafe { self.as_unsafe_world_cell().get_mut_by_id(component_id) }
    }
}

#[cfg(test)]
mod tests {
    use bevy_ptr::OwningPtr;
    use std::panic::AssertUnwindSafe;

    use crate as bevy_ecs;
    use crate::component::ComponentId;
    use crate::prelude::*; // for the `#[derive(Component)]`

    #[derive(Component, Clone, Copy, Debug, PartialEq)]
    struct TableComponent(u32);

    #[derive(Component, Clone, Copy, Debug, PartialEq)]
    #[component(storage = "SparseSet")]
    struct SparseComponent(u32);

    #[test]
    fn entity_ref_get_by_id() {
        let mut world = World::new();
        let entity = world.spawn(TableComponent(42)).id();
        let component_id = world
            .components()
            .get_id(std::any::TypeId::of::<TableComponent>())
            .unwrap();

        let entity = world.entity(entity);
        let table_component = entity.get_by_id(component_id).unwrap();
        // SAFETY: points to a valid `TableComponent`
        let table_component = unsafe { table_component.deref::<TableComponent>() };

        assert_eq!(table_component.0, 42);
    }

    #[test]
    fn entity_mut_get_by_id() {
        let mut world = World::new();
        let entity = world.spawn(TableComponent(42)).id();
        let component_id = world
            .components()
            .get_id(std::any::TypeId::of::<TableComponent>())
            .unwrap();

        let mut entity_mut = world.entity_mut(entity);
        let mut table_component = entity_mut.get_mut_by_id(component_id).unwrap();
        {
            table_component.set_changed();
            let table_component =
                // SAFETY: `table_component` has unique access of the `EntityMut` and is not used afterwards
                unsafe { table_component.into_inner().deref_mut::<TableComponent>() };
            table_component.0 = 43;
        }

        let entity = world.entity(entity);
        let table_component = entity.get_by_id(component_id).unwrap();
        // SAFETY: `TableComponent` is the correct component type
        let table_component = unsafe { table_component.deref::<TableComponent>() };

        assert_eq!(table_component.0, 43);
    }

    #[test]
    fn entity_ref_get_by_id_invalid_component_id() {
        let invalid_component_id = ComponentId::new(usize::MAX);

        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let entity = world.entity(entity);
        assert!(entity.get_by_id(invalid_component_id).is_none());
    }

    #[test]
    fn entity_mut_get_by_id_invalid_component_id() {
        let invalid_component_id = ComponentId::new(usize::MAX);

        let mut world = World::new();
        let mut entity = world.spawn_empty();
        assert!(entity.get_by_id(invalid_component_id).is_none());
        assert!(entity.get_mut_by_id(invalid_component_id).is_none());
    }

    // regression test for https://github.com/bevyengine/bevy/pull/7387
    #[test]
    fn entity_mut_world_scope_panic() {
        let mut world = World::new();

        let mut entity = world.spawn_empty();
        let old_location = entity.location();
        let id = entity.id();
        let res = std::panic::catch_unwind(AssertUnwindSafe(|| {
            entity.world_scope(|w| {
                // Change the entity's `EntityLocation`, which invalidates the original `EntityMut`.
                // This will get updated at the end of the scope.
                w.entity_mut(id).insert(TableComponent(0));

                // Ensure that the entity location still gets updated even in case of a panic.
                panic!("this should get caught by the outer scope")
            });
        }));
        assert!(res.is_err());

        // Ensure that the location has been properly updated.
        assert!(entity.location() != old_location);
    }

    // regression test for https://github.com/bevyengine/bevy/pull/7805
    #[test]
    fn removing_sparse_updates_archetype_row() {
        #[derive(Component, PartialEq, Debug)]
        struct Dense(u8);

        #[derive(Component)]
        #[component(storage = "SparseSet")]
        struct Sparse;

        let mut world = World::new();
        let e1 = world.spawn((Dense(0), Sparse)).id();
        let e2 = world.spawn((Dense(1), Sparse)).id();

        world.entity_mut(e1).remove::<Sparse>();
        assert_eq!(world.entity(e2).get::<Dense>().unwrap(), &Dense(1));
    }

    // regression test for https://github.com/bevyengine/bevy/pull/7805
    #[test]
    fn removing_dense_updates_table_row() {
        #[derive(Component, PartialEq, Debug)]
        struct Dense(u8);

        #[derive(Component)]
        #[component(storage = "SparseSet")]
        struct Sparse;

        let mut world = World::new();
        let e1 = world.spawn((Dense(0), Sparse)).id();
        let e2 = world.spawn((Dense(1), Sparse)).id();

        world.entity_mut(e1).remove::<Dense>();
        assert_eq!(world.entity(e2).get::<Dense>().unwrap(), &Dense(1));
    }

    // regression test for https://github.com/bevyengine/bevy/pull/7805
    #[test]
    fn inserting_sparse_updates_archetype_row() {
        #[derive(Component, PartialEq, Debug)]
        struct Dense(u8);

        #[derive(Component)]
        #[component(storage = "SparseSet")]
        struct Sparse;

        let mut world = World::new();
        let e1 = world.spawn(Dense(0)).id();
        let e2 = world.spawn(Dense(1)).id();

        world.entity_mut(e1).insert(Sparse);
        assert_eq!(world.entity(e2).get::<Dense>().unwrap(), &Dense(1));
    }

    // regression test for https://github.com/bevyengine/bevy/pull/7805
    #[test]
    fn inserting_dense_updates_archetype_row() {
        #[derive(Component, PartialEq, Debug)]
        struct Dense(u8);

        #[derive(Component)]
        struct Dense2;

        #[derive(Component)]
        #[component(storage = "SparseSet")]
        struct Sparse;

        let mut world = World::new();
        let e1 = world.spawn(Dense(0)).id();
        let e2 = world.spawn(Dense(1)).id();

        world.entity_mut(e1).insert(Sparse).remove::<Sparse>();

        // archetype with [e2, e1]
        // table with [e1, e2]

        world.entity_mut(e2).insert(Dense2);

        assert_eq!(world.entity(e1).get::<Dense>().unwrap(), &Dense(0));
    }

    #[test]
    fn inserting_dense_updates_table_row() {
        #[derive(Component, PartialEq, Debug)]
        struct Dense(u8);

        #[derive(Component)]
        struct Dense2;

        #[derive(Component)]
        #[component(storage = "SparseSet")]
        struct Sparse;

        let mut world = World::new();
        let e1 = world.spawn(Dense(0)).id();
        let e2 = world.spawn(Dense(1)).id();

        world.entity_mut(e1).insert(Sparse).remove::<Sparse>();

        // archetype with [e2, e1]
        // table with [e1, e2]

        world.entity_mut(e1).insert(Dense2);

        assert_eq!(world.entity(e2).get::<Dense>().unwrap(), &Dense(1));
    }

    // regression test for https://github.com/bevyengine/bevy/pull/7805
    #[test]
    fn despawning_entity_updates_archetype_row() {
        #[derive(Component, PartialEq, Debug)]
        struct Dense(u8);

        #[derive(Component)]
        #[component(storage = "SparseSet")]
        struct Sparse;

        let mut world = World::new();
        let e1 = world.spawn(Dense(0)).id();
        let e2 = world.spawn(Dense(1)).id();

        world.entity_mut(e1).insert(Sparse).remove::<Sparse>();

        // archetype with [e2, e1]
        // table with [e1, e2]

        world.entity_mut(e2).despawn();

        assert_eq!(world.entity(e1).get::<Dense>().unwrap(), &Dense(0));
    }

    // regression test for https://github.com/bevyengine/bevy/pull/7805
    #[test]
    fn despawning_entity_updates_table_row() {
        #[derive(Component, PartialEq, Debug)]
        struct Dense(u8);

        #[derive(Component)]
        #[component(storage = "SparseSet")]
        struct Sparse;

        let mut world = World::new();
        let e1 = world.spawn(Dense(0)).id();
        let e2 = world.spawn(Dense(1)).id();

        world.entity_mut(e1).insert(Sparse).remove::<Sparse>();

        // archetype with [e2, e1]
        // table with [e1, e2]

        world.entity_mut(e1).despawn();

        assert_eq!(world.entity(e2).get::<Dense>().unwrap(), &Dense(1));
    }

    #[test]
    fn entity_mut_insert_by_id() {
        let mut world = World::new();
        let table_component_id = world.init_component::<TableComponent>();

        let mut entity = world.spawn_empty();
        OwningPtr::make(TableComponent(42), |ptr| {
            // SAFETY: `ptr` matches the component id
            unsafe { entity.insert_by_id(table_component_id, ptr) };
        });

        let components: Vec<_> = world.query::<&TableComponent>().iter(&world).collect();

        assert_eq!(components, vec![&TableComponent(42)]);

        // Compare with `insert_bundle_by_id`

        let mut entity = world.spawn_empty();
        OwningPtr::make(TableComponent(84), |ptr| {
            // SAFETY: `ptr` matches the component id
            unsafe { entity.insert_by_ids(&[table_component_id], vec![ptr].into_iter()) };
        });

        let components: Vec<_> = world.query::<&TableComponent>().iter(&world).collect();

        assert_eq!(components, vec![&TableComponent(42), &TableComponent(84)]);
    }

    #[test]
    fn entity_mut_insert_bundle_by_id() {
        let mut world = World::new();
        let table_component_id = world.init_component::<TableComponent>();
        let sparse_component_id = world.init_component::<SparseComponent>();

        let component_ids = [table_component_id, sparse_component_id];
        let table_component_value = TableComponent(42);
        let sparse_component_value = SparseComponent(84);

        let mut entity = world.spawn_empty();
        OwningPtr::make(table_component_value, |ptr1| {
            OwningPtr::make(sparse_component_value, |ptr2| {
                // SAFETY: `ptr1` and `ptr2` match the component ids
                unsafe { entity.insert_by_ids(&component_ids, vec![ptr1, ptr2].into_iter()) };
            });
        });

        let dynamic_components: Vec<_> = world
            .query::<(&TableComponent, &SparseComponent)>()
            .iter(&world)
            .collect();

        assert_eq!(
            dynamic_components,
            vec![(&TableComponent(42), &SparseComponent(84))]
        );

        // Compare with `World` generated using static type equivalents
        let mut static_world = World::new();

        static_world.spawn((table_component_value, sparse_component_value));
        let static_components: Vec<_> = static_world
            .query::<(&TableComponent, &SparseComponent)>()
            .iter(&static_world)
            .collect();

        assert_eq!(dynamic_components, static_components);
    }
}
