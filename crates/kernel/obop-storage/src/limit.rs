//! Rate limits that hold across server processes, counted with the backend's atomic expiring
//! counter: a fixed window per key ([`hit`]), and a sliding window estimated from two fixed ones
//! ([`hit_sliding`]), which does not let twice the limit through across the end of a window.

use crate::Backend;

/// Count one hit for `key` in a window of `window_ms`. `Err(seconds)` says how long until the
/// window ends when more than `max` hits fell in it. If the backend itself fails the hit is
/// allowed: a limiter must not take the service down with the database.
pub fn hit(b: &dyn Backend, key: &str, max: u64, window_ms: u64) -> Result<(), u64> {
    match b.counter_add(key, window_ms, false) {
        Ok(n) if n > max => {
            let left = b.counter_ttl_ms(key).ok().flatten().unwrap_or(window_ms);
            Err(left.div_ceil(1000).max(1))
        }
        _ => Ok(()),
    }
}

/// As `hit`, as a boolean: `true` while the key is within its limit.
pub fn allow(b: &dyn Backend, key: &str, max: u64, window_ms: u64) -> bool {
    hit(b, key, max, window_ms).is_ok()
}

/// Count one hit for `key` in a sliding window of `window_ms`, estimated from two fixed windows (the
/// current one and the one before it, weighted by how much of it the sliding window still
/// covers). `Err(seconds)` says how long until the current fixed window ends when the estimate is
/// above `max`. A refused hit is counted too. If the backend fails the hit is allowed, as in
/// [`hit`].
pub fn hit_sliding(b: &dyn Backend, key: &str, max: u64, window_ms: u64) -> Result<(), u64> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    hit_sliding_at(b, key, max, window_ms, now)
}

/// [`hit_sliding`] at the time `now_ms` (milliseconds since the epoch).
pub fn hit_sliding_at(
    b: &dyn Backend,
    key: &str,
    max: u64,
    window_ms: u64,
    now_ms: u64,
) -> Result<(), u64> {
    let window_ms = window_ms.max(1);
    let index = now_ms / window_ms;
    let into = now_ms % window_ms;
    // The counter of a window lives two windows, so that the next one can still read it.
    let Ok(current) = b.counter_add(&format!("{key}:{index}"), 2 * window_ms, false) else {
        return Ok(());
    };
    let previous = match index.checked_sub(1) {
        Some(p) => b.counter_get(&format!("{key}:{p}")).unwrap_or(0),
        None => 0,
    };
    // previous * (window - into) / window + current, in whole hits, rounded down.
    let estimate = current
        + (u128::from(previous) * u128::from(window_ms - into) / u128::from(window_ms)) as u64;
    if estimate > max {
        Err((window_ms - into).div_ceil(1000).max(1))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_admits_max_hits_then_refuses_until_it_ends() {
        let b = crate::open("memory:").unwrap();
        for _ in 0..3 {
            assert!(hit(&*b, "k", 3, 80).is_ok());
        }
        let wait = hit(&*b, "k", 3, 80).unwrap_err();
        assert_eq!(wait, 1, "rounded up to a whole second");
        assert!(hit(&*b, "other", 3, 80).is_ok(), "keys are independent");
        std::thread::sleep(std::time::Duration::from_millis(120));
        assert!(allow(&*b, "k", 3, 80), "a new window");
    }

    #[test]
    fn a_sliding_window_does_not_let_twice_the_limit_through_across_a_boundary() {
        let b = crate::open("memory:").unwrap();
        let minute = 60_000;
        // Sixty hits in the last second of a minute: all allowed.
        let t0 = 1_000 * minute + 59_000;
        for _ in 0..60 {
            assert!(hit_sliding_at(&*b, "s", 60, minute, t0).is_ok());
        }
        assert!(hit_sliding_at(&*b, "s", 60, minute, t0).is_err());
        // Half a second into the next minute: a fixed window would start again from zero and let
        // sixty more through. The sliding window still counts almost all of the sixty.
        let t1 = 1_001 * minute + 500;
        let wait = hit_sliding_at(&*b, "s", 60, minute, t1).unwrap_err();
        assert_eq!(wait, 60, "until the end of this minute, rounded up");
        let fixed = crate::open("memory:").unwrap();
        for _ in 0..60 {
            assert!(hit(&*fixed, "f", 60, 80).is_ok());
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
        assert!(hit(&*fixed, "f", 60, 80).is_ok(), "the fixed window does");
        // As the minute goes on, the weight of the last one falls and hits pass again: at 45 s,
        // a quarter of the 61 hits counted then (one was refused) is left, 15, plus this minute's
        // two.
        let t2 = 1_001 * minute + 45_000;
        assert!(hit_sliding_at(&*b, "s", 60, minute, t2).is_ok());
        // Two minutes later nothing of the burst is left.
        let t3 = 1_003 * minute;
        for _ in 0..60 {
            assert!(hit_sliding_at(&*b, "s", 60, minute, t3).is_ok());
        }
        assert!(hit_sliding_at(&*b, "s", 60, minute, t3).is_err());
        assert!(
            hit_sliding_at(&*b, "other", 60, minute, t3).is_ok(),
            "keys are independent"
        );
    }
}
