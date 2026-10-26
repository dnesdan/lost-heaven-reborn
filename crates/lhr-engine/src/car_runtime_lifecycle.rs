//! Host operation progress and admission checks for the canonical car owner.
//! These states do not replace any original vehicle bytes or native flags.
use super::CarRuntime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OperationState {
    NotStarted,
    /// Also retains a failed operation's completed prefix; it cannot be replayed.
    Started,
    Complete,
}
impl OperationState {
    pub(super) fn has_started(self) -> bool {
        self != Self::NotStarted
    }
    pub(super) fn is_complete(self) -> bool {
        self == Self::Complete
    }
}

impl CarRuntime {
    /// Gameplay stops at the first cleanup phase, including a failed prelude
    /// before the car destructor itself. Diagnostic reads remain available.
    pub(super) fn car_is_live(&self) -> bool {
        self.summary.initialized
            && !self.game_done.has_started()
            && !self.car_destruction.has_started()
            && !self.shutdown_started
            && !self
                .driving
                .as_ref()
                .is_some_and(|session| session.actor_cleanup.has_started())
    }
    pub(super) fn can_initialize_game(&self) -> bool {
        self.car_is_live() && !self.game_init.has_started()
    }
    pub(super) fn can_drive(&self) -> bool {
        self.car_is_live() && self.game_init.is_complete()
    }
}
