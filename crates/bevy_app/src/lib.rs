#![warn(missing_docs)]
//! This crate is about everything concerning the highest-level, application layer of a Bevy
//! app.

mod app;
mod plugin;
mod plugin_group;
mod schedule_runner;

#[cfg(feature = "bevy_ci_testing")]
mod ci_testing;

pub use app::*;
pub use bevy_derive::DynamicPlugin;
pub use bevy_ecs::event::*;
pub use plugin::*;
pub use plugin_group::*;
pub use schedule_runner::*;

#[allow(missing_docs)]
pub mod prelude {
    #[doc(hidden)]
    pub use crate::{
        app::{App, AppSet},
        DynamicPlugin, Plugin, PluginGroup,
    };
}
