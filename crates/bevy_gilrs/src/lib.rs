mod converter;
mod gilrs_system;

use bevy_app::{App, AppSet, Plugin};
use bevy_ecs::schedule::IntoScheduledSystem;
use bevy_input::InputSystem;
use bevy_utils::tracing::error;
use gilrs::GilrsBuilder;
use gilrs_system::{gilrs_event_startup_system, gilrs_event_system};

#[derive(Default)]
pub struct GilrsPlugin;

impl Plugin for GilrsPlugin {
    fn build(&self, app: &mut App) {
        match GilrsBuilder::new()
            .with_default_filters(false)
            .set_update_state(false)
            .build()
        {
            Ok(gilrs) => {
                app.insert_non_send_resource(gilrs)
                    .add_system(gilrs_event_startup_system.to(AppSet::PreStartup))
                    .add_system(
                        gilrs_event_system
                            .to(bevy_core::CoreSet::PreUpdate)
                            .before(InputSystem),
                    );
            }
            Err(err) => error!("Failed to start Gilrs. {}", err),
        }
    }
}
