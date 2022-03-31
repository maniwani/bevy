pub use bevy_ecs_macros::SystemLabel;
use bevy_utils::define_label;

define_label!(SystemLabel);
pub(crate) type BoxedSystemLabel = Box<dyn SystemLabel>;
