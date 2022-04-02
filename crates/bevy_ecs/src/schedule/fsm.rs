use crate::{
    self as bevy_ecs,
    change_detection::Mut,
    schedule::{run_scheduled, SystemLabel},
    system::Res,
    world::World,
};

use std::{fmt::Debug, hash::Hash};
use thiserror::Error;

/// Types that can define states in a finite-state machine.
pub trait State: 'static + Send + Sync + Clone + PartialEq + Eq + Debug + Hash {}
impl<T> State for T where T: 'static + Send + Sync + Clone + PartialEq + Eq + Debug + Hash {}

/// A [`SystemLabel`] for the system set that runs during a state's "on enter" transition.
#[derive(Debug, Clone, Hash, PartialEq, Eq, SystemLabel)]
pub struct OnEnter<S: State>(pub S);

/// A [`SystemLabel`] for the system set that runs during a state's "on exit" transition.
#[derive(Debug, Clone, Hash, PartialEq, Eq, SystemLabel)]
pub struct OnExit<S: State>(pub S);

/// A simple finite-state machine whose transitions (enter and exit) can have associated system sets
/// ([`OnEnter(s)`] and [`OnExit(s)`]).
///
/// A state transition can be queued with [`queue_transition`](Fsm::queue_transition), and it will
/// be applied the next time an [`apply_state_transition::<S>`] system runs.
#[derive(Debug, Clone)]
pub struct Fsm<S: State> {
    current_state: S,
    next_state: Option<S>,
    prev_state: Option<S>,
}

impl<S: State> Fsm<S> {
    /// Constructs a new `Fsm` that starts in the specified initial state.
    pub fn new(initial_state: S) -> Self {
        Self {
            current_state: initial_state,
            next_state: None,
            prev_state: None,
        }
    }

    /// Returns the machine's current state.
    pub fn current_state(&self) -> &S {
        &self.current_state
    }

    /// Returns the machine's next state, if it exists.
    pub fn next_state(&self) -> Option<&S> {
        self.next_state.as_ref()
    }

    /// Returns the machine's previous state, if it exists.
    pub fn previous_state(&self) -> Option<&S> {
        self.prev_state.as_ref()
    }

    /// Sets `state` to become the machine's next state.
    ///
    /// If a transition was already queued, replaces and returns the queued state.
    pub fn queue_transition(&mut self, state: S) -> Option<S> {
        std::mem::replace(&mut self.next_state, Some(state))
    }
}

/// If state transition queued, applies it, then runs the systems under [`OnExit(old_state)`]
/// and then [`OnEnter(new_state)`].
pub fn apply_state_transition<S: State>(world: &mut World) {
    world.resource_scope(|world, mut fsm: Mut<Fsm<S>>| {
        if let Some(new_state) = fsm.next_state.take() {
            let old_state = std::mem::replace(&mut fsm.current_state, new_state.clone());
            fsm.prev_state = Some(old_state.clone());
            run_scheduled(OnExit(old_state), world);
            run_scheduled(OnEnter(new_state), world);
        }
    });
}
