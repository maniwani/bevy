use crate::{Plugin, PluginGroup, PluginGroupBuilder};
pub use bevy_derive::AppLabel;
use bevy_ecs::{
    event::Events,
    prelude::FromWorld,
    schedule::{
        apply_buffers, chain, run_scheduled, IntoScheduledSet, IntoScheduledSystem, Scheduled,
        SystemLabel, SystemRegistry,
    },
    system::Resource,
    world::World,
};
use bevy_utils::{tracing::debug, HashMap};
use std::fmt::Debug;

#[cfg(feature = "trace")]
use bevy_utils::tracing::info_span;
bevy_utils::define_label!(AppLabel);

#[allow(clippy::needless_doctest_main)]
/// An ECS application that wraps a [`World`], a runner function, and a [`SubApp`] collection.
///
/// [`App`] instances are constructed using a builder pattern.
///
/// ## Example
/// Here is a simple "Hello World" Bevy app:
/// ```
/// # use bevy_app::prelude::*;
/// # use bevy_ecs::prelude::*;
///
/// fn main() {
///    App::new()
///        .add_system(hello_world_system)
///        .run();
/// }
///
/// fn hello_world_system() {
///    println!("hello world");
/// }
/// ```
pub struct App {
    /// Stores all data used by the main application.
    ///
    /// Note that each sub-app also has its own [`World`].
    pub world: World,
    /// The [runner function](Self::set_runner) is responsible for managing
    /// the main application's event loop and running its systems.
    ///
    /// Usually, the runner is configured by the [`WinitPlugin`][`bevy::winit::WinitPlugin`]
    /// or [`ScheduleRunnerPlugin`](crate::schedule_runner::ScheduleRunnerPlugin).
    pub runner: Box<dyn Fn(App)>,
    sub_apps: HashMap<Box<dyn AppLabel>, SubApp>,
}

/// A "nested" [`App`] with its own [`World`] and runner, enabling a separation of concerns.
struct SubApp {
    app: App,
    runner: Box<dyn Fn(&mut World, &mut App)>,
}

/// System sets providing basic app functionality.
#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemLabel)]
pub enum AppSet {
    /// Systems that run only once when the app starts, before the [`Startup`](AppSet::Startup) set.
    PreStartup,
    /// Systems that run only once when the app starts.
    Startup,
    /// Systems that run only once when the app starts, after the [`Startup`](AppSet::Startup) set.
    PostStartup,
    /// Systems that run each time the app updates.
    Update,
    /// Systems that swap event buffers.
    UpdateEvents,
}

/// Systems providing basic app functionality.
#[doc(hidden)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemLabel)]
pub enum AppSystem {
    /// Runs the systems under the [`Startup`](`AppSet::Startup`) set.
    Startup,
    /// Calls [`apply_buffers`] after the systems under [`PreStartup`](AppSet::PreStartup) run.
    ApplyPreStartup,
    /// Calls [`apply_buffers`] after the systems under [`Startup`](AppSet::Startup) run.
    ApplyStartup,
    /// Calls [`apply_buffers`] after the systems under [`PostStartup`](AppSet::PostStartup) run.
    ApplyPostStartup,
    /// Clears the world's lists of entities with removed components.
    ClearTrackers,
}

/// Internal system sets needed to bypass limitations with [`apply_buffers`].
#[doc(hidden)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemLabel)]
pub(crate) enum AppInternalSet {
    /// Encompasses all startup sets and command application.
    Startup,
}

fn run_once(mut app: App) {
    app.update();
}

/// An event that signals the app to exit. This will close the app process.
#[derive(Debug, Clone, Default)]
pub struct AppExit;

impl Default for App {
    fn default() -> Self {
        let mut app = App::empty();

        #[cfg(feature = "bevy_reflect")]
        app.init_resource::<bevy_reflect::TypeRegistryArc>();
        app.add_event::<AppExit>();

        app.add_system(
            (|world: &mut World| {
                run_scheduled(AppInternalSet::Startup, world);
            })
            .named(AppSystem::Startup),
        );
        app.add_set(AppInternalSet::Startup);
        app.add_many(
            chain![
                AppSet::PreStartup,
                apply_buffers.named(AppSystem::ApplyPreStartup),
                AppSet::Startup,
                apply_buffers.named(AppSystem::ApplyStartup),
                AppSet::PostStartup,
                apply_buffers.named(AppSystem::ApplyPostStartup),
            ]
            .to(AppInternalSet::Startup),
        );
        app.add_set(AppSet::Update);
        app.add_set(AppSet::UpdateEvents.to(AppSet::Update));
        app.add_system(
            World::clear_trackers
                .named(AppSystem::ClearTrackers)
                .to(AppSet::Update)
                .after(AppSet::UpdateEvents),
        );

        #[cfg(feature = "bevy_ci_testing")]
        {
            crate::ci_testing::setup_app(&mut app);
        }

        app
    }
}

impl App {
    /// Constructs a new, empty [`App`] with default engine features enabled.
    ///
    /// This is the preferred constructor for most use cases.
    pub fn new() -> App {
        App::default()
    }

    /// Constructs a new, empty [`App`].
    ///
    /// This constructor should be used if you wish to provide custom scheduling, exit handling, cleanup, etc.
    pub fn empty() -> App {
        let mut world = World::default();
        world.init_resource::<SystemRegistry>();
        App {
            world,
            runner: Box::new(run_once),
            sub_apps: HashMap::default(),
        }
    }

    /// Runs the systems under the [`Update`](AppSet::Update) system set and also updates every [`SubApp`].
    pub fn update(&mut self) {
        #[cfg(feature = "trace")]
        let bevy_frame_update_span = info_span!("frame").entered();
        run_scheduled(AppSet::Update, &mut self.world);
        for sub_app in self.sub_apps.values_mut() {
            (sub_app.runner)(&mut self.world, &mut sub_app.app);
        }
    }

    /// Starts the application by calling its [runner function](Self::set_runner).
    pub fn run(&mut self) {
        #[cfg(feature = "trace")]
        let bevy_app_run_span = info_span!("bevy_app").entered();
        let mut app = std::mem::replace(self, App::empty());
        let runner = std::mem::replace(&mut app.runner, Box::new(run_once));
        (runner)(app);
    }

    /// Schedules a [`System`](bevy_ecs::system::System).
    pub fn add_system<P>(&mut self, system: impl IntoScheduledSystem<P>) -> &mut Self {
        let mut reg = self.world.resource_mut::<SystemRegistry>();
        reg.add_system(system);
        self
    }

    /// Schedules a [`System`](bevy_ecs::system::System) set.
    pub fn add_set(&mut self, set: impl IntoScheduledSet) -> &mut Self {
        let mut reg = self.world.resource_mut::<SystemRegistry>();
        reg.add_set(set);
        self
    }

    /// Schedules multiple systems and system sets.
    pub fn add_many(&mut self, nodes: impl IntoIterator<Item = Scheduled>) -> &mut Self {
        let mut reg = self.world.resource_mut::<SystemRegistry>();
        reg.add_many(nodes);
        self
    }

    /// Adds [`Events::<T>`](bevy_ecs::event::Events) as a resource in the world and schedules its
    /// [`update_system`](bevy_ecs::event::Events::update_system) to run under the
    /// [`UpdateEvents`](AppSet::UpdateEvents) system set.
    ///
    /// # Example
    ///
    /// ```
    /// # use bevy_app::prelude::*;
    /// # use bevy_ecs::prelude::*;
    /// #
    /// # struct MyEvent;
    /// # let mut app = App::new();
    /// #
    /// app.add_event::<MyEvent>();
    /// ```
    pub fn add_event<T>(&mut self) -> &mut Self
    where
        T: Resource,
    {
        self.init_resource::<Events<T>>()
            .add_system(Events::<T>::update_system.to(AppSet::UpdateEvents))
    }

    /// Adds the resource to the [`World`].
    ///
    /// If the resource already exists, this will overwrite its value.
    ///
    /// See [`init_resource`](Self::init_resource) for resources that implement [`Default`] or [`FromWorld`].
    ///
    /// ## Example
    /// ```
    /// # use bevy_app::prelude::*;
    /// #
    /// struct MyCounter {
    ///     counter: usize,
    /// }
    ///
    /// App::new()
    ///    .insert_resource(MyCounter { counter: 0 });
    /// ```
    pub fn insert_resource<R: Resource>(&mut self, resource: R) -> &mut Self {
        self.world.insert_resource(resource);
        self
    }

    /// Adds the non-[`Send`] resource to the [`World`].
    ///
    /// If the resource already exists, this will overwrite its value.
    ///
    /// See [`init_resource`](Self::init_resource) for resources that implement [`Default`] or [`FromWorld`].
    ///
    /// ## Example
    /// ```
    /// # use bevy_app::prelude::*;
    /// #
    /// struct MyCounter {
    ///     counter: usize,
    /// }
    ///
    /// App::new()
    ///     .insert_non_send_resource(MyCounter { counter: 0 });
    /// ```
    pub fn insert_non_send_resource<R: 'static>(&mut self, resource: R) -> &mut Self {
        self.world.insert_non_send_resource(resource);
        self
    }

    /// Adds the resource to the [`World`], initialized to its default value.
    ///
    /// If the resource already exists, nothing happens.
    ///
    /// The resource must implement the [`FromWorld`] trait.
    /// If the [`Default`] trait is implemented, the `FromWorld` trait will use
    /// the [`Default::default`] method to initialize the resource.
    ///
    /// ## Example
    /// ```
    /// # use bevy_app::prelude::*;
    /// #
    /// struct MyCounter {
    ///     counter: usize,
    /// }
    ///
    /// impl Default for MyCounter {
    ///     fn default() -> MyCounter {
    ///         MyCounter {
    ///             counter: 100
    ///         }
    ///     }
    /// }
    ///
    /// App::new()
    ///     .init_resource::<MyCounter>();
    /// ```
    pub fn init_resource<R: Resource + FromWorld>(&mut self) -> &mut Self {
        self.world.init_resource::<R>();
        self
    }

    /// Adds the non-[`Send`] resource to the [`World`], initialized to its default value.
    ///
    /// If the resource already exists, nothing happens.
    ///
    /// The resource must implement the [`FromWorld`] trait.
    /// If the [`Default`] trait is implemented, the `FromWorld` trait will use
    /// the [`Default::default`] method to initialize the resource.
    pub fn init_non_send_resource<R: 'static + FromWorld>(&mut self) -> &mut Self {
        self.world.init_non_send_resource::<R>();
        self
    }

    /// Sets the function that will be used to run the [`App`]. Called once by [`App::run`].
    ///
    /// Often set by internal plugins (e.g. [`WinitPlugin`][bevy_winit::WinitPlugin]).
    ///
    /// ## Example
    /// ```
    /// # use bevy_app::prelude::*;
    /// #
    /// fn my_runner(mut app: App) {
    ///     loop {
    ///         println!("in main loop");
    ///         app.update();
    ///     }
    /// }
    ///
    /// App::new().set_runner(my_runner);
    /// ```
    pub fn set_runner<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(App) + 'static,
    {
        self.runner = Box::new(f);
        self
    }

    /// Imports app configuration from a [`Plugin`].
    ///
    /// ## Example
    /// ```
    /// # use bevy_app::prelude::*;
    /// #
    /// App::new().add_plugin(bevy_log::LogPlugin::default());
    /// ```
    pub fn add_plugin<T>(&mut self, plugin: T) -> &mut Self
    where
        T: Plugin,
    {
        debug!("added plugin: {}", plugin.name());
        plugin.build(self);
        self
    }

    /// Imports app configuration from a [`PluginGroup`].
    ///
    /// ## Example
    /// ```
    /// # use bevy_app::{prelude::*, PluginGroupBuilder};
    /// #
    /// # // Dummy created to avoid using bevy_internal, which pulls in to many dependencies.
    /// # struct MinimalPlugins;
    /// # impl PluginGroup for MinimalPlugins {
    /// #     fn build(&mut self, group: &mut PluginGroupBuilder){;}
    /// # }
    /// #
    /// App::new()
    ///     .add_plugins(MinimalPlugins);
    /// ```
    pub fn add_plugins<T: PluginGroup>(&mut self, mut group: T) -> &mut Self {
        let mut plugin_group_builder = PluginGroupBuilder::default();
        group.build(&mut plugin_group_builder);
        plugin_group_builder.finish(self);
        self
    }

    /// Imports app configuration from a [`PluginGroup`], with some pre-processing function.
    ///
    /// This can be used to add more plugins to the plugin group, disable plugins, etc.
    ///
    /// ## Example
    /// ```
    /// # use bevy_app::{prelude::*, PluginGroupBuilder};
    /// #
    /// # // Dummies created to avoid using bevy_internal which pulls in to many dependencies.
    /// # struct DefaultPlugins;
    /// # impl PluginGroup for DefaultPlugins {
    /// #     fn build(&mut self, group: &mut PluginGroupBuilder){
    /// #         group.add(bevy_log::LogPlugin::default());
    /// #     }
    /// # }
    /// #
    /// # struct MyOwnPlugin;
    /// # impl Plugin for MyOwnPlugin {
    /// #     fn build(&self, app: &mut App){;}
    /// # }
    /// #
    /// App::new()
    ///      .add_plugins_with(DefaultPlugins, |group| {
    ///             group.add_before::<bevy_log::LogPlugin, _>(MyOwnPlugin)
    ///         });
    /// ```
    pub fn add_plugins_with<T, F>(&mut self, mut group: T, f: F) -> &mut Self
    where
        T: PluginGroup,
        F: FnOnce(&mut PluginGroupBuilder) -> &mut PluginGroupBuilder,
    {
        let mut plugin_group_builder = PluginGroupBuilder::default();
        group.build(&mut plugin_group_builder);
        f(&mut plugin_group_builder);
        plugin_group_builder.finish(self);
        self
    }

    /// Adds an [`App`] as a [`SubApp`] to the current one.
    ///
    /// The provided function, `runner`, takes mutable references to the main app's [`World`] and the
    /// sub-app and is called during the main app's [`update`](Self::update).
    pub fn add_sub_app(
        &mut self,
        label: impl AppLabel,
        app: App,
        runner: impl Fn(&mut World, &mut App) + 'static,
    ) -> &mut Self {
        self.sub_apps.insert(
            Box::new(label),
            SubApp {
                app,
                runner: Box::new(runner),
            },
        );
        self
    }

    /// Returns a reference to the sub-[`App`] with the given label.
    ///
    /// # Panics
    ///
    /// Panic if the sub-app does not exist.
    pub fn sub_app(&self, label: impl AppLabel) -> &App {
        match self.get_sub_app(label) {
            Ok(app) => app,
            Err(label) => panic!("Sub-App with label '{:?}' does not exist", label),
        }
    }

    /// Returns a mutable reference to the sub-[`App`] with the given label.
    ///
    /// # Panics
    ///
    /// Panic if the sub-app does not exist.
    pub fn sub_app_mut(&mut self, label: impl AppLabel) -> &mut App {
        match self.get_sub_app_mut(label) {
            Ok(app) => app,
            Err(label) => panic!("Sub-App with label '{:?}' does not exist", label),
        }
    }

    /// Returns a reference to the child [`App`] with the given label.
    ///
    /// # Errors
    ///
    /// Returns [`Err`] if the sub-app does not exist.
    pub fn get_sub_app(&self, label: impl AppLabel) -> Result<&App, impl AppLabel> {
        self.sub_apps
            .get((&label) as &dyn AppLabel)
            .map(|sub_app| &sub_app.app)
            .ok_or(label)
    }

    /// Returns a mutable reference to the child [`App`] with the given label.
    ///
    /// # Errors
    ///
    /// Returns [`Err`] if the sub-app does not exist.
    pub fn get_sub_app_mut(&mut self, label: impl AppLabel) -> Result<&mut App, impl AppLabel> {
        self.sub_apps
            .get_mut((&label) as &dyn AppLabel)
            .map(|sub_app| &mut sub_app.app)
            .ok_or(label)
    }

    /// Adds `T` to the type registry resource for runtime reflection.
    #[cfg(feature = "bevy_reflect")]
    pub fn register_type<T: bevy_reflect::GetTypeRegistration>(&mut self) -> &mut Self {
        let registry = self.world.resource_mut::<bevy_reflect::TypeRegistryArc>();
        registry.write().register::<T>();

        self
    }
}
