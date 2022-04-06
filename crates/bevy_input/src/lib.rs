mod axis;
pub mod gamepad;
mod input;
pub mod keyboard;
pub mod mouse;
pub mod system;
pub mod touch;

pub use axis::*;
use bevy_ecs::schedule::{IntoScheduledSet, IntoScheduledSystem, SystemLabel};
pub use input::*;

pub mod prelude {
    #[doc(hidden)]
    pub use crate::{
        gamepad::{
            Gamepad, GamepadAxis, GamepadAxisType, GamepadButton, GamepadButtonType, GamepadEvent,
            GamepadEventType, Gamepads,
        },
        keyboard::KeyCode,
        mouse::MouseButton,
        touch::{TouchInput, Touches},
        Axis, Input,
    };
}

use bevy_app::prelude::*;
use keyboard::{keyboard_input_system, KeyCode, KeyboardInput};
use mouse::{mouse_button_input_system, MouseButton, MouseButtonInput, MouseMotion, MouseWheel};
use prelude::Gamepads;
use touch::{touch_screen_input_system, TouchInput, Touches};

use gamepad::{
    gamepad_connection_system, gamepad_event_system, GamepadAxis, GamepadButton, GamepadEvent,
    GamepadEventRaw, GamepadSettings,
};

/// Adds keyboard and mouse input.
#[derive(Default)]
pub struct InputPlugin;

#[derive(Debug, Clone, PartialEq, Eq, Hash, SystemLabel)]
pub struct InputSet;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_set(InputSet.to(CoreSet::PreUpdate))
            // keyboard
            .add_event::<KeyboardInput>()
            .init_resource::<Input<KeyCode>>()
            .add_system(keyboard_input_system.to(InputSet))
            // mouse
            .add_event::<MouseButtonInput>()
            .add_event::<MouseMotion>()
            .add_event::<MouseWheel>()
            .init_resource::<Input<MouseButton>>()
            .add_system(mouse_button_input_system.to(InputSet))
            // gamepad
            .add_event::<GamepadEvent>()
            .add_event::<GamepadEventRaw>()
            .init_resource::<GamepadSettings>()
            .init_resource::<Gamepads>()
            .init_resource::<Input<GamepadButton>>()
            .init_resource::<Axis<GamepadAxis>>()
            .init_resource::<Axis<GamepadButton>>()
            .add_system(gamepad_event_system.to(InputSet))
            .add_system(
                gamepad_connection_system
                    .to(CoreSet::PreUpdate)
                    .after(InputSet),
            )
            // touch
            .add_event::<TouchInput>()
            .init_resource::<Touches>()
            .add_system(touch_screen_input_system.to(InputSet));
    }
}

/// The current "press" state of an element
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub enum ElementState {
    Pressed,
    Released,
}

impl ElementState {
    pub fn is_pressed(&self) -> bool {
        matches!(self, ElementState::Pressed)
    }
}
