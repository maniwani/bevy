use crate::{
    self as bevy_ecs,
    change_detection::Mut,
    schedule::{run_systems, SystemLabel},
    system::Res,
    world::World,
};

use std::{fmt::Debug, hash::Hash};
use thiserror::Error;

/// Types that can define states in a finite-state machine.
pub trait State: 'static + Send + Sync + Clone + PartialEq + Eq + Debug + Hash {}
impl<T> State for T where T: 'static + Send + Sync + Clone + PartialEq + Eq + Debug + Hash {}

/// A [`SystemLabel`] for a state's "on enter" transition.
#[derive(Debug, Clone, Hash, PartialEq, Eq, SystemLabel)]
pub struct OnEnter<S: State>(pub S);

/// A [`SystemLabel`] for a state's "on exit" transition.
#[derive(Debug, Clone, Hash, PartialEq, Eq, SystemLabel)]
pub struct OnExit<S: State>(pub S);

/// A simple finite-state machine.
#[derive(Debug, Clone)]
pub struct Fsm<S: State> {
    current_state: S,
    next_state: Option<S>,
    prev_state: Option<S>,
}

impl<S: State> Fsm<S> {
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
    /// # Errors
    ///
    /// Errors if a transition has already been queued or transitions leads to same state.
    pub fn queue_transition(&mut self, state: S) -> Result<(), StateTransitionError> {
        if *self.current_state() == state {
            return Err(StateTransitionError::AlreadyInState);
        }

        if self.next_state.is_some() {
            return Err(StateTransitionError::TransitionAlreadyQueued);
        }

        self.next_state = Some(state);

        Ok(())
    }
}

/// Returns `true` if the machine exists.
pub fn state_exists<S: State>() -> impl FnMut(Option<Res<Fsm<S>>>) -> bool {
    move |fsm: Option<Res<Fsm<S>>>| fsm.is_some()
}

/// Returns `true` if the machine is currently in `state`.
pub fn state_equals<S: State>(state: S) -> impl FnMut(Res<Fsm<S>>) -> bool {
    move |fsm: Res<Fsm<S>>| *fsm.current_state() == state
}

/// Returns `true` if the machine exists and is currently in `state`.
pub fn state_exists_and_equals<S: State>(state: S) -> impl FnMut(Option<Res<Fsm<S>>>) -> bool {
    move |fsm: Option<Res<Fsm<S>>>| match fsm {
        Some(fsm) => *fsm.current_state() == state,
        None => false,
    }
}

/// If state transition queued, applies it, then runs the systems under [`OnExit(old_state)`]
/// and [`OnEnter(new_state)`].
pub fn apply_state_transition<S: State>(world: &mut World) {
    world.resource_scope(|world, mut fsm: Mut<Fsm<S>>| {
        if let Some(new_state) = fsm.next_state.take() {
            let old_state = std::mem::replace(&mut fsm.current_state, new_state.clone());
            fsm.prev_state = Some(old_state.clone());
            run_systems(OnExit(old_state), world);
            run_systems(OnEnter(new_state), world);
        }
    });
}

#[derive(Debug, Error)]
pub enum StateTransitionError {
    #[error("queued transition to same state")]
    AlreadyInState,
    #[error("queued transition when one was already queued")]
    TransitionAlreadyQueued,
}
