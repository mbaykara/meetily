//! A claimed request reserves the native lifecycle until frontend commit/cancel.
use std::time::{Duration, Instant};
pub struct Lease {
    pub request: String,
    pub generation: u64,
    deadline: Instant,
    pub claimed: bool,
}
impl Lease {
    pub fn new(request: String, generation: u64, now: Instant) -> Self {
        Self {
            request,
            generation,
            deadline: now + Duration::from_secs(5),
            claimed: false,
        }
    }
    pub fn claim(&mut self, request: &str, generation: u64, now: Instant) -> bool {
        if self.request != request
            || self.generation != generation
            || self.claimed
            || now >= self.deadline
        {
            return false;
        }
        self.claimed = true;
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expired_request_cannot_claim_or_mutate() {
        let now = Instant::now();
        let mut lease = Lease::new("old".into(), 1, now);
        assert!(!lease.claim("old", 1, now + Duration::from_secs(6)));
        assert!(!lease.claimed);
    }
    #[test]
    fn changed_selection_cancels_pending_preparation() {
        let now = Instant::now();
        let mut lease = Lease::new("request".into(), 1, now);
        assert!(!lease.claim("request", 2, now));
        assert!(!lease.claimed);
    }
    #[test]
    fn claim_is_session_scoped_and_single_use() {
        let now = Instant::now();
        let mut lease = Lease::new("request".into(), 1, now);
        assert!(!lease.claim("other", 1, now));
        assert!(lease.claim("request", 1, now));
        assert!(!lease.claim("request", 1, now));
    }
}
