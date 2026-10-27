//! The queue of outbound deliveries: retries with backoff, then a dead letter. Durable (B2, D59).
//!
//! The queue holds the already-treated payload (what the policy released), the id of the
//! `sync_run` that records the outcome, and the schedule. Jobs live in the storage backend
//! (`connector_job`; the payload sealed with the server key, because it can still carry personal
//! data), so a restart, or another server process on the same database, carries on where the last
//! one stopped. A process that wants work *claims* due jobs under a lease: while the lease runs
//! nobody else takes the job; if the process dies, the lease runs out and another takes it. That
//! gives at-least-once delivery. The job id is stable across retries and goes out as the webhook
//! id, so a receiver can deduplicate the rare repeat. A job that used up its retries moves to
//! `connector_dead` with the reason, where an operator can see it and send it again.

use crate::secretbox::SecretBox;
use base64::Engine as _;
use obop_storage::{Backend, Fields};
use serde_json::{Value as J, json};
use std::sync::Arc;

/// Seconds to wait before retry 1, 2, 3 ...; after the last, the delivery is dead-lettered.
pub const BACKOFF_SECS: [u64; 5] = [5, 30, 300, 1800, 7200];
/// How long a claim lasts. A delivery is one HTTP request with a 20 second timeout, so a live
/// worker is done long before this; a dead one is replaced after it.
pub const LEASE_SECS: u64 = 120;

const JOBS: &str = "connector_job";
const DEAD: &str = "connector_dead";
/// Cross-process lock around claiming, so two workers never take the same job.
const CLAIM_LOCK: &str = "connectors.queue";

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

/// A delivery that gave up, as an operator sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadJob {
    pub id: String,
    pub connection: String,
    pub run: String,
    pub attempts: u32,
    pub reason: String,
    pub at: u64,
}

pub struct Queue {
    backend: Arc<dyn Backend>,
    secrets: SecretBox,
    /// Names this worker in the lease (not a secret).
    owner: String,
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

fn purpose(id: &str) -> String {
    format!("queue-job:{id}")
}

fn num(f: &Fields, k: &str) -> u64 {
    f.get(k).and_then(J::as_u64).unwrap_or(0)
}

fn text<'a>(f: &'a Fields, k: &str) -> &'a str {
    f.get(k).and_then(J::as_str).unwrap_or("")
}

impl Queue {
    pub fn new(backend: Arc<dyn Backend>, secrets: SecretBox) -> Queue {
        Queue {
            backend,
            secrets,
            owner: format!("{}-{}", std::process::id(), crate::util::random_hex(4)),
        }
    }

    /// A queue over a private in-memory store (unit tests).
    pub fn in_memory() -> Queue {
        Queue::new(
            obop_storage::open("memory:").expect("memory backend"),
            SecretBox::for_tests(),
        )
    }

    fn fields_of(&self, job: &Job) -> Fields {
        let body = base64::engine::general_purpose::STANDARD.encode(&job.body);
        let mut f = Fields::new();
        f.insert("connection".into(), job.connection.clone().into());
        f.insert("run".into(), job.run.clone().into());
        f.insert("content_type".into(), job.content_type.clone().into());
        f.insert(
            "body".into(),
            self.secrets.seal(&purpose(&job.id), &body).into(),
        );
        f.insert("attempt".into(), job.attempt.into());
        f.insert("next_at".into(), job.next_at.into());
        f.insert("lease_until".into(), 0.into());
        f.insert("lease_owner".into(), "".into());
        f
    }

    fn job_of(&self, id: &str, f: &Fields) -> Option<Job> {
        let body = self.secrets.open(&purpose(id), text(f, "body")).ok()?;
        let body = base64::engine::general_purpose::STANDARD
            .decode(body)
            .ok()?;
        Some(Job {
            id: id.to_string(),
            connection: text(f, "connection").into(),
            run: text(f, "run").into(),
            content_type: text(f, "content_type").into(),
            body,
            attempt: num(f, "attempt") as u32,
            next_at: num(f, "next_at"),
        })
    }

    /// Queue a job. A job with the same id replaces nothing: ids are unique per delivery. A
    /// refusal of the store is an error: a retry that was not queued is a delivery nobody will
    /// make, and the caller must say so.
    pub fn push(&self, job: Job) -> Result<(), String> {
        let f = self.fields_of(&job);
        match self.backend.insert(JOBS, &job.id, &f) {
            Ok(()) => Ok(()),
            // The same id again (a retry pushed twice) is the same job.
            Err(e)
                if !obop_storage::fault::Fault::is_transient(&e)
                    && (e.to_lowercase().contains("duplicate")
                        || e.to_lowercase().contains("unique")) =>
            {
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Jobs waiting (leased or not).
    pub fn len(&self) -> usize {
        self.backend.count(JOBS).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Claim the jobs that are due at `now` and not leased by anyone: they are leased to this
    /// worker for `LEASE_SECS`. Finish each with `complete` or `requeue`.
    pub fn take_due(&self, now: u64) -> Vec<Job> {
        let Ok(_g) = self.backend.lock(CLAIM_LOCK) else {
            return Vec::new();
        };
        let mut claimed = Vec::new();
        for (id, f) in self.backend.list(JOBS).unwrap_or_default() {
            if num(&f, "next_at") > now || num(&f, "lease_until") > now {
                continue;
            }
            let mut p = Fields::new();
            p.insert("lease_until".into(), (now + LEASE_SECS).into());
            p.insert("lease_owner".into(), self.owner.clone().into());
            if !self.backend.update(JOBS, &id, &p).unwrap_or(false) {
                continue;
            }
            match self.job_of(&id, &f) {
                Some(job) => claimed.push(job),
                // A record that cannot be opened (the key changed) must not be retried forever
                // in silence: move it to the dead letters with the reason.
                None => self.bury(
                    &id,
                    &f,
                    "the queued payload cannot be opened (secret key?)",
                    now,
                ),
            }
        }
        claimed
    }

    /// The delivery is finished (succeeded, or refused for good): forget the job.
    pub fn complete(&self, job: &Job) {
        let _ = self.backend.delete(JOBS, &job.id);
    }

    /// A delivery failed: schedule the next try (releasing the lease) or give up and keep the
    /// job as a dead letter with `reason`.
    pub fn requeue(&self, job: Job, now: u64, jitter_per_mille: u64) -> Requeued {
        self.requeue_because(job, now, jitter_per_mille, "")
    }

    pub fn requeue_because(
        &self,
        mut job: Job,
        now: u64,
        jitter_per_mille: u64,
        reason: &str,
    ) -> Requeued {
        job.attempt += 1;
        match backoff(job.attempt, jitter_per_mille) {
            Some(wait) => {
                job.next_at = now + wait;
                let mut p = Fields::new();
                p.insert("attempt".into(), job.attempt.into());
                p.insert("next_at".into(), job.next_at.into());
                p.insert("lease_until".into(), 0.into());
                p.insert("lease_owner".into(), "".into());
                if !self.backend.update(JOBS, &job.id, &p).unwrap_or(false) {
                    // Not there any more (dropped meanwhile, or never pushed): put it back. If
                    // that is refused the job keeps its lease and is taken again when the lease
                    // ends, so nothing is lost; there is nobody to tell here.
                    let _ = self.push(job.clone());
                }
                Requeued::At(job.next_at)
            }
            None => {
                if let Ok(Some(f)) = self.backend.get(JOBS, &job.id) {
                    self.bury(&job.id, &f, reason, now);
                } else {
                    let f = self.fields_of(&job);
                    self.bury(&job.id, &f, reason, now);
                }
                Requeued::Dead
            }
        }
    }

    /// Move a job to the dead letters (the payload stays sealed).
    fn bury(&self, id: &str, f: &Fields, reason: &str, now: u64) {
        let mut d = f.clone();
        d.insert(
            "reason".into(),
            reason.chars().take(300).collect::<String>().into(),
        );
        d.insert("dead_at".into(), now.into());
        d.remove("lease_until");
        d.remove("lease_owner");
        // The job leaves the queue only once it is in the dead letters: if that write is refused
        // it stays leased and is taken (and buried) again when the lease ends, never lost.
        if let Err(e) = self.backend.insert(DEAD, id, &d) {
            // Already buried (an earlier try whose answer was lost): the job can go. Any other
            // refusal, a full disk included, is not "already buried": the job stays leased and is
            // taken and buried again when the lease ends, never lost.
            if crate::util::row_exists_or(&*self.backend, DEAD, id, e).is_err() {
                return;
            }
        }
        let _ = self.backend.delete(JOBS, id);
    }

    /// Drop every job of a connection (it was paused or revoked), queued and dead. A refusal of
    /// the store is an error: the bodies of a revoked connection must not be answered as gone
    /// while they are still stored. Safe to run again.
    pub fn drop_connection(&self, connection: &str) -> Result<usize, String> {
        let mut n = 0;
        for model in [JOBS, DEAD] {
            for (id, f) in self.backend.list(model)? {
                if text(&f, "connection") == connection && self.backend.delete(model, &id)? {
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    /// Remove the dead letters that gave up more than `keep_days(connection)` days before `at`.
    /// A dead letter keeps the treated payload of a delivery (what the policy let through, in
    /// clear where the owner allowed it) and, unlike the flow log, nothing else ever removed it:
    /// it is held for as long as the connection's retention, like the rest of its records. A
    /// refusal of the store is an error (the run is repeated). Returns how many were removed.
    pub fn purge_dead(&self, at: u64, keep_days: impl Fn(&str) -> i64) -> Result<usize, String> {
        let mut n = 0;
        for (id, f) in self.backend.list(DEAD)? {
            let kept_until =
                num(&f, "dead_at") + (keep_days(text(&f, "connection")).max(1) as u64) * 86_400;
            if kept_until < at && self.backend.delete(DEAD, &id)? {
                n += 1;
            }
        }
        Ok(n)
    }

    /// The deliveries that gave up, oldest first.
    pub fn dead(&self) -> Vec<DeadJob> {
        self.backend
            .list(DEAD)
            .unwrap_or_default()
            .into_iter()
            .map(|(id, f)| DeadJob {
                id,
                connection: text(&f, "connection").into(),
                run: text(&f, "run").into(),
                attempts: num(&f, "attempt") as u32,
                reason: text(&f, "reason").into(),
                at: num(&f, "dead_at"),
            })
            .collect()
    }

    /// Put a dead letter back in the queue, due at `now`, with its attempts counted afresh.
    pub fn revive(&self, id: &str, now: u64) -> bool {
        let Ok(Some(mut f)) = self.backend.get(DEAD, id) else {
            return false;
        };
        f.remove("reason");
        f.remove("dead_at");
        f.insert("attempt".into(), 1.into());
        f.insert("next_at".into(), now.into());
        f.insert("lease_until".into(), 0.into());
        f.insert("lease_owner".into(), "".into());
        if self.backend.insert(JOBS, id, &f).is_err() {
            return false;
        }
        let _ = self.backend.delete(DEAD, id);
        true
    }

    /// For the operator's report: counts of waiting, leased and dead jobs at `now`.
    pub fn stats(&self, now: u64) -> J {
        let jobs = self.backend.list(JOBS).unwrap_or_default();
        let leased = jobs
            .iter()
            .filter(|(_, f)| num(f, "lease_until") > now)
            .count();
        json!({"waiting": jobs.len() - leased, "leased": leased, "dead": self.backend.count(DEAD).unwrap_or(0)})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: &str, next_at: u64) -> Job {
        Job {
            id: id.into(),
            connection: "c1".into(),
            run: "r1".into(),
            content_type: "application/json".into(),
            body: b"{\"n\":1}\xff".to_vec(),
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
    fn retries_then_dead_letters_and_keeps_the_reason() {
        let q = Queue::in_memory();
        let mut j = job("m1", 0);
        j.attempt = 0;
        let mut now = 1000;
        q.push(j.clone()).unwrap();
        for expected in [5, 30, 300, 1800, 7200] {
            let claimed = q.take_due(now).pop().unwrap();
            assert_eq!(
                q.requeue_because(claimed, now, 0, "boom"),
                Requeued::At(now + expected)
            );
            now += expected;
        }
        let last = q.take_due(now).pop().unwrap();
        assert_eq!(last.attempt, 5);
        assert_eq!(
            q.requeue_because(last, now, 0, "receiver down"),
            Requeued::Dead
        );
        assert!(q.is_empty());
        let dead = q.dead();
        assert_eq!(dead.len(), 1);
        assert_eq!(
            (dead[0].id.as_str(), dead[0].reason.as_str()),
            ("m1", "receiver down")
        );
        assert_eq!(dead[0].at, now);
        // An operator can send it again.
        assert!(q.revive("m1", now + 1));
        assert!(q.dead().is_empty());
        let again = q.take_due(now + 1).pop().unwrap();
        assert_eq!((again.id.as_str(), again.attempt), ("m1", 1));
        assert!(!q.revive("m1", now));
    }

    #[test]
    fn a_dead_letter_is_held_for_the_retention_of_its_connection_and_then_removed() {
        let q = Queue::in_memory();
        let bury = |id: &str, connection: &str, at: u64| {
            let mut j = job(id, 0);
            j.connection = connection.into();
            j.attempt = 5;
            assert_eq!(q.requeue_because(j, at, 0, "down"), Requeued::Dead);
        };
        let day = 86_400;
        bury("old", "c1", 1_000);
        bury("young", "c1", 1_000 + 20 * day);
        bury("long", "c2", 1_000);
        let keep = |c: &str| if c == "c2" { 90 } else { 10 };
        // Nothing is older than its connection's retention yet.
        assert_eq!(q.purge_dead(1_000 + 9 * day, keep), Ok(0));
        // Eleven days on, the first letter of c1 is past ten days; the one of c2 (90 days) is not.
        assert_eq!(q.purge_dead(1_000 + 11 * day, keep), Ok(1));
        let left: Vec<String> = q.dead().into_iter().map(|d| d.id).collect();
        assert!(left.contains(&"young".to_string()) && left.contains(&"long".to_string()));
        assert!(!left.contains(&"old".to_string()));
        assert_eq!(q.purge_dead(1_000 + 91 * day, keep), Ok(2));
        assert!(q.dead().is_empty());
    }

    #[test]
    fn a_job_stays_when_the_dead_letter_could_not_be_written_for_a_reason_that_is_no_duplicate() {
        use obop_storage::testing::Flaky;
        let b = obop_storage::open("memory:").unwrap();
        let flaky = Flaky::new(b.clone());
        let q = Queue::new(obop_storage::guarded(flaky.clone()), SecretBox::for_tests());
        let mut j = job("m1", 0);
        j.attempt = 5;
        q.push(j).unwrap();
        let claimed = q.take_due(10).pop().unwrap();
        // A full disk is not "already buried": deleting the job then lost the delivery with no
        // trace at all.
        flaky.fail_inserts_into(DEAD, "could not extend file: No space left on device");
        assert_eq!(q.requeue_because(claimed, 10, 0, "down"), Requeued::Dead);
        assert_eq!(b.count(JOBS).unwrap(), 1, "the job is still queued");
        assert_eq!(b.count(DEAD).unwrap(), 0);
        flaky.allow_models();
        // Once the lease ends it is taken again and buried.
        let again = q.take_due(10 + LEASE_SECS + 1).pop().unwrap();
        assert_eq!(q.requeue_because(again, 200, 0, "down"), Requeued::Dead);
        assert_eq!(b.count(JOBS).unwrap(), 0);
        assert_eq!(b.count(DEAD).unwrap(), 1);
    }

    #[test]
    fn only_due_jobs_are_taken_and_a_connection_can_be_dropped() {
        let q = Queue::in_memory();
        q.push(job("a", 10)).unwrap();
        q.push(job("b", 50)).unwrap();
        assert_eq!(q.take_due(20).len(), 1);
        assert_eq!(q.len(), 2, "a claimed job stays until it is completed");
        assert_eq!(q.drop_connection("c1"), Ok(2));
        assert!(q.is_empty());
    }

    #[test]
    fn a_job_leaves_the_queue_only_once_it_is_in_the_dead_letters() {
        use obop_storage::testing::{Flaky, Mode};
        let b = obop_storage::open("memory:").unwrap();
        let flaky = Flaky::new(b.clone());
        let q = Queue::new(flaky.clone(), SecretBox::for_tests());
        q.push(job("m1", 0)).unwrap();
        let mut last = q.take_due(0).pop().unwrap();
        last.attempt = 5;
        // The dead letters cannot be written: the job must not be deleted behind that.
        flaky.refuse_model(DEAD, Mode::All);
        assert_eq!(
            q.requeue_because(last.clone(), 0, 0, "down"),
            Requeued::Dead
        );
        flaky.allow_models();
        assert!(
            b.get(JOBS, "m1").unwrap().is_some(),
            "still queued, and not lost"
        );
        assert!(q.dead().is_empty());
        // Buried when the store answers.
        assert_eq!(q.requeue_because(last, 0, 0, "down"), Requeued::Dead);
        assert!(b.get(JOBS, "m1").unwrap().is_none());
        assert_eq!(q.dead().len(), 1);
    }

    #[test]
    fn the_payload_is_sealed_at_rest_and_round_trips_binary() {
        let b = obop_storage::open("memory:").unwrap();
        let q = Queue::new(b.clone(), SecretBox::for_tests());
        q.push(job("m1", 0)).unwrap();
        let raw = b.get(JOBS, "m1").unwrap().unwrap();
        let body = raw["body"].as_str().unwrap();
        assert!(body.starts_with("e2.") && !body.contains("eyJ"));
        let got = q.take_due(0).pop().unwrap();
        assert_eq!(got.body, b"{\"n\":1}\xff");
        // The seal is bound to the job id: a payload copied to another job does not open.
        let mut moved = raw.clone();
        moved.insert("lease_until".into(), 0.into());
        b.insert(JOBS, "other", &moved).unwrap();
        q.complete(&got);
        let c = q.take_due(0);
        assert!(c.is_empty(), "the unopenable job is not delivered");
        assert_eq!(q.dead().len(), 1);
    }

    #[test]
    fn a_claimed_job_is_not_taken_twice_and_a_dead_worker_is_replaced_after_the_lease() {
        let b = obop_storage::open("memory:").unwrap();
        let (a, other) = (
            Queue::new(b.clone(), SecretBox::for_tests()),
            Queue::new(b.clone(), SecretBox::for_tests()),
        );
        a.push(job("m1", 100)).unwrap();
        assert_eq!(a.take_due(100).len(), 1, "A claims it");
        assert!(other.take_due(100).is_empty(), "B finds it leased");
        assert!(other.take_due(100 + LEASE_SECS - 1).is_empty());
        // A crashed without completing: after the lease B delivers it (same id, for dedupe).
        let again = other.take_due(100 + LEASE_SECS).pop().unwrap();
        assert_eq!(again.id, "m1");
        other.complete(&again);
        assert!(a.is_empty() && other.is_empty());
    }

    #[test]
    fn concurrent_workers_split_the_jobs_without_overlap() {
        let b = obop_storage::open("memory:").unwrap();
        for i in 0..40 {
            Queue::new(b.clone(), SecretBox::for_tests())
                .push(job(&format!("m{i}"), 0))
                .unwrap();
        }
        let got: Vec<Vec<String>> = std::thread::scope(|s| {
            let hs: Vec<_> = (0..4)
                .map(|_| {
                    let b = b.clone();
                    s.spawn(move || {
                        let q = Queue::new(b, SecretBox::for_tests());
                        q.take_due(0).into_iter().map(|j| j.id).collect::<Vec<_>>()
                    })
                })
                .collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let mut all: Vec<String> = got.into_iter().flatten().collect();
        let n = all.len();
        all.sort();
        all.dedup();
        assert_eq!((n, all.len()), (40, 40), "every job exactly once");
    }
}
