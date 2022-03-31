use crate::{
    archetype::{ArchetypeComponentId, ArchetypeGeneration, ArchetypeId},
    cell::SemiSafeCell,
    change_detection::MAX_CHANGE_AGE,
    component::ComponentId,
    query::{Access, FilteredAccessSet},
    schedule::SystemLabel,
    system::{
        check_system_change_tick, ReadOnlySystemParamFetch, System, SystemParam, SystemParamFetch,
        SystemParamState,
    },
    world::{World, WorldId},
};
use bevy_ecs_macros::all_tuples;
use std::{borrow::Cow, fmt::Debug, hash::Hash, marker::PhantomData};

/// The metadata of a [`System`].
#[derive(Debug, Clone)]
pub struct SystemMeta {
    pub(crate) name: Cow<'static, str>,
    pub(crate) component_access_set: FilteredAccessSet<ComponentId>,
    pub(crate) archetype_component_access: Access<ArchetypeComponentId>,
    pub(crate) last_change_tick: u32,
    // NOTE: This field must be kept private. Making a `SystemMeta` non-`Send` is irreversible to
    // prevent multiple system params from toggling it.
    is_send: bool,
}

impl SystemMeta {
    fn new<T>() -> Self {
        Self {
            name: std::any::type_name::<T>().into(),
            archetype_component_access: Access::default(),
            component_access_set: FilteredAccessSet::default(),
            last_change_tick: 0,
            is_send: true,
        }
    }

    /// Returns true if the system is [`Send`].
    #[inline]
    pub fn is_send(&self) -> bool {
        self.is_send
    }

    /// Sets the system to be not [`Send`].
    ///
    /// This is irreversible.
    #[inline]
    pub fn set_non_send(&mut self) {
        self.is_send = false;
    }
}

// TODO: Use SystemState in FunctionSystem.
// #2777 wants systems initialized upon construction instead of deferring it.
// That would let us avoid unwrapping `Option` fields in `FunctionSystem`.

/// Stores persistent state required by the [parameters](crate::system#system-construction)
/// of a [`System`](crate::system::System).
/// Enables borrowing arbitrary data from the [`World`], just like a [`System`].
///
/// `SystemState` can make working with `&mut World` more convenient, especially when used in combination
/// with [`World::resource_scope`].
///
/// # Notes
///
/// [`SystemState`] instances can be cached to improve performance,
/// and *must* be cached and reused in order for params that rely on local state to work correctly.
/// These include:
/// - [`Added`](crate::query::Added) and [`Changed`](crate::query::Changed) query filters
/// - [`Local`](crate::system::Local) variables that hold state
/// - [`EventReader`](crate::event::EventReader) system parameters, which rely on a [`Local`](crate::system::Local) to track which events have been seen
///
/// # Example
///
/// Basic usage:
/// ```rust
/// use bevy_ecs::prelude::*;
/// use bevy_ecs::{system::SystemState};
/// use bevy_ecs::event::Events;
///
/// struct MyEvent;
/// struct MyResource(u32);
///
/// #[derive(Component)]
/// struct MyComponent;
///
/// // Work directly on the `World`
/// let mut world = World::new();
/// world.init_resource::<Events<MyEvent>>();
///
/// // Construct a `SystemState` struct, passing in a tuple of `SystemParam`
/// // as if you were writing an ordinary system.
/// let mut system_state: SystemState<(
///     EventWriter<MyEvent>,
///     Option<ResMut<MyResource>>,
///     Query<&MyComponent>,
///     )> = SystemState::new(&mut world);
///
/// // Use system_state.get_mut(&mut world) and unpack your system parameters into variables!
/// // You can use system_state.get(&world) if all your parameters are read-only or local.
/// let (event_writer, maybe_resource, query) = system_state.get_mut(&mut world);
/// ```
/// Caching:
/// ```rust
/// use bevy_ecs::prelude::*;
/// use bevy_ecs::{system::SystemState};
/// use bevy_ecs::event::Events;
///
/// struct MyEvent;
/// struct CachedSystemState<'w, 's>{
///    event_state: SystemState<EventReader<'w, 's, MyEvent>>
/// }
///
/// // Create and store a system state once
/// let mut world = World::new();
/// world.init_resource::<Events<MyEvent>>();
/// let initial_state: SystemState<EventReader<MyEvent>>  = SystemState::new(&mut world);
///
/// // The system state is cached in a resource
/// world.insert_resource(CachedSystemState{event_state: initial_state});
///
/// // Later, fetch the cached system state, saving on overhead
/// world.resource_scope(|world, mut cached_state: Mut<CachedSystemState>| {
///     let mut event_reader = cached_state.event_state.get_mut(world);
///
///     for events in event_reader.iter(){
///         println!("Hello World!");
///     };
/// });
/// ```
pub struct SystemState<Param: SystemParam> {
    meta: SystemMeta,
    param_state: <Param as SystemParam>::Fetch,
    world_id: WorldId,
    archetype_generation: ArchetypeGeneration,
}

impl<Param: SystemParam> SystemState<Param> {
    pub fn new(world: &mut World) -> Self {
        let mut meta = SystemMeta::new::<Param>();
        meta.last_change_tick = world.change_tick().wrapping_sub(MAX_CHANGE_AGE);
        let param_state = <Param::Fetch as SystemParamState>::init(world, &mut meta);
        Self {
            meta,
            param_state,
            world_id: world.id(),
            archetype_generation: ArchetypeGeneration::initial(),
        }
    }

    #[inline]
    pub fn meta(&self) -> &SystemMeta {
        &self.meta
    }

    /// Retrieves the [`SystemParam`] values (must all be read-only) from the [`World`].
    ///
    /// This method also ensures the state's [`Access`] is up-to-date before retrieving the data.
    #[inline]
    pub fn get<'w, 's>(
        &'s mut self,
        world: &'w World,
    ) -> <Param::Fetch as SystemParamFetch<'w, 's>>::Item
    where
        Param::Fetch: ReadOnlySystemParamFetch,
    {
        self.verify_world_and_update_archetype_component_access(world);
        // SAFETY: The params cannot request mutable access and world is the same one used to construct this state.
        unsafe { self.get_unchecked(SemiSafeCell::from_ref(world)) }
    }

    /// Retrieves the [`SystemParam`] values from the [`World`].
    ///
    /// This method also ensures the state's [`Access`] is up-to-date before retrieving the data.
    #[inline]
    pub fn get_mut<'w, 's>(
        &'s mut self,
        world: &'w mut World,
    ) -> <Param::Fetch as SystemParamFetch<'w, 's>>::Item {
        self.verify_world_and_update_archetype_component_access(world);
        // SAFE: World is uniquely borrowed and matches the World this SystemState was created with.
        unsafe { self.get_unchecked(SemiSafeCell::from_mut(world)) }
    }

    /// Applies all state queued by the [`SystemParam`] values onto the [`World`].
    /// For example, this will apply any [commands](crate::system::Command) queued with
    /// [`Commands`](crate::system::Commands).
    ///
    /// Call once the data borrowed by [`SystemState::get`] and [`SystemState::get_mut`] is done being used.
    pub fn apply(&mut self, world: &mut World) {
        assert!(
            self.matches_world(world),
            "World does not match. A SystemState can only be used with the world that constructed it."
        );
        self.param_state.apply(world);
    }

    #[inline]
    pub fn matches_world(&self, world: &World) -> bool {
        self.world_id == world.id()
    }

    fn verify_world_and_update_archetype_component_access(&mut self, world: &World) {
        assert!(
            self.matches_world(world),
            "World does not match. A SystemState can only be used with the world that constructed it."
        );
        let archetypes = world.archetypes();
        let new_generation = archetypes.generation();
        let old_generation = std::mem::replace(&mut self.archetype_generation, new_generation);
        let archetype_index_range = old_generation.value()..new_generation.value();

        for archetype_index in archetype_index_range {
            self.param_state.new_archetype(
                &archetypes[ArchetypeId::new(archetype_index)],
                &mut self.meta,
            );
        }
    }

    /// Retrieves the [`SystemParam`] values from the [`World`].
    ///
    /// This method does _not_ update the state's [`Access`] before retrieving the data.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - The given world is the same world used to construct the system state.
    /// - There are no active references that conflict with the system state's access. Mutable access must be unique.
    #[inline]
    pub unsafe fn get_unchecked<'w, 's>(
        &'s mut self,
        world: SemiSafeCell<'w, World>,
    ) -> <Param::Fetch as SystemParamFetch<'w, 's>>::Item {
        let change_tick = world.as_ref().increment_change_tick();
        let param = <Param::Fetch as SystemParamFetch>::get_param(
            &mut self.param_state,
            &self.meta,
            world,
            change_tick,
        );
        self.meta.last_change_tick = change_tick;
        param
    }
}

/// Conversion trait to turn something into a [`System`]. Use this to get a system from a function or closure.
///
/// This trait is blanket implemented for all [`System`] types.
///
/// # Examples
///
/// ```
/// use bevy_ecs::system::IntoSystem;
/// use bevy_ecs::system::Res;
///
/// fn my_system_function(an_usize_resource: Res<usize>) {}
///
/// let system = IntoSystem::system(my_system_function);
/// ```
// This trait requires the generic `Params` because, as far as Rust knows, a type could have
// several impls of `FnMut` with different arguments, even though functions and closures don't.
pub trait IntoSystem<In, Out, Params>: Sized {
    type System: System<In = In, Out = Out>;
    /// Turns this value into its corresponding [`System`].
    ///
    /// Use of this method was formerly required whenever adding a `system` to an `App`.
    /// or other cases where a system is required.
    /// However, since [#2398](https://github.com/bevyengine/bevy/pull/2398),
    /// this is no longer required.
    ///
    /// In future, this method will be removed.
    ///
    /// One use of this method is to assert that a given function is a valid system.
    /// For this case, use [`bevy_ecs::system::assert_is_system`] instead.
    ///
    /// [`bevy_ecs::system::assert_is_system`]: [`crate::system::assert_is_system`]:
    #[deprecated(
        since = "0.7.0",
        note = "`.system()` is no longer needed, as methods which accept systems will convert functions into a system automatically"
    )]
    fn system(self) -> Self::System {
        IntoSystem::into_system(self)
    }
    /// Turns this value into its corresponding [`System`].
    fn into_system(this: Self) -> Self::System;
}

pub struct AlreadyWasSystem;

// Converting a system into a system is a no-op.
impl<In, Out, Sys: System<In = In, Out = Out>> IntoSystem<In, Out, AlreadyWasSystem> for Sys {
    type System = Sys;
    fn into_system(this: Self) -> Sys {
        this
    }
}

/// A system parameter that denotes an external input.
///
/// The input for a [`System`] object must be passed into [`run`](System::run).
///
/// To use an `In<T>` with a [`FunctionSystem`](FunctionSystem), it has to be first parameter
/// in its function signature.
///
/// Note: This type does not implement [`SystemParam`].
///
/// # Examples
///
/// This system takes an external [`usize`] and returns its square.
///
/// ```
/// use bevy_ecs::prelude::*;
///
/// fn square(In(input): In<usize>) -> usize {
///     input * input
/// }
///
/// fn main() {
///     let mut square_system = IntoSystem::into_system(square);
///
///     let mut world = World::default();
///     square_system.initialize(&mut world);
///     assert_eq!(square_system.run(12, &mut world), 144);
/// }
/// ```
pub struct In<T>(pub T);

#[doc(hidden)]
pub struct InputMarker;

/// The [`System`]-type of functions and closures.
///
/// Constructed by calling [`IntoSystem::into_system`] with a function or closure whose arguments all implement
/// [`SystemParam`].
///
/// If the function's first argument is [`In<T>`], `T` becomes the system's [`In`](crate::system::System::In) type
/// (`In = ()` otherwise).
/// The function's return type becomes the system's [`Out`](crate::system::System::Out) type.
pub struct FunctionSystem<In, Out, Param, Marker, F>
where
    Param: SystemParam,
{
    func: F,
    param_state: Option<Param::Fetch>,
    system_meta: SystemMeta,
    world_id: Option<WorldId>,
    archetype_generation: ArchetypeGeneration,
    // NOTE: PhantomData<fn()-> T> gives this safe Send/Sync impls
    #[allow(clippy::type_complexity)]
    marker: PhantomData<fn() -> (In, Out, Marker)>,
}

pub struct IsFunctionSystem;

impl<In, Out, Param, Marker, F> IntoSystem<In, Out, (IsFunctionSystem, Param, Marker)> for F
where
    In: 'static,
    Out: 'static,
    Param: SystemParam + 'static,
    Marker: 'static,
    F: SystemParamFunction<In, Out, Param, Marker> + Send + Sync + 'static,
{
    type System = FunctionSystem<In, Out, Param, Marker, F>;
    fn into_system(func: Self) -> Self::System {
        FunctionSystem {
            func,
            param_state: None,
            system_meta: SystemMeta::new::<F>(),
            world_id: None,
            archetype_generation: ArchetypeGeneration::initial(),
            marker: PhantomData,
        }
    }
}

impl<In, Out, Param, Marker, F> System for FunctionSystem<In, Out, Param, Marker, F>
where
    In: 'static,
    Out: 'static,
    Param: SystemParam + 'static,
    Marker: 'static,
    F: SystemParamFunction<In, Out, Param, Marker> + Send + Sync + 'static,
{
    type In = In;
    type Out = Out;

    #[inline]
    fn name(&self) -> Cow<'static, str> {
        self.system_meta.name.clone()
    }

    #[inline]
    fn update_archetype_component_access(&mut self, world: &World) {
        assert!(
            self.world_id == Some(world.id()),
            "World does not match. A FunctionSystem can only be used with the world that constructed it."
        );
        let archetypes = world.archetypes();
        let new_generation = archetypes.generation();
        let old_generation = std::mem::replace(&mut self.archetype_generation, new_generation);
        let archetype_index_range = old_generation.value()..new_generation.value();

        let param_state = self.param_state.as_mut().unwrap();
        for archetype_index in archetype_index_range {
            param_state.new_archetype(
                &archetypes[ArchetypeId::new(archetype_index)],
                &mut self.system_meta,
            );
        }
    }

    #[inline]
    fn component_access(&self) -> &Access<ComponentId> {
        self.system_meta.component_access_set.combined_access()
    }

    #[inline]
    fn archetype_component_access(&self) -> &Access<ArchetypeComponentId> {
        &self.system_meta.archetype_component_access
    }

    #[inline]
    fn is_send(&self) -> bool {
        self.system_meta.is_send()
    }

    #[inline]
    unsafe fn run_unchecked(&mut self, input: Self::In, world: SemiSafeCell<World>) -> Self::Out {
        let change_tick = world.as_ref().increment_change_tick();
        let out = self.func.run(
            input,
            self.param_state.as_mut().unwrap(),
            &self.system_meta,
            world,
            change_tick,
        );
        self.system_meta.last_change_tick = change_tick;
        out
    }

    #[inline]
    fn apply_buffers(&mut self, world: &mut World) {
        let param_state = self.param_state.as_mut().unwrap();
        param_state.apply(world);
    }

    #[inline]
    fn initialize(&mut self, world: &mut World) {
        self.world_id = Some(world.id());
        self.system_meta.last_change_tick = world.change_tick().wrapping_sub(MAX_CHANGE_AGE);
        self.param_state = Some(<Param::Fetch as SystemParamState>::init(
            world,
            &mut self.system_meta,
        ));
    }

    #[inline]
    fn check_change_tick(&mut self, change_tick: u32) {
        check_system_change_tick(
            &mut self.system_meta.last_change_tick,
            change_tick,
            self.system_meta.name.as_ref(),
        );
    }
    fn default_labels(&self) -> Vec<Box<dyn SystemLabel>> {
        vec![Box::new(self.func.as_system_label())]
    }
}

/// A [`SystemLabel`] that was automatically generated for a system on the basis of its `TypeId`.
pub struct SystemTypeIdLabel<T: 'static>(PhantomData<fn() -> T>);

impl<T> Debug for SystemTypeIdLabel<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SystemTypeIdLabel")
            .field(&std::any::type_name::<T>())
            .finish()
    }
}
impl<T> Hash for SystemTypeIdLabel<T> {
    fn hash<H: std::hash::Hasher>(&self, _state: &mut H) {
        // All SystemTypeIds of a given type are the same.
    }
}
impl<T> Clone for SystemTypeIdLabel<T> {
    fn clone(&self) -> Self {
        Self(PhantomData)
    }
}

impl<T> Copy for SystemTypeIdLabel<T> {}

impl<T> PartialEq for SystemTypeIdLabel<T> {
    #[inline]
    fn eq(&self, _other: &Self) -> bool {
        // All labels of a given type are equal, as they will all have the same type id
        true
    }
}
impl<T> Eq for SystemTypeIdLabel<T> {}

impl<T> SystemLabel for SystemTypeIdLabel<T> {
    fn dyn_clone(&self) -> Box<dyn SystemLabel> {
        Box::new(*self)
    }
}

/// Trait implemented for all functions and closures that can be converted into a [`System`].
//
// This trait requires the generic `Params` because, as far as Rust knows, a type could have
// several impls of `FnMut` with different arguments, even though functions and closures don't.
pub trait SystemParamFunction<In, Out, Param: SystemParam, Marker>: Send + Sync + 'static {
    /// # Safety
    ///
    /// Caller must ensure:
    /// - The `state` was constructed from the given `world`.
    /// - There are no conflicting active references on the retrieved data. Mutable access must be unique.
    unsafe fn run(
        &mut self,
        input: In,
        state: &mut Param::Fetch,
        system_meta: &SystemMeta,
        world: SemiSafeCell<World>,
        change_tick: u32,
    ) -> Out;
}

macro_rules! impl_system_function {
    ($($param: ident),*) => {
        #[allow(non_snake_case)]
        impl<Out, Func: Send + Sync + 'static, $($param: SystemParam),*> SystemParamFunction<(), Out, ($($param,)*), ()> for Func
        where
            Out: 'static,
            for <'a> &'a mut Func:
                FnMut($($param,)*) -> Out +
                FnMut($(<<$param as SystemParam>::Fetch as SystemParamFetch>::Item,)*) -> Out,
        {
            #[inline]
            unsafe fn run(
                &mut self, _input: (),
                state: &mut <($($param,)*) as SystemParam>::Fetch,
                system_meta: &SystemMeta,
                world: SemiSafeCell<World>,
                change_tick: u32,
            ) -> Out {
                // Yes, this is strange, but rustc fails to compile this impl
                // without using this function.
                #[allow(clippy::too_many_arguments)]
                fn call_inner<Out, $($param,)*>(
                    mut f: impl FnMut($($param,)*) -> Out,
                    $($param: $param,)*
                ) -> Out {
                    f($($param,)*)
                }
                let ($($param,)*) = <<($($param,)*) as SystemParam>::Fetch as SystemParamFetch>::get_param(
                    state,
                    system_meta,
                    world,
                    change_tick,
                );
                call_inner(self, $($param,)*)
            }
        }

        #[allow(non_snake_case)]
        impl<Input, Out, Func: Send + Sync + 'static, $($param: SystemParam),*> SystemParamFunction<Input, Out, ($($param,)*), InputMarker> for Func
        where
            Out: 'static,
            for <'a> &'a mut Func:
                FnMut(In<Input>, $($param,)*) -> Out +
                FnMut(In<Input>, $(<<$param as SystemParam>::Fetch as SystemParamFetch>::Item,)*) -> Out,
        {
            #[inline]
            unsafe fn run(
                &mut self,
                input: Input,
                state: &mut <($($param,)*) as SystemParam>::Fetch,
                system_meta: &SystemMeta,
                world: SemiSafeCell<World>,
                change_tick: u32,
            ) -> Out {
                #[allow(clippy::too_many_arguments)]
                fn call_inner<Input, Out, $($param,)*>(
                    mut f: impl FnMut(In<Input>, $($param,)*) -> Out,
                    input: In<Input>,
                    $($param: $param,)*
                ) -> Out {
                    f(input, $($param,)*)
                }
                let ($($param,)*) = <<($($param,)*) as SystemParam>::Fetch as SystemParamFetch>::get_param(
                    state,
                    system_meta,
                    world,
                    change_tick,
                );
                call_inner(self, In(input), $($param,)*)
            }
        }
    };
}

all_tuples!(impl_system_function, 0, 16, P);

/// Implicit conversion of [`System`] types into a [`SystemLabel`] (or several).
///
/// For example, `System`-compatible functions are converted into a [`SystemTypeIdLabel`].
pub trait AsSystemLabel<Marker> {
    type SystemLabel: SystemLabel;
    fn as_system_label(&self) -> Self::SystemLabel;
}

impl<In, Out, Param: SystemParam, Marker, T: SystemParamFunction<In, Out, Param, Marker>>
    AsSystemLabel<(In, Out, Param, Marker)> for T
{
    type SystemLabel = SystemTypeIdLabel<Self>;

    fn as_system_label(&self) -> Self::SystemLabel {
        SystemTypeIdLabel(PhantomData::<fn() -> Self>)
    }
}

impl<T: SystemLabel + Clone> AsSystemLabel<()> for T {
    type SystemLabel = T;

    fn as_system_label(&self) -> Self::SystemLabel {
        self.clone()
    }
}
