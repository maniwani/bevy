mod multi_threaded;
mod single_threaded;

use crate::{
    schedule::{
        runner::single_threaded::SingleThreadedRunner, BoxedRunCondition, RegistryId, SystemLabel,
        SystemRegistry,
    },
    system::BoxedSystem,
    world::World,
};

#[cfg(feature = "trace")]
use bevy_utils::tracing::Instrument;
use bevy_utils::HashMap;

use downcast_rs::{impl_downcast, Downcast};
use fixedbitset::FixedBitSet;

use self::single_threaded::SimpleRunner;

/// Types that can run [`System`] instances on the data stored in a [`World`].
pub trait SystemRunner: Downcast + Send + Sync {
    fn run(&mut self, world: &mut World);
}

impl_downcast!(SystemRunner);

/// Internal resource used to signal the executor to apply the buffers of
/// pending systems in the currently running schedule.
pub(crate) struct RunnerApplyBuffers(pub bool);

impl Default for RunnerApplyBuffers {
    fn default() -> Self {
        Self(false)
    }
}

/// Applies the buffers of all pending systems in the currently running schedule
/// (in the order specified by the schedule).
// TODO: Remind user to apply commands before running a nested schedule.
pub fn apply_buffers(world: &mut World) {
    // Best place to do this check.
    world.check_change_ticks();

    let mut should_apply_buffers = world
        .get_resource_mut::<RunnerApplyBuffers>()
        .expect("RunnerApplyBuffers resource does not exist.");

    // The executor resets this to false, so if it's true, something is weird.
    assert!(
        !should_apply_buffers.0,
        "pending commands should have been applied"
    );
    should_apply_buffers.0 = true;
}

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
    pub(crate) systems_topsort: Vec<RegistryId>,
    /// A list of sets (their ids), topsorted according to hierarchy graph.
    pub(crate) sets_topsort: Vec<RegistryId>,
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

/// Runs the system or system set given by `label` on the world.
pub fn run_systems(label: impl SystemLabel, world: &mut World) {
    let mut reg = world.resource_mut::<SystemRegistry>();
    let id = *reg.ids.get(&label.dyn_clone()).expect("unknown label");
    match id {
        RegistryId::System(_) => {
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
        RegistryId::Set(_) => {
            let mut reg = world.resource_mut::<SystemRegistry>();
            let mut runner = reg.runners.get_mut(&id).unwrap().take().unwrap();
            assert!(runner.systems.is_empty());
            assert!(runner.system_conditions.is_empty());
            assert!(runner.set_conditions.is_empty());

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
