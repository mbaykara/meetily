//! Deterministic policy. All times are monotonic elapsed seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Observation {
    Active,
    Inactive,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Start,
    Stop(u64),
}
#[derive(Default)]
pub struct Policy {
    pub owned: Option<u64>,
    participants: [bool; 2],
    positives: u8,
    absent_since: Option<u64>,
    suppressed: bool,
    pending: bool,
}
impl Policy {
    pub fn participants(&self) -> [bool; 2] {
        self.participants
    }
    pub fn tick_apps(
        &mut self,
        now: u64,
        selected: [bool; 2],
        observations: [Observation; 2],
        session: Option<u64>,
        can_start: bool,
    ) -> Option<Action> {
        let enabled = selected.into_iter().any(|enabled| enabled);
        // Keep an existing episode observable through manual suppression and
        // post-processing. Do not seed a future episode while starts are gated.
        if self.participants.into_iter().any(|participant| participant)
            || can_start
            || session.is_some()
        {
            for index in 0..2 {
                if selected[index] && observations[index] == Observation::Active {
                    self.participants[index] = true;
                }
            }
        }
        let scope = if self.participants.into_iter().any(|participant| participant) {
            self.participants
        } else {
            selected
        };
        let scoped: Vec<_> = observations
            .into_iter()
            .zip(scope)
            .filter_map(|(observation, included)| included.then_some(observation))
            .collect();
        let observation = combined(&scoped);
        let action = self.tick_inner(now, enabled, observation, session, can_start);
        // An abandoned/manual/failed-start episode ends only after its own
        // participants have had the full confirmed-absence grace.
        if action.is_none()
            && self.owned.is_none()
            && !self.suppressed
            && observation == Observation::Inactive
            && self
                .absent_since
                .map(|since| now.saturating_sub(since) >= 10)
                .unwrap_or(false)
        {
            self.participants = [false; 2];
            self.reset_confirmation();
        }
        action
    }
    pub fn reset_confirmation(&mut self) {
        self.positives = 0;
        self.absent_since = None;
        self.pending = false;
    }
    pub fn tick(
        &mut self,
        now: u64,
        enabled: bool,
        observation: Observation,
        session: Option<u64>,
    ) -> Option<Action> {
        self.tick_inner(now, enabled, observation, session, true)
    }
    fn tick_inner(
        &mut self,
        now: u64,
        enabled: bool,
        observation: Observation,
        session: Option<u64>,
        can_start: bool,
    ) -> Option<Action> {
        if !enabled {
            *self = Self::default();
            return None;
        }
        if self.owned.is_some() && self.owned != session {
            self.owned = None;
            self.suppressed = true;
        }
        if session.is_some() && self.owned.is_none() {
            self.suppressed = true;
        }
        match observation {
            Observation::Unknown => {
                self.positives = 0;
                self.absent_since = None;
            }
            Observation::Active => {
                self.absent_since = None;
                self.positives = if can_start {
                    self.positives.saturating_add(1)
                } else {
                    0
                };
                if can_start
                    && self.positives >= 2
                    && session.is_none()
                    && !self.suppressed
                    && !self.pending
                {
                    self.pending = true;
                    return Some(Action::Start);
                }
            }
            Observation::Inactive => {
                self.positives = 0;
                let since = *self.absent_since.get_or_insert(now);
                if now.saturating_sub(since) >= 10 {
                    self.suppressed = false;
                    if let Some(id) = self.owned {
                        if !self.pending {
                            self.pending = true;
                            return Some(Action::Stop(id));
                        }
                    }
                }
            }
        }
        None
    }
    pub fn started(&mut self, session: Option<u64>) {
        self.pending = false;
        self.owned = session;
        if session.is_none() {
            self.suppressed = true;
        }
    }
    pub fn stopped(&mut self, success: bool) {
        self.pending = false;
        if success {
            self.owned = None;
            // Automatic stop already required confirmed call absence. The next
            // active call must not need an extra inactive sample after saving.
            self.suppressed = false;
            self.participants = [false; 2];
            self.reset_confirmation();
        }
    }
}
pub fn combined(observations: &[Observation]) -> Observation {
    if observations.contains(&Observation::Active) {
        Observation::Active
    } else if observations.contains(&Observation::Unknown) {
        Observation::Unknown
    } else {
        Observation::Inactive
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use Observation::{Active as A, Inactive as I, Unknown as U};
    fn owned() -> Policy {
        let mut p = Policy::default();
        p.tick(0, true, Observation::Active, None);
        assert_eq!(
            p.tick(2, true, Observation::Active, None),
            Some(Action::Start)
        );
        p.started(Some(7));
        p
    }
    #[test]
    fn new_selection_requires_fresh_confirmations() {
        let mut policy = Policy::default();
        assert_eq!(policy.tick(0, true, Observation::Active, None), None);
        policy.reset_confirmation();
        assert_eq!(policy.tick(2, true, Observation::Inactive, None), None);
        assert_eq!(policy.tick(4, true, Observation::Active, None), None);
        assert_eq!(
            policy.tick(6, true, Observation::Active, None),
            Some(Action::Start)
        );
    }
    #[test]
    fn duplicate_start() {
        let mut p = owned();
        assert_eq!(p.tick(4, true, Observation::Active, Some(7)), None);
    }
    #[test]
    fn unknown_never_stops() {
        let mut p = owned();
        for t in 0..30 {
            assert_eq!(p.tick(t, true, Observation::Unknown, Some(7)), None);
        }
    }
    #[test]
    fn reconnect_grace() {
        let mut p = owned();
        p.tick(0, true, Observation::Inactive, Some(7));
        assert_eq!(p.tick(8, true, Observation::Inactive, Some(7)), None);
        p.tick(9, true, Observation::Active, Some(7));
        assert_eq!(p.tick(10, true, Observation::Inactive, Some(7)), None);
    }
    #[test]
    fn stop_after_ten_seconds() {
        let mut p = owned();
        p.tick(0, true, Observation::Inactive, Some(7));
        assert_eq!(
            p.tick(10, true, Observation::Inactive, Some(7)),
            Some(Action::Stop(7))
        );
        assert_eq!(p.tick(12, true, Observation::Inactive, Some(7)), None);
    }
    #[test]
    fn manual_stop_suppresses() {
        let mut p = owned();
        for t in 0..30 {
            assert_eq!(p.tick(t, true, Observation::Active, None), None);
        }
    }
    #[test]
    fn failed_start_cannot_own_manual_session() {
        let mut p = Policy::default();
        p.tick(0, true, Observation::Active, None);
        p.tick(2, true, Observation::Active, None);
        p.started(None);
        p.tick(4, true, Observation::Inactive, Some(8));
        assert_eq!(p.tick(20, true, Observation::Inactive, Some(8)), None);
        assert_eq!(p.owned, None);
    }
    #[test]
    fn replacement_is_never_stopped() {
        let mut p = owned();
        p.tick(0, true, Observation::Inactive, Some(8));
        assert_eq!(p.tick(20, true, Observation::Inactive, Some(8)), None);
    }
    #[test]
    fn automatic_stop_rearms_for_call_started_during_save() {
        let mut policy = owned();
        policy.tick(0, true, Observation::Inactive, Some(7));
        assert_eq!(
            policy.tick(10, true, Observation::Inactive, Some(7)),
            Some(Action::Stop(7))
        );
        policy.stopped(true);
        // Native orchestration skips policy while saving. Call B may already
        // be active when saving completes, without another inactive sample.
        assert_eq!(policy.tick(30, true, Observation::Active, None), None);
        assert_eq!(
            policy.tick(32, true, Observation::Active, None),
            Some(Action::Start)
        );
        policy.started(Some(8));
        assert_eq!(policy.tick(34, true, Observation::Active, Some(8)), None);
    }
    #[test]
    fn stop_failure_retries() {
        let mut p = owned();
        p.tick(0, true, Observation::Inactive, Some(7));
        p.tick(10, true, Observation::Inactive, Some(7));
        p.stopped(false);
        assert_eq!(
            p.tick(12, true, Observation::Inactive, Some(7)),
            Some(Action::Stop(7))
        );
    }
    #[test]
    fn disabled_relinquishes() {
        let mut p = owned();
        assert_eq!(p.tick(0, false, Observation::Inactive, Some(7)), None);
        assert_eq!(p.owned, None);
    }
    fn apps(
        policy: &mut Policy,
        now: u64,
        observations: [Observation; 2],
        session: Option<u64>,
    ) -> Option<Action> {
        policy.tick_apps(now, [true, true], observations, session, true)
    }
    fn zoom_episode() -> Policy {
        let mut policy = Policy::default();
        assert_eq!(apps(&mut policy, 0, [U, A], None), None);
        assert_eq!(apps(&mut policy, 2, [U, A], None), Some(Action::Start));
        policy.started(Some(7));
        policy
    }
    #[test]
    fn zoom_end_ignores_never_active_unknown_teams() {
        let mut policy = zoom_episode();
        assert_eq!(policy.participants(), [false, true]);
        assert_eq!(apps(&mut policy, 4, [U, I], Some(7)), None);
        assert_eq!(apps(&mut policy, 12, [U, I], Some(7)), None);
        assert_eq!(
            apps(&mut policy, 14, [U, I], Some(7)),
            Some(Action::Stop(7))
        );
    }
    #[test]
    fn teams_end_ignores_never_active_unknown_zoom() {
        let mut policy = Policy::default();
        apps(&mut policy, 0, [A, U], None);
        assert_eq!(apps(&mut policy, 2, [A, U], None), Some(Action::Start));
        policy.started(Some(7));
        apps(&mut policy, 4, [I, U], Some(7));
        assert_eq!(
            apps(&mut policy, 14, [I, U], Some(7)),
            Some(Action::Stop(7))
        );
    }
    #[test]
    fn late_participant_and_its_unknown_state_extend_recording() {
        let mut policy = zoom_episode();
        assert_eq!(apps(&mut policy, 4, [A, I], Some(7)), None);
        assert_eq!(policy.participants(), [true, true]);
        for now in [6, 16, 30] {
            assert_eq!(apps(&mut policy, now, [U, I], Some(7)), None);
        }
        apps(&mut policy, 32, [I, I], Some(7));
        assert_eq!(apps(&mut policy, 40, [I, I], Some(7)), None);
        assert_eq!(
            apps(&mut policy, 42, [I, I], Some(7)),
            Some(Action::Stop(7))
        );
    }
    #[test]
    fn participant_unknown_and_global_permission_loss_reset_end_timer() {
        let mut policy = zoom_episode();
        apps(&mut policy, 4, [U, I], Some(7));
        for now in [12, 20, 30] {
            assert_eq!(apps(&mut policy, now, [U, U], Some(7)), None);
        }
        apps(&mut policy, 32, [U, I], Some(7));
        assert_eq!(apps(&mut policy, 40, [U, I], Some(7)), None);
        assert_eq!(
            apps(&mut policy, 42, [U, I], Some(7)),
            Some(Action::Stop(7))
        );
    }
    #[test]
    fn manual_stop_retains_participants_until_end_and_then_rearms() {
        let mut policy = zoom_episode();
        for now in [4, 6, 10] {
            assert_eq!(apps(&mut policy, now, [U, A], None), None);
        }
        apps(&mut policy, 12, [U, I], None);
        apps(&mut policy, 22, [U, I], None);
        assert_eq!(apps(&mut policy, 24, [U, A], None), None);
        assert_eq!(apps(&mut policy, 26, [U, A], None), Some(Action::Start));
    }
    #[test]
    fn failed_start_suppression_ends_only_after_its_participants_end() {
        let mut policy = Policy::default();
        apps(&mut policy, 0, [U, A], None);
        assert_eq!(apps(&mut policy, 2, [U, A], None), Some(Action::Start));
        policy.started(None);
        for now in [4, 6, 30] {
            assert_eq!(apps(&mut policy, now, [U, A], None), None);
        }
        apps(&mut policy, 32, [U, I], None);
        apps(&mut policy, 42, [U, I], None);
        assert_eq!(apps(&mut policy, 44, [A, U], None), None);
        assert_eq!(apps(&mut policy, 46, [A, U], None), Some(Action::Start));
    }
    #[test]
    fn automatic_stop_clears_episode_for_new_app_after_saving() {
        let mut policy = zoom_episode();
        apps(&mut policy, 4, [U, I], Some(7));
        assert_eq!(
            apps(&mut policy, 14, [U, I], Some(7)),
            Some(Action::Stop(7))
        );
        policy.stopped(true);
        assert_eq!(policy.participants(), [false, false]);
        assert_eq!(
            policy.tick_apps(16, [true, true], [U, A], None, false),
            None
        );
        assert_eq!(apps(&mut policy, 30, [A, U], None), None);
        assert_eq!(apps(&mut policy, 32, [A, U], None), Some(Action::Start));
        policy.started(Some(8));
        apps(&mut policy, 34, [I, U], Some(8));
        assert_eq!(
            apps(&mut policy, 44, [I, U], Some(8)),
            Some(Action::Stop(8))
        );
    }
    #[test]
    fn save_gate_preserves_manual_end_observation_without_starting() {
        let mut policy = zoom_episode();
        policy.tick_apps(4, [true, true], [U, A], None, false);
        policy.tick_apps(6, [true, true], [U, I], None, false);
        policy.tick_apps(16, [true, true], [U, I], None, false);
        assert_eq!(
            policy.tick_apps(18, [true, true], [U, A], None, false),
            None
        );
        assert_eq!(apps(&mut policy, 30, [U, A], None), None);
        assert_eq!(apps(&mut policy, 32, [U, A], None), Some(Action::Start));
    }
    #[test]
    fn added_unknown_source_does_not_join_but_disabling_relinquishes() {
        let mut policy = Policy::default();
        policy.tick_apps(0, [false, true], [U, A], None, true);
        assert_eq!(
            policy.tick_apps(2, [false, true], [U, A], None, true),
            Some(Action::Start)
        );
        policy.started(Some(7));
        policy.reset_confirmation();
        apps(&mut policy, 4, [U, I], Some(7));
        assert_eq!(
            apps(&mut policy, 14, [U, I], Some(7)),
            Some(Action::Stop(7))
        );
        assert_eq!(
            policy.tick_apps(16, [false, false], [U, U], Some(7), true),
            None
        );
        for now in [18, 28, 40] {
            assert_eq!(apps(&mut policy, now, [I, I], Some(7)), None);
        }
        assert_eq!(policy.owned, None);
    }
    #[test]
    fn replacement_manual_session_is_never_stopped_or_claimed() {
        let mut policy = zoom_episode();
        apps(&mut policy, 4, [U, A], Some(8));
        apps(&mut policy, 6, [U, I], Some(8));
        assert_eq!(apps(&mut policy, 16, [U, I], Some(8)), None);
        assert_eq!(policy.owned, None);
        apps(&mut policy, 18, [U, A], Some(8));
        assert_eq!(apps(&mut policy, 20, [U, A], Some(8)), None);
    }
    #[test]
    fn stop_failure_keeps_participants_and_unknown_still_resets_retry_grace() {
        let mut policy = zoom_episode();
        apps(&mut policy, 4, [U, I], Some(7));
        assert_eq!(
            apps(&mut policy, 14, [U, I], Some(7)),
            Some(Action::Stop(7))
        );
        policy.stopped(false);
        assert_eq!(apps(&mut policy, 16, [U, U], Some(7)), None);
        apps(&mut policy, 20, [U, I], Some(7));
        assert_eq!(apps(&mut policy, 28, [U, I], Some(7)), None);
        assert_eq!(
            apps(&mut policy, 30, [U, I], Some(7)),
            Some(Action::Stop(7))
        );
    }
    #[test]
    fn overlapping_calls() {
        assert_eq!(
            combined(&[Observation::Inactive, Observation::Active]),
            Observation::Active
        );
        assert_eq!(
            combined(&[Observation::Inactive, Observation::Unknown]),
            Observation::Unknown
        );
    }
}
