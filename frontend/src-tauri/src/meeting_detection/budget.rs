//! Hard limits shared by the native scanner and deterministic tests.
use std::time::Duration;
pub const MAX_ELEMENTS: usize = 2500;
pub const MAX_DEPTH: usize = 18;
pub const MAX_SCAN_TIME: Duration = Duration::from_millis(900);
#[derive(Default)]
pub struct Budget {
    visited: usize,
}
impl Budget {
    pub fn visit(&mut self, depth: usize, elapsed: Duration) -> bool {
        self.visited += 1;
        self.visited <= MAX_ELEMENTS && depth <= MAX_DEPTH && elapsed < MAX_SCAN_TIME
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn element_limit() {
        let mut budget = Budget::default();
        for _ in 0..MAX_ELEMENTS {
            assert!(budget.visit(0, Duration::ZERO));
        }
        assert!(!budget.visit(0, Duration::ZERO));
    }
    #[test]
    fn large_shell_does_not_consume_another_windows_budget() {
        let mut shell = Budget::default();
        for _ in 0..700 {
            assert!(shell.visit(5, Duration::ZERO));
        }
        let mut call = Budget::default();
        assert!(call.visit(1, Duration::ZERO));
    }
    #[test]
    fn time_and_depth_limits() {
        assert!(!Budget::default().visit(0, MAX_SCAN_TIME));
        assert!(!Budget::default().visit(MAX_DEPTH + 1, Duration::ZERO));
        assert!(Budget::default().visit(MAX_DEPTH, Duration::ZERO));
    }
}
