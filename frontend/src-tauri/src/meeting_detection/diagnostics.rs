//! Diagnostics contain categories and native counters only, never inspected UI data.
use super::policy::Observation;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanReason {
    Disabled,
    UnsupportedPlatform,
    PermissionDenied,
    WorkerFailed,
    ProcessAbsent,
    ConfirmedCall,
    RecognizedIdle,
    PartialCall,
    UnrecognizedLayout,
    AccessibilityError,
    MissingRole,
    MissingChildren,
    InvalidAttribute,
    ElementLimit,
    DepthLimit,
    WindowTimeLimit,
    ApplicationTimeLimit,
    WindowLimit,
    NoWindows,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppReport {
    pub observation: Observation,
    pub complete: bool,
    pub reason: ScanReason,
}
impl AppReport {
    pub const fn unknown(reason: ScanReason) -> Self {
        Self {
            observation: Observation::Unknown,
            complete: false,
            reason,
        }
    }
    pub const fn absent() -> Self {
        Self {
            observation: Observation::Inactive,
            complete: true,
            reason: ScanReason::ProcessAbsent,
        }
    }
    pub fn merge(self, other: Self) -> Self {
        if self.reason == ScanReason::ProcessAbsent {
            return other;
        }
        let complete = self.complete && other.complete;
        if self.observation == Observation::Active || other.observation == Observation::Active {
            Self {
                observation: Observation::Active,
                complete,
                reason: ScanReason::ConfirmedCall,
            }
        } else if self.observation == Observation::Unknown {
            Self { complete, ..self }
        } else if other.observation == Observation::Unknown {
            Self { complete, ..other }
        } else {
            Self {
                observation: Observation::Inactive,
                complete,
                reason: ScanReason::RecognizedIdle,
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub selected: [bool; 2],
    pub accessibility: bool,
    pub applications: [AppReport; 2],
    pub participants: [bool; 2],
    pub owned_session: Option<u64>,
    pub native_session: Option<u64>,
    pub waiting_for_save: bool,
}
#[derive(Default)]
pub struct Transitions {
    previous: Option<Snapshot>,
}
impl Transitions {
    pub fn changed(&mut self, snapshot: Snapshot) -> bool {
        if self.previous == Some(snapshot) {
            false
        } else {
            self.previous = Some(snapshot);
            true
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_output_changes_only_for_state_reason_completeness_or_ownership() {
        let mut transitions = Transitions::default();
        let mut snapshot = Snapshot {
            selected: [true, true],
            accessibility: true,
            applications: [AppReport::absent(); 2],
            participants: [false; 2],
            owned_session: None,
            native_session: None,
            waiting_for_save: false,
        };
        assert!(transitions.changed(snapshot));
        assert!(!transitions.changed(snapshot));
        snapshot.participants = [false, true];
        assert!(transitions.changed(snapshot));
        assert!(!transitions.changed(snapshot));
        snapshot.owned_session = Some(42);
        assert!(transitions.changed(snapshot));
        assert!(!transitions.changed(snapshot));
        snapshot.applications[0] = AppReport::unknown(ScanReason::WindowTimeLimit);
        assert!(transitions.changed(snapshot));
        assert!(!transitions.changed(snapshot));
    }
    #[test]
    fn a_second_unreadable_process_is_unknown_unless_a_call_is_confirmed() {
        let idle = AppReport {
            observation: Observation::Inactive,
            complete: true,
            reason: ScanReason::RecognizedIdle,
        };
        let unknown = AppReport::unknown(ScanReason::AccessibilityError);
        assert_eq!(idle.merge(unknown).observation, Observation::Unknown);
        let active = AppReport {
            observation: Observation::Active,
            complete: false,
            reason: ScanReason::ConfirmedCall,
        };
        assert_eq!(unknown.merge(active).observation, Observation::Active);
    }
}
