use crate::{
    schedule::{Runner, RunnerApplyBuffers, SystemRunner},
    world::World,
};

#[cfg(feature = "trace")]
use bevy_utils::tracing::Instrument;
use fixedbitset::FixedBitSet;

/// Runs systems on a single thread.
pub struct SingleThreadedRunner {
    inner: Runner,
    /// Sets whose run criteria have either been evaluated or skipped.
    visited_sets: FixedBitSet,
    /// Systems that have completed.
    completed_systems: FixedBitSet,
    /// Systems that have completed but have not had their buffers applied.
    unapplied_systems: FixedBitSet,
}

impl SystemRunner for SingleThreadedRunner {
    fn run(&mut self, world: &mut World) {
        self.rebuild_metadata();
        self.run_inner(world);
    }
}

impl SingleThreadedRunner {
    pub(crate) fn with(inner: Runner) -> Self {
        Self {
            inner,
            visited_sets: FixedBitSet::new(),
            completed_systems: FixedBitSet::new(),
            unapplied_systems: FixedBitSet::new(),
        }
    }

    pub(crate) fn into_inner(self) -> Runner {
        self.inner
    }

    fn rebuild_metadata(&mut self) {
        // pre-allocate space
        let sys_count = self.inner.systems.len();
        let set_count = self.inner.set_conditions.len();
        self.visited_sets.grow(set_count);
        self.completed_systems.grow(sys_count);
        self.unapplied_systems.grow(sys_count);
    }

    fn run_inner(&mut self, world: &mut World) {
        #[cfg(feature = "trace")]
        let _schedule_span = bevy_utils::tracing::info_span!("schedule").entered();

        // insert this resource if it doesn't exist
        world.init_resource::<RunnerApplyBuffers>();

        for sys_idx in 0..self.inner.systems.len() {
            if self.completed_systems.contains(sys_idx) {
                continue;
            }

            let system = &mut self.inner.systems[sys_idx];

            #[cfg(feature = "trace")]
            let should_run_span =
                bevy_utils::tracing::info_span!("test_conditions", name = &*system.name())
                    .entered();

            let mut should_run = true;

            // evaluate set run criteria in hierarchical order
            let system_sets = self.inner.system_sets.get(&sys_idx).unwrap();
            for set_idx in system_sets.ones() {
                if self.visited_sets.contains(set_idx) {
                    continue;
                } else {
                    self.visited_sets.set(set_idx, true);
                }

                let conditions_met =
                    self.inner.set_conditions[set_idx]
                        .iter_mut()
                        .all(|condition| {
                            #[cfg(feature = "trace")]
                            let _condition_span = bevy_utils::tracing::info_span!(
                                "condition",
                                name = &*condition.name()
                            )
                            .entered();
                            condition.run((), world)
                        });

                if !conditions_met {
                    // skip all descendant systems
                    let set_systems = self.inner.set_systems.get(&set_idx).unwrap();
                    self.completed_systems.union_with(&set_systems);
                }

                should_run &= conditions_met;
            }

            if !should_run {
                continue;
            }

            // evaluate the system's run criteria
            should_run = self.inner.system_conditions[sys_idx]
                .iter_mut()
                .all(|condition| {
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
            self.check_apply_buffers(world);
        }

        // poll for `apply_buffers`
        self.check_apply_buffers(world);
    }

    #[inline]
    fn check_apply_buffers(&mut self, world: &mut World) {
        let mut should_apply_buffers = world.resource_mut::<RunnerApplyBuffers>();
        if should_apply_buffers.0 {
            // reset flag
            should_apply_buffers.0 = false;

            // apply buffers in topological order
            // TODO: determinism
            for sys_idx in self.unapplied_systems.ones() {
                let system = self.inner.systems.get_mut(sys_idx).unwrap();
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
    inner: Runner,
    /// Sets whose run criteria have either been evaluated or skipped.
    visited_sets: FixedBitSet,
    /// Systems that have completed.
    completed_systems: FixedBitSet,
}

impl SystemRunner for SimpleRunner {
    fn run(&mut self, world: &mut World) {
        self.rebuild_metadata();
        self.run_inner(world);
    }
}

impl SimpleRunner {
    pub(crate) fn with(inner: Runner) -> Self {
        Self {
            inner,
            visited_sets: FixedBitSet::new(),
            completed_systems: FixedBitSet::new(),
        }
    }

    pub(crate) fn into_inner(self) -> Runner {
        self.inner
    }

    fn rebuild_metadata(&mut self) {
        // pre-allocate space
        let sys_count = self.inner.systems.len();
        let set_count = self.inner.set_conditions.len();
        self.visited_sets.grow(set_count);
        self.completed_systems.grow(sys_count);
    }

    fn run_inner(&mut self, world: &mut World) {
        #[cfg(feature = "trace")]
        let _schedule_span = bevy_utils::tracing::info_span!("schedule").entered();

        for sys_idx in 0..self.inner.systems.len() {
            if self.completed_systems.contains(sys_idx) {
                continue;
            }

            let system = &mut self.inner.systems[sys_idx];

            #[cfg(feature = "trace")]
            let should_run_span =
                bevy_utils::tracing::info_span!("test_conditions", name = &*system.name())
                    .entered();

            let mut should_run = true;

            // evaluate set run criteria in hierarchical order
            let system_sets = self.inner.system_sets.get(&sys_idx).unwrap();
            for set_idx in system_sets.ones() {
                if self.visited_sets.contains(set_idx) {
                    continue;
                } else {
                    self.visited_sets.set(set_idx, true);
                }

                let conditions_met =
                    self.inner.set_conditions[set_idx]
                        .iter_mut()
                        .all(|condition| {
                            #[cfg(feature = "trace")]
                            let _condition_span = bevy_utils::tracing::info_span!(
                                "condition",
                                name = &*condition.name()
                            )
                            .entered();
                            condition.run((), world)
                        });

                if !conditions_met {
                    // skip all descendant systems
                    let set_systems = self.inner.set_systems.get(&set_idx).unwrap();
                    self.completed_systems.union_with(&set_systems);
                }

                should_run &= conditions_met;
            }

            if !should_run {
                continue;
            }

            // evaluate the system's run criteria
            should_run = self.inner.system_conditions[sys_idx]
                .iter_mut()
                .all(|condition| {
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
