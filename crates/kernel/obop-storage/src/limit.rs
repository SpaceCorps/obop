//! Rate limits that hold across server processes: a fixed window per key, counted with the
//! backend's atomic expiring counter.

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
}
