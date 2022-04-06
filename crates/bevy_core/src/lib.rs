#![warn(missing_docs)]
//! This crate provides core functionality for Bevy Engine.

mod float_ord;
mod name;
mod task_pool_options;
mod time;

pub use bytemuck::{bytes_of, cast_slice, Pod, Zeroable};
pub use float_ord::*;
pub use name::*;
pub use task_pool_options::*;
pub use time::*;

pub mod prelude {
    //! The Bevy Core Prelude.
    #[doc(hidden)]
    pub use crate::{
        CoreSet, DefaultTaskPoolOptions, FixedTime, FixedTimestepState, Name, Time, Timer,
    };
}

use bevy_app::{prelude::*, AppSystem};
use bevy_ecs::{
    entity::Entity,
    schedule::{apply_buffers, seq, IntoScheduledSet, IntoScheduledSystem, SystemLabel},
};
use bevy_utils::HashSet;

use std::ops::Range;

/// Systems sets comprising the standard execution sequence.
#[derive(Debug, Clone, PartialEq, Eq, Hash, SystemLabel)]
pub enum CoreSet {
    /// Systems that run before the systems in the other core sets (but after [`Time`] is updated).
    First,
    /// Systems that run with the fixed timestep. Intended for most gameplay systems (e.g. physics).
    ///
    /// **Note:** Fixed timestep does not mean fixed *interval*. This set can repeat several times in a single frame. Use [`FixedTime`] instead of [`Time`] to ensure correct behavior.
    FixedUpdate,
    /// Systems that run before systems in [`Update`](CoreSet::Update).
    /// Intended for systems that need to perform setup for systems in [`Update`](CoreSet::Update).
    PreUpdate,
    /// Systems that run each frame.
    ///
    /// **Note:** By default, systems and sets with no parent set specified are added here.
    Update,
    /// Systems that run after systems in [`Update`](CoreSet::Update).
    /// Intended for systems that need to process the results of systems in [`Update`](CoreSet::Update).
    PostUpdate,
    /// Systems that run after the systems in the other core sets.
    Last,
}

/// Systems comprising the standard execution sequence.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, SystemLabel)]
pub enum CoreSystem {
    /// Advances [`Time`]. First thing that runs in a frame.
    Time,
    /// Advances [`FixedTime`] and runs the systems in [`FixedUpdate`](CoreSet::FixedUpdate).
    FixedUpdate,
    /// Calls [`apply_buffers`] after the systems in [`First`](CoreSet::First).
    ApplyFirst,
    /// Calls [`apply_buffers`] after the systems in [`FixedUpdate`](CoreSet::FixedUpdate).
    ApplyFixedUpdate,
    /// Calls [`apply_buffers`] after the systems in [`PreUpdate`](CoreSet::PreUpdate).
    ApplyPreUpdate,
    /// Calls [`apply_buffers`] after the systems in [`Update`](CoreSet::Update).
    ApplyUpdate,
    /// Calls [`apply_buffers`] after the systems in [`PostUpdate`](CoreSet::PostUpdate).
    ApplyPostUpdate,
    /// Calls [`apply_buffers`] after the systems in [`Last`](CoreSet::Last).
    ApplyLast,
}

/// Internal system sets needed to bypass limitations with [`apply_buffers`].
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, SystemLabel)]
pub(crate) enum CoreInternalSet {
    /// Encompasses the [`FixedUpdate`](CoreSet::FixedUpdate) system set and
    /// [`ApplyFixedUpdate`](CoreSystem::ApplyFixedUpdate).
    FixedUpdate,
}

/// Adds a standard system execution sequence and time-related resources.
#[derive(Default)]
pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.world
            .get_resource::<DefaultTaskPoolOptions>()
            .cloned()
            .unwrap_or_default()
            .create_default_pools(&mut app.world);

        app.init_resource::<Time>()
            .init_resource::<FixedTime>()
            .init_resource::<FixedTimestepState>()
            .register_type::<HashSet<String>>()
            .register_type::<Option<String>>()
            .register_type::<Entity>()
            .register_type::<Name>()
            .register_type::<Range<f32>>()
            .register_type::<Timer>()
            .add_many(
                seq![
                    update_time.named(CoreSystem::Time),
                    CoreSet::First,
                    apply_buffers.named(CoreSystem::ApplyFirst),
                    fixed_update.named(CoreSystem::FixedUpdate),
                    CoreSet::PreUpdate,
                    apply_buffers.named(CoreSystem::ApplyPreUpdate),
                    CoreSet::Update,
                    apply_buffers.named(CoreSystem::ApplyUpdate),
                    CoreSet::PostUpdate,
                    apply_buffers.named(CoreSystem::ApplyPostUpdate),
                    CoreSet::Last,
                    apply_buffers.named(CoreSystem::ApplyLast),
                ]
                .to(AppSet::Update)
                .after(AppSet::UpdateEvents)
                .before(AppSystem::ClearTrackers),
            )
            .add_many(
                seq![
                    CoreSet::FixedUpdate,
                    apply_buffers.named(CoreSystem::ApplyFixedUpdate),
                ]
                .to(CoreInternalSet::FixedUpdate),
            );

        register_rust_types(app);
        register_math_types(app);
    }
}

fn register_rust_types(app: &mut App) {
    app.register_type::<bool>()
        .register_type::<u8>()
        .register_type::<u16>()
        .register_type::<u32>()
        .register_type::<u64>()
        .register_type::<u128>()
        .register_type::<usize>()
        .register_type::<i8>()
        .register_type::<i16>()
        .register_type::<i32>()
        .register_type::<i64>()
        .register_type::<i128>()
        .register_type::<isize>()
        .register_type::<f32>()
        .register_type::<f64>()
        .register_type::<String>()
        .register_type::<Option<String>>();
}

fn register_math_types(app: &mut App) {
    app.register_type::<bevy_math::IVec2>()
        .register_type::<bevy_math::IVec3>()
        .register_type::<bevy_math::IVec4>()
        .register_type::<bevy_math::UVec2>()
        .register_type::<bevy_math::UVec3>()
        .register_type::<bevy_math::UVec4>()
        .register_type::<bevy_math::Vec2>()
        .register_type::<bevy_math::Vec3>()
        .register_type::<bevy_math::Vec4>()
        .register_type::<bevy_math::Mat3>()
        .register_type::<bevy_math::Mat4>()
        .register_type::<bevy_math::Quat>();
}
