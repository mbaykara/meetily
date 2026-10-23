//! A stopped session retains shared frontend state until saving completes.
use std::sync::atomic::{AtomicU64, Ordering};
#[derive(Default)]
pub struct PostProcessing {
    session: AtomicU64,
}
impl PostProcessing {
    pub fn pending(&self) -> u64 {
        self.session.load(Ordering::SeqCst)
    }
    pub fn ensure_start_available(&self) -> Result<(), String> {
        if self.pending() == 0 {
            Ok(())
        } else {
            Err("The previous recording is still being saved. Use Retry save in Recording Settings if saving failed.".into())
        }
    }
    pub fn begin(&self, session: u64) -> Result<(), String> {
        self.session
            .compare_exchange(0, session, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| ())
            .map_err(|_| "Previous session post-processing has not completed".into())
    }
    pub fn complete(&self, session: u64) {
        let _ = self
            .session
            .compare_exchange(session, 0, Ordering::SeqCst, Ordering::SeqCst);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slow_save_blocks_replacement_start_and_token_overwrite() {
        let gate = PostProcessing::default();
        gate.begin(1).unwrap();
        assert!(gate.ensure_start_available().is_err());
        assert!(gate.begin(2).is_err());
        assert_eq!(gate.pending(), 1);
        gate.complete(2);
        assert!(gate.ensure_start_available().is_err());
        gate.complete(1);
        assert!(gate.ensure_start_available().is_ok());
        gate.begin(2).unwrap();
        gate.complete(2);
        assert!(gate.ensure_start_available().is_ok());
    }
    #[test]
    fn failed_save_stays_reserved_until_same_sessions_retry_succeeds() {
        let gate = PostProcessing::default();
        gate.begin(7).unwrap();
        // A failed save never completes the reservation.
        assert!(gate.ensure_start_available().is_err());
        assert_eq!(gate.pending(), 7);
        gate.complete(7);
        assert!(gate.ensure_start_available().is_ok());
    }
}
