#![warn(missing_docs)]
//! `bevy_hierarchy` can be used to define hierarchies of entities.
//!
//! Most commonly, these hierarchies are used for inheriting `Transform` values
//! from the [`Parent`] to its [`Children`].

mod components;
pub use components::*;

mod hierarchy;
pub use hierarchy::*;

mod child_builder;
pub use child_builder::*;

mod systems;
pub use systems::*;

#[doc(hidden)]
pub mod prelude {
    #[doc(hidden)]
    pub use crate::{child_builder::*, components::*, hierarchy::*, HierarchyPlugin};
}

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;

/// Adds [`Parent`] and [`Children`] components and systems for handling them.
#[derive(Default)]
pub struct HierarchyPlugin;

/// Systems related to hierarchy upkeep.
#[derive(Debug, Clone, PartialEq, Eq, Hash, SystemLabel)]
pub enum HierarchySystem {
    /// Initializes [`Parent`] components at startup.
    ParentInit,
    /// Updates [`Parent`] components when the hierarchy changes.
    ParentUpdate,
}

impl Plugin for HierarchyPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Children>()
            .register_type::<Parent>()
            .register_type::<PreviousParent>()
            .add_system(
                parent_update_system
                    .named(HierarchySystem::ParentInit)
                    .to(AppSet::PostStartup),
            )
            .add_system(
                parent_update_system
                    .named(HierarchySystem::ParentUpdate)
                    .to(bevy_core::CoreSet::PostUpdate),
            );
    }
}
