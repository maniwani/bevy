use crate::{
    schedule::{Runner, RunnerApplyBuffers, Schedule},
    world::World,
};

#[cfg(feature = "trace")]
use bevy_utils::tracing::Instrument;
use fixedbitset::FixedBitSet;

/// Runs systems on a single thread.
pub struct SingleThreadedRunner {
    /// Sets whose run criteria have either been evaluated or skipped.
    visited_sets: FixedBitSet,
    /// Systems that have completed.
    completed_systems: FixedBitSet,
    /// Systems that have completed but have not had their buffers applied.
    unapplied_systems: FixedBitSet,
}

impl Default for SingleThreadedRunner {
    fn default() -> Self {
        Self {
            visited_sets: FixedBitSet::new(),
            completed_systems: FixedBitSet::new(),
            unapplied_systems: FixedBitSet::new(),
        }
    }
}

impl Runner for SingleThreadedRunner {
    fn init(&mut self, schedule: &mut Schedule) {
        // pre-allocate space
        let sys_count = schedule.systems.len();
        let set_count = schedule.set_conditions.len();
        self.visited_sets.grow(set_count);
        self.completed_systems.grow(sys_count);
        self.unapplied_systems.grow(sys_count);
    }

    fn run(&mut self, schedule: &mut Schedule, world: &mut World) {
        // insert this resource if it doesn't exist
        world.init_resource::<RunnerApplyBuffers>();
        #[cfg(feature = "trace")]
        let _schedule_span = bevy_utils::tracing::info_span!("run_schedule").entered();
        for sys_idx in 0..schedule.systems.len() {
            if self.completed_systems.contains(sys_idx) {
                continue;
            }

            let system = schedule.systems[sys_idx].get_mut();
            #[cfg(feature = "trace")]
            let should_run_span =
                bevy_utils::tracing::info_span!("check_conditions", name = &*system.name())
                    .entered();

            let mut should_run = true;
            // evaluate set run criteria in hierarchical order
            for set_idx in schedule.system_sets[&sys_idx].ones() {
                if self.visited_sets.contains(set_idx) {
                    continue;
                } else {
                    self.visited_sets.set(set_idx, true);
                }

                let set_conditions = schedule.set_conditions[set_idx].get_mut();
                let set_conditions_met = set_conditions.iter_mut().all(|condition| {
                    #[cfg(feature = "trace")]
                    let _condition_span =
                        bevy_utils::tracing::info_span!("condition", name = &*condition.name())
                            .entered();
                    condition.run((), world)
                });

                if !set_conditions_met {
                    // skip all descendant systems
                    self.completed_systems.union_with(&schedule.set_systems[&set_idx]);
                }

                should_run &= set_conditions_met;
            }

            if !should_run {
                continue;
            }

            // evaluate the system's run criteria
            let system_conditions = schedule.system_conditions[sys_idx].get_mut();
            should_run = system_conditions.iter_mut().all(|condition| {
                #[cfg(feature = "trace")]
                let _condition_span =
                    bevy_utils::tracing::info_span!("condition", name = &*condition.name())
                        .entered();
                condition.run((), world)
            });

            #[cfg(feature = "trace")]
            should_run_span.exit();

            // mark system as completed regardless
            self.completed_systems.set(sys_idx, true);

            if !should_run {
                continue;
            }

            #[cfg(feature = "trace")]
            let system_span =
                bevy_utils::tracing::info_span!("system", name = &*system.name()).entered();
            system.run((), world);
            #[cfg(feature = "trace")]
            system_span.exit();

            // system might have pending commands
            self.unapplied_systems.set(sys_idx, true);

            // poll for `apply_buffers`
            self.check_apply_buffers(schedule, world);
        }

        // poll for `apply_buffers`
        self.check_apply_buffers(schedule, world);
    }
}

impl SingleThreadedRunner {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    fn check_apply_buffers(&mut self, schedule: &mut Schedule, world: &mut World) {
        let mut should_apply_buffers = world.resource_mut::<RunnerApplyBuffers>();
        if should_apply_buffers.0 {
            // reset flag
            should_apply_buffers.0 = false;

            // apply buffers in topological order
            // TODO: determinism
            for sys_idx in self.unapplied_systems.ones() {
                let system = schedule.systems[sys_idx].get_mut();
                #[cfg(feature = "trace")]
                let _apply_buffers_span =
                    bevy_utils::tracing::info_span!("apply_buffers", name = &*system.name())
                        .entered();
                system.apply_buffers(world);
            }

            self.unapplied_systems.clear();
        }
    }
}

/// Runs systems on a single thread and immediately applies their buffers.
pub struct SimpleRunner {
    /// Sets whose run criteria have either been evaluated or skipped.
    visited_sets: FixedBitSet,
    /// Systems that have completed.
    completed_systems: FixedBitSet,
}

impl Default for SimpleRunner {
    fn default() -> Self {
        Self {
            visited_sets: FixedBitSet::new(),
            completed_systems: FixedBitSet::new(),
        }
    }
}

impl Runner for SimpleRunner {
    fn init(&mut self, schedule: &mut Schedule) {
        // pre-allocate space
        let sys_count = schedule.systems.len();
        let set_count = schedule.set_conditions.len();
        self.visited_sets.grow(set_count);
        self.completed_systems.grow(sys_count);
    }

    fn run(&mut self, schedule: &mut Schedule, world: &mut World) {
        #[cfg(feature = "trace")]
        let _schedule_span = bevy_utils::tracing::info_span!("run_schedule").entered();
        for sys_idx in 0..schedule.systems.len() {
            if self.completed_systems.contains(sys_idx) {
                continue;
            }

            let system = schedule.systems[sys_idx].get_mut();
            #[cfg(feature = "trace")]
            let should_run_span =
                bevy_utils::tracing::info_span!("check_conditions", name = &*system.name())
                    .entered();

            let mut should_run = true;
            // evaluate set run criteria in hierarchical order
            for set_idx in schedule.system_sets[&sys_idx].ones() {
                if self.visited_sets.contains(set_idx) {
                    continue;
                } else {
                    self.visited_sets.set(set_idx, true);
                }

                let set_conditions = schedule.set_conditions[set_idx].get_mut();
                let conditions_met = set_conditions.iter_mut().all(|condition| {
                    #[cfg(feature = "trace")]
                    let _condition_span =
                        bevy_utils::tracing::info_span!("condition", name = &*condition.name())
                            .entered();
                    condition.run((), world)
                });

                if !conditions_met {
                    // skip all descendant systems
                    self.completed_systems.union_with(&schedule.set_systems[&set_idx]);
                }

                should_run &= conditions_met;
            }

            if !should_run {
                continue;
            }

            // evaluate the system's run criteria
            let system_conditions = schedule.system_conditions[sys_idx].get_mut();
            should_run = system_conditions.iter_mut().all(|condition| {
                #[cfg(feature = "trace")]
                let _condition_span =
                    bevy_utils::tracing::info_span!("condition", name = &*condition.name())
                        .entered();
                condition.run((), world)
            });

            #[cfg(feature = "trace")]
            should_run_span.exit();

            // mark system as completed regardless
            self.completed_systems.set(sys_idx, true);

            if !should_run {
                continue;
            }

            #[cfg(feature = "trace")]
            let system_span =
                bevy_utils::tracing::info_span!("system", name = &*system.name()).entered();
            system.run((), world);
            #[cfg(feature = "trace")]
            system_span.exit();

            #[cfg(feature = "trace")]
            let _apply_buffers_span =
                bevy_utils::tracing::info_span!("apply_buffers", name = &*system.name()).entered();
            system.apply_buffers(world);
        }
    }
}

impl SimpleRunner {
    pub fn new() -> Self {
        Self::default()
    }
}
