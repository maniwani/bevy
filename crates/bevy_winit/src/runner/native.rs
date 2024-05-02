use std::borrow::BorrowMut;
use std::collections::VecDeque;
use std::mem;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::mpsc::{
    channel, Receiver, RecvError, RecvTimeoutError, SendError, Sender, TryRecvError,
};

use bevy_a11y::AccessibilityRequested;
use bevy_app::{App, AppEvent, AppExit, PluginsState, SubApps};
use bevy_derive::{Deref, DerefMut};
#[cfg(target_os = "android")]
use bevy_ecs::system::SystemParam;
use bevy_ecs::{
    event::ManualEventReader,
    prelude::*,
    storage::{ThreadLocalTask, ThreadLocalTaskSendError, ThreadLocalTaskSender},
    system::SystemState,
};
use bevy_input::{
    mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel},
    touchpad::{TouchpadMagnify, TouchpadRotate},
};
use bevy_math::{ivec2, DVec2, Vec2};
use bevy_tasks::tick_global_task_pools_on_main_thread;
use bevy_utils::{
    default,
    synccell::SyncCell,
    tracing::{trace, warn},
    Duration, Instant,
};
use bevy_window::{
    ApplicationLifetime, CursorEntered, CursorLeft, CursorMoved, FileDragAndDrop, Ime,
    ReceivedCharacter, RequestRedraw, Window, WindowBackendScaleFactorChanged,
    WindowCloseRequested, WindowDestroyed, WindowFocused, WindowMoved, WindowResized,
    WindowScaleFactorChanged, WindowThemeChanged,
};
#[cfg(target_os = "android")]
use bevy_window::{PrimaryWindow, RawHandleWrapper};
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event_loop::ControlFlow;

use crate::{
    accessibility::{AccessKitAdapters, WinitActionHandlers},
    converters::{self, convert_event, convert_window_event},
    run, run_return,
    system::CachedWindow,
    ActiveState, UpdateMode, WindowAndInputEventWriters, WinitAppRunnerState, WinitSettings,
    WinitWindowEntityMap, WinitWindows,
};

/// [`EventLoopProxy`](winit::event_loop::EventLoopProxy) wrapped in a [`SyncCell`].
/// Allows systems to wake the [`winit`] event loop from any thread.
#[derive(Resource, Deref, DerefMut)]
pub struct EventLoopProxy<T: 'static>(pub SyncCell<winit::event_loop::EventLoopProxy<T>>);

impl<T> EventLoopProxy<T> {
    pub(crate) fn new(value: winit::event_loop::EventLoopProxy<T>) -> Self {
        Self(SyncCell::new(value))
    }
}

/// Sending half of an [`Event`] channel.
pub struct WinitEventSender<T: Send + 'static> {
    pub(crate) event_send: Sender<winit::event::Event<T>>,
    pub(crate) clear_send: Sender<u64>,
    pub(crate) last_event_sent: u64,
}

/// Receiving half of an [`Event`] channel.
#[derive(Resource)]
pub struct WinitEventReceiver<T: Send + 'static> {
    pub(crate) event_recv: SyncCell<Receiver<winit::event::Event<T>>>,
    pub(crate) clear_recv: SyncCell<Receiver<u64>>,
    pub(crate) processed: SyncCell<VecDeque<winit::event::Event<T>>>,
    pub(crate) last_event_processed: u64,
    pub(crate) state: WinitAppRunnerState,
}

/// Constructs a new [`WinitEventSender`] and [`WinitEventReceiver`] channel pair.
pub fn winit_channel<T: Send + 'static>() -> (WinitEventSender<T>, WinitEventReceiver<T>) {
    let (clear_send, clear_recv) = channel();
    let (event_send, event_recv) = channel();
    let processed = VecDeque::new();

    let sender = WinitEventSender {
        clear_send,
        event_send,
        last_event_sent: 0,
    };

    let receiver = WinitEventReceiver {
        event_recv: SyncCell::new(event_recv),
        clear_recv: SyncCell::new(clear_recv),
        processed: SyncCell::new(processed),
        last_event_processed: 0,
        state: default(),
    };

    (sender, receiver)
}

impl<T> WinitEventSender<T>
where
    T: Send + 'static,
{
    pub(crate) fn send(
        &mut self,
        event: winit::event::Event<T>,
    ) -> Result<(), SendError<winit::event::Event<T>>> {
        self.last_event_sent = self.last_event_sent.checked_add(1).unwrap();
        self.event_send.send(event)
    }

    /// Informs the receiver that there is a new batch of events to be read.
    pub(crate) fn send_clear(
        &mut self,
        event: winit::event::Event<T>,
    ) -> Result<(), SendError<winit::event::Event<T>>> {
        assert!(matches!(event, winit::event::Event::AboutToWait));
        self.send(event)?;
        self.clear_send.send(self.last_event_sent).unwrap();
        Ok(())
    }
}

impl<T> WinitEventReceiver<T>
where
    T: Send + 'static,
{
    fn process_event(&mut self, event: winit::event::Event<T>) {
        match &event {
            winit::event::Event::WindowEvent { event, .. } => {
                self.state.window_event_received = true;
                if matches!(event, winit::event::WindowEvent::RedrawRequested) {
                    self.state.redraw_requested = true;
                }
            }
            winit::event::Event::DeviceEvent { .. } => {
                self.state.device_event_received = true;
            }
            winit::event::Event::Suspended => {
                self.state.active = ActiveState::WillSuspend;
            }
            winit::event::Event::Resumed => {
                if self.state.active == ActiveState::NotYetStarted {
                    self.state.just_started = true;
                } else {
                    self.state.just_started = false;
                }

                self.state.active = ActiveState::Active;
            }
            _ => (),
        }
        self.last_event_processed = self.last_event_processed.checked_add(1).unwrap();
        self.processed.get().push_back(event);
    }

    fn process_events_until(&mut self, clear_event: u64) {
        while self.last_event_processed < clear_event {
            let event = self.event_recv.get().try_recv().unwrap();
            self.process_event(event);
        }
    }

    pub(crate) fn recv(&mut self) -> Result<(), RecvError> {
        let rx = self.clear_recv.get();
        let mut event = rx.recv()?;
        while let Ok(n) = rx.try_recv() {
            assert!(n > event);
            event = n;
        }
        self.process_events_until(event);
        Ok(())
    }

    pub(crate) fn try_recv(&mut self) -> Result<(), TryRecvError> {
        let rx = self.clear_recv.get();
        let mut event = rx.try_recv()?;
        while let Ok(n) = rx.try_recv() {
            assert!(n > event);
            event = n;
        }
        self.process_events_until(event);
        Ok(())
    }

    pub(crate) fn recv_timeout(&mut self, timeout: Duration) -> Result<(), RecvTimeoutError> {
        let rx = self.clear_recv.get();
        let mut event = rx.recv_timeout(timeout)?;
        while let Ok(n) = rx.try_recv() {
            assert!(n > event);
            event = n;
        }
        self.process_events_until(event);
        Ok(())
    }
}

#[cfg(target_os = "android")]
#[derive(SystemParam)]
struct ExtraAndroidParams<'w, 's> {
    resume:
        Query<'w, 's, Entity, (With<Window>, With<CachedWindow>, Without<RawHandleWrapper>)>,
    suspend: Query<'w, 's, Entity, With<PrimaryWindow>>,
    handlers: ResMut<'w, WinitActionHandlers>,
    accessibility_requested: Res<'w, AccessibilityRequested>,
    main_thread: ThreadLocal<'w, 's>,
    commands: Commands<'w, 's>,
}

pub(crate) fn flush_winit_events<T: Send + 'static>(
    mut queue: ResMut<WinitEventReceiver<T>>,
    mut windows: Query<(&mut Window, &mut CachedWindow)>,
    mut event_writers: WindowAndInputEventWriters,
    #[cfg(not(target_os = "android"))] map: Res<WinitWindowEntityMap>,
    #[cfg(target_os = "android")] mut map: ResMut<WinitWindowEntityMap>,
    #[cfg(target_os = "android")] mut extra: ExtraAndroidParams,
) {
    // TODO: Should this use a system local instead?
    let just_started = queue.state.just_started;

    for event in queue.processed.get().drain(..) {
        match event {
            winit::event::Event::WindowEvent {
                window_id, event, ..
            } => {
                let Some(window) = map.get_window_entity(window_id) else {
                    warn!(
                        "Skipped event {:?} for unknown winit Window Id {:?}",
                        event, window_id
                    );
                    continue;
                };

                let Ok((mut win, mut cache)) = windows.get_mut(window) else {
                    warn!(
                        "Window {:?} is missing `Window` component, skipping event {:?}",
                        win, event
                    );
                    continue;
                };

                match event {
                    winit::event::WindowEvent::Resized(size) => {
                        win
                            .resolution
                            .set_physical_resolution(size.width, size.height);

                        winit_events.send(WindowResized {
                            window,
                            width: win.width(),
                            height: win.height(),
                        });
                    }
                    winit::event::WindowEvent::CloseRequested => {
                        winit_events.send(WindowCloseRequested {
                                window,
                            });
                    }
                    winit::event::WindowEvent::KeyboardInput { ref event, .. } => {
                        winit_events.send(converters::convert_keyboard_input(event, window));
                    }
                    winit::event::WindowEvent::CursorMoved { position, .. } => {
                        let physical_position = DVec2::new(position.x, position.y);
                        let last_position = win.physical_cursor_position();
                        let delta = last_position.map(|last_pos| {
                            (physical_position.as_vec2() - last_pos) / win.resolution.scale_factor()
                        });
                        win.set_physical_cursor_position(Some(physical_position));
                        let position =
                            (physical_position / win.resolution.scale_factor() as f64).as_vec2();
                        winit_events.send(CursorMoved {
                            window,
                            position,
                            delta,
                        });
                    }
                    winit::event::WindowEvent::CursorEntered { .. } => {
                        winit_events.send(CursorEntered { window });
                    }
                    winit::event::WindowEvent::CursorLeft { .. } => {
                        window.set_physical_cursor_position(None);
                        winit_events.send(CursorLeft { window });
                    }
                    winit::event::WindowEvent::MouseInput { state, button, .. } => {
                        winit_events.send(MouseButtonInput {
                            button: converters::convert_mouse_button(button),
                            state: converters::convert_element_state(state),
                            window,
                        });
                    }
                    winit::event::WindowEvent::TouchpadMagnify { delta, .. } => {
                        winit_events.send(TouchpadMagnify(delta as f32));
                    }
                    winit::event::WindowEvent::TouchpadRotate { delta, .. } => {
                        winit_events.send(TouchpadRotate(delta));
                    }
                    winit::event::WindowEvent::MouseWheel { delta, .. } => match delta {
                        winit::event::MouseScrollDelta::LineDelta(x, y) => {
                            winit_events.send(MouseWheel {
                                unit: MouseScrollUnit::Line,
                                x,
                                y,
                                window,
                            });
                        }
                        winit::event::MouseScrollDelta::PixelDelta(p) => {
                            winit_events.send(MouseWheel {
                                unit: MouseScrollUnit::Pixel,
                                x: p.x as f32,
                                y: p.y as f32,
                                window,
                            });
                        }
                    },
                    winit::event::WindowEvent::Touch(touch) => {
                        let location =
                            touch.location.to_logical(window.resolution.scale_factor() as f64);
                        winit_events.send(converters::convert_touch_input(touch, location, window));
                    }
                    winit::event::WindowEvent::ScaleFactorChanged {
                        scale_factor,
                        ..,
                    } => {
                        let prior_factor = win.resolution.scale_factor();
                        win.resolution.set_scale_factor(scale_factor);
                        // This may not match the new scale factor if an override is active.
                        let scale_factor = win.resolution.scale_factor();
                        let scale_factor_override = win.resolution.scale_factor_override();

                        let mut new_inner_size =
                            PhysicalSize::new(win.physical_width(), win.physical_height());

                        if let Some(forced_factor) = scale_factor_override {
                            // This window is overriding the OS-suggested DPI, so its physical
                            // size should be set based on the overriding value. Its logical
                            // size already incorporates any resize constraints.
                            let maybe_new_inner_size = 
                                LogicalSize::new(win.width(), win.height())
                                    .to_physical::<u32>(forced_factor as f64);

                            // TODO
                        }
                        
                        win.resolution.set_physical_resolution(
                            new_inner_size.width,
                            new_inner_size.height,
                        );

                        let new_logical_width = new_inner_size.width as f32 / new_factor;
                        let new_logical_height = new_inner_size.height as f32 / new_factor;

                        let width_changed = relative_ne!(win.width(), new_logical_width);
                        let height_changed = relative_ne!(win.height(), new_logical_height);
                        let scale_factor_changed = relative_ne!(scale_factor, prior_factor);
                        
                        winit_events.send(WindowBackendScaleFactorChanged {
                            window,
                            scale_factor,
                        });

                        if scale_factor_override.is_none() && scale_factor_changed {
                            winit_events.send(WindowScaleFactorChanged {
                                window,
                                scale_factor,
                            });
                        }

                        if width_changed || height_changed {
                            winit_events.send(WindowResized {
                                window,
                                width: new_logical_width,
                                height: new_logical_height,
                            });
                        }
                    }
                    winit::event::WindowEvent::Focused(focused) => {
                        window.focused = focused;
                        winit_events.send(WindowFocused {
                            window,
                            focused,
                        });
                    }
                    winit::event::WindowEvent::DroppedFile(path_buf) => {
                        winit_events.send(FileDragAndDrop::DroppedFile {
                            window,
                            path_buf,
                        });
                    }
                    winit::event::WindowEvent::HoveredFile(path_buf) => {
                        winit_events.send(FileDragAndDrop::HoveredFile {
                            window,
                            path_buf,
                        });
                    }
                    winit::event::WindowEvent::HoveredFileCancelled => {
                        winit_events.send(FileDragAndDrop::HoveredFileCanceled {
                            window,
                        });
                    }
                    winit::event::WindowEvent::Moved(position) => {
                        let position = ivec2(position.x, position.y);
                        window.position.set(position);
                        winit_events.send(WindowMoved {
                            window,
                            position,
                        });
                    }
                    winit::event::WindowEvent::Ime(event) => match event {
                        winit::event::Ime::Preedit(value, cursor) => {
                            winit_events.send(Ime::Preedit {
                                window,
                                value,
                                cursor,
                            });
                        }
                        winit::event::Ime::Commit(value) => {
                            winit_events.send(Ime::Commit { window, value });
                        }
                        winit::event::Ime::Enabled => {
                            winit_events.send(Ime::Enabled { window });
                        }
                        winit::event::Ime::Disabled => {
                            winit_events.send(Ime::Disabled { window });
                        }
                    },
                    winit::event::WindowEvent::ThemeChanged(theme) => {
                        winit_events.send(WindowThemeChanged {
                            window,
                            theme: converters::convert_winit_theme(theme),
                        });
                    }
                    winit::event::WindowEvent::Destroyed => {
                        winit_events.send(WindowDestroyed {
                            window,
                        });
                    }
                    _ => {}
                }

                if window.is_changed() {
                    cache.window = window.clone();
                }
            }
            winit::event::Event::DeviceEvent { event, .. } => {
                match event {
                    winit::event::DeviceEvent::MouseMotion { delta: (x, y) } => {
                        winit_events.send(MouseMotion {
                            delta: Vec2::new(x as f32, y as f32),
                        });                    
                    }
                    _ => {}
                }
            }
            winit::event::Event::Suspended => {
                winit_events.send(ApplicationLifetime::Suspended);
                #[cfg(target_os = "android")]
                {
                    // Android sending this event invalidates the existing window, so remove and
                    // drop its handle.
                    if let Ok(entity) = extra.suspend.get_single() {
                        extra.commands.entity(entity).remove::<RawHandleWrapper>();
                    }
                }
            }
            winit::event::Event::Resumed => {
                if just_started {
                    winit_events.send(ApplicationLifetime::Started);
                } else {
                    winit_events.send(ApplicationLifetime::Resumed);
                }

                #[cfg(target_os = "android")]
                {
                    if let Ok(entity) = extra.resume.get_single() {
                        use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};

                        // Re-create the window on the backend and link it to its entity.
                        let (window, _) = windows.get(entity).unwrap();

                        let raw_handle_wrapper = extra.main_thread.run(|tls| {
                            tls.resource_scope(|tls, mut winit_windows: Mut<WinitWindows>| {
                                tls.resource_scope(|tls, mut adapters: Mut<AccessKitAdapters>| {
                                    // SAFETY: `bevy_winit` guarantees that this resource can
                                    // only be inserted by its `App` runner and that the stored
                                    // pointer is valid.
                                    let event_loop = unsafe {
                                        tls.resource::<crate::EventLoopWindowTarget<AppEvent>>()
                                            .get()
                                    };

                                    let winit_window = winit_windows.create_window(
                                        event_loop,
                                        entity,
                                        window,
                                        &mut map,
                                        &mut adapters,
                                        &mut extra.handlers,
                                        &extra.accessibility_requested,
                                    );

                                    RawHandleWrapper {
                                        window_handle: winit_window
                                            .window_handle()
                                            .unwrap()
                                            .as_raw(),
                                        display_handle: winit_window
                                            .display_handle()
                                            .unwrap()
                                            .as_raw(),
                                    }
                                })
                            })
                        });

                        // Re-insert the component.
                        extra.commands.entity(entity).insert(raw_handle_wrapper);
                    }
                }
            }
            _ => (),
        }
    }
}

enum ControlFlow {
    Poll,
    Wait,
    WaitUntil(Instant),
    Exit,
}


pub(crate) fn spawn_app_thread(
    mut sub_apps: SubApps,
    send: bevy_app::AppEventSender,
    recv: bevy_app::AppEventReceiver,
    event_loop_proxy: winit::event_loop::EventLoopProxy<AppEvent>,
) {
    std::thread::Builder::new()
        .name("event-loop-proxy".to_string())
        .spawn(move || {
            while let Ok(event) = recv.recv() {
                // got something from the app, forward it to the event loop
                if event_loop_proxy.send_event(event).is_err() {
                    break;
                }
            }
        })
        .unwrap();

    std::thread::Builder::new()
        .name("app".to_string())
        .spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let mut app_exit_event_reader = ManualEventReader::<AppExit>::default();
                let mut redraw_event_reader = ManualEventReader::<RequestRedraw>::default();
                let mut focused_windows_state: SystemState<(
                    Res<WinitSettings>,
                    Query<&Window>,
                )> = SystemState::from_world(sub_apps.main.world_mut());

                let mut rx = sub_apps
                    .main
                    .world_mut()
                    .remove_resource::<WinitEventReceiver<AppEvent>>()
                    .unwrap();

                #[cfg(any(target_os = "android", target_os = "ios", target_os = "macos"))]
                {
                    let mut create_windows =
                        IntoSystem::into_system(crate::system::create_windows::<AppEvent>);
                    create_windows.initialize(sub_apps.main.world_mut());
                    create_windows.run((), sub_apps.main.world_mut());
                }

                let mut control_flow = ControlFlow::Poll;
                loop {
                    let now = Instant::now();
                    match control_flow {
                        ControlFlow::Poll => match rx.try_recv() {
                            Ok(_) | Err(TryRecvError::Empty) => {}
                            Err(TryRecvError::Disconnected) => {
                                trace!("terminating app because event loop disconnected");
                                return;
                            }
                        },
                        ControlFlow::Wait => match rx.recv() {
                            Ok(_) => {}
                            Err(_) => {
                                trace!("terminating app because event loop disconnected");
                                return;
                            }
                        },
                        ControlFlow::WaitUntil(t) => {
                            let timeout = t.checked_duration_since(now).unwrap_or(Duration::ZERO);
                            rx.state.wait_elapsed = timeout.is_zero();
                            match rx.recv_timeout(timeout) {
                                Ok(_) | Err(RecvTimeoutError::Timeout) => {}
                                Err(RecvTimeoutError::Disconnected) => {
                                    trace!("terminating app because event loop disconnected");
                                    return;
                                }
                            }
                        }
                        ControlFlow::Exit => {
                            trace!("exiting app");
                            trace!("returning app to main thread");
                            if send.send(AppEvent::Exit(sub_apps)).is_err() {
                                warn!("failed to return app to main thread");
                            }
                            return;
                        }
                    }

                    let should_update = check_if_app_should_update(
                        &mut rx.state,
                        &mut sub_apps,
                        &mut control_flow,
                    );

                    // TODO
                    if should_update {
                        rx.state.reset_on_update();
                        rx.state.last_update = now;

                        sub_apps.main.world_mut().insert_resource(rx);
                        sub_apps.update();
                        rx = sub_apps
                            .main
                            .world_mut()
                            .remove_resource::<WinitEventReceiver<AppEvent>>()
                            .unwrap();

                        check_when_app_should_update_next(
                            &mut rx.state,
                            &mut sub_apps,
                            &mut control_flow,
                        );
                    }
                }
            }));

            if let Some(payload) = result.err() {
                send.send(AppEvent::Error(payload)).unwrap();
            }
        })
        .unwrap();
}

/// The default [`App::runner`] for the [`WinitPlugin`](super::WinitPlugin).
pub(crate) fn winit_runner(mut app: App) {
    let mut event_loop = app
        .tls
        .remove_resource::<crate::EventLoop<AppEvent>>()
        .unwrap()
        .into_inner();
    
    // create proxy (so events from the app can wake the event loop)
    let event_loop_proxy = event_loop.create_proxy();

    let mut app_exit_event_reader = ManualEventReader::<AppExit>::default();

    let (mut winit_send, winit_recv) = winit_channel::<AppEvent>();
    let mut winit_recv = Some(winit_recv);
    let mut locals = None;
    let mut should_start = false;

    let event_cb = move |event: winit::event::Event<AppEvent>,
                         event_loop: &winit::event_loop::EventLoopWindowTarget<AppEvent>,
                         control_flow: &mut ControlFlow| {
        #[cfg(feature = "trace")]
        let _span = bevy_utils::tracing::info_span!("winit event_handler").entered();

        // TODO: in future, have runner start before building plugins
        if app.plugins_state() != PluginsState::Cleaned {
            if app.plugins_state() != PluginsState::Ready {
                tick_global_task_pools_on_main_thread();
                // Loop until the plugins are ready.
                event_loop.set_control_flow(ControlFlow::Poll);
            } else {
                app.finish();
                app.cleanup();
                should_start = true;
                // Now that the plugins are all set, wait for events to process.
                event_loop.set_control_flow(ControlFlow::Wait);
            }

            if let Some(app_exit_events) = app.world().get_resource::<Events<AppExit>>() {
                if app_exit_event_reader.read(app_exit_events).last().is_some() {
                    event_loop.exit();
                    return;
                }
            }
        }

        if mem::take(&mut should_start) {
            // split app
            let (mut sub_apps, tls, send, recv, _) = mem::take(&mut app).into_parts();
            locals = Some(tls);

            // insert winit -> app channel
            let winit_recv = winit_recv.take().unwrap();
            sub_apps.main.world_mut().insert_resource(winit_recv);

            // send sub-apps to separate thread
            spawn_app_thread(sub_apps, send, recv, event_loop_proxy.clone());
        }

        match &mut event {
            winit::event::Event::WindowEvent { window_id, event } => {
                // Let AccessKit process the event before it reaches the engine.
                if let Some(tls) = locals.as_mut() {
                    tls.resource_scope(|tls, winit_windows: Mut<WinitWindows>| {
                        tls.resource_scope(|tls, access_kit_adapters: Mut<AccessKitAdapters>| {
                            if let Some(entity) = winit_windows.get_window(window_id) {
                                if let Some(winit_win) = winit_windows.get_window(entity) {
                                    if let Some(adapter) = access_kit_adapters.get(&entity) {
                                        adapter.process_event(winit_win, &event)
                                    }
                                }
                            };
                        });
                    });
                }

                if let winit::event::WindowEvent::ScaleFactorChanged {
                    scale_factor,
                    inner_size_writer,
                } = event {
                    // This event requires special handling because writes to the inner size must
                    // happen here. It can't be written asynchronously.
                    if let Some(tls) = locals.as_mut() {
                        tls.resource_scope(|tls, winit_windows: Mut<WinitWindows>| {
                            if let Some(win) = winit_windows.cached_windows.get(&window_id) {
                                let mut new_inner_size =
                                    PhysicalSize::new(win.physical_width(), win.physical_height());

                                if let Some(forced_factor) =
                                    win.resolution.scale_factor_override()
                                {
                                    // This window is overriding the OS-suggested DPI, so its
                                    // physical size should be set based on the override. Its
                                    // logical size already incorporates any resize constraints.
                                    let maybe_new_inner_size = LogicalSize::new(
                                            win.width(),
                                            win.height(),
                                        )
                                        .to_physical::<u32>(forced_factor);

                                    match inner_size_writer.request_inner_size(new_inner_size) {
                                        Ok(_) => new_inner_size = maybe_new_inner_size,
                                        Err(err) => warn!("winit failed to resize window: {err}"),
                                    }
                                }
                            }
                        });
                    }
                }
            }
            _ => {}
        }

        match event {
            winit::event::Event::UserEvent(event) => {
                match event {
                    AppEvent::Task(task) => {
                        let tls = locals.as_mut().unwrap();
                        tls.insert_resource(crate::EventLoopWindowTarget::new(event_loop));
                        {
                            #[cfg(feature = "trace")]
                            let _span = bevy_utils::tracing::info_span!("TLS access").entered();
                            task();
                        }
                        tls.remove_resource::<crate::EventLoopWindowTarget<AppEvent>>();
                    }
                    AppEvent::Exit(_) => {
                        event_loop.exit();
                    }
                    AppEvent::Error(payload) => {
                        resume_unwind(payload);
                    }
                }
            }
            winit::event::Event::AboutToWait => {
                let _ = winit_send.send_clear(event);
            }
            _ => {
                let _ = winit_send.send(event);
            }
        }
    };
}

fn check_if_app_should_update(
    runner_state: &mut WinitAppRunnerState,
    sub_apps: &mut SubApps,
    control_flow: &mut ControlFlow,
) {
    if !runner_state.active.should_run() {
        return;
    }

    let (config, windows) = focused_windows_state.get(sub_apps.main.world());
    let focused = windows.iter().any(|window| window.focused);
    let mut should_update = match config.update_mode(focused) {
        UpdateMode::Continuous => {
            runner_state.redraw_requested
                || runner_state.window_event_received
                || runner_state.device_event_received   
        }
        UpdateMode::Reactive { .. } => {
            runner_state.wait_elapsed
                || runner_state.redraw_requested
                || runner_state.window_event_received
                || runner_state.device_event_received
        }
        UpdateMode::ReactiveLowPower { .. } => {
            runner_state.wait_elapsed
                || runner_state.redraw_requested
                || runner_state.window_event_received
        }
    };

    // force a few updates to ensure the app is properly initialized
    if runner_state.startup_forced_updates > 0 {
        runner_state.startup_forced_updates -= 1;
        should_update = true;
    }
}


fn check_when_app_should_update_next(
    runner_state: &mut WinitAppRunnerState,
    sub_apps: &mut SubApps,
    control_flow: &mut ControlFlow,
) {
    // decide when to run the next update
    let (config, windows) = focused_windows_state.get(sub_apps.main.world());
    let focused = windows.iter().any(|window| window.focused);
    match config.update_mode(focused) {
        UpdateMode::Continuous => *control_flow = ControlFlow::Poll,
        UpdateMode::Reactive { wait }
        | UpdateMode::ReactiveLowPower { wait } => {
            if let Some(next) = runner_state.last_update.checked_add(wait) {
                runner_state.scheduled_update = Some(next);
                *control_flow = ControlFlow::WaitUntil(next);
            } else {
                // TODO: warn user
                runner_state.scheduled_update = None;
                *control_flow = ControlFlow::Wait;
            }
        }
    }

    if let Some(redraw_events) = sub_apps.main.world().get_resource::<Events<RequestRedraw>>() {
        if redraw_event_reader.read(redraw_events).last().is_some() {
            runner_state.redraw_requested = true;
            *control_flow = ControlFlow::Poll;
        }
    }

    if runner_state.active == ActiveState::WillSuspend {
        runner_state.active = ActiveState::Suspended;
        *control_flow = ControlFlow::Wait;
    }

    if let Some(exit_events) = sub_apps.main.world().get_resource::<Events<AppExit>>() {
        if app_exit_event_reader.read(exit_events).last().is_some() {
            *control_flow = ControlFlow::Exit;
        }
    }
}
