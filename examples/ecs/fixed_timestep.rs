use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_system(
            (|mut time: ResMut<FixedTime>| {
                time.set_steps_per_second(10.0);
            })
            .to(AppSet::Startup),
        )
        .add_system(fixed_update.to(CoreSet::FixedUpdate))
        .add_system(frame_update.to(CoreSet::Update))
        .run();
}

fn frame_update(mut last_time: Local<f32>, time: Res<Time>) {
    info!(
        "time since last frame_update: {}",
        time.seconds_since_startup() - *last_time
    );
    *last_time = time.seconds_since_startup();
}

fn fixed_update(
    mut last_time: Local<f32>,
    time: Res<Time>,
    fixed_time: Res<FixedTime>,
    accumulator: Res<FixedTimestepState>,
) {
    info!(
        "time since last fixed_update: {}\n",
        time.seconds_since_startup() - *last_time
    );
    info!("fixed timestep: {}\n", fixed_time.delta_seconds());
    info!(
        "time accrued toward next fixed_update: {}\n",
        accumulator.overstep().as_secs_f32()
    );
    info!(
        "time accrued toward next fixed_update (% of timestep): {}",
        accumulator.overstep_percentage(fixed_time.delta())
    );
    *last_time = time.seconds_since_startup();
}
