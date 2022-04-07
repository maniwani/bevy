mod multi_threaded;
mod single_threaded;

use crate::{
    schedule::{BoxedRunCondition, RegId, SystemLabel, SystemRegistry},
    system::{AsSystemLabel, BoxedSystem},
    world::World,
};

use self::single_threaded::{SimpleRunner, SingleThreadedRunner};

#[cfg(feature = "trace")]
use bevy_utils::tracing::Instrument;
use bevy_utils::HashMap;

use fixedbitset::FixedBitSet;

use std::cell::RefCell;

/// Types that can run [`System`] instances on the data stored in a [`World`].
pub(super) trait Runner: Send + Sync {
    fn init(&mut self, schedule: &mut Schedule);
    fn run(&mut self, schedule: &mut Schedule, world: &mut World);
}

/// Internal resource used by [`apply_buffers`] to signal the runner to apply the buffers
/// of all completed but unapplied systems to the [`World`].
///
/// **Note** this only applies systems within the set being run and their buffers
/// are applied in topological order.
pub(super) struct RunnerApplyBuffers(pub bool);

impl Default for RunnerApplyBuffers {
    fn default() -> Self {
        Self(false)
    }
}

/// Signals the runner to call [`apply_buffers`](crate::System::apply_buffers) for all
/// completed but unapplied systems on the [`World`].
///
/// **Note** this only applies systems within the set being run and their buffers
/// are applied in topological order.
pub fn apply_buffers(world: &mut World) {
    let mut should_apply_buffers = world.resource_mut::<RunnerApplyBuffers>();
    assert!(
        !should_apply_buffers.0,
        "some buffers did not get applied when expected"
    );
    should_apply_buffers.0 = true;
    world.check_change_ticks();
}

/// Temporary container and schematic for running a collection of systems.
pub(super) struct Schedule {
    /// List of systems, topsorted according to system dependency graph.
    pub(super) systems: Vec<RefCell<BoxedSystem>>,
    /// List of systems' conditions, topsorted according to system dependency graph.
    pub(super) system_conditions: Vec<RefCell<Vec<BoxedRunCondition>>>,
    /// List of sets' conditions, topsorted according to set hierarchy graph.
    pub(super) set_conditions: Vec<RefCell<Vec<BoxedRunCondition>>>,
    /// Map relating each system index to its number of dependencies and list of dependents.
    pub(super) system_deps: HashMap<usize, (usize, Vec<usize>)>,
    /// Map relating each system index to a bitset of its ancestor sets.
    pub(super) system_sets: HashMap<usize, FixedBitSet>,
    /// Map relating each set index to a bitset of its descendant systems.
    pub(super) set_systems: HashMap<usize, FixedBitSet>,
    /// List of systems (their ids), topsorted according to dependency graph.
    /// Used to return systems and their conditions to the registry.
    pub(super) systems_topsort: Vec<RegId>,
    /// List of sets (their ids), topsorted according to hierarchy graph.
    /// Used to return set conditions to the registry.
    pub(super) sets_topsort: Vec<RegId>,
}

// SAFETY: the cell-wrapped data is only accessed by one thread at a time (runner thread)
unsafe impl Sync for Schedule {}

impl Default for Schedule {
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

/// Runs the system or system set given by `label` on the world.
pub fn run_scheduled<M>(label: impl AsSystemLabel<M>, world: &mut World) {
    let mut reg = world.resource_mut::<SystemRegistry>();
    let id = *reg
        .ids
        .get(&label.as_system_label().dyn_clone())
        .expect("unknown label");
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
            // Take things out of the registry.
            // TODO: Refresh set metadata and confirm nothing is missing from registry.
            let mut reg = world.resource_mut::<SystemRegistry>();
            let mut schedule = reg.set_schedules.get_mut(&id).unwrap().take().unwrap();
            assert!(schedule.systems.is_empty());
            assert!(schedule.system_conditions.is_empty());
            assert!(schedule.set_conditions.is_empty());

            for sys_id in schedule.systems_topsort.iter() {
                let system = reg.systems.get_mut(&sys_id).unwrap().take().unwrap();
                schedule.systems.push(RefCell::new(system));

                let conditions = reg
                    .system_conditions
                    .get_mut(&sys_id)
                    .unwrap()
                    .take()
                    .unwrap();

                schedule.system_conditions.push(RefCell::new(conditions));
            }

            for set_id in schedule.sets_topsort.iter() {
                let conditions = reg.set_conditions.get_mut(&set_id).unwrap().take().unwrap();
                schedule.set_conditions.push(RefCell::new(conditions));
            }

            // TODO: Select from simple, single-threaded, and multi-threaded.
            let mut runner = SingleThreadedRunner::new();
            runner.init(&mut schedule);
            runner.run(&mut schedule, world);

            // Return things to the registry.
            let sys_iter = schedule.systems_topsort.iter().zip(
                schedule
                    .systems
                    .drain(..)
                    .zip(schedule.system_conditions.drain(..)),
            );

            let sub_iter = schedule
                .sets_topsort
                .iter()
                .zip(schedule.set_conditions.drain(..));

            // If user removed anything that was just running, everything will be dropped here.
            // No risk of overwriting valid data since the registry ids are unique (`u64`).
            // TODO: Log message when stuff gets dropped.
            let mut reg = world.resource_mut::<SystemRegistry>();
            for (sys_id, (system, conditions)) in sys_iter {
                reg.systems
                    .get_mut(&sys_id)
                    .map(|container| container.insert(system.into_inner()));

                reg.system_conditions
                    .get_mut(&sys_id)
                    .map(|container| container.insert(conditions.into_inner()));
            }

            for (set_id, conditions) in sub_iter {
                reg.set_conditions
                    .get_mut(&set_id)
                    .map(|container| container.insert(conditions.into_inner()));
            }

            reg.set_schedules
                .get_mut(&id)
                .map(|container| container.insert(schedule));
        }
    }
}
