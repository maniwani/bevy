use bevy_utils::tracing::warn;

use crate::{
    archetype::ArchetypeComponentId, change_detection::MAX_CHANGE_AGE, component::ComponentId,
    ptr::SemiSafeCell, query::Access, schedule::SystemLabel, world::World,
};
use std::borrow::Cow;

/// An ECS system, typically converted from functions and closures whose arguments all implement
/// [`SystemParam`](crate::system::SystemParam).
///
/// **Note**: Only systems with `In = ()` and `Out = ()` can be added to a [`Schedule`](crate::schedule::Schedule).
/// When constructing a `Schedule`, use a [`SystemDescriptor`](crate::schedule::SystemDescriptor) to
/// specify when a system runs relative to others.
pub trait System: Send + Sync + 'static {
    /// The input to the system.
    type In;
    /// The system's output.
    type Out;

    /// Returns the system's name.
    fn name(&self) -> Cow<'static, str>;
    /// Updates the [archetype component](crate::archetype::ArchetypeComponentId) [`Access`]
    /// of the system to account for account for every [`Archetype`](crate::archetype::Archetype) in `world`.
    fn update_archetype_component_access(&mut self, world: &World);
    /// Returns the system's [component](crate::component::ComponentId) [`Access`].
    fn component_access(&self) -> &Access<ComponentId>;
    /// Returns the system's [archetype component](crate::archetype::ArchetypeComponentId) [`Access`].
    fn archetype_component_access(&self) -> &Access<ArchetypeComponentId>;
    /// Returns `true` if the system only accesses data that is [`Send`].
    fn is_send(&self) -> bool;
    fn is_exclusive(&self) -> bool;
    /// Runs the system with the given `input` on `world`.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - The given world is the same world used to construct the system.
    /// - There are no active references that conflict with the system's access. Mutable access must be unique.
    unsafe fn run_unchecked(&mut self, input: Self::In, world: SemiSafeCell<World>) -> Self::Out;
    /// Runs the system with the given `input` on `world`.
    fn run(&mut self, input: Self::In, world: &mut World) -> Self::Out {
        // checks that world matches as well
        self.update_archetype_component_access(world);
        // SAFETY: The world is exclusively borrowed.
        unsafe { self.run_unchecked(input, SemiSafeCell::from_mut(world)) }
    }
    /// Applies deferred operations such as [commands](crate::system::Command) on `world`.
    fn apply_buffers(&mut self, world: &mut World);
    /// Initializes the internal state of the system from the given `world`.
    fn initialize(&mut self, _world: &mut World);
    fn check_change_tick(&mut self, change_tick: u32);
    /// The default labels for the system
    fn default_labels(&self) -> Vec<Box<dyn SystemLabel>> {
        Vec::new()
    }
}

/// A convenient type alias for a boxed [`System`] trait object.
pub type BoxedSystem<In = (), Out = ()> = Box<dyn System<In = In, Out = Out>>;

pub(crate) fn check_system_change_tick(
    last_change_tick: &mut u32,
    change_tick: u32,
    system_name: &str,
) {
    let age = change_tick.wrapping_sub(*last_change_tick);
    // This comparison assumes that `age` has not overflowed `u32::MAX` before, which will be true
    // so long as this check always runs before that can happen.
    if age > MAX_CHANGE_AGE {
        warn!(
            "System '{}' has not run for {} ticks. \
            Changes older than {} ticks will not be detected.",
            system_name,
            age,
            MAX_CHANGE_AGE - 1,
        );
        *last_change_tick = change_tick.wrapping_sub(MAX_CHANGE_AGE);
    }
}
