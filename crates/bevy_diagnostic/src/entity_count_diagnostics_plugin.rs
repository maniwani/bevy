use bevy_app::{App, AppSet, Plugin};
use bevy_ecs::{schedule::IntoScheduledSystem, system::ResMut, world::World};

use crate::{Diagnostic, DiagnosticId, Diagnostics};

/// Adds an "entity count" diagnostic.
#[derive(Default)]
pub struct EntityCountDiagnosticsPlugin;

impl Plugin for EntityCountDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.add_system(Self::setup_system.to(AppSet::Startup))
            .add_system(Self::diagnostic_system);
    }
}

impl EntityCountDiagnosticsPlugin {
    pub const ENTITY_COUNT: DiagnosticId =
        DiagnosticId::from_u128(187513512115068938494459732780662867798);

    pub fn setup_system(mut diagnostics: ResMut<Diagnostics>) {
        diagnostics.add(Diagnostic::new(Self::ENTITY_COUNT, "entity_count", 20));
    }

    pub fn diagnostic_system(world: &mut World) {
        let entity_count = world.entities().len();
        if let Some(mut diagnostics) = world.get_resource_mut::<Diagnostics>() {
            diagnostics.add_measurement(Self::ENTITY_COUNT, entity_count as f64);
        }
    }
}
