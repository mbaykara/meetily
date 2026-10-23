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
    positives: u8,
    absent_since: Option<u64>,
    suppressed: bool,
    pending: bool,
}
impl Policy {
    pub fn tick(
        &mut self,
        now: u64,
        enabled: bool,
        observation: Observation,
        session: Option<u64>,
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
                self.positives = self.positives.saturating_add(1);
                if self.positives >= 2 && session.is_none() && !self.suppressed && !self.pending {
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
            self.suppressed = true;
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
