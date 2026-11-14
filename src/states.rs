use avilex_macros::Resource;

use crate::resources::ParallelResourceAccessor;
use crate::system::IntoSystem;
use crate::system::SystemNode;
use crate::system::condition::Condition;
use crate::{
    app::App,
    resources::{ResMut, Resource},
};
use std::fmt::Debug;

/// A marker trait for types that can be used as finite state machines in an [`App`].
///
/// Derive this using `#[derive(States, PartialEq, Eq, Clone, Debug)]`.
///
/// [`App`]: crate::app::App
pub trait States: PartialEq + Eq + Send + Sync + Clone + Debug + 'static {}

/// A [`Resource`] holding the active state of type `T` along with its previous value.
///
/// Updated automatically at the start of each frame when a transition is queued via [`NextState<T>`].
///
/// [`Resource`]: crate::ecs::resources::Resource
/// [`NextState<T>`]: crate::app::states::NextState
pub struct State<T: States> {
    current: T,
    last: T,
    just_changed: bool,
}

impl<T: States> Resource for State<T> {}

impl<T: States> State<T> {
    /// Returns a reference to the currently active state.
    pub fn current(&self) -> &T {
        &self.current
    }

    /// Returns a reference to the state that was active before the most recent transition.
    ///
    /// Equals [`State::current`] if no transition has occurred yet.
    ///
    /// [`State::current`]: Self::current
    pub fn last(&self) -> &T {
        &self.last
    }

    /// Returns `true` if the state transitioned at the start of the current frame.
    pub fn just_changed(&self) -> bool {
        self.just_changed
    }
}

/// A [`Resource`] used to queue a state transition to a new value of `T`.
///
/// Queued transitions are applied at the start of the next frame, updating [`State<T>`].
///
/// [`Resource`]: crate::ecs::resources::Resource
/// [`State<T>`]: crate::app::states::State
pub struct NextState<T: States> {
    state: Option<T>,
}

impl<T: States> Resource for NextState<T> {}

impl<T: States> NextState<T> {
    /// Queues a transition to `next_state` to be applied at the start of the next frame.
    ///
    /// Overwrites any previously queued transition that has not been applied yet.
    pub fn set(&mut self, next_state: T) {
        self.state = Some(next_state);
    }

    /// Cancels any pending state transition that has not been applied yet.
    pub fn clear(&mut self) {
        self.state = None;
    }

    /// Returns a reference to the pending next state, or `None` if no transition is queued.
    pub fn current_next_state(&self) -> Option<&T> {
        self.state.as_ref()
    }
}

fn state_transition_system<T: States>(
    mut state: ResMut<State<T>>,
    mut next_state: ResMut<NextState<T>>,
) {
    state.just_changed = false;
    if let Some(next) = next_state.state.take()
        && state.current != next
    {
        state.last = state.current.clone();
        state.current = next;
        state.just_changed = true;
    }
}

pub(crate) fn setup_states_schedules_and_systems<T: States>(app: &mut App, state: T) {
    let new_sys = SystemNode::new(IntoSystem::into_system(state_transition_system::<T>));
    if !app.world().contains_resource::<StateTransitionSystems>() {
        app.world().insert_resource(StateTransitionSystems {
            systems: vec![new_sys],
        });
    } else {
        let mut systems = app.world().resource_mut::<StateTransitionSystems>();
        let new_identity = new_sys.system().identity();
        if systems
            .systems
            .iter()
            .all(|sys| !sys.system().identity().dyn_system_eq(&*new_identity))
        {
            systems.systems.push(new_sys);
        }
    }
    app.insert_resource(State {
        current: state.clone(),
        last: state.clone(),
        just_changed: false,
    })
    .insert_resource(NextState::<T> { state: None });
}

#[derive(Resource)]
pub(crate) struct StateTransitionSystems {
    pub(crate) systems: Vec<SystemNode<(), ()>>,
}

/// A run [`Condition`] that returns `true` while [`State<T>`] matches `expected`.
///
/// [`Condition`]: crate::app::system::condition::Condition
/// [`State<T>`]: crate::app::states::State
pub fn in_state<T: States>(
    expected: T,
) -> impl Condition<ParallelResourceAccessor<State<T>>> + Clone {
    move |state_accessor: ParallelResourceAccessor<State<T>>| {
        state_accessor.scope(|state| *state.current() == expected)
    }
}

/// A run [`Condition`] that returns `true` on the frame [`State<T>`] transitions into `state`.
///
/// [`Condition`]: crate::app::system::condition::Condition
/// [`State<T>`]: crate::app::states::State
pub fn entered_state<T: States>(
    state: T,
) -> impl Condition<ParallelResourceAccessor<State<T>>> + Clone {
    move |state_accessor: ParallelResourceAccessor<State<T>>| {
        state_accessor.scope(|res_state| *res_state.current() == state && res_state.just_changed())
    }
}

/// A run [`Condition`] that returns `true` on the frame [`State<T>`] transitions out of `state`.
///
/// [`Condition`]: crate::app::system::condition::Condition
/// [`State<T>`]: crate::app::states::State
pub fn exited_state<T: States>(
    state: T,
) -> impl Condition<ParallelResourceAccessor<State<T>>> + Clone {
    move |state_accessor: ParallelResourceAccessor<State<T>>| {
        state_accessor.scope(|res_state| *res_state.last() == state && res_state.just_changed())
    }
}
