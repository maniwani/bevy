use crate::{
    schedule::{label::SystemLabel, BoxedRunCondition, BoxedSystemLabel, IntoRunCondition},
    system::{AsSystemLabel, BoxedSystem, IntoSystem, System},
};

use bevy_utils::HashSet;

/// Unique identifier for a system or system set stored in the [`SystemRegistry`].
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub enum RegId {
    System(u64),
    Set(u64),
}

impl RegId {
    /// Returns `true` if this identifies a system.
    pub fn is_system(&self) -> bool {
        match self {
            RegId::System(_) => true,
            _ => false,
        }
    }

    /// Returns `true` if this identifies a system set.
    pub fn is_set(&self) -> bool {
        match self {
            RegId::Set(_) => true,
            _ => false,
        }
    }

    pub fn to_str(&self) -> &str {
        match self {
            RegId::System(_) => "system",
            RegId::Set(_) => "set",
        }
    }
}

/// Pick a consistent ordering for a `RegId` pair.
pub(crate) fn sort_pair(a: RegId, b: RegId) -> (RegId, RegId) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Before or after.
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub(crate) enum Order {
    Before,
    After,
}

/// Information for system graph construction. See [`SystemRegistry`](crate::schedule::SystemRegistry).
#[derive(Debug, Clone)]
pub(crate) struct Scheduling {
    pub(crate) name: Option<BoxedSystemLabel>,
    pub(crate) sets: HashSet<BoxedSystemLabel>,
    pub(crate) edges: Vec<(Order, BoxedSystemLabel)>,
}

impl Scheduling {
    pub(crate) fn name(&self) -> Option<&BoxedSystemLabel> {
        self.name.as_ref()
    }

    pub(crate) fn sets(&self) -> &HashSet<BoxedSystemLabel> {
        &self.sets
    }

    pub(crate) fn edges(&self) -> &[(Order, BoxedSystemLabel)] {
        &self.edges
    }
}

/// Information for system graph construction. See [`SystemRegistry`](crate::schedule::SystemRegistry).
#[derive(Debug, Clone)]
pub(crate) struct IndexedScheduling {
    pub(crate) sets: HashSet<RegId>,
    pub(crate) edges: Vec<(Order, RegId)>,
}

impl IndexedScheduling {
    pub(crate) fn sets(&self) -> &HashSet<RegId> {
        &self.sets
    }

    pub(crate) fn edges(&self) -> &[(Order, RegId)] {
        &self.edges
    }
}

/// Encapsulates a [`System`](crate::system::System) set and information on when it should run.
pub struct ScheduledSet {
    pub(crate) scheduling: Scheduling,
    pub(crate) conditions: Vec<BoxedRunCondition>,
}

fn new_set(label: BoxedSystemLabel) -> ScheduledSet {
    ScheduledSet {
        scheduling: Scheduling {
            name: Some(label),
            sets: HashSet::new(),
            edges: Vec::new(),
        },
        conditions: Vec::new(),
    }
}

/// Types that can be converted into a [`ScheduledSet`].
///
/// Implemented for types the implement [`SystemLabel`] and boxed trait objects.
pub trait IntoScheduledSet: sealed::IntoScheduledSet {
    fn schedule(self) -> ScheduledSet;
    /// Configures the system set to run under the set given by `label`.
    fn to(self, label: impl SystemLabel) -> ScheduledSet;
    /// Configures the system set to run before `label`.
    fn before<M>(self, label: impl AsSystemLabel<M>) -> ScheduledSet;
    /// Configures the system set to run after `label`.
    fn after<M>(self, label: impl AsSystemLabel<M>) -> ScheduledSet;
    /// Configures the system set to run only if `condition` returns `true`.
    fn iff<P>(self, condition: impl IntoRunCondition<P>) -> ScheduledSet;
}

impl<L> IntoScheduledSet for L
where
    L: SystemLabel + sealed::IntoScheduledSet,
{
    fn schedule(self) -> ScheduledSet {
        new_set(Box::new(self))
    }

    fn to(self, label: impl SystemLabel) -> ScheduledSet {
        new_set(Box::new(self)).to(label)
    }

    fn before<M>(self, label: impl AsSystemLabel<M>) -> ScheduledSet {
        new_set(Box::new(self)).before(label)
    }

    fn after<M>(self, label: impl AsSystemLabel<M>) -> ScheduledSet {
        new_set(Box::new(self)).after(label)
    }

    fn iff<P>(self, condition: impl IntoRunCondition<P>) -> ScheduledSet {
        new_set(Box::new(self)).iff(condition)
    }
}

impl IntoScheduledSet for BoxedSystemLabel {
    fn schedule(self) -> ScheduledSet {
        new_set(self)
    }

    fn to(self, label: impl SystemLabel) -> ScheduledSet {
        new_set(self).to(label)
    }

    fn before<M>(self, label: impl AsSystemLabel<M>) -> ScheduledSet {
        new_set(self).before(label)
    }

    fn after<M>(self, label: impl AsSystemLabel<M>) -> ScheduledSet {
        new_set(self).after(label)
    }

    fn iff<P>(self, condition: impl IntoRunCondition<P>) -> ScheduledSet {
        new_set(self).iff(condition)
    }
}

impl IntoScheduledSet for ScheduledSet {
    fn schedule(self) -> ScheduledSet {
        self
    }

    fn to(mut self, label: impl SystemLabel) -> ScheduledSet {
        self.scheduling.sets.insert(label.dyn_clone());
        self
    }

    fn before<M>(mut self, label: impl AsSystemLabel<M>) -> ScheduledSet {
        self.scheduling
            .edges
            .push((Order::Before, label.as_system_label().dyn_clone()));
        self
    }

    fn after<M>(mut self, label: impl AsSystemLabel<M>) -> ScheduledSet {
        self.scheduling
            .edges
            .push((Order::After, label.as_system_label().dyn_clone()));
        self
    }

    fn iff<P>(mut self, condition: impl IntoRunCondition<P>) -> ScheduledSet {
        self.conditions
            .push(Box::new(IntoSystem::into_system(condition)));
        self
    }
}

/// Encapsulates a [`System`](crate::system::System) and information on when it should run.
pub struct ScheduledSystem {
    pub(crate) system: BoxedSystem,
    pub(crate) scheduling: Scheduling,
    pub(crate) conditions: Vec<BoxedRunCondition>,
}

fn new_system(system: BoxedSystem) -> ScheduledSystem {
    ScheduledSystem {
        system,
        scheduling: Scheduling {
            name: None,
            sets: HashSet::new(),
            edges: Vec::new(),
        },
        conditions: Vec::new(),
    }
}

/// Types that can be converted into a [`ScheduledSystem`].
///
/// Implemented for types that implement [`System<In=(), Out=()>`](crate::system::System)
/// and boxed trait objects.
pub trait IntoScheduledSystem<Params>: sealed::IntoScheduledSystem<Params> {
    fn schedule(self) -> ScheduledSystem;
    /// Sets `name` as the unique label for this instance of the system.
    fn named(self, name: impl SystemLabel) -> ScheduledSystem;
    /// Configures the system to run under the set given by `label`.
    fn to(self, label: impl SystemLabel) -> ScheduledSystem;
    /// Configures the system to run before `label`.
    fn before<M>(self, label: impl AsSystemLabel<M>) -> ScheduledSystem;
    /// Configures the system to run after `label`.
    fn after<M>(self, label: impl AsSystemLabel<M>) -> ScheduledSystem;
    /// Configures the system to run only if `condition` returns `true`.
    fn iff<P>(self, condition: impl IntoRunCondition<P>) -> ScheduledSystem;
}

impl<Params, F> IntoScheduledSystem<Params> for F
where
    F: IntoSystem<(), (), Params> + sealed::IntoScheduledSystem<Params>,
{
    fn schedule(self) -> ScheduledSystem {
        new_system(Box::new(IntoSystem::into_system(self)))
    }

    fn named(self, name: impl SystemLabel) -> ScheduledSystem {
        new_system(Box::new(IntoSystem::into_system(self))).named(name)
    }

    fn to(self, label: impl SystemLabel) -> ScheduledSystem {
        new_system(Box::new(IntoSystem::into_system(self))).to(label)
    }

    fn before<M>(self, label: impl AsSystemLabel<M>) -> ScheduledSystem {
        new_system(Box::new(IntoSystem::into_system(self))).before(label)
    }

    fn after<M>(self, label: impl AsSystemLabel<M>) -> ScheduledSystem {
        new_system(Box::new(IntoSystem::into_system(self))).after(label)
    }

    fn iff<P>(self, condition: impl IntoRunCondition<P>) -> ScheduledSystem {
        new_system(Box::new(IntoSystem::into_system(self))).iff(condition)
    }
}

impl IntoScheduledSystem<()> for BoxedSystem<(), ()> {
    fn schedule(self) -> ScheduledSystem {
        new_system(self)
    }

    fn named(self, name: impl SystemLabel) -> ScheduledSystem {
        new_system(self).named(name)
    }

    fn to(self, label: impl SystemLabel) -> ScheduledSystem {
        new_system(self).to(label)
    }

    fn before<M>(self, label: impl AsSystemLabel<M>) -> ScheduledSystem {
        new_system(self).before(label)
    }

    fn after<M>(self, label: impl AsSystemLabel<M>) -> ScheduledSystem {
        new_system(self).after(label)
    }

    fn iff<P>(self, condition: impl IntoRunCondition<P>) -> ScheduledSystem {
        new_system(self).iff(condition)
    }
}

impl IntoScheduledSystem<()> for ScheduledSystem {
    fn schedule(self) -> ScheduledSystem {
        self
    }

    fn named(mut self, name: impl SystemLabel) -> ScheduledSystem {
        self.scheduling.name = Some(name.dyn_clone());
        self
    }

    fn to(mut self, label: impl SystemLabel) -> ScheduledSystem {
        self.scheduling.sets.insert(label.dyn_clone());
        self
    }

    fn before<M>(mut self, label: impl AsSystemLabel<M>) -> ScheduledSystem {
        self.scheduling
            .edges
            .push((Order::Before, label.as_system_label().dyn_clone()));
        self
    }

    fn after<M>(mut self, label: impl AsSystemLabel<M>) -> ScheduledSystem {
        self.scheduling
            .edges
            .push((Order::After, label.as_system_label().dyn_clone()));
        self
    }

    fn iff<P>(mut self, condition: impl IntoRunCondition<P>) -> ScheduledSystem {
        self.conditions
            .push(Box::new(IntoSystem::into_system(condition)));
        self
    }
}

mod sealed {
    use crate::{
        schedule::{BoxedSystemLabel, SystemLabel},
        system::{BoxedSystem, IntoSystem},
    };

    use super::{ScheduledSet, ScheduledSystem};

    // These traits are private because non-`()` systems cannot be used.
    // The type system doesn't allow for mixed type collections.
    // Maybe we could do funky transmutes on the fn pointers like we do for `CommandQueue`.
    pub trait IntoScheduledSystem<Params> {}

    impl<Params, F: IntoSystem<(), (), Params>> IntoScheduledSystem<Params> for F {}

    impl IntoScheduledSystem<()> for BoxedSystem<(), ()> {}

    impl IntoScheduledSystem<()> for ScheduledSystem {}

    pub trait IntoScheduledSet {}

    impl<L: SystemLabel> IntoScheduledSet for L {}

    impl IntoScheduledSet for BoxedSystemLabel {}

    impl IntoScheduledSet for ScheduledSet {}
}

/// Enum for pseudo-type elision of scheduled systems and system sets.
pub enum Scheduled {
    System(ScheduledSystem),
    Set(ScheduledSet),
}

impl Scheduled {
    fn name(&self) -> BoxedSystemLabel {
        match self {
            Self::System(system) => system.scheduling.name().unwrap().dyn_clone(),
            Self::Set(set) => set.scheduling.name().unwrap().dyn_clone(),
        }
    }
}

impl ScheduledSet {
    pub fn into_enum(self) -> Scheduled {
        Scheduled::Set(self)
    }
}

impl ScheduledSystem {
    pub fn into_enum(self) -> Scheduled {
        Scheduled::System(self)
    }
}

#[doc(hidden)]
/// Describes multiple systems and system sets.
pub struct Group(Vec<Scheduled>);

impl Group {
    pub fn new(mut vec: Vec<Scheduled>) -> Self {
        Self(vec)
    }

    /// Configures the nodes to run below the set given by `label`.
    pub fn to(mut self, label: impl SystemLabel) -> Self {
        for node in self.0.iter_mut() {
            match node {
                Scheduled::System(system) => {
                    system.scheduling.sets.insert(label.dyn_clone());
                }
                Scheduled::Set(set) => {
                    set.scheduling.sets.insert(label.dyn_clone());
                }
            };
        }

        self
    }

    /// Configures the nodes to run before `label`.
    pub fn before(mut self, label: impl SystemLabel) -> Self {
        for node in self.0.iter_mut() {
            match node {
                Scheduled::System(system) => {
                    system
                        .scheduling
                        .edges
                        .push((Order::Before, label.dyn_clone()));
                }
                Scheduled::Set(set) => {
                    set.scheduling
                        .edges
                        .push((Order::Before, label.dyn_clone()));
                }
            }
        }

        self
    }

    /// Configures the nodes to run after `label`.
    pub fn after(mut self, label: impl SystemLabel) -> Self {
        for node in self.0.iter_mut() {
            match node {
                Scheduled::System(system) => {
                    system
                        .scheduling
                        .edges
                        .push((Order::After, label.dyn_clone()));
                }
                Scheduled::Set(set) => {
                    set.scheduling.edges.push((Order::After, label.dyn_clone()));
                }
            }
        }

        self
    }
}

impl IntoIterator for Group {
    type Item = Scheduled;
    type IntoIter = std::vec::IntoIter<Scheduled>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[doc(hidden)]
/// Describes multiple systems and system sets that are ordered in a sequence.
pub struct Chain(Vec<Scheduled>);

impl Chain {
    pub fn new(mut vec: Vec<Scheduled>) -> Self {
        let n = vec.len();
        let names = vec.iter().skip(1).map(|c| c.name()).collect::<Vec<_>>();
        for (node, next_node) in vec.iter_mut().take(n - 1).zip(names.into_iter()) {
            match node {
                Scheduled::System(system) => {
                    system.scheduling.edges.push((Order::Before, next_node));
                }
                Scheduled::Set(set) => {
                    set.scheduling.edges.push((Order::Before, next_node));
                }
            }
        }

        Self(vec)
    }

    /// Configures the nodes to run below the set given by `label`.
    pub fn to(mut self, label: impl SystemLabel) -> Self {
        for node in self.0.iter_mut() {
            match node {
                Scheduled::System(system) => {
                    system.scheduling.sets.insert(label.dyn_clone());
                }
                Scheduled::Set(set) => {
                    set.scheduling.sets.insert(label.dyn_clone());
                }
            };
        }

        self
    }

    /// Configures the nodes to run before `label`.
    pub fn before(mut self, label: impl SystemLabel) -> Self {
        if let Some(last) = self.0.last_mut() {
            match last {
                Scheduled::System(system) => {
                    system
                        .scheduling
                        .edges
                        .push((Order::Before, label.dyn_clone()));
                }
                Scheduled::Set(set) => {
                    set.scheduling
                        .edges
                        .push((Order::Before, label.dyn_clone()));
                }
            }
        }

        self
    }

    /// Configures the nodes to run after `label`.
    pub fn after(mut self, label: impl SystemLabel) -> Self {
        if let Some(first) = self.0.first_mut() {
            match first {
                Scheduled::System(system) => {
                    system
                        .scheduling
                        .edges
                        .push((Order::After, label.dyn_clone()));
                }
                Scheduled::Set(set) => {
                    set.scheduling.edges.push((Order::After, label.dyn_clone()));
                }
            }
        }

        self
    }
}

impl IntoIterator for Chain {
    type Item = Scheduled;
    type IntoIter = std::vec::IntoIter<Scheduled>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// A mixed group of systems and system sets.
#[macro_export]
macro_rules! group {
    ($($x:expr),+ $(,)?) => {
        bevy_ecs::schedule::Group::new(vec![$(($x).schedule().into_enum()),+])
    };
}

pub use group;

/// A mixed group of systems and system sets, ordered in a sequence.
#[macro_export]
macro_rules! chain {
    ($($x:expr),+ $(,)?) => {
        bevy_ecs::schedule::Chain::new(vec![$(($x).schedule().into_enum()),+])
    };
}

pub use chain;
