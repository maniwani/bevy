use crate::{
    schedule::{label::SystemLabel, BoxedRunCondition, BoxedSystemLabel, IntoRunCondition},
    system::{AsSystemLabel, BoxedSystem, IntoSystem, System},
};

use bevy_utils::HashSet;

/// Unique identifier for a system or system set stored in the [`SystemRegistry`].
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub enum RegistryId {
    System(u64),
    Set(u64),
}

impl RegistryId {
    /// Returns `true` if this identifies a system.
    pub fn is_system(&self) -> bool {
        match self {
            RegistryId::System(_) => true,
            _ => false,
        }
    }

    /// Returns `true` if this identifies a system set.
    pub fn is_set(&self) -> bool {
        match self {
            RegistryId::Set(_) => true,
            _ => false,
        }
    }

    pub fn to_str(&self) -> &str {
        match self {
            RegistryId::System(_) => "system",
            RegistryId::Set(_) => "set",
        }
    }
}

/// Pick a consistent ordering for a `RegistryId` pair.
pub(crate) fn sort_pair(a: RegistryId, b: RegistryId) -> (RegistryId, RegistryId) {
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
pub(crate) struct GraphConfig {
    pub(crate) name: Option<BoxedSystemLabel>,
    pub(crate) sets: HashSet<BoxedSystemLabel>,
    pub(crate) edges: Vec<(Order, BoxedSystemLabel)>,
}

impl GraphConfig {
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
pub(crate) struct IndexedGraphConfig {
    pub(crate) sets: HashSet<RegistryId>,
    pub(crate) edges: Vec<(Order, RegistryId)>,
}

impl IndexedGraphConfig {
    pub(crate) fn sets(&self) -> &HashSet<RegistryId> {
        &self.sets
    }

    pub(crate) fn edges(&self) -> &[(Order, RegistryId)] {
        &self.edges
    }
}

/// Encapsulates a [`System`](crate::system::System) set and information on when it should run.
pub struct SetDescriptor {
    pub(crate) config: GraphConfig,
    pub(crate) run_criteria: Vec<BoxedRunCondition>,
}

fn new_set(label: BoxedSystemLabel) -> SetDescriptor {
    SetDescriptor {
        config: GraphConfig {
            name: Some(label),
            sets: HashSet::new(),
            edges: Vec::new(),
        },
        run_criteria: Vec::new(),
    }
}

/// Types that can be converted into a [`SetDescriptor`].
///
/// Implementation is restricted to types the implement [`SystemLabel`] and matching trait objects.
pub trait IntoSetDescriptor: sealed::IntoSetDescriptor {
    fn into_descriptor(self) -> SetDescriptor;
    /// Configures the system set to run under the set given by `label`.
    fn to(self, label: impl SystemLabel) -> SetDescriptor;
    /// Configures the system set to run before `label`.
    fn before<M>(self, label: impl AsSystemLabel<M>) -> SetDescriptor;
    /// Configures the system set to run after `label`.
    fn after<M>(self, label: impl AsSystemLabel<M>) -> SetDescriptor;
    /// Configures the system set to run only if `condition` returns `true`.
    fn iff<P>(self, condition: impl IntoRunCondition<P>) -> SetDescriptor;
}

impl<L> IntoSetDescriptor for L
where
    L: SystemLabel + sealed::IntoSetDescriptor,
{
    fn into_descriptor(self) -> SetDescriptor {
        new_set(Box::new(self))
    }

    fn to(self, label: impl SystemLabel) -> SetDescriptor {
        new_set(Box::new(self)).to(label)
    }

    fn before<M>(self, label: impl AsSystemLabel<M>) -> SetDescriptor {
        new_set(Box::new(self)).before(label)
    }

    fn after<M>(self, label: impl AsSystemLabel<M>) -> SetDescriptor {
        new_set(Box::new(self)).after(label)
    }

    fn iff<P>(self, condition: impl IntoRunCondition<P>) -> SetDescriptor {
        new_set(Box::new(self)).iff(condition)
    }
}

impl IntoSetDescriptor for BoxedSystemLabel {
    fn into_descriptor(self) -> SetDescriptor {
        new_set(self)
    }

    fn to(self, label: impl SystemLabel) -> SetDescriptor {
        new_set(self).to(label)
    }

    fn before<M>(self, label: impl AsSystemLabel<M>) -> SetDescriptor {
        new_set(self).before(label)
    }

    fn after<M>(self, label: impl AsSystemLabel<M>) -> SetDescriptor {
        new_set(self).after(label)
    }

    fn iff<P>(self, condition: impl IntoRunCondition<P>) -> SetDescriptor {
        new_set(self).iff(condition)
    }
}

impl IntoSetDescriptor for SetDescriptor {
    fn into_descriptor(self) -> SetDescriptor {
        self
    }

    fn to(mut self, label: impl SystemLabel) -> SetDescriptor {
        self.config.sets.insert(label.dyn_clone());
        self
    }

    fn before<M>(mut self, label: impl AsSystemLabel<M>) -> SetDescriptor {
        self.config
            .edges
            .push((Order::Before, label.as_system_label().dyn_clone()));
        self
    }

    fn after<M>(mut self, label: impl AsSystemLabel<M>) -> SetDescriptor {
        self.config
            .edges
            .push((Order::After, label.as_system_label().dyn_clone()));
        self
    }

    fn iff<P>(mut self, condition: impl IntoRunCondition<P>) -> SetDescriptor {
        self.run_criteria
            .push(Box::new(IntoSystem::into_system(condition)));
        self
    }
}

/// Encapsulates a [`System`](crate::system::System) and information on when it should run.
pub struct SystemDescriptor {
    pub(crate) system: BoxedSystem,
    pub(crate) config: GraphConfig,
    pub(crate) run_criteria: Vec<BoxedRunCondition>,
}

fn new_system(system: BoxedSystem) -> SystemDescriptor {
    SystemDescriptor {
        system,
        config: GraphConfig {
            name: None,
            sets: HashSet::new(),
            edges: Vec::new(),
        },
        run_criteria: Vec::new(),
    }
}

/// Types that can be converted into a [`SystemDescriptor`].
///
/// Implementation is restricted to types that implement [`System<In=(), Out=()>`](crate::system::System)
/// and matching trait objects.
pub trait IntoSystemDescriptor<Params>: sealed::IntoSystemDescriptor<Params> {
    fn into_descriptor(self) -> SystemDescriptor;
    /// Sets `name` as the unique label for this instance of the system.
    fn named(self, name: impl SystemLabel) -> SystemDescriptor;
    /// Configures the system to run under the set given by `label`.
    fn to(self, label: impl SystemLabel) -> SystemDescriptor;
    /// Configures the system to run before `label`.
    fn before<M>(self, label: impl AsSystemLabel<M>) -> SystemDescriptor;
    /// Configures the system to run after `label`.
    fn after<M>(self, label: impl AsSystemLabel<M>) -> SystemDescriptor;
    /// Configures the system to run only if `condition` returns `true`.
    fn iff<P>(self, condition: impl IntoRunCondition<P>) -> SystemDescriptor;
}

impl<Params, F> IntoSystemDescriptor<Params> for F
where
    F: IntoSystem<(), (), Params> + sealed::IntoSystemDescriptor<Params>,
{
    fn into_descriptor(self) -> SystemDescriptor {
        new_system(Box::new(IntoSystem::into_system(self)))
    }

    fn named(self, name: impl SystemLabel) -> SystemDescriptor {
        new_system(Box::new(IntoSystem::into_system(self))).named(name)
    }

    fn to(self, label: impl SystemLabel) -> SystemDescriptor {
        new_system(Box::new(IntoSystem::into_system(self))).to(label)
    }

    fn before<M>(self, label: impl AsSystemLabel<M>) -> SystemDescriptor {
        new_system(Box::new(IntoSystem::into_system(self))).before(label)
    }

    fn after<M>(self, label: impl AsSystemLabel<M>) -> SystemDescriptor {
        new_system(Box::new(IntoSystem::into_system(self))).after(label)
    }

    fn iff<P>(self, condition: impl IntoRunCondition<P>) -> SystemDescriptor {
        new_system(Box::new(IntoSystem::into_system(self))).iff(condition)
    }
}

impl IntoSystemDescriptor<()> for BoxedSystem<(), ()> {
    fn into_descriptor(self) -> SystemDescriptor {
        new_system(self)
    }

    fn named(self, name: impl SystemLabel) -> SystemDescriptor {
        new_system(self).named(name)
    }

    fn to(self, label: impl SystemLabel) -> SystemDescriptor {
        new_system(self).to(label)
    }

    fn before<M>(self, label: impl AsSystemLabel<M>) -> SystemDescriptor {
        new_system(self).before(label)
    }

    fn after<M>(self, label: impl AsSystemLabel<M>) -> SystemDescriptor {
        new_system(self).after(label)
    }

    fn iff<P>(self, condition: impl IntoRunCondition<P>) -> SystemDescriptor {
        new_system(self).iff(condition)
    }
}

impl IntoSystemDescriptor<()> for SystemDescriptor {
    fn into_descriptor(self) -> SystemDescriptor {
        self
    }

    fn named(mut self, name: impl SystemLabel) -> SystemDescriptor {
        self.config.name = Some(name.dyn_clone());
        self
    }

    fn to(mut self, label: impl SystemLabel) -> SystemDescriptor {
        self.config.sets.insert(label.dyn_clone());
        self
    }

    fn before<M>(mut self, label: impl AsSystemLabel<M>) -> SystemDescriptor {
        self.config
            .edges
            .push((Order::Before, label.as_system_label().dyn_clone()));
        self
    }

    fn after<M>(mut self, label: impl AsSystemLabel<M>) -> SystemDescriptor {
        self.config
            .edges
            .push((Order::After, label.as_system_label().dyn_clone()));
        self
    }

    fn iff<P>(mut self, condition: impl IntoRunCondition<P>) -> SystemDescriptor {
        self.run_criteria
            .push(Box::new(IntoSystem::into_system(condition)));
        self
    }
}

mod sealed {
    use crate::{
        schedule::{BoxedSystemLabel, SystemLabel},
        system::{BoxedSystem, IntoSystem},
    };

    use super::{SetDescriptor, SystemDescriptor};

    // These traits are private because non-`()` systems cannot be used.
    // The type system doesn't allow for mixed type collections.
    // Maybe we could do funky transmutes on the fn pointers like we do for `CommandQueue`.
    pub trait IntoSystemDescriptor<Params> {}

    impl<Params, F: IntoSystem<(), (), Params>> IntoSystemDescriptor<Params> for F {}

    impl IntoSystemDescriptor<()> for BoxedSystem<(), ()> {}

    impl IntoSystemDescriptor<()> for SystemDescriptor {}

    pub trait IntoSetDescriptor {}

    impl<L: SystemLabel> IntoSetDescriptor for L {}

    impl IntoSetDescriptor for BoxedSystemLabel {}

    impl IntoSetDescriptor for SetDescriptor {}
}

/// Generalizes system and system set descriptors.
pub enum Descriptor {
    System(SystemDescriptor),
    Set(SetDescriptor),
}

impl Descriptor {
    fn name(&self) -> BoxedSystemLabel {
        match self {
            Self::System(system) => system.config.name().unwrap().dyn_clone(),
            Self::Set(set) => set.config.name().unwrap().dyn_clone(),
        }
    }
}

impl SetDescriptor {
    pub fn into_descriptor_enum(self) -> Descriptor {
        Descriptor::Set(self)
    }
}

impl SystemDescriptor {
    pub fn into_descriptor_enum(self) -> Descriptor {
        Descriptor::System(self)
    }
}

#[doc(hidden)]
/// Describes multiple systems and system sets.
pub struct Group(Vec<Descriptor>);

impl Group {
    pub fn new(mut vec: Vec<Descriptor>) -> Self {
        Self(vec)
    }

    /// Configures the nodes to run below the set given by `label`.
    pub fn to(mut self, label: impl SystemLabel) -> Self {
        for node in self.0.iter_mut() {
            match node {
                Descriptor::System(system) => {
                    system.config.sets.insert(label.dyn_clone());
                }
                Descriptor::Set(set) => {
                    set.config.sets.insert(label.dyn_clone());
                }
            };
        }

        self
    }

    /// Configures the nodes to run before `label`.
    pub fn before(mut self, label: impl SystemLabel) -> Self {
        for node in self.0.iter_mut() {
            match node {
                Descriptor::System(system) => {
                    system.config.edges.push((Order::Before, label.dyn_clone()));
                }
                Descriptor::Set(set) => {
                    set.config.edges.push((Order::Before, label.dyn_clone()));
                }
            }
        }

        self
    }

    /// Configures the nodes to run after `label`.
    pub fn after(mut self, label: impl SystemLabel) -> Self {
        for node in self.0.iter_mut() {
            match node {
                Descriptor::System(system) => {
                    system.config.edges.push((Order::After, label.dyn_clone()));
                }
                Descriptor::Set(set) => {
                    set.config.edges.push((Order::After, label.dyn_clone()));
                }
            }
        }

        self
    }
}

impl IntoIterator for Group {
    type Item = Descriptor;
    type IntoIter = std::vec::IntoIter<Descriptor>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[doc(hidden)]
/// Describes multiple systems and system sets that are ordered in a sequence.
pub struct Chain(Vec<Descriptor>);

impl Chain {
    pub fn new(mut vec: Vec<Descriptor>) -> Self {
        let n = vec.len();
        let names = vec.iter().skip(1).map(|c| c.name()).collect::<Vec<_>>();
        for (node, next_node) in vec.iter_mut().take(n - 1).zip(names.into_iter()) {
            match node {
                Descriptor::System(system) => {
                    system.config.edges.push((Order::Before, next_node));
                }
                Descriptor::Set(set) => {
                    set.config.edges.push((Order::Before, next_node));
                }
            }
        }

        Self(vec)
    }

    /// Configures the nodes to run below the set given by `label`.
    pub fn to(mut self, label: impl SystemLabel) -> Self {
        for node in self.0.iter_mut() {
            match node {
                Descriptor::System(system) => {
                    system.config.sets.insert(label.dyn_clone());
                }
                Descriptor::Set(set) => {
                    set.config.sets.insert(label.dyn_clone());
                }
            };
        }

        self
    }

    /// Configures the nodes to run before `label`.
    pub fn before(mut self, label: impl SystemLabel) -> Self {
        if let Some(last) = self.0.last_mut() {
            match last {
                Descriptor::System(system) => {
                    system.config.edges.push((Order::Before, label.dyn_clone()));
                }
                Descriptor::Set(set) => {
                    set.config.edges.push((Order::Before, label.dyn_clone()));
                }
            }
        }

        self
    }

    /// Configures the nodes to run after `label`.
    pub fn after(mut self, label: impl SystemLabel) -> Self {
        if let Some(first) = self.0.first_mut() {
            match first {
                Descriptor::System(system) => {
                    system.config.edges.push((Order::After, label.dyn_clone()));
                }
                Descriptor::Set(set) => {
                    set.config.edges.push((Order::After, label.dyn_clone()));
                }
            }
        }

        self
    }
}

impl IntoIterator for Chain {
    type Item = Descriptor;
    type IntoIter = std::vec::IntoIter<Descriptor>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[macro_export]
/// A mixed group of systems and system sets.
macro_rules! group {
    ($($x:expr),+ $(,)?) => {
        bevy_ecs::schedule::Group::new(vec![$(($x).into_descriptor().into_descriptor_enum()),+])
    };
}

#[macro_export]
/// A mixed group of systems and system sets, ordered in a sequence.
macro_rules! chain {
    ($($x:expr),+ $(,)?) => {
        bevy_ecs::schedule::Chain::new(vec![$(($x).into_descriptor().into_descriptor_enum()),+])
    };
}
