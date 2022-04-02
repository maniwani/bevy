use crate::{App, Plugin};
use bevy_utils::{tracing::debug, HashMap};
use std::any::TypeId;

/// Combines multiple [`Plugin`] instances into one.
pub trait PluginGroup {
    /// Builds all the plugins in the group.
    fn build(&mut self, group: &mut PluginGroupBuilder);
}

/// A [`Plugin`] that is part of a [`PluginGroup`].
struct PluginEntry {
    plugin: Box<dyn Plugin>,
    enabled: bool,
}

/// Facilitates the creation and configuration of a [`PluginGroup`].
/// Defines a [`Plugin`] build order to enable plugins being dependent on other plugins.
#[derive(Default)]
pub struct PluginGroupBuilder {
    plugins: HashMap<TypeId, PluginEntry>,
    order: Vec<TypeId>,
}

impl PluginGroupBuilder {
    /// Appends the [`Plugin`] to the end of the current set.
    pub fn add<T: Plugin>(&mut self, plugin: T) -> &mut Self {
        self.order.push(TypeId::of::<T>());
        self.plugins.insert(
            TypeId::of::<T>(),
            PluginEntry {
                plugin: Box::new(plugin),
                enabled: true,
            },
        );
        self
    }

    /// Configures the [`Plugin`] to be built before the `Target` plugin.
    pub fn add_before<Target: Plugin, T: Plugin>(&mut self, plugin: T) -> &mut Self {
        let target_index = self
            .order
            .iter()
            .enumerate()
            .find(|(_i, ty)| **ty == TypeId::of::<Target>())
            .map(|(i, _)| i)
            .unwrap_or_else(|| {
                panic!(
                    "Plugin does not exist: {}.",
                    std::any::type_name::<Target>()
                )
            });
        self.order.insert(target_index, TypeId::of::<T>());
        self.plugins.insert(
            TypeId::of::<T>(),
            PluginEntry {
                plugin: Box::new(plugin),
                enabled: true,
            },
        );
        self
    }

    /// Configures the [`Plugin`] to be built after the `Target` plugin.
    pub fn add_after<Target: Plugin, T: Plugin>(&mut self, plugin: T) -> &mut Self {
        let target_index = self
            .order
            .iter()
            .enumerate()
            .find(|(_i, ty)| **ty == TypeId::of::<Target>())
            .map(|(i, _)| i)
            .unwrap_or_else(|| {
                panic!(
                    "Plugin does not exist: {}.",
                    std::any::type_name::<Target>()
                )
            });
        self.order.insert(target_index + 1, TypeId::of::<T>());
        self.plugins.insert(
            TypeId::of::<T>(),
            PluginEntry {
                plugin: Box::new(plugin),
                enabled: true,
            },
        );
        self
    }

    /// Enables the [`Plugin`].
    ///
    /// Plugins within a [`PluginGroup`] are enabled by default. This function will
    /// re-activate a plugin if [`disable`](Self::disable) was called earlier.
    pub fn enable<T: Plugin>(&mut self) -> &mut Self {
        let mut plugin_entry = self
            .plugins
            .get_mut(&TypeId::of::<T>())
            .expect("Cannot enable a plugin that does not exist.");
        plugin_entry.enabled = true;
        self
    }

    /// Disables the [`Plugin`], stopping it from being built by an [`App`] with the rest of the [`PluginGroup`].
    pub fn disable<T: Plugin>(&mut self) -> &mut Self {
        let mut plugin_entry = self
            .plugins
            .get_mut(&TypeId::of::<T>())
            .expect("Cannot disable a plugin that does not exist.");
        plugin_entry.enabled = false;
        self
    }

    /// Consumes the [`PluginGroupBuilder`] and runs the [`build`](Plugin::build) function of each
    /// contained [`Plugin`] in the specified order.
    pub fn finish(self, app: &mut App) {
        for ty in self.order.iter() {
            if let Some(entry) = self.plugins.get(ty) {
                if entry.enabled {
                    debug!("added plugin: {}", entry.plugin.name());
                    entry.plugin.build(app);
                }
            }
        }
    }
}
