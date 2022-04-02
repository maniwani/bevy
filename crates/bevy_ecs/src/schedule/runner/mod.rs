mod multi_threaded;
mod single_threaded;

use crate::{
    schedule::{BoxedRunCondition, RegId, SystemLabel, SystemRegistry},
    system::BoxedSystem,
    world::World,
};

use self::single_threaded::{SimpleRunner, SingleThreadedRunner};

#[cfg(feature = "trace")]
use bevy_utils::tracing::Instrument;
use bevy_utils::HashMap;

use downcast_rs::{impl_downcast, Downcast};
use fixedbitset::FixedBitSet;

/// Types that can run [`System`] instances on the data stored in a [`World`].
pub trait SystemRunner: Downcast + Send + Sync {
    fn run(&mut self, world: &mut World);
}

impl_downcast!(SystemRunner);

/// Internal resource used by [`apply_buffers`] to signal the runner to apply the buffers
/// of all completed but "unflushed" systems to the [`World`].
///
/// **Note** that it is only systems under the schedule being run and their buffers
/// are applied in topological order.
pub(super) struct RunnerApplyBuffers(pub bool);

impl Default for RunnerApplyBuffers {
    fn default() -> Self {
        Self(false)
    }
}

/// Signals the runner to call [`apply_buffers`](crate::System::apply_buffers) for all
/// completed but "unflushed" systems on the [`World`].
///
/// **Note** that it is only systems under the schedule being run and their buffers
/// are applied in topological order.
pub fn apply_buffers(world: &mut World) {
    let mut should_apply_buffers = world.resource_mut::<RunnerApplyBuffers>();
    assert!(!should_apply_buffers.0, "pending commands not applied");
    should_apply_buffers.0 = true;
    world.check_change_ticks();
}

/// Temporarily takes ownership of systems and the schematic for running them in order
/// and testing their conditions.
pub(crate) struct Runner {
    /// A list of systems, topsorted according to system dependency graph.
    pub(crate) systems: Vec<BoxedSystem>,
    /// A list of system run criteria, topsorted according to system dependency graph.
    pub(crate) system_conditions: Vec<Vec<BoxedRunCondition>>,
    /// A list of set run criteria, topsorted according to set hierarchy graph.
    pub(crate) set_conditions: Vec<Vec<BoxedRunCondition>>,
    /// A map relating each system index to its number of dependencies and list of dependents.
    pub(crate) system_deps: HashMap<usize, (usize, Vec<usize>)>,
    /// A map relating each system index to a bitset of its ancestor sets.
    pub(crate) system_sets: HashMap<usize, FixedBitSet>,
    /// A map relating each set index to a bitset of its descendant systems.
    pub(crate) set_systems: HashMap<usize, FixedBitSet>,
    /// A list of systems (their ids), topsorted according to dependency graph.
    /// Used to return systems and their conditions to the registry.
    pub(crate) systems_topsort: Vec<RegId>,
    /// A list of sets (their ids), topsorted according to hierarchy graph.
    /// Used to return set conditions to the registry.
    pub(crate) sets_topsort: Vec<RegId>,
}

impl Default for Runner {
    fn default() -> Self {
        Self {
            systems_topsort: Vec::new(),
            systems: Vec::new(),
            system_conditions: Vec::new(),
            system_deps: HashMap::new(),
            system_sets: HashMap::new(),
            sets_topsort: Vec::new(),
            set_conditions: Vec::new(),
            set_systems: HashMap::new(),
        }
    }
}

impl Runner {
    pub(crate) fn simple(self) -> SimpleRunner {
        SimpleRunner::with(self)
    }

    pub(crate) fn single_threaded(self) -> SingleThreadedRunner {
        SingleThreadedRunner::with(self)
    }
}

/// Runs the system or system set given by `schedule` on the world.
pub fn run_scheduled(schedule: impl SystemLabel, world: &mut World) {
    let mut reg = world.resource_mut::<SystemRegistry>();
    let id = *reg.ids.get(&schedule.dyn_clone()).expect("unknown label");
    match id {
        RegId::System(_) => {
            // TODO: Need to confirm that system is available before taking ownership.
            let mut reg = world.resource_mut::<SystemRegistry>();
            let mut system = reg.systems.get_mut(&id).unwrap().take().unwrap();
            let mut system_conditions = reg.system_conditions.get_mut(&id).unwrap().take().unwrap();

            let mut should_run = true;
            for cond in system_conditions.iter_mut() {
                should_run &= cond.run((), world);
            }

            if should_run {
                system.run((), world);
                system.apply_buffers(world);
            }

            let mut reg = world.resource_mut::<SystemRegistry>();
            reg.systems
                .get_mut(&id)
                .map(|container| container.insert(system));

            reg.system_conditions
                .get_mut(&id)
                .map(|container| container.insert(system_conditions));
        }
        RegId::Set(_) => {
            let mut reg = world.resource_mut::<SystemRegistry>();
            let mut runner = reg.runners.get_mut(&id).unwrap().take().unwrap();
            assert!(runner.systems.is_empty());
            assert!(runner.system_conditions.is_empty());
            assert!(runner.set_conditions.is_empty());

            // TODO: Need to refresh set metadata first.
            // TODO: Then, need to confirm that all its systems are available before taking ownership.
            for sys_id in runner.systems_topsort.iter() {
                runner
                    .systems
                    .push(reg.systems.get_mut(&sys_id).unwrap().take().unwrap());

                runner.system_conditions.push(
                    reg.system_conditions
                        .get_mut(&sys_id)
                        .unwrap()
                        .take()
                        .unwrap(),
                );
            }

            for set_id in runner.sets_topsort.iter() {
                runner
                    .set_conditions
                    .push(reg.set_conditions.get_mut(&set_id).unwrap().take().unwrap());
            }

            let mut runner = runner.single_threaded();
            runner.run(world);
            let mut runner = runner.into_inner();

            let sys_iter = runner.systems_topsort.iter().zip(
                runner
                    .systems
                    .drain(..)
                    .zip(runner.system_conditions.drain(..)),
            );

            let sub_iter = runner
                .sets_topsort
                .iter()
                .zip(runner.set_conditions.drain(..));

            // If the systems and sets were removed while they were running,
            // they'll just be dropped here.
            // TODO: Say something if container wasn't empty.
            // TODO: User might be doing something weird (removing systems then re-adding under same name).
            let mut reg = world.resource_mut::<SystemRegistry>();
            for (sys_id, (system, run_criteria)) in sys_iter {
                reg.systems
                    .get_mut(&sys_id)
                    .map(|container| container.insert(system));

                reg.system_conditions
                    .get_mut(&sys_id)
                    .map(|container| container.insert(run_criteria));
            }

            for (set_id, run_criteria) in sub_iter {
                reg.set_conditions
                    .get_mut(&set_id)
                    .map(|container| container.insert(run_criteria));
            }

            reg.runners
                .get_mut(&id)
                .map(|container| container.insert(runner));
        }
    }
}
