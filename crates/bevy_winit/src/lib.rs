#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![forbid(unsafe_code)]
#![doc(
    html_logo_url = "https://bevyengine.org/assets/icon.png",
    html_favicon_url = "https://bevyengine.org/assets/icon.png"
)]

//! `bevy_winit` provides utilities to handle window creation and the eventloop through [`winit`]
//!
//! The [`WinitPlugin`] is one of the [`DefaultPlugins`]. It registers an [`App`]
//! runner that manages the [`App`] using an [`EventLoop`](winit::event_loop::EventLoop).
//!
//! [`DefaultPlugins`]: https://docs.rs/bevy/latest/bevy/struct.DefaultPlugins.html

pub mod accessibility;
mod converters;
mod runner;
mod system;
mod winit_config;
pub mod winit_event;
mod winit_windows;

use approx::relative_eq;
use bevy_a11y::AccessibilityRequested;
use bevy_utils::Instant;
pub use system::create_windows;
use system::{changed_windows, despawn_windows, CachedWindow};
use winit::dpi::{LogicalSize, PhysicalSize};
pub use winit_config::*;
pub use winit_event::*;
pub use winit_windows::*;

use winit::event_loop::EventLoopBuilder;
#[cfg(target_os = "android")]
pub use winit::platform::android::activity as android_activity;

#[cfg(not(target_arch = "wasm32"))]
use bevy_app::First;
use bevy_app::{App, AppEvent, Last, Plugin};
use bevy_derive::{Deref, DerefMut};
// use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use bevy_input::{
    keyboard::KeyboardInput,
    mouse::{MouseButtonInput, MouseMotion, MouseWheel},
    touch::TouchInput,
    touchpad::{TouchpadMagnify, TouchpadRotate},
};
use bevy_utils::{tracing::warn, Instant};
use bevy_window::{
    exit_on_all_closed, ApplicationLifetime, CursorEntered, CursorLeft, CursorMoved,
    FileDragAndDrop, Ime, ReceivedCharacter, WindowBackendScaleFactorChanged, WindowCloseRequested,
    WindowDestroyed, WindowFocused, WindowMoved, WindowResized, WindowScaleFactorChanged,
    WindowThemeChanged,
};

/// [`AndroidApp`] provides an interface to query the application state as well as monitor events
/// (for example lifecycle and input events).
#[cfg(target_os = "android")]
pub static ANDROID_APP: std::sync::OnceLock<android_activity::AndroidApp> =
    std::sync::OnceLock::new();

/// A [`Plugin`] that uses `winit` to create and manage windows, and receive window and input
/// events.
///
/// This plugin will add systems and resources that sync with the `winit` backend and also replace
/// the exising [`App`] runner with one that constructs an [event loop] to receive window and input
/// events from the OS.
///
/// [event loop]: winit::event_loop::EventLoop
#[derive(Default)]
pub struct WinitPlugin {
    /// Allows the event loop to be created on any thread instead of only the main thread.
    ///
    /// See [`EventLoopBuilder::build`] for more information on this.
    ///
    /// # Supported platforms
    ///
    /// Only works on Linux (X11/Wayland) and Windows. This field is ignored on other platforms.
    pub run_on_any_thread: bool,
}

impl Plugin for WinitPlugin {
    fn build(&self, app: &mut App) {
        // setup event loop
        let mut event_loop_builder = EventLoopBuilder::<AppEvent>::with_user_event();

        // linux check is needed because x11 might be enabled on other platforms.
        #[cfg(all(target_os = "linux", feature = "x11"))]
        {
            use winit::platform::x11::EventLoopBuilderExtX11;

            // This allows a Bevy app to be started and ran outside of the main thread.
            // A use case for this is to allow external applications to spawn a thread
            // which runs a Bevy app without requiring the Bevy app to need to reside on
            // the main thread, which can be problematic.
            event_loop_builder.with_any_thread(self.run_on_any_thread);
        }

        // linux check is needed because wayland might be enabled on other platforms.
        #[cfg(all(target_os = "linux", feature = "wayland"))]
        {
            use winit::platform::wayland::EventLoopBuilderExtWayland;
            event_loop_builder.with_any_thread(self.run_on_any_thread);
        }

        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            event_loop_builder.with_any_thread(self.run_on_any_thread);
        }

        #[cfg(target_os = "android")]
        {
            use winit::platform::android::EventLoopBuilderExtAndroid;
            let msg = "Bevy must be setup with the #[bevy_main] macro on Android";
            event_loop_builder.with_android_app(ANDROID_APP.get().expect(msg).clone());
        }

        let event_loop = crate::EventLoop::new(event_loop_builder.build());

        #[cfg(not(target_arch = "wasm32"))]
        app.init_resource::<WinitWindowEntityMap>();

        // setup app
        app.init_non_send_resource::<WinitWindows>()
            .init_resource::<WinitSettings>()
            .add_event::<WinitEvent>()
            .set_runner(winit_runner)
            .add_systems(
                Last,
                (
                    // `exit_on_all_closed` seemingly conflicts with `changed_windows`
                    // but does not actually access any data that would alias (only metadata)
                    changed_windows.ambiguous_with(exit_on_all_closed),
                    despawn_windows,
                    create_windows::<AppEvent>,
                )
                    // apply all changes before despawning windows for consistent event ordering
                    .chain(),
            );

        // TODO: schedule after TimeSystem
        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(First, flush_winit_events::<AppEvent>);

        app.add_plugins(AccessKitPlugin);

        let event_loop = event_loop_builder
            .build()
            .expect("Failed to build event loop");

        // iOS, macOS, and Android don't like it if you create windows before the event loop is
        // initialized.
        //
        // See:
        // - https://github.com/rust-windowing/winit/blob/master/README.md#macos
        // - https://github.com/rust-windowing/winit/blob/master/README.md#ios
        #[cfg(not(any(target_os = "android", target_os = "ios", target_os = "macos")))]
        {
            // TODO: rework app setup
            #[cfg(not(target_arch = "wasm32"))]
            let sender = crate::EventLoopProxy::new(event_loop.create_proxy());
            #[cfg(target_arch = "wasm32")]
            let sender = crate::NoopSender;

            let world = app.sub_apps.main.world_mut();

            app.tls.insert_channel(world, sender);

            app.tls
                .insert_resource(crate::EventLoopWindowTarget::new(&event_loop));

            // Otherwise, we want to create a window before `bevy_render` initializes the renderer
            // so that we have a surface to use as a hint. This improves compatibility with `wgpu`
            // backends, especially WASM/WebGL2.
            let mut create_windows = IntoSystem::into_system(create_windows::<AppEvent>);
            create_windows.initialize(world);
            create_windows.run((), world);

            app.tls
                .remove_resource::<crate::EventLoopWindowTarget<AppEvent>>();

            app.tls.remove_channel(world);
        }

        app.insert_non_send_resource(event_loop);
    }
}

#[derive(SystemParam)]
struct WindowAndInputEventWriters<'w> {
    // `winit` `WindowEvent`s
    window_resized: EventWriter<'w, WindowResized>,
    window_close_requested: EventWriter<'w, WindowCloseRequested>,
    window_scale_factor_changed: EventWriter<'w, WindowScaleFactorChanged>,
    window_backend_scale_factor_changed: EventWriter<'w, WindowBackendScaleFactorChanged>,
    window_focused: EventWriter<'w, WindowFocused>,
    window_moved: EventWriter<'w, WindowMoved>,
    window_theme_changed: EventWriter<'w, WindowThemeChanged>,
    window_destroyed: EventWriter<'w, WindowDestroyed>,
    lifetime: EventWriter<'w, ApplicationLifetime>,
    keyboard_input: EventWriter<'w, KeyboardInput>,
    character_input: EventWriter<'w, ReceivedCharacter>,
    mouse_button_input: EventWriter<'w, MouseButtonInput>,
    touchpad_magnify_input: EventWriter<'w, TouchpadMagnify>,
    touchpad_rotate_input: EventWriter<'w, TouchpadRotate>,
    mouse_wheel_input: EventWriter<'w, MouseWheel>,
    touch_input: EventWriter<'w, TouchInput>,
    ime_input: EventWriter<'w, Ime>,
    file_drag_and_drop: EventWriter<'w, FileDragAndDrop>,
    cursor_moved: EventWriter<'w, CursorMoved>,
    cursor_entered: EventWriter<'w, CursorEntered>,
    cursor_left: EventWriter<'w, CursorLeft>,
    // `winit` `DeviceEvent`s
    mouse_motion: EventWriter<'w, MouseMotion>,
}

/// Persistent state that is used to run the [`App`] according to the current [`UpdateMode`].
struct WinitAppRunnerState {
    /// Current active state of the app.
    active: ActiveState,
    /// Is `true` if active state just went from `NotYetStarted` to `Active`.
    just_started: bool,
    /// Is `true` if a new [`WindowEvent`](winit::event::WindowEvent) has been received since the
    /// last update.
    window_event_received: bool,
    /// Is `true` if a new [`DeviceEvent`](winit::event::DeviceEvent) has been received since the
    /// last update.
    #[cfg(not(target_arch = "wasm32"))]
    device_event_received: bool,
    /// Is `true` if the app has requested a redraw since the last update.
    redraw_requested: bool,
    /// Is `true` if enough time has elapsed since `last_update`.
    wait_elapsed: bool,
    /// The time the most recent update started.
    last_update: Instant,
    /// Number of "forced" updates to trigger on application start
    startup_forced_updates: u32,
}

impl WinitAppRunnerState {
    fn reset_on_update(&mut self) {
        self.redraw_requested = false;
        self.window_event_received = false;
        self.device_event_received = false;
        self.wait_elapsed = false;
    }
}

#[derive(PartialEq, Eq)]
pub(crate) enum ActiveState {
    NotYetStarted,
    Active,
    Suspended,
    WillSuspend,
}

impl ActiveState {
    #[inline]
    pub(crate) fn should_run(&self) -> bool {
        match self {
            ActiveState::NotYetStarted | ActiveState::Suspended => false,
            ActiveState::Active | ActiveState::WillSuspend => true,
        }
    }
}

impl Default for WinitAppRunnerState {
    fn default() -> Self {
        Self {
            active: ActiveState::NotYetStarted,
            just_started: false,
            window_event_received: false,
            #[cfg(not(target_arch = "wasm32"))]
            device_event_received: false,
            redraw_requested: false,
            wait_elapsed: false,
            last_update: Instant::now(),
            // 3 seems to be enough, 5 is a safe margin
            startup_forced_updates: 5,
        }
    }
}

#[derive(ThreadLocalResource, Deref, DerefMut)]
pub(crate) struct EventLoop<T: 'static>(winit::event_loop::EventLoop<T>);

impl<T> EventLoop<T> {
    pub fn new(value: winit::event_loop::EventLoop<T>) -> Self {
        Self(value)
    }

    /// Returns a reference to the [`EventLoopWindowTarget`].
    ///
    /// # Safety
    ///
    /// The target pointer must be valid. That means the [`EventLoop`] used to create the target
    /// must still exist and must not have moved.
    ///
    /// [`EventLoopWindowTarget`]: winit::event_loop::EventLoopWindowTarget
    /// [`EventLoop`]: winit::event_loop::EventLoop
    pub unsafe fn get(&self) -> &'_ winit::event_loop::EventLoopWindowTarget<T> {
        &*self.ptr
    }
}
