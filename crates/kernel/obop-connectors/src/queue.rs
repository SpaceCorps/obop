//! A small in-process queue for outbound deliveries: retries with backoff, then a dead letter.
//!
//! The queue holds the already-treated payload (what the policy released), the id of the
//! `sync_run` that records the outcome, and the schedule. It lives in memory: a restart drops
//! pending retries (the run stays `running` until the next maintenance pass marks it failed), and
//! the owner resends by hand. That is stated in docs/CONNECTORS.md, not hidden.

use std::sync::Mutex;

/// Seconds to wait before retry 1, 2, 3 ...; after the last, the delivery is dead-lettered.
pub const BACKOFF_SECS: [u64; 5] = [5, 30, 300, 1800, 7200];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    /// Stable across retries; sent as the webhook id so the receiver can deduplicate.
    pub id: String,
    pub connection: String,
    pub run: String,
    pub content_type: String,
    pub body: Vec<u8>,
    /// Attempts made so far.
    pub attempt: u32,
    pub next_at: u64,
}

#[derive(Default)]
pub struct Queue {
    jobs: Mutex<Vec<Job>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Requeued {
    /// Will be tried again at this time.
    At(u64),
    /// Out of retries.
    Dead,
}

/// The wait before the retry that follows failed attempt number `attempt` (1-based), with up to
/// 20 % of jitter taken off so a burst of failures does not retry in step. `None` after the last.
pub fn backoff(attempt: u32, jitter_per_mille: u64) -> Option<u64> {
    let base = *BACKOFF_SECS.get(attempt as usize - 1)?;
    Some(base - base * jitter_per_mille.min(200) / 1000)
}

impl Queue {
    pub fn push(&self, job: Job) {
        self.jobs.lock().unwrap().push(job);
    }

    pub fn len(&self) -> usize {
        self.jobs.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Take the jobs that are due at `now`.
    pub fn take_due(&self, now: u64) -> Vec<Job> {
        let mut g = self.jobs.lock().unwrap();
        let (due, later): (Vec<Job>, Vec<Job>) = g.drain(..).partition(|j| j.next_at <= now);
        *g = later;
        due
    }

    /// A delivery failed: schedule the next try or give up.
    pub fn requeue(&self, mut job: Job, now: u64, jitter_per_mille: u64) -> Requeued {
        job.attempt += 1;
        match backoff(job.attempt, jitter_per_mille) {
            Some(wait) => {
                job.next_at = now + wait;
                let at = job.next_at;
                self.push(job);
                Requeued::At(at)
            }
            None => Requeued::Dead,
        }
    }

    /// Drop every job of a connection (it was paused or revoked).
    pub fn drop_connection(&self, connection: &str) -> usize {
        let mut g = self.jobs.lock().unwrap();
        let before = g.len();
        g.retain(|j| j.connection != connection);
        before - g.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(next_at: u64) -> Job {
        Job {
            id: "msg_1".into(),
            connection: "c1".into(),
            run: "r1".into(),
            content_type: "application/json".into(),
            body: b"{}".to_vec(),
            attempt: 1,
            next_at,
        }
    }

    #[test]
    fn backoff_grows_then_gives_up() {
        assert_eq!(backoff(1, 0), Some(5));
        assert_eq!(backoff(5, 0), Some(7200));
        assert_eq!(backoff(6, 0), None);
        assert_eq!(backoff(3, 200), Some(240), "jitter only shortens");
        assert_eq!(backoff(3, 900), Some(240), "and is capped");
    }

    #[test]
    fn retries_then_dead_letters() {
        let q = Queue::default();
        let mut j = job(0);
        j.attempt = 0;
        let mut now = 1000;
        for expected in [5, 30, 300, 1800, 7200] {
            assert_eq!(q.requeue(j.clone(), now, 0), Requeued::At(now + expected));
            j = q.take_due(now + expected).pop().unwrap();
            now += expected;
        }
        assert_eq!(q.requeue(j, now, 0), Requeued::Dead);
        assert!(q.is_empty());
    }

    #[test]
    fn only_due_jobs_are_taken_and_a_connection_can_be_dropped() {
        let q = Queue::default();
        q.push(job(10));
        q.push(job(50));
        assert_eq!(q.take_due(20).len(), 1);
        assert_eq!(q.len(), 1);
        assert_eq!(q.drop_connection("c1"), 1);
        assert!(q.is_empty());
    }
}
