use crate::{
    archetype::ArchetypeComponentId,
    cell::SemiSafeCell,
    query::Access,
    schedule::{Runner, RunnerApplyBuffers, SystemRunner},
    world::World,
};

use bevy_tasks::{ComputeTaskPool, Scope, TaskPool};
#[cfg(feature = "trace")]
use bevy_utils::tracing::Instrument;

use async_channel::{Receiver, Sender};
use fixedbitset::FixedBitSet;

/// Per-system data used by the [`MultiThreadedRunner`].
struct SystemTaskMetadata {
    /// Notifies system task to start running.
    start_sender: Sender<()>,
    /// Receives the start signal.
    start_receiver: Receiver<()>,
    /// Indices of the systems that directly depend on this one.
    dependents: Vec<usize>,
    /// The number of dependencies the system has in total.
    dependencies_total: usize,
    /// The number of dependencies the system has that have not finished.
    dependencies_remaining: usize,
    /// The `ArchetypeComponentId` access of this system.
    // This exists because systems are borrowed while they're running, and we have to
    // clear and rebuild the executor's active access whenever systems complete.
    archetype_component_access: Access<ArchetypeComponentId>,
    /// Can the system access the data it needs from any thread?
    is_send: bool,
}

/// A `Runner` that run systems concurrently using a task pool.
pub struct MultiThreadedRunner {
    inner: Runner,
    /// Metadata for scheduling and running system tasks.
    system_task_metadata: Vec<SystemTaskMetadata>,
    /// Notifies executor that system tasks have completed.
    finish_sender: Sender<usize>,
    /// Receives task completion events.
    finish_receiver: Receiver<usize>,
    /// Union of the accesses of all currently running systems.
    active_archetype_component_access: Access<ArchetypeComponentId>,
    /// Is a non-send system task currently running?
    non_send_task_running: bool,

    /// Sets whose run criteria have been evaluated or skipped.
    visited_sets: FixedBitSet,
    /// Systems that have no remaining dependencies and are waiting to run.
    ready_systems: FixedBitSet,
    /// Systems that are currently running.
    running_systems: FixedBitSet,
    /// Systems that have completed.
    completed_systems: FixedBitSet,
    /// Systems that have completed but have not had their buffers applied.
    unapplied_systems: FixedBitSet,
}

impl Default for MultiThreadedRunner {
    fn default() -> Self {
        let (finish_sender, finish_receiver) = async_channel::unbounded();
        Self {
            finish_sender,
            finish_receiver,
            non_send_task_running: false,
            ..Default::default()
        }
    }
}

impl SystemRunner for MultiThreadedRunner {
    fn run(&mut self, world: &mut World) {
        self.rebuild_metadata();
        self.run_inner(world);
    }
}

impl MultiThreadedRunner {
    fn rebuild_metadata(&mut self) {
        // pre-allocate space
        let sys_count = self.inner.systems.len();
        let set_count = self.inner.set_conditions.len();

        self.visited_sets.grow(set_count);
        self.ready_systems.grow(sys_count);
        self.running_systems.grow(sys_count);
        self.completed_systems.grow(sys_count);
        self.unapplied_systems.grow(sys_count);

        self.system_task_metadata.clear();
        self.system_task_metadata
            .reserve(sys_count.saturating_sub(self.system_task_metadata.len()));

        for index in 0..sys_count {
            let (start_sender, start_receiver) = async_channel::bounded(1);
            let (num_dependencies, dependents) =
                self.inner.system_deps.get(&index).unwrap().clone();

            self.system_task_metadata.push(SystemTaskMetadata {
                start_sender,
                start_receiver,
                dependents,
                dependencies_total: num_dependencies,
                dependencies_remaining: num_dependencies,
                is_send: self.inner.systems[index].is_send(),
                archetype_component_access: Default::default(),
            });
        }
    }

    #[inline]
    fn run_inner(&mut self, world: &mut World) {
        #[cfg(feature = "trace")]
        let _schedule_span = bevy_utils::tracing::info_span!("schedule").entered();

        let compute_pool = world
            .get_resource_or_insert_with(|| ComputeTaskPool(TaskPool::default()))
            .clone();

        // insert this resource if it doesn't exist
        world.init_resource::<RunnerApplyBuffers>();

        // systems with no dependencies are ready
        for (index, system_meta) in self.system_task_metadata.iter_mut().enumerate() {
            if system_meta.dependencies_total == 0 {
                self.ready_systems.set(index, true);
            }
        }

        let world = SemiSafeCell::from_mut(world);

        compute_pool.scope(|scope| {
            let mut executor = |scope| async {
                while self.completed_systems.count_ones(..) != self.completed_systems.len() {
                    self.spawn_system_tasks(scope, world).await;
                    if self.running_systems.count_ones(..) != 0 {
                        #[cfg(feature = "trace")]
                        let _await_span =
                            bevy_utils::tracing::info_span!("await_tasks").entered();

                        // wait until something finishes
                        let index = self
                            .finish_receiver
                            .recv()
                            .await
                            .unwrap_or_else(|error| unreachable!(error));

                        #[cfg(feature = "trace")]
                        drop(wait_guard);

                        self.finish_system_and_signal_dependents(index);
                        // more than one could have finished
                        while let Ok(index) = self.finish_receiver.try_recv() {
                            self.finish_system_and_signal_dependents(index);
                        }

                        // have to rebuild because access doesn't count the number of readers
                        self.active_archetype_component_access.clear();
                        for sys_idx in self.running_systems.ones() {
                            let system_meta = &self.system_task_metadata[sys_idx];
                            self.active_archetype_component_access
                                .extend(&system_meta.archetype_component_access);
                        }
                    }

                    if self.running_systems.count_ones(..) == 0 {
                        // poll for `apply_buffers`
                        // SAFETY: cannot alias because no other systems are running
                        unsafe { self.check_apply_buffers(world.as_mut()) };
                    }
                }
                debug_assert_eq!(self.ready_systems.count_ones(..), 0);
                debug_assert_eq!(self.running_systems.count_ones(..), 0);

                // poll for `apply_buffers`
                // SAFETY: cannot alias because no other systems are running
                unsafe {
                    self.check_apply_buffers(world.as_mut());
                }

                self.visited_sets.clear();
                self.completed_systems.clear();
            };

            #[cfg(feature = "trace")]
            let executor_span = bevy_utils::tracing::info_span!("executor task");
            #[cfg(feature = "trace")]
            let executor = executor.instrument(executor_span);
            // TODO: modify `bevy_tasks` so we can spawn tasks from `&Scope`
            // TODO: this can't work because of the `&mut` and LocalExecutor
            scope.spawn(executor(scope));
        });
    }

    async fn spawn_system_tasks<'scope, 'world: 'scope>(
        &mut self,
        scope: &'scope mut Scope<'scope, ()>,
        world: SemiSafeCell<'world, World>,
    ) {
        #[cfg(feature = "trace")]
        let span = bevy_utils::tracing::info_span!("spawn system tasks");
        #[cfg(feature = "trace")]
        let _guard = span.enter();

        // TODO: reduce loop overhead
        while let Some(index) = self.ready_systems.ones().next() {
            if !self.system_can_run(index, world) {
                continue;
            }

            if !self.system_should_run(index, world) {
                continue;
            }

            // SAFETY: splitting borrow, no aliasing
            let system =
                unsafe { &mut *core::slice::from_mut(&mut self.inner.systems[index]).as_mut_ptr() };

            let system_meta = &self.system_task_metadata[index];
            let start_receiver = system_meta.start_receiver.clone();
            let finish_sender = self.finish_sender.clone();

            #[cfg(feature = "trace")]
            let task_span = bevy_utils::tracing::info_span!("system task", name = &*system.name());
            #[cfg(feature = "trace")]
            let system_span = bevy_utils::tracing::info_span!("system", name = &*system.name());

            let task = async move {
                start_receiver
                    .recv()
                    .await
                    .unwrap_or_else(|error| unreachable!(error));

                #[cfg(feature = "trace")]
                let system_guard = system_span.enter();
                // SAFETY: access does not conflict with another running task
                unsafe { system.run_unchecked((), world) };
                #[cfg(feature = "trace")]
                drop(system_guard);

                finish_sender
                    .send(index)
                    .await
                    .unwrap_or_else(|error| unreachable!(error));
            };

            #[cfg(feature = "trace")]
            let task = task.instrument(task_span);

            if system_meta.is_send {
                scope.spawn(task);
            } else {
                scope.spawn_local(task);
                self.non_send_task_running = true;
            }

            system_meta
                .start_sender
                .send(())
                .await
                .unwrap_or_else(|error| unreachable!(error));

            self.active_archetype_component_access
                .extend(&system_meta.archetype_component_access);
            self.ready_systems.set(index, false);
            self.running_systems.set(index, true);
        }
    }

    /// Checks if the data accessed by the system at `index` is available.
    #[inline]
    fn system_can_run(&mut self, index: usize, world: SemiSafeCell<World>) -> bool {
        let system_meta = &self.system_task_metadata[index];
        if self.non_send_task_running && !system_meta.is_send {
            // all non-`Send` systems conflict (they all have to run on same thread)
            return false;
        }

        // systems that don't borrow are still considered conflicts to avoid UB below
        if self.active_archetype_component_access.writes_all {
            return false;
        }

        let system = &mut self.inner.systems[index];

        // SAFETY: `system_can_run` never gets here while system with `&mut World` is running
        let world = unsafe { world.as_ref() };
        system.update_archetype_component_access(world);

        // need the access of the system, its run criteria, and the run criteria of its not-yet-seen labels
        let mut access = system.archetype_component_access().clone();
        for condition in self.inner.system_conditions[index].iter_mut() {
            condition.update_archetype_component_access(world);
            access.extend(condition.archetype_component_access());
        }

        let system_sets = self.inner.system_sets.get_mut(&index).unwrap();
        for set_idx in system_sets.difference(&self.visited_sets) {
            for condition in self.inner.set_conditions[set_idx].iter_mut() {
                condition.update_archetype_component_access(world);
                access.extend(condition.archetype_component_access());
            }
        }

        access.is_compatible(&self.active_archetype_component_access)
    }

    /// Evaluates the run criteria of the system at `index` to determine if it should run.
    #[inline]
    fn system_should_run(&mut self, index: usize, world: SemiSafeCell<World>) -> bool {
        #[cfg(feature = "trace")]
        let _should_run_span =
            bevy_utils::tracing::info_span!("test_conditions", name = &*system.name()).entered();

        let mut should_run = true;

        // evaluate the set run criteria in hierarchical order
        // SAFETY: data is never mutated
        let system_sets = unsafe {
            &*core::slice::from_ref(self.inner.system_sets.get(&index).unwrap()).as_ptr()
        };

        for set_idx in system_sets.ones() {
            if self.visited_sets.contains(set_idx) {
                continue;
            } else {
                self.visited_sets.set(set_idx, true);
            }

            let conditions_met = self.inner.set_conditions[set_idx]
                .iter_mut()
                .all(|condition| {
                    #[cfg(feature = "trace")]
                    let _condition_span =
                        bevy_utils::tracing::info_span!("condition", name = &*condition.name())
                            .entered();
                    // SAFETY: access does not conflict with another running task
                    unsafe { condition.run_unchecked((), world) }
                });

            if !conditions_met {
                // skip all descendant systems
                // SAFETY: data is never mutated (this is a bad hack tho)
                let set_systems = unsafe {
                    &*core::slice::from_ref(self.inner.set_systems.get(&set_idx).unwrap()).as_ptr()
                };

                for sys_idx in set_systems.ones() {
                    if self.completed_systems.contains(sys_idx) {
                        // same system could be skipped multiple times
                        continue;
                    }

                    self.skip_system_and_signal_dependents(sys_idx);
                }
            }

            should_run &= conditions_met;
        }

        if !should_run {
            // system was skipped above
            return false;
        }

        // evaluate the system's run criteria
        should_run = self.inner.system_conditions[index]
            .iter_mut()
            .all(|condition| {
                #[cfg(feature = "trace")]
                let _condition_span =
                    bevy_utils::tracing::info_span!("condition", name = &*condition.name())
                        .entered();
                // SAFETY: access does not conflict with another running task
                unsafe { condition.run_unchecked((), world) }
            });

        if !should_run {
            self.skip_system_and_signal_dependents(index);
            return false;
        }

        true
    }

    #[inline]
    fn finish_system_and_signal_dependents(&mut self, index: usize) {
        if !self.system_task_metadata[index].is_send {
            self.non_send_task_running = false;
        }
        self.running_systems.set(index, false);
        self.completed_systems.set(index, true);
        self.unapplied_systems.set(index, true);
        self.signal_dependents(index);
    }

    #[inline]
    fn skip_system_and_signal_dependents(&mut self, index: usize) {
        self.ready_systems.set(index, false);
        self.completed_systems.set(index, true);
        self.signal_dependents(index);
    }

    #[inline]
    fn signal_dependents(&mut self, index: usize) {
        #[cfg(feature = "trace")]
        let _span = bevy_utils::tracing::info_span!("signal_dependents");

        // SAFETY: system cannot have itself as a dependent (this is a bad hack tho)
        let system_meta = unsafe {
            &mut *core::slice::from_mut(&mut self.system_task_metadata[index]).as_mut_ptr()
        };

        for &idx in system_meta.dependents.iter() {
            let dependent_meta = &mut self.system_task_metadata[idx];
            dependent_meta.dependencies_remaining -= 1;
            if (dependent_meta.dependencies_remaining == 0) && !self.completed_systems.contains(idx)
            {
                self.ready_systems.set(idx, true);
            }
        }
    }

    #[inline]
    fn check_apply_buffers(&mut self, world: &mut World) {
        let mut should_apply_buffers = world.resource_mut::<RunnerApplyBuffers>();
        if should_apply_buffers.0 {
            // reset flag
            should_apply_buffers.0 = false;

            // apply commands in topological order
            // TODO: determinism
            for index in self.unapplied_systems.ones() {
                let system = self.inner.systems.get_mut(index).unwrap();
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
