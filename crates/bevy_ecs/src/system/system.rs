use bevy_utils::tracing::warn;

use crate::{
    archetype::ArchetypeComponentId, cell::SemiSafeCell, change_detection::MAX_CHANGE_AGE,
    component::ComponentId, query::Access, schedule::SystemLabel, world::World,
};
use std::borrow::Cow;

/// An ECS system, converted from functions and closures whose arguments all implement
/// [`SystemParam`](crate::system::SystemParam).
///
/// **Note**: Only systems with `In = ()` and `Out = ()` can be added to a [`SystemRegistry`](crate::schedule::SystemRegistry).
/// Use the [`SystemDescriptor`](crate::schedule::SystemDescriptor) methods to
/// specify when a system should run relative to others.
pub trait System: Send + Sync + 'static {
    /// The input to the system.
    type In;
    /// The output of the system.
    type Out;

    /// Returns the system's name.
    fn name(&self) -> Cow<'static, str>;
    /// Initializes the system's internal state from the [`World`].
    fn initialize(&mut self, world: &mut World);
    /// Runs the system with the given `input` on the [`World`].
    ///
    /// This method ensures the system's [`Access`] is up-to-date before retrieving the data.
    fn run(&mut self, input: Self::In, world: &mut World) -> Self::Out {
        // verifies world matches
        self.update_archetype_component_access(world);
        // SAFETY: world is exclusively borrowed and same one used to construct this system
        unsafe { self.run_unchecked(input, SemiSafeCell::from_mut(world)) }
    }
    /// Runs the system with the given `input` on the [`World`].
    ///
    /// This method does _not_ update the system's [`Access`] before retrieving the data.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - The world is the same world used to construct the system.
    /// - There are no conflicting active references on the retrieved data. Mutable access must be unique.
    unsafe fn run_unchecked(&mut self, input: Self::In, world: SemiSafeCell<World>) -> Self::Out;
    /// Applies deferred operations such as [commands](crate::system::Command) on the world.  
    fn apply_buffers(&mut self, world: &mut World);
    /// Returns the system's [component](crate::component::ComponentId) [`Access`].
    fn component_access(&self) -> &Access<ComponentId>;
    /// Returns the system's current [archetype component](crate::archetype::ArchetypeComponentId) [`Access`].
    fn archetype_component_access(&self) -> &Access<ArchetypeComponentId>;
    /// Updates the archetype component [`Access`] of the system to account for each
    /// [`Archetype`](crate::archetype::Archetype) in the [`World`].
    fn update_archetype_component_access(&mut self, world: &World);
    /// Returns `true` if the system only accesses data that is [`Send`].
    fn is_send(&self) -> bool;
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
