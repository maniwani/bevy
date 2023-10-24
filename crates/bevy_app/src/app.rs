use crate::{
    app_thread_channel, AppEvent, AppEventReceiver, AppEventSender, Main, MainSchedulePlugin,
    PlaceholderPlugin, Plugin, Plugins, PluginsState, SubApp, SubApps,
};
pub use bevy_derive::AppLabel;
use bevy_ecs::{
    prelude::*,
    schedule::{IntoSystemConfigs, IntoSystemSetConfigs, ScheduleBuildSettings, ScheduleLabel},
    storage::ThreadLocalStorage,
};

#[cfg(feature = "trace")]
use bevy_utils::tracing::info_span;
use bevy_utils::{intern::Interned, thiserror::Error, tracing::debug};
use std::fmt::Debug;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

bevy_utils::define_label!(
    /// A strongly-typed class of labels used to identify an [`App`].
    AppLabel,
    APP_LABEL_INTERNER
);

pub use bevy_utils::label::DynEq;

/// A shorthand for `Interned<dyn AppLabel>`.
pub type InternedAppLabel = Interned<dyn AppLabel>;

#[derive(Debug, Error)]
pub(crate) enum AppError {
    #[error("duplicate plugin {plugin_name:?}")]
    DuplicatePlugin { plugin_name: String },
}

#[allow(clippy::needless_doctest_main)]
/// [`App`] is the primary API for writing user applications. It automates the setup of a
/// [standard lifecycle](Main) and provides interface glue for [plugins](`Plugin`).
///
/// A single [`App`] can contain multiple [`SubApp`] instances, but [`App`] methods only affect
/// the "main" one. To access a particular [`SubApp`], use [`get_sub_app`](App::get_sub_app)
/// or [`get_sub_app_mut`](App::get_sub_app_mut).
///
///
/// # Examples
///
/// Here is a simple "Hello World" Bevy app:
///
/// ```
/// # use bevy_app::prelude::*;
/// # use bevy_ecs::prelude::*;
/// #
/// fn main() {
///    App::new()
///        .add_systems(Update, hello_world_system)
///        .run();
/// }
///
/// fn hello_world_system() {
///    println!("hello world");
/// }
/// ```
pub struct App {
    #[doc(hidden)]
    pub sub_apps: SubApps,
    #[doc(hidden)]
    pub tls: ThreadLocalStorage,
    send: AppEventSender,
    recv: AppEventReceiver,
    /// The function that will manage the app's lifecycle.
    ///
    /// Bevy provides the [`WinitPlugin`] and [`ScheduleRunnerPlugin`] for windowed and headless
    /// applications, respectively.
    ///
    /// [`WinitPlugin`]: https://docs.rs/bevy/latest/bevy/winit/struct.WinitPlugin.html
    /// [`ScheduleRunnerPlugin`]: https://docs.rs/bevy/latest/bevy/app/struct.ScheduleRunnerPlugin.html
    pub(crate) runner: Option<RunnerFn>,
}

impl Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "App {{ sub_apps: ")?;
        f.debug_map()
            .entries(self.sub_apps.sub_apps.iter().map(|(k, v)| (k, v)))
            .finish()?;
        write!(f, "}}")
    }
}

impl Default for App {
    fn default() -> Self {
        let mut app = App::empty();
        app.sub_apps.main.update_schedule = Some(Main.intern());

        #[cfg(feature = "bevy_reflect")]
        app.init_resource::<AppTypeRegistry>();
        app.add_plugins(MainSchedulePlugin);
        app.add_event::<AppExit>();

        #[cfg(feature = "bevy_ci_testing")]
        {
            crate::ci_testing::setup_app(&mut app);
        }

        app
    }
}

impl App {
    /// Creates a new [`App`] with some default structure to enable core engine features.
    /// This is the preferred constructor for most use cases.
    pub fn new() -> App {
        App::default()
    }

    /// Creates a new empty [`App`] with minimal default configuration.
    ///
    /// Use this constructor if you want to customize scheduling, exit handling, cleanup, etc.
    pub fn empty() -> App {
        let (send, recv) = app_thread_channel();
        Self {
            sub_apps: SubApps::new(),
            tls: ThreadLocalStorage::new(),
            send,
            recv,
            runner: Some(Box::new(run_once)),
        }
    }

    /// Disassembles the [`App`] and returns its individual parts.
    #[doc(hidden)]
    pub fn into_parts(
        self,
    ) -> (
        SubApps,
        ThreadLocalStorage,
        AppEventSender,
        AppEventReceiver,
        Option<RunnerFn>,
    ) {
        let Self {
            sub_apps,
            tls,
            send,
            recv,
            runner,
        } = self;

        (sub_apps, tls, send, recv, runner)
    }

    /// Returns an [`App`] assembled from the given individual parts.
    #[doc(hidden)]
    pub fn from_parts(
        sub_apps: SubApps,
        tls: ThreadLocalStorage,
        send: AppEventSender,
        recv: AppEventReceiver,
        runner: Option<RunnerFn>,
    ) -> Self {
        App {
            sub_apps,
            tls,
            send,
            recv,
            runner,
        }
    }

    /// Inserts the channel to [`ThreadLocals`] into all sub-apps.
    #[doc(hidden)]
    pub fn insert_tls_channel(&mut self) {
        self.sub_apps.iter_mut().for_each(|sub_app| {
            self.tls
                .insert_channel(sub_app.world_mut(), self.send.clone());
        });
    }

    /// Removes the channel to [`ThreadLocals`] from all sub-apps.
    #[doc(hidden)]
    pub fn remove_tls_channel(&mut self) {
        self.sub_apps
            .iter_mut()
            .for_each(|sub_app| self.tls.remove_channel(sub_app.world_mut()));
    }

    /// Runs the default schedules of all sub-apps (starting with the "main" app) once.
    pub fn update(&mut self) {
        if self.is_building_plugins() {
            panic!("App::update() was called while a plugin was building.");
        }

        self.insert_tls_channel();

        // disassemble
        let (mut sub_apps, tls, send, recv, runner) = std::mem::take(self).into_parts();

        #[cfg(not(target_arch = "wasm32"))]
        {
            // Move sub-apps to another thread and run an event loop in this thread.
            let thread_send = send.clone();
            let thread = std::thread::spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    sub_apps.update();
                    thread_send.send(AppEvent::Exit(sub_apps)).unwrap();
                }));

                if let Some(payload) = result.err() {
                    thread_send.send(AppEvent::Error(payload)).unwrap();
                }
            });

            loop {
                // this loop never exits because multiple copies of sender exist
                let event = recv.recv().unwrap();
                match event {
                    AppEvent::Task(task) => {
                        task();
                    }
                    AppEvent::Exit(x) => {
                        sub_apps = x;
                        thread.join().unwrap();
                        break;
                    }
                    AppEvent::Error(payload) => {
                        resume_unwind(payload);
                    }
                }
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            sub_apps.update();
        }

        // reassemble
        *self = App::from_parts(sub_apps, tls, send, recv, runner);

        self.remove_tls_channel();
    }

    /// Runs the [`App`] by calling its [runner](Self::set_runner).
    ///
    /// This will (re)build the [`App`] first. For general usage, see the example on the item
    /// level documentation.
    ///
    /// # Caveats
    ///
    /// **This method is not required to return.**
    ///
    /// Headless apps can generally expect this method to return control to the caller when
    /// it completes, but that is not the case for windowed apps. Windowed apps are typically
    /// driven by an event loop and some platforms expect the program to terminate when the
    /// event loop ends.
    ///
    /// Bevy uses `winit` as its default window manager. See [`WinitSettings::return_from_run`]
    /// for a mechanism (with many additional caveats) that will guarantee [`App::run()`] returns
    /// to the caller.
    ///
    /// # Panics
    ///
    /// Panics if not all plugins have been built.
    ///
    /// [`WinitSettings::return_from_run`]: https://docs.rs/bevy/latest/bevy/winit/struct.WinitSettings.html#structfield.return_from_run
    pub fn run(&mut self) {
        #[cfg(feature = "trace")]
        let _bevy_app_run_span = info_span!("App::run").entered();
        if self.is_building_plugins() {
            panic!("App::run() was called while a plugin was building.");
        }

        // Insert channel here because some sub-apps are moved to a different thread during
        // plugin build.
        self.insert_tls_channel();

        if self.plugins_state() == PluginsState::Ready {
            // If we're already ready, we finish up now and advance one frame.
            // This prevents black frames during the launch transition on iOS.
            self.finish();
            self.cleanup();
            self.update();
        }

        let mut app = std::mem::replace(self, App::empty());
        let runner = app.runner.take().expect("an app runner");
        (runner)(app);
    }

    /// Sets the function that will be called when the app is run.
    ///
    /// The runner function `f` is called only once by [`App::run`]. If the
    /// presence of a main loop in the app is desired, it is the responsibility of the runner
    /// function to provide it.
    ///
    /// The runner function is usually not set manually, but by Bevy integrated plugins
    /// (e.g. `WinitPlugin`).
    ///
    /// # Examples
    ///
    /// ```
    /// # use bevy_app::prelude::*;
    /// #
    /// fn my_runner(mut app: App) {
    ///     loop {
    ///         println!("In main loop");
    ///         app.update();
    ///     }
    /// }
    ///
    /// App::new()
    ///     .set_runner(my_runner);
    /// ```
    pub fn set_runner(&mut self, f: impl FnOnce(App) + 'static) -> &mut Self {
        self.runner = Some(Box::new(f));
        self
    }

    /// Returns the state of all plugins. This is usually called by the event loop, but can be
    /// useful for situations where you want to use [`App::update`].
    // TODO: &mut self -> &self
    #[inline]
    pub fn plugins_state(&mut self) -> PluginsState {
        let mut overall_plugins_state = match self.main_mut().plugins_state {
            PluginsState::Adding => {
                let mut state = PluginsState::Ready;
                let plugins = std::mem::take(&mut self.main_mut().plugins);
                for plugin in &plugins.registry {
                    // plugins installed to main need to see all sub-apps
                    if !plugin.ready(self) {
                        state = PluginsState::Adding;
                        break;
                    }
                }
                self.main_mut().plugins = plugins;
                state
            }
            state => state,
        };

        // overall state is the earliest state of any sub-app
        self.sub_apps.iter_mut().skip(1).for_each(|s| {
            overall_plugins_state = overall_plugins_state.min(s.plugins_state());
        });

        overall_plugins_state
    }

    /// Runs [`Plugin::finish`] for each plugin. This is usually called by the event loop once all
    /// plugins are ready, but can be useful for situations where you want to use [`App::update`].
    pub fn finish(&mut self) {
        // plugins installed to main should see all sub-apps
        let plugins = std::mem::take(&mut self.main_mut().plugins);
        for plugin in &plugins.registry {
            plugin.finish(self);
        }
        let main = self.main_mut();
        main.plugins = plugins;
        main.plugins_state = PluginsState::Finished;
        self.sub_apps.iter_mut().skip(1).for_each(|s| s.finish());
    }

    /// Runs [`Plugin::cleanup`] for each plugin. This is usually called by the event loop after
    /// [`App::finish`], but can be useful for situations where you want to use [`App::update`].
    pub fn cleanup(&mut self) {
        // plugins installed to main should see all sub-apps
        let plugins = std::mem::take(&mut self.main_mut().plugins);
        for plugin in &plugins.registry {
            plugin.cleanup(self);
        }
        let main = self.main_mut();
        main.plugins = plugins;
        main.plugins_state = PluginsState::Cleaned;
        self.sub_apps.iter_mut().skip(1).for_each(|s| s.cleanup());
    }

    /// Returns `true` if any of the sub-apps are building plugins.
    pub(crate) fn is_building_plugins(&self) -> bool {
        self.sub_apps.iter().any(|s| s.is_building_plugins())
    }

    /// Adds [`State<S>`] and [`NextState<S>`] resources, [`OnEnter`] and [`OnExit`] schedules
    /// for each state variant (if they don't already exist), an instance of [`apply_state_transition::<S>`]
    /// in [`StateTransition`] so that transitions happen before [`Update`] and an instance of
    /// [`run_enter_schedule::<S>`] in [`StateTransition`] with a [`run_once`] condition to
    /// run the on enter schedule of the initial state.
    ///
    /// If you would like to control how other systems run based on the current state,
    /// you can emulate this behavior using the [`in_state`] [`Condition`].
    ///
    /// Note that you can also apply state transitions at other points in the schedule
    /// by adding the [`apply_state_transition`] system manually.
    ///
    /// [`StateTransition`]: crate::StateTransition
    /// [`Update`]: crate::Update
    /// [`run_once`]: bevy_ecs::schedule::common_conditions::run_once
    /// [`run_enter_schedule::<S>`]: bevy_ecs::schedule::run_enter_schedule
    pub fn add_state<S: States>(&mut self) -> &mut Self {
        self.main_mut().add_state::<S>();
        self
    }

    /// Adds a collection of systems to `schedule` (stored in the main world's [`Schedules`]).
    ///
    /// # Examples
    ///
    /// ```
    /// # use bevy_app::prelude::*;
    /// # use bevy_ecs::prelude::*;
    /// #
    /// # let mut app = App::new();
    /// # fn system_a() {}
    /// # fn system_b() {}
    /// # fn system_c() {}
    /// # fn should_run() -> bool { true }
    /// #
    /// app.add_systems(Update, (system_a, system_b, system_c));
    /// app.add_systems(Update, (system_a, system_b).run_if(should_run));
    /// ```
    pub fn add_systems<M>(
        &mut self,
        schedule: impl ScheduleLabel,
        systems: impl IntoSystemConfigs<M>,
    ) -> &mut Self {
        self.main_mut().add_systems(schedule, systems);
        self
    }

    /// Configures a system set in the default schedule, adding the set if it does not exist.
    #[deprecated(since = "0.12.0", note = "Please use `configure_sets` instead.")]
    #[track_caller]
    pub fn configure_set(
        &mut self,
        schedule: impl ScheduleLabel,
        set: impl IntoSystemSetConfigs,
    ) -> &mut Self {
        self.configure_sets(schedule, set)
    }

    /// Configures a collection of system sets in the default schedule, adding any sets that do not exist.
    #[track_caller]
    pub fn configure_sets(
        &mut self,
        schedule: impl ScheduleLabel,
        sets: impl IntoSystemSetConfigs,
    ) -> &mut Self {
        self.main_mut().configure_sets(schedule, sets);
        self
    }

    /// Initializes `T` event handling by inserting an event queue resource ([`Events::<T>`])
    /// and scheduling an [`event_update_system`] in [`First`](crate::First).
    ///
    /// See [`Events`] for information on how to define events.
    ///
    /// # Examples
    ///
    /// ```
    /// # use bevy_app::prelude::*;
    /// # use bevy_ecs::prelude::*;
    /// #
    /// # #[derive(Event)]
    /// # struct MyEvent;
    /// # let mut app = App::new();
    /// #
    /// app.add_event::<MyEvent>();
    /// ```
    ///
    /// [`event_update_system`]: bevy_ecs::event::event_update_system
    pub fn add_event<T>(&mut self) -> &mut Self
    where
        T: Event,
    {
        self.main_mut().add_event::<T>();
        self
    }

    /// Inserts the [`Resource`] into the app, overwriting any existing resource of the same type.
    ///
    /// There is also an [`init_resource`](Self::init_resource) for resources that have
    /// [`Default`] or [`FromWorld`] implementations.
    ///
    /// # Examples
    ///
    /// ```
    /// # use bevy_app::prelude::*;
    /// # use bevy_ecs::prelude::*;
    /// #
    /// #[derive(Resource)]
    /// struct MyCounter {
    ///     counter: usize,
    /// }
    ///
    /// App::new()
    ///    .insert_resource(MyCounter { counter: 0 });
    /// ```
    pub fn insert_resource<R: Resource>(&mut self, resource: R) -> &mut Self {
        self.main_mut().insert_resource(resource);
        self
    }

    /// Inserts the [`Resource`], initialized with its default value, into the app,
    /// if there is no existing instance of `R`.
    ///
    /// `R` must implement [`FromWorld`].
    /// If `R` implements [`Default`], [`FromWorld`] will be automatically implemented and
    /// initialize the [`Resource`] with [`Default::default`].
    ///
    /// # Examples
    ///
    /// ```
    /// # use bevy_app::prelude::*;
    /// # use bevy_ecs::prelude::*;
    /// #
    /// #[derive(Resource)]
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
        self.main_mut().init_resource::<R>();
        self
    }

    /// Inserts the [`!Send`](Send) resource into the app, overwriting any existing resource
    /// of the same type.
    ///
    /// There is also an [`init_non_send_resource`](Self::init_non_send_resource) for
    /// resources that implement [`Default`].
    ///
    /// # Examples
    ///
    /// ```
    /// # use bevy_app::prelude::*;
    /// # use bevy_ecs::prelude::*;
    /// #
    /// #[derive(ThreadLocalResource)]
    /// struct MyCounter {
    ///     counter: usize,
    /// }
    ///
    /// App::new()
    ///     .insert_non_send_resource(MyCounter { counter: 0 });
    /// ```
    pub fn insert_non_send_resource<R: ThreadLocalResource>(&mut self, resource: R) -> &mut Self {
        self.tls.insert_resource(resource);
        self
    }

    /// Inserts the [`!Send`](Send) resource into the app, initialized with its default value,
    /// if there is no existing instance of `R`.
    pub fn init_non_send_resource<R: ThreadLocalResource + Default>(&mut self) -> &mut Self {
        self.tls.init_resource::<R>();
        self
    }

    pub(crate) fn add_boxed_plugin(
        &mut self,
        plugin: Box<dyn Plugin>,
    ) -> Result<&mut Self, AppError> {
        debug!("added plugin: {}", plugin.name());
        if plugin.is_unique()
            && !self
                .main_mut()
                .plugins
                .names
                .insert(plugin.name().to_string())
        {
            Err(AppError::DuplicatePlugin {
                plugin_name: plugin.name().to_string(),
            })?;
        }

        // Reserve position in the plugin registry. If the plugin adds more plugins,
        // they'll all end up in insertion order.
        let index = self.main().plugins.registry.len();
        self.main_mut()
            .plugins
            .registry
            .push(Box::new(PlaceholderPlugin));

        self.main_mut().plugin_build_depth += 1;
        let result = catch_unwind(AssertUnwindSafe(|| plugin.build(self)));
        self.main_mut().plugin_build_depth -= 1;

        if let Err(payload) = result {
            resume_unwind(payload);
        }

        self.main_mut().plugins.registry[index] = plugin;
        Ok(self)
    }

    /// Returns `true` if the [`Plugin`] has already been added.
    pub fn is_plugin_added<T>(&self) -> bool
    where
        T: Plugin,
    {
        self.main().is_plugin_added::<T>()
    }

    /// Returns a vector of references to all plugins of type `T` that have been added.
    ///
    /// This can be used to read the settings of any existing plugins.
    /// This vector will be empty if no plugins of that type have been added.
    /// If multiple copies of the same plugin are added to the [`App`], they will be listed in insertion order in this vector.
    ///
    /// ```rust
    /// # use bevy_app::prelude::*;
    /// # #[derive(Default)]
    /// # struct ImagePlugin {
    /// #    default_sampler: bool,
    /// # }
    /// # impl Plugin for ImagePlugin {
    /// #    fn build(&self, app: &mut App) {}
    /// # }
    /// # let mut app = App::new();
    /// # app.add_plugins(ImagePlugin::default());
    /// let default_sampler = app.get_added_plugins::<ImagePlugin>()[0].default_sampler;
    /// ```
    pub fn get_added_plugins<T>(&self) -> Vec<&T>
    where
        T: Plugin,
    {
        self.main().get_added_plugins::<T>()
    }

    /// Installs a [`Plugin`] collection.
    ///
    /// Bevy prioritizes modularity as a core principle. **All** engine features are implemented
    /// as plugins, even the complex ones like rendering.
    ///
    /// [`Plugin`]s can be grouped into a set by using a [`PluginGroup`].
    ///
    /// There are built-in [`PluginGroup`]s that provide core engine functionality.
    /// The [`PluginGroup`]s available by default are `DefaultPlugins` and `MinimalPlugins`.
    ///
    /// To customize the plugins in the group (reorder, disable a plugin, add a new plugin
    /// before / after another plugin), call [`build()`](super::PluginGroup::build) on the group,
    /// which will convert it to a [`PluginGroupBuilder`](crate::PluginGroupBuilder).
    ///
    /// You can also specify a group of [`Plugin`]s by using a tuple over [`Plugin`]s and
    /// [`PluginGroup`]s. See [`Plugins`] for more details.
    ///
    /// ## Examples
    /// ```
    /// # use bevy_app::{prelude::*, PluginGroupBuilder, NoopPluginGroup as MinimalPlugins};
    /// #
    /// # // Dummies created to avoid using `bevy_log`,
    /// # // which pulls in too many dependencies and breaks rust-analyzer
    /// # pub struct LogPlugin;
    /// # impl Plugin for LogPlugin {
    /// #     fn build(&self, app: &mut App) {}
    /// # }
    /// App::new()
    ///     .add_plugins(MinimalPlugins);
    /// App::new()
    ///     .add_plugins((MinimalPlugins, LogPlugin));
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if one of the plugins had already been added to the application.
    ///
    /// [`PluginGroup`]:super::PluginGroup
    #[track_caller]
    pub fn add_plugins<M>(&mut self, plugins: impl Plugins<M>) -> &mut Self {
        if matches!(
            self.plugins_state(),
            PluginsState::Cleaned | PluginsState::Finished
        ) {
            panic!(
                "Plugins cannot be added after App::cleanup() or App::finish() has been called."
            );
        }
        plugins.add_to_app(self);
        self
    }

    /// Registers the type `T` in the [`TypeRegistry`](bevy_reflect::TypeRegistry) resource,
    /// adding reflect data as specified in the [`Reflect`](bevy_reflect::Reflect) derive:
    ///
    /// ```rust,ignore
    /// #[derive(Reflect)]
    /// #[reflect(Component, Serialize, Deserialize)] // will register ReflectComponent, ReflectSerialize, ReflectDeserialize
    /// ```
    ///
    /// See [`bevy_reflect::TypeRegistry::register`] for more information.
    #[cfg(feature = "bevy_reflect")]
    pub fn register_type<T: bevy_reflect::GetTypeRegistration>(&mut self) -> &mut Self {
        self.main_mut().register_type::<T>();
        self
    }

    /// Associates type data `D` with type `T` in the [`TypeRegistry`](bevy_reflect::TypeRegistry) resource.
    ///
    /// Most of the time [`register_type`](Self::register_type) can be used instead to register a
    /// type you derived [`Reflect`](bevy_reflect::Reflect) for. However, in cases where you want to
    /// add a piece of type data that was not included in the list of `#[reflect(...)]` type data in
    /// the derive, or where the type is generic and cannot register e.g. `ReflectSerialize`
    /// unconditionally without knowing the specific type parameters, this method can be used to
    /// insert additional type data.
    ///
    /// # Example
    /// ```rust
    /// use bevy_app::App;
    /// use bevy_reflect::{ReflectSerialize, ReflectDeserialize};
    ///
    /// App::new()
    ///     .register_type::<Option<String>>()
    ///     .register_type_data::<Option<String>, ReflectSerialize>()
    ///     .register_type_data::<Option<String>, ReflectDeserialize>();
    /// ```
    ///
    /// See [`bevy_reflect::TypeRegistry::register_type_data`].
    #[cfg(feature = "bevy_reflect")]
    pub fn register_type_data<
        T: bevy_reflect::Reflect + bevy_reflect::TypePath,
        D: bevy_reflect::TypeData + bevy_reflect::FromType<T>,
    >(
        &mut self,
    ) -> &mut Self {
        self.main_mut().register_type_data::<T, D>();
        self
    }

    /// Returns a reference to the [`World`].
    pub fn world(&self) -> &World {
        self.main().world()
    }

    /// Returns a mutable reference to the [`World`].
    pub fn world_mut(&mut self) -> &mut World {
        self.main_mut().world_mut()
    }

    /// Returns a reference to the main [`SubApp`].
    pub fn main(&self) -> &SubApp {
        &self.sub_apps.main
    }

    /// Returns a mutable reference to the main [`SubApp`].
    pub fn main_mut(&mut self) -> &mut SubApp {
        &mut self.sub_apps.main
    }

    /// Returns a reference to the [`SubApp`] with the given label.
    ///
    /// # Panics
    ///
    /// Panics if the sub-app doesn't exist.
    pub fn sub_app(&self, label: impl AppLabel) -> &SubApp {
        let str = label.intern();
        self.get_sub_app(label).unwrap_or_else(|| {
            panic!("No sub-app with label '{:?}' exists.", str);
        })
    }

    /// Returns a reference to the [`SubApp`] with the given label.
    ///
    /// # Panics
    ///
    /// Panics if the a sub-app doesn't exist.
    pub fn sub_app_mut(&mut self, label: impl AppLabel) -> &mut SubApp {
        let str = label.intern();
        self.get_sub_app_mut(label).unwrap_or_else(|| {
            panic!("No sub-app with label '{:?}' exists.", str);
        })
    }

    /// Returns a reference to the [`SubApp`] with the given label, if it exists.
    pub fn get_sub_app(&self, label: impl AppLabel) -> Option<&SubApp> {
        self.sub_apps.sub_apps.get(&label.intern())
    }

    /// Returns a mutable reference to the [`SubApp`] with the given label, if it exists.
    pub fn get_sub_app_mut(&mut self, label: impl AppLabel) -> Option<&mut SubApp> {
        self.sub_apps.sub_apps.get_mut(&label.intern())
    }

    /// Inserts a [`SubApp`] with the given label.
    pub fn insert_sub_app(&mut self, label: impl AppLabel, sub_app: SubApp) {
        self.sub_apps.sub_apps.insert(label.intern(), sub_app);
    }

    /// Removes the [`SubApp`] with the given label, if it exists.
    pub fn remove_sub_app(&mut self, label: impl AppLabel) -> Option<SubApp> {
        self.sub_apps.sub_apps.remove(&label.intern())
    }

    /// Inserts a new `schedule` under the provided `label`, overwriting any existing
    /// schedule with the same label.
    pub fn add_schedule(&mut self, schedule: Schedule) -> &mut Self {
        self.main_mut().add_schedule(schedule);
        self
    }

    /// Initializes an empty `schedule` under the provided `label`, if it does not exist.
    ///
    /// See [`add_schedule`](Self::add_schedule) to insert an existing schedule.
    pub fn init_schedule(&mut self, label: impl ScheduleLabel) -> &mut Self {
        self.main_mut().init_schedule(label);
        self
    }

    /// Returns a reference to the [`Schedule`] with the provided `label` if it exists.
    pub fn get_schedule(&self, label: impl ScheduleLabel) -> Option<&Schedule> {
        self.main().get_schedule(label)
    }

    /// Returns a mutable reference to the [`Schedule`] with the provided `label` if it exists.
    pub fn get_schedule_mut(&mut self, label: impl ScheduleLabel) -> Option<&mut Schedule> {
        self.main_mut().get_schedule_mut(label)
    }

    /// Runs function `f` with the [`Schedule`] associated with `label`.
    ///
    /// **Note:** This will create the schedule if it does not already exist.
    pub fn edit_schedule(
        &mut self,
        label: impl ScheduleLabel,
        f: impl FnMut(&mut Schedule),
    ) -> &mut Self {
        self.main_mut().edit_schedule(label, f);
        self
    }

    /// Applies the provided [`ScheduleBuildSettings`] to all schedules.
    pub fn configure_schedules(
        &mut self,
        schedule_build_settings: ScheduleBuildSettings,
    ) -> &mut Self {
        self.world_mut()
            .resource_mut::<Schedules>()
            .configure_schedules(schedule_build_settings);
        self
    }

    /// When doing [ambiguity checking](bevy_ecs::schedule::ScheduleBuildSettings) this
    /// ignores systems that are ambiguous on [`Component`] T.
    ///
    /// This settings only applies to the main world. To apply this to other worlds call the
    /// [corresponding method](World::allow_ambiguous_component) on World
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use bevy_app::prelude::*;
    /// # use bevy_ecs::prelude::*;
    /// # use bevy_ecs::schedule::{LogLevel, ScheduleBuildSettings};
    /// # use bevy_utils::default;
    ///
    /// #[derive(Component)]
    /// struct A;
    ///
    /// // these systems are ambiguous on A
    /// fn system_1(_: Query<&mut A>) {}
    /// fn system_2(_: Query<&A>) {}
    ///
    /// let mut app = App::new();
    /// app.configure_schedules(ScheduleBuildSettings {
    ///   ambiguity_detection: LogLevel::Error,
    ///   ..default()
    /// });
    ///
    /// app.add_systems(Update, ( system_1, system_2 ));
    /// app.allow_ambiguous_component::<A>();
    ///
    /// // running the app does not error.
    /// app.update();
    /// ```
    pub fn allow_ambiguous_component<T: Component>(&mut self) -> &mut Self {
        self.world_mut().allow_ambiguous_component::<T>();
        self
    }

    /// When doing [ambiguity checking](bevy_ecs::schedule::ScheduleBuildSettings) this
    /// ignores systems that are ambiguous on [`Resource`] T.
    ///
    /// This settings only applies to the main world. To apply this to other worlds call the
    /// [corresponding method](World::allow_ambiguous_resource) on World
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use bevy_app::prelude::*;
    /// # use bevy_ecs::prelude::*;
    /// # use bevy_ecs::schedule::{LogLevel, ScheduleBuildSettings};
    /// # use bevy_utils::default;
    ///
    /// #[derive(Resource)]
    /// struct R;
    ///
    /// // these systems are ambiguous on R
    /// fn system_1(_: ResMut<R>) {}
    /// fn system_2(_: Res<R>) {}
    ///
    /// let mut app = App::new();
    /// app.configure_schedules(ScheduleBuildSettings {
    ///   ambiguity_detection: LogLevel::Error,
    ///   ..default()
    /// });
    /// app.insert_resource(R);
    ///
    /// app.add_systems(Update, ( system_1, system_2 ));
    /// app.allow_ambiguous_resource::<R>();
    ///
    /// // running the app does not error.
    /// app.update();
    /// ```
    pub fn allow_ambiguous_resource<T: Resource>(&mut self) -> &mut Self {
        self.world_mut().allow_ambiguous_resource::<T>();
        self
    }
}

type RunnerFn = Box<dyn FnOnce(App)>;

fn run_once(mut app: App) {
    // wait for plugins to finish setting up
    let plugins_state = app.plugins_state();
    if plugins_state != PluginsState::Cleaned {
        while app.plugins_state() == PluginsState::Adding {
            #[cfg(not(target_arch = "wasm32"))]
            bevy_tasks::tick_global_task_pools_on_main_thread();
        }
        app.finish();
        app.cleanup();
    }

    // If plugins where cleaned before the runner start, an update already ran
    if plugins_state == PluginsState::Cleaned {
        return;
    }

    // disassemble
    let (mut sub_apps, mut tls, send, recv, _) = app.into_parts();

    #[cfg(not(target_arch = "wasm32"))]
    {
        // Move sub-apps to another thread and run an event loop in this thread.
        let thread = std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                sub_apps.update();
                send.send(AppEvent::Exit(sub_apps)).unwrap();
            }));

            if let Some(payload) = result.err() {
                send.send(AppEvent::Error(payload)).unwrap();
            }
        });

        loop {
            let event = recv.recv().unwrap();
            match event {
                AppEvent::Task(task) => {
                    task();
                }
                AppEvent::Exit(_) => {
                    thread.join().unwrap();
                    break;
                }
                AppEvent::Error(payload) => {
                    resume_unwind(payload);
                }
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        sub_apps.update();
    }

    tls.clear();
}

/// An event that indicates the [`App`]  should exit. If one or more of these are present at the
/// end of an update, the [runner](App::set_runner) will end and ([maybe](App::run)) return
/// control to the caller.
///
/// This event can be used to detect when an exit is requested. Make sure that systems listening
/// for this event run before the current update ends.
#[derive(Event, Debug, Clone, Default)]
pub struct AppExit;

#[cfg(test)]
mod tests {
    use std::marker::PhantomData;

    use bevy_ecs::{
        schedule::{OnEnter, States},
        system::Commands,
    };

    use crate::{App, Plugin};

    struct PluginA;
    impl Plugin for PluginA {
        fn build(&self, _app: &mut crate::App) {}
    }
    struct PluginB;
    impl Plugin for PluginB {
        fn build(&self, _app: &mut crate::App) {}
    }
    struct PluginC<T>(T);
    impl<T: Send + Sync + 'static> Plugin for PluginC<T> {
        fn build(&self, _app: &mut crate::App) {}
    }
    struct PluginD;
    impl Plugin for PluginD {
        fn build(&self, _app: &mut crate::App) {}
        fn is_unique(&self) -> bool {
            false
        }
    }

    #[test]
    fn can_add_two_plugins() {
        App::new().add_plugins((PluginA, PluginB));
    }

    #[test]
    #[should_panic]
    fn cant_add_twice_the_same_plugin() {
        App::new().add_plugins((PluginA, PluginA));
    }

    #[test]
    fn can_add_twice_the_same_plugin_with_different_type_param() {
        App::new().add_plugins((PluginC(0), PluginC(true)));
    }

    #[test]
    fn can_add_twice_the_same_plugin_not_unique() {
        App::new().add_plugins((PluginD, PluginD));
    }

    #[test]
    #[should_panic]
    fn cant_call_app_run_from_plugin_build() {
        struct PluginRun;
        struct InnerPlugin;
        impl Plugin for InnerPlugin {
            fn build(&self, _: &mut crate::App) {}
        }
        impl Plugin for PluginRun {
            fn build(&self, app: &mut crate::App) {
                app.add_plugins(InnerPlugin).run();
            }
        }
        App::new().add_plugins(PluginRun);
    }

    #[derive(States, PartialEq, Eq, Debug, Default, Hash, Clone)]
    enum AppState {
        #[default]
        MainMenu,
    }
    fn bar(mut commands: Commands) {
        commands.spawn_empty();
    }

    fn foo(mut commands: Commands) {
        commands.spawn_empty();
    }

    #[test]
    fn add_systems_should_create_schedule_if_it_does_not_exist() {
        let mut app = App::new();
        app.add_state::<AppState>()
            .add_systems(OnEnter(AppState::MainMenu), (foo, bar));

        app.world_mut().run_schedule(OnEnter(AppState::MainMenu));
        assert_eq!(app.world().entities().len(), 2);
    }

    #[test]
    fn add_systems_should_create_schedule_if_it_does_not_exist2() {
        let mut app = App::new();
        app.add_systems(OnEnter(AppState::MainMenu), (foo, bar))
            .add_state::<AppState>();

        app.world_mut().run_schedule(OnEnter(AppState::MainMenu));
        assert_eq!(app.world().entities().len(), 2);
    }

    #[test]
    fn test_derive_app_label() {
        use super::AppLabel;
        use crate::{self as bevy_app};

        #[derive(AppLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
        struct UnitLabel;

        #[derive(AppLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
        struct TupleLabel(u32, u32);

        #[derive(AppLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
        struct StructLabel {
            a: u32,
            b: u32,
        }

        #[derive(AppLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
        struct EmptyTupleLabel();

        #[derive(AppLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
        struct EmptyStructLabel {}

        #[derive(AppLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
        enum EnumLabel {
            #[default]
            Unit,
            Tuple(u32, u32),
            Struct {
                a: u32,
                b: u32,
            },
        }

        #[derive(AppLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
        struct GenericLabel<T>(PhantomData<T>);

        assert_eq!(UnitLabel.intern(), UnitLabel.intern());
        assert_eq!(EnumLabel::Unit.intern(), EnumLabel::Unit.intern());
        assert_ne!(UnitLabel.intern(), EnumLabel::Unit.intern());
        assert_ne!(UnitLabel.intern(), TupleLabel(0, 0).intern());
        assert_ne!(EnumLabel::Unit.intern(), EnumLabel::Tuple(0, 0).intern());

        assert_eq!(TupleLabel(0, 0).intern(), TupleLabel(0, 0).intern());
        assert_eq!(
            EnumLabel::Tuple(0, 0).intern(),
            EnumLabel::Tuple(0, 0).intern()
        );
        assert_ne!(TupleLabel(0, 0).intern(), TupleLabel(0, 1).intern());
        assert_ne!(
            EnumLabel::Tuple(0, 0).intern(),
            EnumLabel::Tuple(0, 1).intern()
        );
        assert_ne!(TupleLabel(0, 0).intern(), EnumLabel::Tuple(0, 0).intern());
        assert_ne!(
            TupleLabel(0, 0).intern(),
            StructLabel { a: 0, b: 0 }.intern()
        );
        assert_ne!(
            EnumLabel::Tuple(0, 0).intern(),
            EnumLabel::Struct { a: 0, b: 0 }.intern()
        );

        assert_eq!(
            StructLabel { a: 0, b: 0 }.intern(),
            StructLabel { a: 0, b: 0 }.intern()
        );
        assert_eq!(
            EnumLabel::Struct { a: 0, b: 0 }.intern(),
            EnumLabel::Struct { a: 0, b: 0 }.intern()
        );
        assert_ne!(
            StructLabel { a: 0, b: 0 }.intern(),
            StructLabel { a: 0, b: 1 }.intern()
        );
        assert_ne!(
            EnumLabel::Struct { a: 0, b: 0 }.intern(),
            EnumLabel::Struct { a: 0, b: 1 }.intern()
        );
        assert_ne!(
            StructLabel { a: 0, b: 0 }.intern(),
            EnumLabel::Struct { a: 0, b: 0 }.intern()
        );
        assert_ne!(
            StructLabel { a: 0, b: 0 }.intern(),
            EnumLabel::Struct { a: 0, b: 0 }.intern()
        );
        assert_ne!(StructLabel { a: 0, b: 0 }.intern(), UnitLabel.intern(),);
        assert_ne!(
            EnumLabel::Struct { a: 0, b: 0 }.intern(),
            EnumLabel::Unit.intern()
        );

        assert_eq!(
            GenericLabel::<u32>(PhantomData).intern(),
            GenericLabel::<u32>(PhantomData).intern()
        );
        assert_ne!(
            GenericLabel::<u32>(PhantomData).intern(),
            GenericLabel::<u64>(PhantomData).intern()
        );
    }
}
