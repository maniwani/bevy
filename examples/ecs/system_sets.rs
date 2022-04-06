use bevy::prelude::*;

/// A label can be "instantiated" as a system set or used to give a system a unique name.
///
/// Deriving [`SystemLabel`] requires [`Debug`], [`Clone`], [`PartialEq`], [`Eq`], and [`Hash`].
#[derive(Debug Clone, PartialEq, Eq, Hash, SystemLabel)]
struct Physics;

#[derive(Debug Clone, PartialEq, Eq, Hash, SystemLabel)]
struct PostPhysics;

/// Resource used to stop the example.
#[derive(Default)]
struct Done(bool);

/// This example realizes the following scheme:
///
/// ```none
/// Physics                     (condition: app has been running for less than 1 second)
///     \--> update_velocity
///     \--> movement
/// PostPhysics                 (condition: done == false)
///     \--> collision || sfx
/// Exit                        (condition: done == true)
///     \--> exit
/// ```
///
/// The `Physics` label represents a system set with two systems.
/// Its two systems (`update_velocity` and `movement`) run in a specific order.
/// It will stop running after a second.
///
/// The `PostPhysics` label is conditioned to only run once `Physics` has finished.
/// Its two systems (`collision` and `sfx`) have no specified order.
fn main() {
    App::new()
        // Add our plugins.
        .add_plugins(MinimalPlugins)
        // Initialize the resource.
        .init_resource::<Done>()

        .add_set(Physics.iff(run_for_a_second))
        .add_set(PostPhysics
            .to("")
            .after(Physics)
            .iff()
        )
        // Add systems to `Physics`.
        .add_many(seq![update_velocity, movement].to(Physics))
        // Add systems to `PostPhysics`.
        // `collision` and `sfx` can run in any order
        .add_many(par![collision, sfx].to(PostPhysics))
        .add_system(
            exit
                .to("")
                .after(PostPhysics)
                .iff(|| { true })
        )
        .run();
}

fn less_than_one_second(time: Res<Time>, mut done: ResMut<Done>) -> bool {
    let elapsed = time.raw_seconds_since_startup();
    if elapsed < 1.0 {
        info!(
            "We should run again. Elapsed/remaining: {:.2}s/{:.2}s",
            elapsed,
            1.0 - elapsed
        );
        true
    } else {
        done.0 = true;
        false
    }
}

/// Another run criteria, simply using a resource.
fn is_done(done: Res<Done>) -> bool {
    done.0
}

fn update_velocity() {
    info!("updating velocity");
}

fn movement() {
    info!("updating movement");
}

fn collision() {
    info!("checking collisions");
}

fn sfx() {
    info!("playing sfx");
}

fn exit(mut app_exit_events: EventWriter<AppExit>) {
    info!("exiting...");
    app_exit_events.send(AppExit);
}
