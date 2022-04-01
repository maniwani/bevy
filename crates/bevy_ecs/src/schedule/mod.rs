#![allow(warnings)]
//! Tools for controlling system execution.
//!
//! When using Bevy ECS, systems are usually run from a [`SystemRegistry`], not directly.
//!
//! # Running systems in order
//!
//! - Systems can be arranged through a combination of `.label()`, `.before()`, and `.after()` methods.
//! - When systems have an undefined order, their command queues may be applied in a different order each app run.

mod condition;
mod descriptor;
mod fsm;
mod graph_utils;
mod label;
mod runner;

pub use self::condition::*;
pub use self::descriptor::*;
pub use self::fsm::*;
use self::graph_utils::*;
pub use self::label::*;
pub use self::runner::*;

use bevy_utils::{
    tracing::{error, warn},
    HashMap, HashSet,
};

use crate::{
    change_detection::CHECK_TICK_THRESHOLD,
    component::ComponentId,
    query::Access,
    system::BoxedSystem,
    world::{World, WorldId},
};

use std::collections::VecDeque;
use std::{
    borrow::Cow,
    fmt::{Debug, Write},
};

use fixedbitset::FixedBitSet;
use petgraph::{
    algo::tarjan_scc,
    dot::{Config as DotConfig, Dot},
    prelude::*,
};
use thiserror::Error;

enum Cycle {
    Dependency,
    Hierarchy,
}

// pub enum ResponseLevel {
//     Ignore,
//     Warn,
//     Error,
// }

// pub struct ScheduleBuildChecker {
//     /// Sets response to [`Ambiguity`](crate::schedule::ScheduleBuildError::Ambiguity) errors.
//     access_conflicts: ResponseLevel,
//     /// Sets response to [`InvalidDependency`](crate::schedule::ScheduleBuildError::InvalidDependency) errors.
//     invalid_dependencies: ResponseLevel,
//     /// Sets response to [`CrossDependency`](crate::schedule::ScheduleBuildError::CrossDependency) errors.
//     cross_edges: ResponseLevel,
// }

// impl Default for ScheduleBuildChecker {
//     fn default() -> Self {
//         Self {
//             access_conflicts: ResponseLevel::Warn,
//             invalid_dependencies: ResponseLevel::Warn,
//             cross_edges: ResponseLevel::Warn,
//         }
//     }
// }

/// A schematic for running a `System` collection on a `World`.
pub(crate) struct SetMetadata {
    /// A graph containing all systems and sets directly under this set.
    graph: DiGraphMap<RegistryId, ()>,
    /// A graph containing all systems under this set.
    flat: DiGraphMap<RegistryId, ()>,
    /// The sub-hierarchy under this set.
    hier: DiGraphMap<RegistryId, ()>,
    /// A cached topological order for `graph`.
    topsort: Vec<RegistryId>,
    /// A cached topological order for `flat`.
    flat_topsort: Vec<RegistryId>,
    /// A cached topological order for `hier`.
    hier_topsort: Vec<RegistryId>,
    /// The combined component access of all systems under this set.
    component_access: Access<ComponentId>,
    /// Does this metadata needs to be rebuilt before set can run again?
    modified: bool,
    executor_modified: bool,
}

impl Default for SetMetadata {
    fn default() -> Self {
        Self {
            graph: DiGraphMap::new(),
            flat: DiGraphMap::new(),
            hier: DiGraphMap::new(),
            topsort: vec![],
            flat_topsort: vec![],
            hier_topsort: vec![],
            component_access: Access::default(),
            modified: true,
            executor_modified: true,
        }
    }
}

pub struct Builder<'a> {
    registry: &'a mut SystemRegistry,
}

impl Builder<'_> {
    pub fn add_system<P>(&mut self, system: impl IntoSystemDescriptor<P>) {
        self.registry.add_system(system);
    }

    pub fn add_set(&mut self, set: impl IntoSetDescriptor) {
        self.registry.add_set(set);
    }

    pub fn add_many(&mut self, nodes: impl IntoIterator<Item = Descriptor>) {
        self.registry.add_many(nodes);
    }

    pub fn add_states<S: State>(&mut self, states: impl IntoIterator<Item = S>) {
        self.registry.add_states(states);
    }

    pub fn contains(&self, label: impl SystemLabel) -> bool {
        self.registry.contains(label)
    }
}

/// Stores systems.
pub struct SystemRegistry {
    world_id: Option<WorldId>,
    next_id: u64,
    ids: HashMap<BoxedSystemLabel, RegistryId>,
    // names: HashMap<RegistryId, BoxedSystemLabel>,
    hier: DiGraphMap<RegistryId, ()>,
    pub(crate) systems: HashMap<RegistryId, Option<BoxedSystem>>,
    pub(crate) system_conditions: HashMap<RegistryId, Option<Vec<BoxedRunCondition>>>,
    pub(crate) runners: HashMap<RegistryId, Option<Runner>>,
    pub(crate) set_conditions: HashMap<RegistryId, Option<Vec<BoxedRunCondition>>>,
    pub(crate) set_metadata: HashMap<RegistryId, SetMetadata>,
    uninit_nodes: Vec<GraphConfig>,
}

impl Default for SystemRegistry {
    fn default() -> Self {
        Self {
            world_id: None,
            next_id: 0,
            ids: HashMap::new(),
            hier: DiGraphMap::new(),
            systems: HashMap::new(),
            system_conditions: HashMap::new(),
            runners: HashMap::new(),
            set_conditions: HashMap::new(),
            set_metadata: HashMap::new(),
            uninit_nodes: Vec::new(),
        }
    }
}

impl SystemRegistry {
    /// Constructs an empty `SystemRegistry`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a [`System`](crate::system::System) and returns its ID.
    pub fn add_system<P>(&mut self, system: impl IntoSystemDescriptor<P>) -> RegistryId {
        let SystemDescriptor {
            system,
            mut config,
            run_criteria,
        } = system.into_descriptor();

        if config.name().is_none() {
            config.name = Some(system.name().dyn_clone());
        }

        let name = config.name().unwrap();
        assert!(!self.ids.contains_key(name), "name already used");

        let id = RegistryId::System(self.next_id);
        self.next_id += 1;
        self.ids.insert(name.dyn_clone(), id);

        self.systems.insert(id, Some(system));
        self.system_conditions.insert(id, Some(run_criteria));
        self.uninit_nodes.push(config);

        id
    }

    /// Registers a [`System`](crate::system::System) set and returns its ID.
    pub fn add_set(&mut self, set: impl IntoSetDescriptor) -> RegistryId {
        let SetDescriptor {
            config,
            run_criteria,
        } = set.into_descriptor();

        let name = config.name().unwrap();
        assert!(!self.ids.contains_key(name), "name already used");

        let id = RegistryId::Set(self.next_id);
        self.next_id += 1;
        self.ids.insert(name.dyn_clone(), id);

        self.set_metadata.insert(id, SetMetadata::default());
        self.set_conditions.insert(id, Some(run_criteria));
        self.runners.insert(id, None);
        self.uninit_nodes.push(config);

        id
    }

    /// Registers multiple systems and system sets at once and returns their IDs.
    pub fn add_many(&mut self, nodes: impl IntoIterator<Item = Descriptor>) -> Vec<RegistryId> {
        let ids = nodes
            .into_iter()
            .map(|desc| match desc {
                Descriptor::System(system) => self.add_system(system),
                Descriptor::Set(set) => self.add_set(set),
            })
            .collect();

        ids
    }

    /// Registers an "on enter" and an "on exit" system set for each state.
    pub fn add_states<S: State>(&mut self, states: impl IntoIterator<Item = S>) {
        states.into_iter().for_each(|state| {
            self.add_set(OnEnter(state.clone()));
            self.add_set(OnExit(state.clone()));
        });
    }

    /// Returns `true` if the registry contains a system or set named `label`.
    pub fn contains(&self, label: impl SystemLabel) -> bool {
        self.ids.contains_key(&label.dyn_clone())
    }
}

// internal construction
impl SystemRegistry {
    fn refresh(&mut self, label: impl SystemLabel, world: &mut World) -> Result<()> {
        if let Some(world_id) = self.world_id {
            assert!(
                world_id == world.id(),
                "cannot initialize systems on a different world"
            );
        } else {
            self.world_id = Some(world.id());
        }

        assert!(
            self.systems.len() <= CHECK_TICK_THRESHOLD as usize,
            "too many systems"
        );

        if !self.uninit_nodes.is_empty() {
            self.initialize(world)?;
        }

        let id = *self.ids.get(&label.dyn_clone()).unwrap();
        if let Some(set) = self.set_metadata.get(&id) {
            if set.modified {
                self.walk_down(id)?;
            }
        }

        Ok(())
    }

    fn initialize(&mut self, world: &mut World) -> Result<()> {
        // check for obvious errors
        for config in self.uninit_nodes.iter() {
            for label in config.sets().iter() {
                if config.name() == Some(label) {
                    return Err(ScheduleBuildError::HierarchyLoop);
                }
                if self.ids.get(label).is_none() {
                    return Err(ScheduleBuildError::UnknownSetLabel);
                }
                if self.ids.get(label).unwrap().is_system() {
                    return Err(ScheduleBuildError::InvalidSetLabel);
                }
            }
            for (_order, label) in config.edges().iter() {
                if config.name() == Some(label) {
                    return Err(ScheduleBuildError::DependencyLoop);
                }
                if self.ids.get(label).is_none() {
                    return Err(ScheduleBuildError::UnknownDependencyLabel);
                }
            }
        }

        // convert labels to ids
        let configs_indexed = self
            .uninit_nodes
            .drain(..)
            .map(|config| {
                let name = config.name().unwrap();
                let id = *self.ids.get(name).unwrap();

                let sets = config
                    .sets()
                    .iter()
                    .map(|label| *self.ids.get(label).unwrap())
                    .collect::<HashSet<_>>();
                let edges = config
                    .edges()
                    .iter()
                    .map(|(order, label)| (*order, *self.ids.get(label).unwrap()))
                    .collect::<Vec<_>>();

                (id, IndexedGraphConfig { sets, edges })
            })
            .collect::<HashMap<_, _>>();

        // init the systems
        for (id, config) in configs_indexed.iter() {
            match id {
                RegistryId::System(_) => {
                    let system = self.systems.get_mut(&id).unwrap();
                    system.as_mut().unwrap().initialize(world);
                    let conditions = self.system_conditions.get_mut(&id).unwrap();
                    conditions
                        .iter_mut()
                        .flatten()
                        .for_each(|system| system.initialize(world));
                }
                RegistryId::Set(_) => {
                    let conditions = self.set_conditions.get_mut(&id).unwrap();
                    conditions
                        .iter_mut()
                        .flatten()
                        .for_each(|system| system.initialize(world));
                }
            }
        }

        // add nodes to hierarchy
        for (&id, config) in configs_indexed.iter() {
            self.hier.add_node(id);
            for &set_id in config.sets().iter() {
                self.hier.add_edge(set_id, id, ());
            }

            // mark all sets above as modified
            self.walk_up(id)?;

            for &set_id in config.sets().iter() {
                let set = self.set_metadata.get_mut(&set_id).unwrap();
                set.graph.add_node(id);
            }

            for &(order, other_id) in config.edges().iter() {
                let (before, after) = match order {
                    Order::Before => (id, other_id),
                    Order::After => (other_id, id),
                };

                let other_sets = configs_indexed.get(&other_id).unwrap().sets();
                if config.sets().is_disjoint(other_sets) {
                    // TODO: consider allowing when satisfiable
                    return Err(ScheduleBuildError::CrossDependency);
                } else {
                    for set_id in config.sets().intersection(other_sets) {
                        let set = self.set_metadata.get_mut(&set_id).unwrap();
                        set.graph.add_edge(before, after, ());
                    }
                }
            }
        }

        Ok(())
    }

    fn walk_up(&mut self, id: RegistryId) -> Result<()> {
        #[cfg(feature = "trace")]
        let _guard = bevy_utils::tracing::info_span!("propagate change").entered();

        // BFS
        let mut queue = VecDeque::new();
        queue.push_back(id);
        let mut visited = HashSet::new();
        visited.insert(id);

        while let Some(id) = queue.pop_front() {
            if id.is_set() {
                let set = self.set_metadata.get_mut(&id).unwrap();
                if set.modified {
                    continue;
                } else {
                    set.modified = true;
                    set.executor_modified = true;
                }
            }

            for parent_id in self.hier.neighbors_directed(id, Direction::Incoming) {
                assert!(parent_id.is_set());
                if !visited.contains(&parent_id) {
                    visited.insert(parent_id);
                    queue.push_back(parent_id);
                }
            }
        }

        Ok(())
    }

    fn walk_down(&mut self, id: RegistryId) -> Result<()> {
        assert!(id.is_set(), "only sets can have nodes below them");
        #[cfg(feature = "trace")]
        let _guard = bevy_utils::tracing::info_span!("rebuild set graph").entered();

        let mut sub_hier = DiGraphMap::<RegistryId, ()>::new();

        // BFS
        let mut queue = VecDeque::new();
        queue.push_back(id);
        let mut visited = HashSet::new();
        visited.insert(id);

        while let Some(id) = queue.pop_front() {
            for child_id in self.hier.neighbors_directed(id, Direction::Outgoing) {
                sub_hier.add_edge(id, child_id, ());
                if child_id.is_set() {
                    if !visited.contains(&child_id) {
                        visited.insert(child_id);
                        queue.push_back(child_id);
                    }
                }
            }
        }

        // intersecting sets must be ambiguous
        let mut intersecting_sets = HashMap::new();
        for id in sub_hier.nodes() {
            let parents = sub_hier
                .neighbors_directed(id, Direction::Incoming)
                .collect::<Vec<_>>();

            for (i, &a) in parents.iter().enumerate() {
                assert!(a.is_set());
                for &b in parents.iter().skip(i + 1) {
                    assert!(b.is_set());
                    intersecting_sets
                        .entry(sort_pair(a, b))
                        .or_insert_with(HashSet::new)
                        .insert(id);
                }
            }
        }

        // topsort
        // we'll build up everything in reverse topological order
        let scc = tarjan_scc(&sub_hier);
        self.check_graph_cycles(id, &sub_hier, &scc, Cycle::Hierarchy)?;
        let topsort = scc.into_iter().flatten().rev().collect::<Vec<_>>();

        let sets_topsort = topsort
            .iter()
            .cloned()
            .filter_map(|id| if id.is_set() { Some(id) } else { None })
            .collect::<Vec<_>>();

        let result = check_graph(&sub_hier, &topsort);
        self.check_hierarchy(&result.transitive_edges)?;
        self.check_ambiguous(&sub_hier, &intersecting_sets, &result.ambiguities)?;

        // update component access
        for &set_id in sets_topsort.iter().rev() {
            let set = self.set_metadata.get(&set_id).unwrap();
            let children = set.graph.nodes().collect::<Vec<_>>();
            for child in children.iter() {
                match child {
                    RegistryId::System(_) => {
                        let set = self.set_metadata.get_mut(&set_id).unwrap();
                        let system = self.systems.get(&child).unwrap();
                        let access = system.as_ref().unwrap().component_access();
                        set.component_access.extend(&access);
                    }
                    RegistryId::Set(_) => {
                        let [set, subset] =
                            self.set_metadata.get_many_mut([&set_id, &child]).unwrap();
                        set.component_access.extend(&subset.component_access);
                    }
                }
            }
        }

        // flatten (to system nodes and system-system edges only)
        for &set_id in sets_topsort.iter().rev() {
            let mut flat = DiGraphMap::<RegistryId, ()>::new();
            let set = self.set_metadata.get(&set_id).unwrap();
            for child in set.graph.nodes() {
                match child {
                    RegistryId::System(_) => {
                        flat.add_node(child);
                    }
                    RegistryId::Set(_) => {
                        let subset = self.set_metadata.get(&child).unwrap();
                        flat.extend(subset.flat.all_edges());
                    }
                }
            }

            for (before, after, _) in set.graph.all_edges() {
                match (before, after) {
                    (RegistryId::System(_), RegistryId::System(_)) => {
                        flat.add_edge(before, after, ());
                    }
                    (RegistryId::Set(_), RegistryId::System(_)) => {
                        for u in self.set_metadata.get(&before).unwrap().flat.nodes() {
                            flat.add_edge(u, after, ());
                        }
                    }
                    (RegistryId::System(_), RegistryId::Set(_)) => {
                        for v in self.set_metadata.get(&after).unwrap().flat.nodes() {
                            flat.add_edge(before, v, ());
                        }
                    }
                    (RegistryId::Set(_), RegistryId::Set(_)) => {
                        for u in self.set_metadata.get(&before).unwrap().flat.nodes() {
                            for v in self.set_metadata.get(&after).unwrap().flat.nodes() {
                                flat.add_edge(u, v, ());
                            }
                        }
                    }
                }
            }

            let subgraph = self.set_metadata.get_mut(&set_id).unwrap();
            subgraph.flat = flat;
        }

        // topsort graphs and check for errors
        // topsort flat graphs and check for errors
        // runner data

        for &set_id in sets_topsort.iter().rev() {
            let set = self.set_metadata.get(&set_id).unwrap();
            let scc = tarjan_scc(&set.graph);
            self.check_graph_cycles(set_id, &set.graph, &scc, Cycle::Dependency)?;
        }

        let set = self.set_metadata.get_mut(&id).unwrap();
        set.hier = sub_hier;
        set.hier_topsort = topsort;

        if set.executor_modified {
            set.executor_modified = false;
            self.rebuild_set_executor(id);
        }

        Ok(())
    }
}

impl SystemRegistry {
    pub(crate) fn rebuild_set_executor(&mut self, set_id: RegistryId) {
        assert!(set_id.is_set());
        let set = self.set_metadata.get(&set_id).unwrap();
        let sys_count = set.flat.node_count();
        let set_count = set.hier.node_count();

        // println!("graph");
        // println!(
        //     "{:?}",
        //     Dot::with_config(&set.graph, &[DotConfig::EdgeNoLabel])
        // );
        // println!("flat");
        // println!(
        //     "{:?}",
        //     Dot::with_config(&set.flat, &[DotConfig::EdgeNoLabel])
        // );
        // println!("hier");
        // println!(
        //     "{:?}",
        //     Dot::with_config(&set.hier, &[DotConfig::EdgeNoLabel])
        // );

        let systems_topsort = set.flat_topsort.clone();
        let sys_idxs_topsort = systems_topsort
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, sys_id)| (sys_id, i))
            .collect::<HashMap<_, _>>();

        let (set_idxs_topsort, sets_topsort): (Vec<_>, Vec<_>) = set
            .hier_topsort
            .iter()
            .cloned()
            .enumerate()
            .filter_map(|(i, id)| if id.is_set() { Some((i, id)) } else { None })
            .unzip();

        let result = check_graph(&set.hier, &set.hier_topsort);

        // number of dependencies and the immediate dependents of each system
        let mut system_deps = HashMap::with_capacity(sys_count);
        for (i, &sys_id) in systems_topsort.iter().enumerate() {
            let num_dependencies = set
                .flat
                .neighbors_directed(sys_id, Direction::Incoming)
                .count();
            let dependents = set
                .flat
                .neighbors_directed(sys_id, Direction::Outgoing)
                .map(|id| *sys_idxs_topsort.get(&id).unwrap())
                .collect::<Vec<_>>();
            system_deps.insert(i, (num_dependencies, dependents));
        }

        // systems below each set
        let mut set_systems = HashMap::with_capacity(set_count);
        for (set_idx, set_id) in sets_topsort.iter().enumerate() {
            let mut bitset = FixedBitSet::with_capacity(sys_count);
            let set = self.set_metadata.get(&set_id).unwrap();
            for sys_id in set.flat_topsort.iter() {
                let sys_idx = *sys_idxs_topsort.get(&sys_id).unwrap();
                bitset.set(sys_idx, true);
            }
            set_systems.insert(set_idx, bitset);
        }

        // sets above each set
        // nodes were topsorted, so graph reachability matrix is upper triangular
        let mut above_sets = HashMap::with_capacity(set_count);
        for (new_col, &old_col) in set_idxs_topsort.iter().enumerate() {
            let mut bitset = FixedBitSet::with_capacity(set_count);
            for (new_row, &old_row) in set_idxs_topsort.iter().enumerate().take(new_col) {
                bitset.set(
                    new_row,
                    result.reachable[index(old_row, old_col, sys_count + set_count)],
                );
            }
            above_sets.insert(sets_topsort[new_col], bitset);
        }

        // sets above each system
        // TODO: avoid redundant copies of the same bitset
        let mut system_sets = HashMap::with_capacity(sys_count);
        for (sys_idx, &sys_id) in systems_topsort.iter().enumerate() {
            let mut bitset = FixedBitSet::with_capacity(set_count);
            for set_id in set.hier.neighbors_directed(sys_id, Direction::Incoming) {
                assert!(set_id.is_set());
                bitset.union_with(&above_sets.get(&set_id).unwrap());
            }
            system_sets.insert(sys_idx, bitset);
        }

        self.runners.insert(
            set_id,
            Some(Runner {
                systems: Vec::with_capacity(sys_count),
                system_conditions: Vec::with_capacity(sys_count),
                set_conditions: Vec::with_capacity(set_count),
                systems_topsort,
                sets_topsort,
                system_deps,
                system_sets,
                set_systems,
            }),
        );
    }
}

// helper methods
impl SystemRegistry {
    fn get_node_name(&self, id: RegistryId) -> Cow<'static, str> {
        match id {
            RegistryId::System(_) => "mysterious system".into(),
            RegistryId::Set(_) => "mysterious set".into(),
        }
    }

    fn check_hierarchy(&self, transitive_edges: &Vec<(RegistryId, RegistryId)>) -> Result<()> {
        if transitive_edges.is_empty() {
            return Ok(());
        }

        let mut message =
            String::from("system set hierarchy contains transitive (redundant) edge(s)");
        for &(parent, child) in transitive_edges.iter() {
            writeln!(
                message,
                " -- {:?} '{:?}' is already under set '{:?}' from a longer path",
                child.to_str(),
                self.get_node_name(child),
                self.get_node_name(parent),
            )
            .unwrap();
        }

        error!("{}", message);
        Err(ScheduleBuildError::InvalidHierarchy)
    }

    fn check_ambiguous(
        &self,
        graph: &DiGraphMap<RegistryId, ()>,
        intersecting_sets: &HashMap<(RegistryId, RegistryId), HashSet<RegistryId>>,
        ambiguities: &HashSet<(RegistryId, RegistryId)>,
    ) -> Result<()> {
        let required_ambiguous = intersecting_sets.keys().cloned().collect();
        let actual_ambiguous = ambiguities
            .iter()
            .filter_map(|&(a, b)| {
                if a.is_set() && b.is_set() {
                    Some(sort_pair(a, b))
                } else {
                    None
                }
            })
            .collect::<HashSet<_>>();

        if actual_ambiguous.is_superset(&required_ambiguous) {
            return Ok(());
        }

        let mut message =
            String::from("intersecting system sets have dependent/hierarchical relation");
        for &(a, b) in required_ambiguous.difference(&actual_ambiguous) {
            let shared_nodes = intersecting_sets.get(&(a, b)).unwrap();
            writeln!(
                message,
                " -- '{:?}' and '{:?}' share these nodes: {:?}",
                a, b, shared_nodes,
            )
            .unwrap();
        }

        error!("{}", message);
        Err(ScheduleBuildError::MissingAmbiguity)
    }

    fn check_conflicts(
        &self,
        intersecting_sets: &HashMap<(RegistryId, RegistryId), HashSet<RegistryId>>,
        ambiguities: &HashSet<(RegistryId, RegistryId)>,
        set_id: RegistryId,
        world: &World,
    ) -> Result<()> {
        let get_combined_access = |id| {
            let mut access = Access::default();
            match id {
                RegistryId::System(_) => {
                    let system = self.systems.get(&id).unwrap();
                    access.extend(system.as_ref().unwrap().component_access());
                    for system in self.system_conditions.get(&id).unwrap().iter().flatten() {
                        access.extend(system.component_access());
                    }
                }
                RegistryId::Set(_) => {
                    access.extend(&self.set_metadata.get(&id).unwrap().component_access);
                    for system in self.set_conditions.get(&id).unwrap().iter().flatten() {
                        access.extend(system.component_access());
                    }
                }
            }

            access
        };

        let mut conflicting_pairs = vec![];
        for &(a, b) in ambiguities.iter() {
            if a.is_set() && b.is_set() {
                if intersecting_sets.contains_key(&sort_pair(a, b)) {
                    // TODO: check for conflicts in symmetric difference
                    continue;
                }
            }

            let conflicts = get_combined_access(a).get_conflicts(&get_combined_access(b));
            if !conflicts.is_empty() {
                conflicting_pairs.push((a, b, conflicts));
            }
        }

        if conflicting_pairs.is_empty() {
            return Ok(());
        }

        let mut message = String::new();
        writeln!(
            message,
            "system set '{:?}' contains {} pairs of nodes with unknown order and conflicting access",
            self.get_node_name(set_id),
            conflicting_pairs.len(),
        )
        .unwrap();

        for (i, (a, b, conflicts)) in conflicting_pairs.iter().enumerate() {
            let mut component_ids = conflicts
                .iter()
                .map(|id| world.components().get_info(*id).unwrap().name())
                .collect::<Vec<_>>();
            component_ids.sort_unstable();

            // TODO: include system fn signatures
            // TODO: include run criteria fn ids and signatures
            writeln!(
                message,
                " -- {}: {} {:?} and {} {:?} conflict on these components: {:?}",
                i,
                a.to_str(),
                self.get_node_name(*a),
                b.to_str(),
                self.get_node_name(*b),
                component_ids,
            )
            .unwrap();
        }

        warn!("{}", message);
        Err(ScheduleBuildError::Ambiguity)
    }

    fn check_graph_cycles(
        &self,
        set_id: RegistryId,
        set_graph: &DiGraphMap<RegistryId, ()>,
        strongly_connected_components: &Vec<Vec<RegistryId>>,
        cycle: Cycle,
    ) -> Result<()> {
        if strongly_connected_components.len() == set_graph.node_count() {
            return Ok(());
        }

        let lower_bound = strongly_connected_components
            .iter()
            .filter(|scc| scc.len() > 1)
            .count();

        let (mut message, error) = match cycle {
            Cycle::Dependency => (
                format!(
                    "graph of system set '{:?}' contains {} (or more) cycle(s)",
                    self.get_node_name(set_id),
                    lower_bound,
                ),
                ScheduleBuildError::DependencyCycle,
            ),
            Cycle::Hierarchy => (
                format!(
                    "hierarchy under system set '{:?}' contains {} (or more) cycle(s)",
                    self.get_node_name(set_id),
                    lower_bound,
                ),
                ScheduleBuildError::HierarchyCycle,
            ),
        };

        writeln!(message, " -- these groups contain at least one cycle each:",).unwrap();

        let iter = strongly_connected_components
            .iter()
            .filter(|scc| scc.len() > 1)
            .enumerate();

        for (i, scc) in iter {
            let ids = scc
                .iter()
                .map(|&node_id| self.get_node_name(node_id))
                .collect::<Vec<_>>();

            writeln!(message, " ---- {}: {:?}", i, ids,).unwrap();
        }

        error!("{}", message);
        Err(error)
    }
}

impl SystemRegistry {
    /// All system change ticks are scanned for risk of age overflow once the world counter
    /// has incremented at least [`CHECK_TICK_THRESHOLD`](crate::change_detection::CHECK_TICK_THRESHOLD)
    /// times since the previous `check_tick` scan.
    ///
    /// During each scan, any change ticks older than [`MAX_CHANGE_AGE`](crate::change_detection::MAX_CHANGE_AGE)
    /// are clamped to that age. This prevents false positives that would appear because of overflow.
    // TODO: parallelize
    fn check_change_ticks(&mut self, world: &mut World) {
        let change_tick = world.change_tick();
        if change_tick.wrapping_sub(world.last_check_tick()) >= CHECK_TICK_THRESHOLD {
            #[cfg(feature = "trace")]
            let span = bevy_utils::tracing::info_span!("check system ticks");
            #[cfg(feature = "trace")]
            let _guard = span.enter();
            for system in self.systems.values_mut().flatten() {
                system.check_change_tick(change_tick);
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, ScheduleBuildError>;

/// Errors that make the system graph unsolvable (some of these can be suppressed).
#[derive(Error, Debug)]
pub enum ScheduleBuildError {
    /// A node was assigned to an unknown set.
    #[error("unknown set")]
    UnknownSetLabel,
    /// A node was ordered with an unknown node.
    #[error("unknown dependency")]
    UnknownDependencyLabel,
    /// A system's name was used as a set label.
    #[error("system label used as set label")]
    InvalidSetLabel,
    /// A set contains itself.
    #[error("set contains itself")]
    HierarchyLoop,
    /// System set hierarchy contains a cycle.
    #[error("set hierarchy contains cycle")]
    HierarchyCycle,
    /// System set hierarchy contains an invalid edge.
    #[error("set hierarchy contains transitive edge")]
    InvalidHierarchy,
    /// A node depends on itself.
    #[error("node depends on itself")]
    DependencyLoop,
    /// Dependency graph contains a cycle.
    #[error("dependency graph contains cycle")]
    DependencyCycle,
    /// Dependency graph has an edge between nodes that do not conflict.
    #[error("node depends on other node but no access conflict")]
    InvalidDependency,
    /// Dependency graph has an edge between nodes that have no set in common.
    #[error("node depends on other node under different set")]
    CrossDependency,
    /// Intersecting system sets were found to have a dependent/hierarchical relation.
    #[error("intersecting sets have dependent/hierarchical relation")]
    MissingAmbiguity,
    /// Parallel nodes have an access conflict.
    #[error("parallel nodes have access conflict")]
    Ambiguity,
}

#[cfg(test)]
mod tests {
    use crate::{
        self as bevy_ecs,
        change_detection::Mut,
        component::Component,
        schedule::*,
        system::{Local, Query, ResMut},
        world::World,
        {chain, group},
    };

    struct Order(pub Vec<usize>);

    #[derive(Component)]
    struct A;

    #[derive(Component)]
    struct B;

    #[derive(Component)]
    struct C;

    #[derive(SystemLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum TestSet {
        All,
        A,
        B,
        C,
        X,
    }

    #[derive(SystemLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum TestSystem {
        Foo,
        Bar,
        Baz,
    }

    fn exclusive(num: usize) -> impl FnMut(&mut World) {
        move |world| world.resource_mut::<Order>().0.push(num)
    }

    fn normal(num: usize) -> impl FnMut(ResMut<Order>) {
        move |mut resource: ResMut<Order>| resource.0.push(num)
    }

    #[test]
    fn system() {
        let mut world = World::new();
        world.insert_resource(SystemRegistry::new());
        let mut reg = world.resource_mut::<SystemRegistry>();

        fn foo() {}
        reg.add_system(foo.named(TestSystem::Foo));

        let result = world.resource_scope(|world, mut reg: Mut<SystemRegistry>| {
            reg.refresh(TestSystem::Foo, world)
        });

        assert!(result.is_ok());
    }

    #[test]
    fn correct_order() {}

    #[test]
    fn invalid_set_label() {
        fn foo() {}
        fn bar() {}

        let mut world = World::new();
        world.insert_resource(SystemRegistry::new());
        let mut reg = world.resource_mut::<SystemRegistry>();

        reg.add_set(TestSet::X);
        reg.add_system(foo.named(TestSystem::Foo));
        reg.add_system(foo.named(TestSystem::Bar).to(TestSystem::Foo));

        let result = world
            .resource_scope(|world, mut reg: Mut<SystemRegistry>| reg.refresh(TestSet::X, world));

        assert!(matches!(result, Err(ScheduleBuildError::InvalidSetLabel)));
    }

    #[test]
    fn dependency_loop() {
        fn foo() {}

        let mut world = World::new();
        world.insert_resource(SystemRegistry::new());
        let mut reg = world.resource_mut::<SystemRegistry>();

        reg.add_system(foo.named(TestSystem::Foo).after(TestSystem::Foo));

        let result = world.resource_scope(|world, mut reg: Mut<SystemRegistry>| {
            reg.refresh(TestSystem::Foo, world)
        });

        assert!(matches!(result, Err(ScheduleBuildError::DependencyLoop)));
    }

    #[test]
    fn dependency_cycle() {
        fn foo() {}
        fn bar() {}
        fn baz() {}

        let mut world = World::new();
        world.insert_resource(SystemRegistry::new());
        let mut reg = world.resource_mut::<SystemRegistry>();

        reg.add_set(TestSet::All);
        reg.add_system(
            foo.named(TestSystem::Foo)
                .after(TestSystem::Baz)
                .to(TestSet::All),
        );
        reg.add_system(
            bar.named(TestSystem::Bar)
                .after(TestSystem::Foo)
                .to(TestSet::All),
        );
        reg.add_system(
            baz.named(TestSystem::Baz)
                .after(TestSystem::Bar)
                .to(TestSet::All),
        );

        let result = world
            .resource_scope(|world, mut reg: Mut<SystemRegistry>| reg.refresh(TestSet::All, world));

        assert!(matches!(result, Err(ScheduleBuildError::DependencyCycle)));
    }

    #[test]
    fn redundant_dependencies() {}

    #[test]
    fn cross_dependencies() {
        fn foo() {}
        fn bar() {}

        let mut world = World::new();
        world.insert_resource(SystemRegistry::new());
        let mut reg = world.resource_mut::<SystemRegistry>();

        reg.add_set(TestSet::All);
        reg.add_set(TestSet::A.to(TestSet::All));
        reg.add_set(TestSet::B.to(TestSet::All));

        reg.add_system(foo.named(TestSystem::Foo).to(TestSet::A));
        reg.add_system(
            bar.named(TestSystem::Bar)
                .after(TestSystem::Foo)
                .to(TestSet::B),
        );

        let result = world
            .resource_scope(|world, mut reg: Mut<SystemRegistry>| reg.refresh(TestSet::All, world));

        // Foo and Bar do not belong to the same set.
        // This isn't automatically *invalid*, but, like, just order A and B.
        assert!(matches!(result, Err(ScheduleBuildError::CrossDependency)));
    }

    #[test]
    fn hierarchy_loop() {
        let mut world = World::new();
        world.insert_resource(SystemRegistry::new());
        let mut reg = world.resource_mut::<SystemRegistry>();

        reg.add_set(TestSet::X.to(TestSet::X));

        let result = world
            .resource_scope(|world, mut reg: Mut<SystemRegistry>| reg.refresh(TestSet::X, world));

        assert!(matches!(result, Err(ScheduleBuildError::HierarchyLoop)));
    }

    #[test]
    fn hierarchy_invalid() {
        let mut world = World::new();
        world.insert_resource(SystemRegistry::new());
        let mut reg = world.resource_mut::<SystemRegistry>();

        reg.add_set(TestSet::A);
        reg.add_set(TestSet::B.to(TestSet::A));
        reg.add_set(TestSet::C.to(TestSet::B).to(TestSet::A));

        let result = world
            .resource_scope(|world, mut reg: Mut<SystemRegistry>| reg.refresh(TestSet::A, world));

        // A cannot be parent and grandparent to C at same time.
        assert!(matches!(result, Err(ScheduleBuildError::InvalidHierarchy)));
    }

    #[test]
    fn hierarchy_cycle() {
        let mut world = World::new();
        world.insert_resource(SystemRegistry::new());
        let mut reg = world.resource_mut::<SystemRegistry>();

        reg.add_set(TestSet::A.to(TestSet::C));
        reg.add_set(TestSet::B.to(TestSet::A));
        reg.add_set(TestSet::C.to(TestSet::B));

        let result = world
            .resource_scope(|world, mut reg: Mut<SystemRegistry>| reg.refresh(TestSet::A, world));

        assert!(matches!(result, Err(ScheduleBuildError::HierarchyCycle)));
    }

    #[test]
    fn missing_ambiguity() {
        fn foo() {}

        let mut world = World::new();
        world.insert_resource(SystemRegistry::new());
        let mut reg = world.resource_mut::<SystemRegistry>();

        reg.add_set(TestSet::All);
        reg.add_set(TestSet::A.to(TestSet::All).before(TestSet::B));
        reg.add_set(TestSet::B.to(TestSet::All));

        reg.add_system(foo.named(TestSystem::Foo).to(TestSet::A).to(TestSet::B));

        let result = world
            .resource_scope(|world, mut reg: Mut<SystemRegistry>| reg.refresh(TestSet::All, world));

        // How is A supposed to run before B if Foo is in both A and B?
        assert!(matches!(result, Err(ScheduleBuildError::MissingAmbiguity)));
    }

    #[test]
    fn ambiguity() {
        fn foo_reader(_query: Query<&A>) {}
        fn bar_writer(_query: Query<&mut A>) {}

        let mut world = World::new();
        world.insert_resource(SystemRegistry::new());
        let mut reg = world.resource_mut::<SystemRegistry>();

        reg.add_set(TestSet::X);
        reg.add_system(foo_reader.named(TestSystem::Foo).to(TestSet::X));
        reg.add_system(bar_writer.named(TestSystem::Bar).to(TestSet::X));

        let result = world
            .resource_scope(|world, mut reg: Mut<SystemRegistry>| reg.refresh(TestSet::X, world));

        assert!(matches!(result, Err(ScheduleBuildError::Ambiguity)));
    }
}
