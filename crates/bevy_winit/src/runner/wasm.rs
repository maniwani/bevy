use approx::relative_ne;
use bevy_a11y::AccessibilityRequested;
use bevy_app::{App, AppEvent, AppExit, PluginsState};
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
use bevy_utils::{
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
use winit::{event::StartCause, event_loop::ControlFlow};

use crate::create_windows;
use crate::{
    accessibility::{AccessKitAdapters, WinitActionHandlers},
    converters, run, run_return,
    system::CachedWindow,
    ActiveState, UpdateMode, WindowAndInputEventWriters, WinitAppRunnerState, WinitSettings,
    WinitWindows,
};

/// The default [`App::runner`] for the [`WinitPlugin`](super::WinitPlugin).
pub(crate) fn winit_runner(mut app: App) {
    let mut event_loop = app
        .tls
        .remove_resource::<crate::EventLoop<AppEvent>>()
        .unwrap()
        .into_inner();

    // TODO
    let mut create_windows = IntoSystem::into_system(create_windows::<AppEvent>);
    create_windows.initialize(app.world_mut());

    let mut runner_state = WinitAppRunnerState::default();

    // prepare structures to access data in the world
    let mut event_writer_system_state: SystemState<(
        WindowAndInputEventWriters,
        Query<(&mut Window, &mut CachedWindow)>,
    )> = SystemState::new(app.world_mut());

    #[cfg(target_os = "android")]
    let mut create_window_system_state: SystemState<(
        ResMut<WinitActionHandlers>,
        ResMut<AccessibilityRequested>,
    )> = SystemState::from_world(app.world_mut());

    let mut app_exit_event_reader = ManualEventReader::<AppExit>::default();
    let mut redraw_event_reader = ManualEventReader::<RequestRedraw>::default();

    let winit_events: &mut Vec<WinitEvent>;

    let mut focused_windows_state: SystemState<(Res<WinitSettings>, Query<&Window>)> =
        SystemState::from_world(app.world_mut());

    let event_cb = move |event: winit::event::Event<AppEvent>,
                         event_loop: &winit::event_loop::EventLoopWindowTarget<AppEvent>,
                         control_flow: &mut ControlFlow| {
        #[cfg(feature = "trace")]
        let _span = bevy_utils::tracing::info_span!("winit event_handler").entered();

        // TODO: in future, have runner start before building plugins
        if app.plugins_state() != PluginsState::Cleaned {
            if app.plugins_state() != PluginsState::Ready {
                #[cfg(not(target_arch = "wasm32"))]
                tick_global_task_pools_on_main_thread();
            } else {
                app.finish();
                app.cleanup();
            }

            // Loop until the plugins are ready. We use redraw requests to loop again "immediately"
            // instead of setting `ControlFlow::Poll` to avoid busy waiting.
            runner_state.redraw_requested = true;

            if let Some(app_exit_events) = app.world().get_resource::<Events<AppExit>>() {
                if app_exit_event_reader.read(app_exit_events).last().is_some() {
                    event_loop.exit();
                    return;
                }
            }
        }

        app.tls
            .insert_resource(crate::EventLoopWindowTarget::new(event_loop));

        match event {
            winit::event::Event::NewEvents(start_cause) => match start_cause {
                StartCause::Init => {}
                _ => {
                    if let Some(next) = runner_state.scheduled_update {
                        let now = Instant::now();
                        let remaining = next.checked_duration_since(now).unwrap_or(Duration::ZERO);
                        runner_state.wait_elapsed = remaining.is_zero();
                    }
                }
            },
            winit::event::Event::UserEvent(..) => {
                // TODO: redraw requests
            }
            winit::event::Event::AboutToWait => {
                check_if_app_should_update(&mut runner_state, &mut app, event_loop);
                if runner_state.active == ActiveState::WillSuspend {
                    #[cfg(target_os = "android")]
                    {
                        android_drop_primary_window_handle(app);
                    }
                }
            }
            winit::event::Event::WindowEvent {
                window_id, event, ..
            } => 'window_event: {
                let (mut event_writers, mut windows) =
                    event_writer_system_state.get_mut(app.sub_apps.main.world_mut());

                let Some(window) = app
                    .tls
                    .resource_scope(|_, winit_windows: Mut<WinitWindows>| {
                        winit_windows.get_window(window_id)
                    })
                else {
                    warn!(
                        "Skipped event {:?} for unknown winit Window Id {:?}",
                        event, window_id
                    );
                    break 'window_event;
                };

                let Ok((mut win, mut cache)) = windows.get_mut(window) else {
                    warn!(
                        "Window {:?} is missing `Window` component, skipping event {:?}",
                        window, event
                    );
                    break 'window_event;
                };

                let access_kit_adapters = tls_guard.resource::<AccessKitAdapters>();

                // Allow AccessKit to respond to `WindowEvent`s before they reach the engine.
                if let Some(adapter) = access_kit_adapters.get(&window) {
                    if let Some(winit_win) = winit_windows.get_window(window) {
                        adapter.process_event(winit_win, &event);
                    }
                }

                runner_state.window_event_received = true;
                match event {
                    winit::event::WindowEvent::Resized(size) => {
                        win.resolution
                            .set_physical_resolution(size.width, size.height);

                        winit_events.send(WindowResized {
                            window,
                            width: win.width(),
                            height: win.height(),
                        });
                    }
                    winit::event::WindowEvent::CloseRequested => {
                        winit_events.send(WindowCloseRequested { window });
                    }
                    winit::event::WindowEvent::KeyboardInput { ref event, .. } => {
                        if event.state.is_pressed() {
                            if let Some(char) = &event.text {
                                let char = char.clone();
                                winit_events.send(ReceivedCharacter { window, char });
                            }
                        }

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
                        win.set_physical_cursor_position(None);
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
                        let location = touch.location.to_logical(win.resolution.scale_factor());
                        winit_events.send(converters::convert_touch_input(touch, location, window));
                    }
                    winit::event::WindowEvent::ScaleFactorChanged {
                        scale_factor,
                        inner_size_writer,
                    } => {
                        let prior_factor = win.resolution.scale_factor();
                        win.resolution.set_scale_factor(scale_factor);
                        // This may not match the new scale factor if there is an override.
                        let newer_factor = win.resolution.scale_factor();
                        let scale_factor_override = win.resolution.scale_factor_override();

                        let mut new_inner_size =
                            PhysicalSize::new(win.physical_width(), win.physical_height());

                        if let Some(forced_factor) = scale_factor_override {
                            // This window is overriding its OS-suggested DPI, so its physical size
                            // should be set based on the override. Its logical size already
                            // incorporates any resize constraints.
                            let maybe_new_inner_size = LogicalSize::new(win.width(), win.height())
                                .to_physical::<u32>(forced_factor as f64);
                            match inner_size_writer.request_inner_size(new_inner_size) {
                                Ok(_) => new_inner_size = maybe_new_inner_size,
                                Err(err) => warn!("winit failed to resize window: {err}"),
                            }
                        }

                        win.resolution
                            .set_physical_resolution(new_inner_size.width, new_inner_size.height);

                        let new_logical_width = new_inner_size.width as f32 / new_factor;
                        let new_logical_height = new_inner_size.height as f32 / new_factor;

                        let width_changed = relative_ne!(win.width(), new_logical_width);
                        let height_changed = relative_ne!(win.height(), new_logical_height);
                        let scale_factor_changed = relative_ne!(newer_factor, prior_factor);

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
                        win.focused = focused;
                        winit_events.send(WindowFocused { window, focused });
                    }
                    WindowEvent::Occluded(occluded) => {
                        winit_events.send(WindowOccluded { window, occluded });
                    }
                    winit::event::WindowEvent::DroppedFile(path_buf) => {
                        winit_events.send(FileDragAndDrop::DroppedFile { window, path_buf });
                    }
                    winit::event::WindowEvent::HoveredFile(path_buf) => {
                        winit_events.send(FileDragAndDrop::HoveredFile { window, path_buf });
                    }
                    winit::event::WindowEvent::HoveredFileCancelled => {
                        winit_events.send(FileDragAndDrop::HoveredFileCanceled { window });
                    }
                    winit::event::WindowEvent::Moved(position) => {
                        let position = ivec2(position.x, position.y);
                        window.position.set(position);
                        winit_events.send(WindowMoved { window, position });
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
                            winit_events.send(Ime::Commit { window, value })
                        }
                        winit::event::Ime::Enabled => winit_events.send(Ime::Enabled { window }),
                        winit::event::Ime::Disabled => winit_events.send(Ime::Disabled { window }),
                    },
                    winit::event::WindowEvent::ThemeChanged(theme) => {
                        winit_events.send(WindowThemeChanged {
                            window,
                            theme: converters::convert_winit_theme(theme),
                        });
                    }
                    winit::event::WindowEvent::Destroyed => {
                        winit_events.send(WindowDestroyed { window });
                    }
                    winit::event::WindowEvent::RedrawRequested => {
                        if should_run {
                            // TODO: forward winit events
                            runner_state.reset_on_update();
                            runner_state.last_update = Instant::now();
                            app.update();

                            check_when_app_should_update_next(
                                &mut runner_state,
                                &mut app,
                                event_loop,
                            )
                        }
                    }
                    _ => {}
                }

                if win.is_changed() {
                    cache.window = win.clone();
                }
            }
            winit::event::Event::DeviceEvent { event, .. } => {
                runner_state.device_event_received = true;
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
                // Let the app update one more time so it can react to the suspension before
                // actually suspending.
                runner_state.active = ActiveState::WillSuspend;
            }
            winit::event::Event::Resumed => {
                #[cfg(any(target_os = "android", target_os = "ios", target_os = "macos"))]
                create_windows.run((), app.world_mut());

                match runner_state.active {
                    ActiveState::NotYetStarted => winit_events.send(ApplicationLifetime::Started),
                    _ => winit_events.send(ApplicationLifetime::Resumed),
                }

                runner_state.active = ActiveState::Active;
                runner_state.redraw_requested = true;

                #[cfg(target_os = "android")]
                {
                    android_init_primary_window_handle(app);
                    event_loop.set_control_flow(ControlFlow::Wait);
                }
            }
            _ => {}
        }

        // TODO: run create_windows with F: Added<Window>
        // (even if app did not update, windows could have been created by plugin setup)

        app.tls
            .remove_resource::<crate::EventLoopWindowTarget<AppEvent>>();
    };
}

#[cfg(target_os = "android")]
fn android_init_primary_window_handle(app: &mut App) {
    let mut query = app
        .world_mut()
        .query_filtered::<(Entity, &Window), (With<CachedWindow>, Without<RawHandleWrapper>)>();

    if let Ok((entity, window)) = query.get_single(app.world()) {
        // Re-create the window on the backend and link it to its entity.
        use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
        let window = window.clone();

        let (mut handlers, accessibility_requested) =
            create_window_system_state.get_mut(app.world_mut());

        let tls = &mut app.tls;

        let wrapper = tls.resource_scope(|tls, mut winit_windows: Mut<WinitWindows>| {
            tls.resource_scope(|tls, mut adapters: Mut<AccessKitAdapters>| {
                let winit_window = winit_windows.create_window(
                    event_loop,
                    entity,
                    &window,
                    &mut adapters,
                    &mut handlers,
                    &accessibility_requested,
                );

                RawHandleWrapper {
                    window_handle: winit_window.window_handle().unwrap().as_raw(),
                    display_handle: winit_window.display_handle().unwrap().as_raw(),
                }
            })
        });

        app.world_mut().entity_mut(entity).insert(wrapper);
    }
}

#[cfg(target_os = "android")]
fn android_drop_primary_window_handle(
    world: &mut World,
    mut query: QueryState<Entity, With<PrimaryWindow>>,
) {
    // Android sending this event invalidates the existing window, so remove and drop its handle.
    let mut query = app
        .world_mut()
        .query_filtered::<Entity, (With<PrimaryWindow>, With<RawHandleWrapper>)>();

    if let Ok(entity) = query.get_single(app.world()) {
        app.world_mut()
            .entity_mut(entity)
            .remove::<RawHandleWrapper>();
    }
}

fn check_if_app_should_update(
    runner_state: &mut WinitAppRunnerState,
    app: &mut App,
    event_loop: &EventLoopWindowTarget<UserEvent>,
) {
    if !runner_state.active.should_run() {
        return;
    }

    let (config, windows) = focused_windows_state.get(app.world());
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

    // since app is running on web main thread, updates should synchronize with the screen refresh
    if should_update && runner_state.active != ActiveState::WillSuspend {
        let visible = windows.iter().any(|window| window.visible);
        if visible {
            app.tls
                .resource_scope(|_, winit_windows: Mut<WinitWindows>| {
                    for window in winit_windows.windows.values() {
                        window.request_redraw();
                    }
                });
        }
    }
}

fn check_when_app_should_update_next(
    runner_state: &mut WinitAppRunnerState,
    app: &mut App,
    event_loop: &EventLoopWindowTarget<UserEvent>,
) {
    // decide when app should next update
    let (config, windows) = focused_windows_state.get(app.world());
    let focused = windows.iter().any(|window| window.focused);
    match config.update_mode(focused) {
        UpdateMode::Continuous => {
            runner_state.redraw_requested = true;
            event_loop.set_control_flow(ControlFlow::Wait);
        }
        UpdateMode::Reactive { wait } | UpdateMode::ReactiveLowPower { wait } => {
            if let Some(next) = runner_state.last_update.checked_add(wait) {
                runner_state.scheduled_update = Some(next);
                event_loop.set_control_flow(ControlFlow::WaitUntil(next));
            } else {
                // TODO: warn the user
                runner_state.scheduled_update = None;
                event_loop.set_control_flow(ControlFlow::Wait);
            }
        }
    }

    if let Some(redraw_events) = app.world().get_resource::<Events<RequestRedraw>>() {
        if redraw_event_reader.read(redraw_events).last().is_some() {
            runner_state.redraw_requested = true;
        }
    }

    if runner_state.active == ActiveState::WillSuspend {
        runner_state.active = ActiveState::Suspended;
        event_loop.set_control_flow(ControlFlow::Wait);
    }

    if let Some(exit_events) = app.world().get_resource::<Events<AppExit>>() {
        if app_exit_event_reader.read(exit_events).last().is_some() {
            trace!("exiting app");
            event_loop.exit();
        }
    }
}

// TODO: This code will go. I'm in the middle of refactoring it into all the code above.
#[allow(clippy::too_many_arguments /* TODO: probs can reduce # of args */)]
fn handle_winit_event(
    app: &mut App,
    app_exit_event_reader: &mut ManualEventReader<AppExit>,
    runner_state: &mut WinitAppRunnerState,
    create_window: &mut SystemState<CreateWindowParams<Added<Window>>>,
    event_writer_system_state: &mut SystemState<(
        EventWriter<WindowResized>,
        NonSend<WinitWindows>,
        Query<(&mut Window, &mut CachedWindow)>,
        NonSend<AccessKitAdapters>,
    )>,
    focused_windows_state: &mut SystemState<(Res<WinitSettings>, Query<&Window>)>,
    redraw_event_reader: &mut ManualEventReader<RequestRedraw>,
    winit_events: &mut Vec<WinitEvent>,
    event: Event<UserEvent>,
    event_loop: &EventLoopWindowTarget<UserEvent>,
) {
    #[cfg(feature = "trace")]
    let _span = bevy_utils::tracing::info_span!("winit event_handler").entered();

    if app.plugins_state() != PluginsState::Cleaned {
        if app.plugins_state() != PluginsState::Ready {
            #[cfg(not(target_arch = "wasm32"))]
            tick_global_task_pools_on_main_thread();
        } else {
            app.finish();
            app.cleanup();
        }
        runner_state.redraw_requested = true;

        if let Some(app_exit_events) = app.world().get_resource::<Events<AppExit>>() {
            if app_exit_event_reader.read(app_exit_events).last().is_some() {
                event_loop.exit();
                return;
            }
        }
    }

    match event {
        Event::AboutToWait => {
            //
            if should_update {
                let visible = windows.iter().any(|window| window.visible);
                let (_, winit_windows, _, _) = event_writer_system_state.get_mut(app.world_mut());
                if visible && runner_state.active != ActiveState::WillSuspend {
                    for window in winit_windows.windows.values() {
                        window.request_redraw();
                    }
                } else {
                    // there are no windows, or they are not visible.
                    // winit won't send events on some platforms, so trigger an update manually.
                    run_app_update_if_should(
                        runner_state,
                        app,
                        focused_windows_state,
                        event_loop,
                        create_window,
                        app_exit_event_reader,
                        redraw_event_reader,
                        winit_events,
                    );
                    if runner_state.active != ActiveState::Suspended {
                        event_loop.set_control_flow(ControlFlow::Poll);
                    }
                }
            }
        }
        Event::NewEvents(cause) => {
            runner_state.wait_elapsed = match cause {
                StartCause::WaitCancelled {
                    requested_resume: Some(resume),
                    ..
                } => resume >= Instant::now(),
                _ => true,
            };
        }
        Event::WindowEvent {
            event, window_id, ..
        } => {
            let (mut window_resized, winit_windows, mut windows, access_kit_adapters) =
                event_writer_system_state.get_mut(app.world_mut());

            let Some(window) = winit_windows.get_window(window_id) else {
                warn!("Skipped event {event:?} for unknown winit Window Id {window_id:?}");
                return;
            };

            let Ok((mut win, _)) = windows.get_mut(window) else {
                warn!("Window {window:?} is missing `Window` component, skipping event {event:?}");
                return;
            };

            // Allow AccessKit to respond to `WindowEvent`s before they reach
            // the engine.
            if let Some(adapter) = access_kit_adapters.get(&window) {
                if let Some(winit_window) = winit_windows.get_window(window) {
                    adapter.process_event(winit_window, &event);
                }
            }

            runner_state.window_event_received = true;

            match event {
                WindowEvent::Resized(size) => {
                    react_to_resize(&mut win, size, &mut window_resized, window);
                }
                WindowEvent::CloseRequested => winit_events.send(WindowCloseRequested { window }),
                WindowEvent::KeyboardInput { ref event, .. } => {
                    if event.state.is_pressed() {
                        if let Some(char) = &event.text {
                            let char = char.clone();
                            winit_events.send(ReceivedCharacter { window, char });
                        }
                    }
                    winit_events.send(converters::convert_keyboard_input(event, window));
                }
                WindowEvent::CursorMoved { position, .. } => {
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
                WindowEvent::CursorEntered { .. } => {
                    winit_events.send(CursorEntered { window });
                }
                WindowEvent::CursorLeft { .. } => {
                    win.set_physical_cursor_position(None);
                    winit_events.send(CursorLeft { window });
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    winit_events.send(MouseButtonInput {
                        button: converters::convert_mouse_button(button),
                        state: converters::convert_element_state(state),
                        window,
                    });
                }
                WindowEvent::TouchpadMagnify { delta, .. } => {
                    winit_events.send(TouchpadMagnify(delta as f32));
                }
                WindowEvent::TouchpadRotate { delta, .. } => {
                    winit_events.send(TouchpadRotate(delta));
                }
                WindowEvent::MouseWheel { delta, .. } => match delta {
                    event::MouseScrollDelta::LineDelta(x, y) => {
                        winit_events.send(MouseWheel {
                            unit: MouseScrollUnit::Line,
                            x,
                            y,
                            window,
                        });
                    }
                    event::MouseScrollDelta::PixelDelta(p) => {
                        winit_events.send(MouseWheel {
                            unit: MouseScrollUnit::Pixel,
                            x: p.x as f32,
                            y: p.y as f32,
                            window,
                        });
                    }
                },
                WindowEvent::Touch(touch) => {
                    let location = touch
                        .location
                        .to_logical(win.resolution.scale_factor() as f64);
                    winit_events.send(converters::convert_touch_input(touch, location, window));
                }
                WindowEvent::ScaleFactorChanged {
                    scale_factor,
                    mut inner_size_writer,
                } => {
                    let prior_factor = win.resolution.scale_factor();
                    win.resolution.set_scale_factor(scale_factor as f32);
                    // Note: this may be different from new_scale_factor if
                    // `scale_factor_override` is set to Some(thing)
                    let new_factor = win.resolution.scale_factor();

                    let mut new_inner_size =
                        PhysicalSize::new(win.physical_width(), win.physical_height());
                    let scale_factor_override = win.resolution.scale_factor_override();
                    if let Some(forced_factor) = scale_factor_override {
                        // This window is overriding the OS-suggested DPI, so its physical size
                        // should be set based on the overriding value. Its logical size already
                        // incorporates any resize constraints.
                        let maybe_new_inner_size = LogicalSize::new(win.width(), win.height())
                            .to_physical::<u32>(forced_factor as f64);
                        if let Err(err) = inner_size_writer.request_inner_size(new_inner_size) {
                            warn!("Winit Failed to resize the window: {err}");
                        } else {
                            new_inner_size = maybe_new_inner_size;
                        }
                    }
                    let new_logical_width = new_inner_size.width as f32 / new_factor;
                    let new_logical_height = new_inner_size.height as f32 / new_factor;

                    let width_equal = relative_eq!(win.width(), new_logical_width);
                    let height_equal = relative_eq!(win.height(), new_logical_height);
                    win.resolution
                        .set_physical_resolution(new_inner_size.width, new_inner_size.height);

                    winit_events.send(WindowBackendScaleFactorChanged {
                        window,
                        scale_factor,
                    });
                    if scale_factor_override.is_none() && !relative_eq!(new_factor, prior_factor) {
                        winit_events.send(WindowScaleFactorChanged {
                            window,
                            scale_factor,
                        });
                    }

                    if !width_equal || !height_equal {
                        winit_events.send(WindowResized {
                            window,
                            width: new_logical_width,
                            height: new_logical_height,
                        });
                    }
                }
                WindowEvent::Focused(focused) => {
                    win.focused = focused;
                    winit_events.send(WindowFocused { window, focused });
                }
                WindowEvent::Occluded(occluded) => {
                    winit_events.send(WindowOccluded { window, occluded });
                }
                WindowEvent::DroppedFile(path_buf) => {
                    winit_events.send(FileDragAndDrop::DroppedFile { window, path_buf });
                }
                WindowEvent::HoveredFile(path_buf) => {
                    winit_events.send(FileDragAndDrop::HoveredFile { window, path_buf });
                }
                WindowEvent::HoveredFileCancelled => {
                    winit_events.send(FileDragAndDrop::HoveredFileCanceled { window });
                }
                WindowEvent::Moved(position) => {
                    let position = ivec2(position.x, position.y);
                    win.position.set(position);
                    winit_events.send(WindowMoved { window, position });
                }
                WindowEvent::Ime(event) => match event {
                    event::Ime::Preedit(value, cursor) => {
                        winit_events.send(Ime::Preedit {
                            window,
                            value,
                            cursor,
                        });
                    }
                    event::Ime::Commit(value) => {
                        winit_events.send(Ime::Commit { window, value });
                    }
                    event::Ime::Enabled => {
                        winit_events.send(Ime::Enabled { window });
                    }
                    event::Ime::Disabled => {
                        winit_events.send(Ime::Disabled { window });
                    }
                },
                WindowEvent::ThemeChanged(theme) => {
                    winit_events.send(WindowThemeChanged {
                        window,
                        theme: convert_winit_theme(theme),
                    });
                }
                WindowEvent::Destroyed => {
                    winit_events.send(WindowDestroyed { window });
                }
                WindowEvent::RedrawRequested => {
                    run_app_update_if_should(
                        runner_state,
                        app,
                        focused_windows_state,
                        event_loop,
                        create_window,
                        app_exit_event_reader,
                        redraw_event_reader,
                        winit_events,
                    );
                }
                _ => {}
            }

            let mut windows = app.world_mut().query::<(&mut Window, &mut CachedWindow)>();
            if let Ok((window_component, mut cache)) = windows.get_mut(app.world_mut(), window) {
                if window_component.is_changed() {
                    cache.window = window_component.clone();
                }
            }
        }
        Event::DeviceEvent { event, .. } => {
            runner_state.device_event_received = true;
            if let DeviceEvent::MouseMotion { delta: (x, y) } = event {
                let delta = Vec2::new(x as f32, y as f32);
                winit_events.send(MouseMotion { delta });
            }
        }
        Event::Suspended => {
            winit_events.send(ApplicationLifetime::Suspended);
            // Mark the state as `WillSuspend`. This will let the schedule run one last time
            // before actually suspending to let the application react
            runner_state.active = ActiveState::WillSuspend;
        }
        Event::Resumed => {
            #[cfg(any(target_os = "android", target_os = "ios", target_os = "macos"))]
            {
                if runner_state.active == ActiveState::NotYetStarted {
                    create_windows(event_loop, create_window.get_mut(app.world_mut()));
                    create_window.apply(app.world_mut());
                }
            }

            match runner_state.active {
                ActiveState::NotYetStarted => winit_events.send(ApplicationLifetime::Started),
                _ => winit_events.send(ApplicationLifetime::Resumed),
            }
            runner_state.active = ActiveState::Active;
            runner_state.redraw_requested = true;

            #[cfg(target_os = "android")]
            {
                // Get windows that are cached but without raw handles. Those window were already created, but got their
                // handle wrapper removed when the app was suspended.
                let mut query = app
                        .world_mut()
                        .query_filtered::<(Entity, &Window), (With<CachedWindow>, Without<bevy_window::RawHandleWrapper>)>();

                if let Ok((entity, window)) = query.get_single(app.world()) {
                    use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
                    let window = window.clone();

                    let (
                        ..,
                        mut winit_windows,
                        mut adapters,
                        mut handlers,
                        accessibility_requested,
                    ) = create_window.get_mut(app.world_mut());

                    let winit_window = winit_windows.create_window(
                        event_loop,
                        entity,
                        &window,
                        &mut adapters,
                        &mut handlers,
                        &accessibility_requested,
                    );

                    let wrapper = RawHandleWrapper {
                        window_handle: winit_window.window_handle().unwrap().as_raw(),
                        display_handle: winit_window.display_handle().unwrap().as_raw(),
                    };

                    app.world_mut().entity_mut(entity).insert(wrapper);
                }
                event_loop.set_control_flow(ControlFlow::Wait);
            }
        }
        Event::UserEvent(RequestRedraw) => {
            runner_state.redraw_requested = true;
        }
        _ => (),
    }

    // We drain events after every received winit event in addition to on app update to ensure
    // the work of pushing events into event queues is spread out over time in case the app becomes
    // dormant for a long stretch.
    forward_winit_events(winit_events, app);
}

fn react_to_resize(
    win: &mut Mut<'_, Window>,
    size: winit::dpi::PhysicalSize<u32>,
    window_resized: &mut EventWriter<WindowResized>,
    window: Entity,
) {
    win.resolution
        .set_physical_resolution(size.width, size.height);

    window_resized.send(WindowResized {
        window,
        width: win.width(),
        height: win.height(),
    });
}
