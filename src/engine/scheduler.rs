use std::time::Duration;

use crate::core::model::RunMode;

pub fn should_stop(run_mode: &RunMode, completed_actions: u64, elapsed: Duration) -> bool {
    match run_mode {
        RunMode::Infinite => false,
        RunMode::Count { total } => completed_actions >= *total,
        RunMode::Timed { duration_ms } => elapsed.as_millis() >= u128::from(*duration_ms),
    }
}

pub fn next_interval_ms(base_interval_ms: u64, jitter_ms: Option<u64>, iteration: u64) -> u64 {
    let jitter_ms = jitter_ms.unwrap_or(0);
    if jitter_ms == 0 {
        return base_interval_ms;
    }

    let spread = (iteration % (jitter_ms.saturating_mul(2) + 1)) as i64 - jitter_ms as i64;
    base_interval_ms.saturating_add_signed(spread)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{next_interval_ms, should_stop};
    use crate::core::model::RunMode;

    #[test]
    fn count_mode_stops_at_target() {
        assert!(should_stop(&RunMode::Count { total: 3 }, 3, Duration::ZERO));
    }

    #[test]
    fn infinite_mode_never_stops_by_scheduler() {
        assert!(!should_stop(
            &RunMode::Infinite,
            10,
            Duration::from_secs(10)
        ));
    }

    #[test]
    fn jitter_interval_stays_near_base() {
        let result = next_interval_ms(20, Some(3), 4);
        assert!((17..=23).contains(&result));
    }
}
