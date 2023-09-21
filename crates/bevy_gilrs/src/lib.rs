//! Systems and type definitions for gamepad handling in Bevy.
//!
//! This crate is built on top of [GilRs](gilrs), a library
//! that handles abstracting over platform-specific gamepad APIs.

#![allow(clippy::type_complexity)]
#![warn(missing_docs)]

mod converter;
mod gilrs_system;
mod rumble;

use bevy_app::{App, Plugin, PostUpdate, PreStartup, PreUpdate};
use bevy_ecs::prelude::*;
use bevy_input::InputSystem;
use bevy_utils::tracing::error;
use gilrs::GilrsBuilder;
use gilrs_system::{gilrs_event_startup_system, gilrs_event_system};
use rumble::{play_gilrs_rumble, RunningRumbleEffects};

// TODO: This can be a normal resource on all targets except WASM.
/// An instance of [`gilrs::Gilrs`].
#[derive(ThreadLocalResource)]
pub struct Gilrs(gilrs::Gilrs);
impl std::ops::Deref for Gilrs {
    type Target = gilrs::Gilrs;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for Gilrs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Plugin that provides gamepad handling to an [`App`].
#[derive(Default)]
pub struct GilrsPlugin;

/// Updates the running gamepad rumble effects.
#[derive(Debug, PartialEq, Eq, Clone, Hash, SystemSet)]
pub struct RumbleSystem;

impl Plugin for GilrsPlugin {
    fn build(&self, app: &mut App) {
        match GilrsBuilder::new()
            .with_default_filters(false)
            .set_update_state(false)
            .build()
        {
            Ok(gilrs) => {
                app.insert_non_send_resource(crate::Gilrs(gilrs))
                    .init_non_send_resource::<RunningRumbleEffects>()
                    .add_systems(PreStartup, gilrs_event_startup_system)
                    .add_systems(PreUpdate, gilrs_event_system.before(InputSystem))
                    .add_systems(PostUpdate, play_gilrs_rumble.in_set(RumbleSystem));
            }
            Err(err) => error!("Failed to start Gilrs. {}", err),
        }
    }
}
