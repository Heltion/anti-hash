//! Runtime-independent heuristic length planning, tested on native Rust.
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug)]
pub(crate) struct WorkerSettings {
    pub automatic: Option<usize>,
    pub maximum: usize,
}
impl WorkerSettings {
    pub fn new(cpus: Option<usize>, supported: bool) -> Self {
        let cpus = cpus.filter(|&n| n > 0 && supported);
        Self {
            automatic: cpus.map(|n| n.min(4)),
            maximum: cpus.unwrap_or(1),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Success,
    Failed,
    Timeout,
    Cancelled,
}

pub(crate) struct Planner {
    concurrency: usize,
    next: Option<usize>,
    failures: BTreeSet<usize>,
    pub best: Option<usize>,
    pub attempts: usize,
    pub lower: usize,
    pub stalled: bool,
}
impl Planner {
    pub fn new(concurrency: usize) -> Self {
        Self {
            concurrency: concurrency.max(1),
            next: Some(1),
            failures: BTreeSet::new(),
            best: None,
            attempts: 0,
            lower: 0,
            stalled: false,
        }
    }
    pub fn wave(&mut self) -> Vec<usize> {
        if self.stalled {
            return vec![];
        }
        if let Some(upper) = self.best {
            let gap = upper - self.lower;
            let width = self.concurrency.min(gap.saturating_sub(1));
            (1..=width)
                .map(|i| self.lower + ((gap as u128 * i as u128) / (width as u128 + 1)) as usize)
                .collect()
        } else {
            (0..self.concurrency.min(4))
                .filter_map(|_| {
                    let length = self.next?;
                    self.next = length.checked_mul(2);
                    Some(length)
                })
                .collect()
        }
    }
    /// Record results as they arrive; only a strictly shorter success replaces best.
    pub fn record(&mut self, length: usize, outcome: Outcome) -> bool {
        self.attempts += 1;
        let improved = outcome == Outcome::Success && self.best.is_none_or(|n| length < n);
        if improved {
            self.best = Some(length);
        }
        if matches!(outcome, Outcome::Failed | Outcome::Timeout) {
            self.failures.insert(length);
        }
        if let Some(upper) = self.best {
            self.lower = self
                .failures
                .range(..upper)
                .next_back()
                .copied()
                .unwrap_or(0);
        }
        improved
    }
    pub fn finish_wave(&mut self, outcomes: &[Outcome]) {
        self.stalled = self.best.is_none()
            && outcomes
                .iter()
                .all(|o| matches!(o, Outcome::Timeout | Outcome::Cancelled));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sequential_and_parallel_refinement_reaches_boundary() {
        for workers in [1, 2, 3, 4, 8, 24] {
            for target in [1, 2, 3, 6, 16, 31, 32, 65] {
                let mut planner = Planner::new(workers);
                let mut seen = BTreeSet::new();
                loop {
                    let wave = planner.wave();
                    if wave.is_empty() {
                        break;
                    }
                    assert!(wave.len() <= workers);
                    let mut outcomes = vec![];
                    for length in wave.into_iter().rev() {
                        assert!(seen.insert(length));
                        let outcome = if length >= target {
                            Outcome::Success
                        } else {
                            Outcome::Failed
                        };
                        planner.record(length, outcome);
                        outcomes.push(outcome);
                    }
                    planner.finish_wave(&outcomes);
                }
                assert_eq!(planner.best, Some(target));
                assert_eq!(planner.lower, target - 1);
                assert_eq!(planner.attempts, seen.len());
            }
        }
    }
    #[test]
    fn expansion_is_powers_of_two_and_manual_refinement_uses_full_count() {
        let mut planner = Planner::new(24);
        assert_eq!(planner.wave(), [1, 2, 4, 8]);
        assert_eq!(planner.wave(), [16, 32, 64, 128]);
        planner.record(64, Outcome::Failed);
        planner.record(128, Outcome::Success);
        assert_eq!(planner.wave().len(), 24);
    }
    #[test]
    fn cancellation_keeps_best_and_does_not_create_failure_boundary() {
        let mut planner = Planner::new(4);
        assert!(planner.record(8, Outcome::Success));
        assert!(!planner.record(4, Outcome::Cancelled));
        assert_eq!(planner.lower, 0);
        planner.finish_wave(&[Outcome::Success, Outcome::Cancelled]);
        assert_eq!(planner.best, Some(8));
        assert!(!planner.stalled);
    }
    #[test]
    fn timed_out_expansion_stops_without_claiming_nonexistence() {
        let mut planner = Planner::new(2);
        planner.record(1, Outcome::Timeout);
        planner.record(2, Outcome::Timeout);
        planner.finish_wave(&[Outcome::Timeout, Outcome::Timeout]);
        assert!(planner.wave().is_empty());
        assert_eq!(planner.best, None);
    }
    #[test]
    fn auto_is_capped_but_manual_count_is_not() {
        for cpus in [1, 2, 4, 8, 24, 192] {
            let settings = WorkerSettings::new(Some(cpus), true);
            assert_eq!(settings.automatic, Some(cpus.min(4)));
            assert_eq!(settings.maximum, cpus);
        }
        for cpus in [None, Some(0)] {
            let settings = WorkerSettings::new(cpus, true);
            assert_eq!(settings.automatic, None);
            assert_eq!(settings.maximum, 1);
        }
        assert_eq!(WorkerSettings::new(Some(24), false).automatic, None);
        assert_eq!(WorkerSettings::new(Some(24), false).maximum, 1);
    }
    #[test]
    fn overflow_ends_expansion_without_wrapping() {
        let mut planner = Planner::new(4);
        planner.next = Some(usize::MAX);
        assert_eq!(planner.wave(), [usize::MAX]);
        assert!(planner.wave().is_empty());
    }
}
