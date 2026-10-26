//! Which backups to keep (OBR-DAT-047 item 2, D73).
//!
//! The policy is three numbers: how many of the most recent days, weeks and months to keep a
//! backup of. For each period (a UTC calendar day, an ISO week from Monday, a calendar month) in
//! which at least one backup exists, the newest backup of the period is kept; the `n` most recent
//! such periods count. A backup kept by any rule is kept; the rest may be deleted. Whatever the
//! numbers, the newest backup is never deleted. The default is the one of the requirement: 35
//! daily, 12 monthly, no weekly and no yearly backups.
//!
//! Retention bounds how long erased personal data survives in backups, so it is a setting an
//! operator states in a privacy notice, not an internal detail: `OBOP_BACKUP_KEEP_DAILY`,
//! `OBOP_BACKUP_KEEP_WEEKLY`, `OBOP_BACKUP_KEEP_MONTHLY`. The counts alone count periods *that
//! have a backup*, so a gap or a sparse schedule stretches them; `OBOP_BACKUP_MAX_AGE_DAYS`
//! (default 400) is the bound that holds whatever the counts do: a scheduled backup is not kept
//! beyond it (the newest of all excepted, and nothing is removed from this host before every
//! sink holds a verified copy).
//!
//! Pure functions of the list of backups; the clock is injected where time matters (the
//! scheduler), so the rules are tested over simulated months in microseconds.

use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retention {
    pub daily: usize,
    pub weekly: usize,
    pub monthly: usize,
}

impl Default for Retention {
    fn default() -> Self {
        Retention {
            daily: 35,
            weekly: 0,
            monthly: 12,
        }
    }
}

impl Retention {
    /// The policy from the three variables, the default where one is unset. A value that is not a
    /// number is an error, not a silent default: retention is a promise.
    pub fn from_vars(
        daily: Option<&str>,
        weekly: Option<&str>,
        monthly: Option<&str>,
    ) -> Result<Retention, String> {
        let d = Retention::default();
        let n = |v: Option<&str>, name: &str, default: usize| -> Result<usize, String> {
            match v.map(str::trim).filter(|s| !s.is_empty()) {
                None => Ok(default),
                Some(s) => s
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n <= 10_000)
                    .ok_or_else(|| {
                        format!("{name} must be a whole number (0 to 10000), not {s:?}")
                    }),
            }
        };
        Ok(Retention {
            daily: n(daily, "OBOP_BACKUP_KEEP_DAILY", d.daily)?,
            weekly: n(weekly, "OBOP_BACKUP_KEEP_WEEKLY", d.weekly)?,
            monthly: n(monthly, "OBOP_BACKUP_KEEP_MONTHLY", d.monthly)?,
        })
    }

    pub fn from_env() -> Result<Retention, String> {
        let var = |n: &str| std::env::var(n).ok();
        Self::from_vars(
            var("OBOP_BACKUP_KEEP_DAILY").as_deref(),
            var("OBOP_BACKUP_KEEP_WEEKLY").as_deref(),
            var("OBOP_BACKUP_KEEP_MONTHLY").as_deref(),
        )
    }

    /// In words, for a privacy notice and the log.
    pub fn describe(&self) -> String {
        format!(
            "{} daily, {} weekly and {} monthly backups (the newest of each period)",
            self.daily, self.weekly, self.monthly
        )
    }
}

/// Why a backup is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Why {
    Newest,
    Daily,
    Weekly,
    Monthly,
}

impl Why {
    pub fn as_str(self) -> &'static str {
        match self {
            Why::Newest => "newest",
            Why::Daily => "daily",
            Why::Weekly => "weekly",
            Why::Monthly => "monthly",
        }
    }
}

/// What the policy decides for a list of backups given as `(id, unix seconds)`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Decision {
    /// Kept, with every rule that keeps it, newest first.
    pub keep: Vec<(String, Vec<Why>)>,
    /// Not kept, oldest first.
    pub delete: Vec<String>,
}

impl Decision {
    pub fn kept_by(&self, why: Why) -> usize {
        self.keep.iter().filter(|(_, w)| w.contains(&why)).count()
    }
}

fn civil(days: i64) -> (i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m)
}

fn day_of(t: u64) -> i64 {
    (t / 86_400) as i64
}

/// ISO week number counted from the first Monday: the 1970-01-01 was a Thursday.
fn week_of(t: u64) -> i64 {
    (day_of(t) + 3).div_euclid(7)
}

fn month_of(t: u64) -> i64 {
    let (y, m) = civil(day_of(t));
    y * 12 + m
}

/// How old a scheduled backup may get before it goes, whatever the counts say: 400 days, the
/// twelve months of the monthly rule plus the month it is in and a month's grace. The counts keep
/// "the N most recent periods that have a backup", so after an outage they would stretch past
/// twelve months by the length of the gap; this is the bound that holds regardless.
pub const DEFAULT_MAX_AGE_DAYS: u64 = 400;

/// `OBOP_BACKUP_MAX_AGE_DAYS`: a number of days (1 to 3650); `0` or `off` removes the bound, which
/// leaves the promise to the counts alone.
pub fn max_age_from_var(v: Option<&str>) -> Result<Option<u64>, String> {
    match v.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(Some(DEFAULT_MAX_AGE_DAYS * 86_400)),
        Some("0" | "off") => Ok(None),
        Some(s) => s
            .parse::<u64>()
            .ok()
            .filter(|d| (1..=3650).contains(d))
            .map(|d| Some(d * 86_400))
            .ok_or_else(|| {
                format!(
                    "OBOP_BACKUP_MAX_AGE_DAYS must be a number of days (1 to 3650), 0 or off, not {s:?}"
                )
            }),
    }
}

/// Apply `policy` to `backups`, and the age bound `max_age = (now, seconds)` when there is one:
/// a backup older than that is not kept by any rule, except the newest of all.
pub fn decide_at(
    policy: Retention,
    backups: &[(String, u64)],
    max_age: Option<(u64, u64)>,
) -> Decision {
    let mut d = decide(policy, backups);
    let Some((now, limit)) = max_age else {
        return d;
    };
    let age_of = |id: &str| {
        backups
            .iter()
            .find(|(i, _)| i == id)
            .map_or(0, |(_, t)| now.saturating_sub(*t))
    };
    let (keep, too_old): (Vec<_>, Vec<_>) = std::mem::take(&mut d.keep)
        .into_iter()
        .partition(|(id, why)| why.contains(&Why::Newest) || age_of(id) <= limit);
    d.keep = keep;
    // Oldest first, as `delete` always is.
    let mut gone: Vec<String> = too_old.into_iter().map(|(id, _)| id).collect();
    gone.reverse();
    gone.extend(std::mem::take(&mut d.delete));
    gone.sort_by_key(|id| backups.iter().find(|(i, _)| i == id).map_or(0, |(_, t)| *t));
    d.delete = gone;
    d
}

/// Apply `policy` to `backups`.
pub fn decide(policy: Retention, backups: &[(String, u64)]) -> Decision {
    let mut sorted: Vec<&(String, u64)> = backups.iter().collect();
    // Newest first; the id breaks ties so the answer does not depend on input order.
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
    let mut why: Vec<(String, BTreeSet<Why>)> = sorted
        .iter()
        .map(|(id, _)| (id.clone(), BTreeSet::new()))
        .collect();
    if let Some(first) = why.first_mut() {
        first.1.insert(Why::Newest);
    }
    let mut mark = |period: fn(u64) -> i64, n: usize, w: Why| {
        let mut seen: Vec<i64> = Vec::new();
        for (i, (_, t)) in sorted.iter().enumerate() {
            let p = period(*t);
            // The first backup met in a period (walking from the newest) is its newest.
            if !seen.contains(&p) {
                if seen.len() >= n {
                    break;
                }
                seen.push(p);
                why[i].1.insert(w);
            }
        }
    };
    mark(day_of, policy.daily, Why::Daily);
    mark(week_of, policy.weekly, Why::Weekly);
    mark(month_of, policy.monthly, Why::Monthly);
    let mut d = Decision::default();
    for (id, w) in why {
        if w.is_empty() {
            d.delete.push(id);
        } else {
            d.keep.push((id, w.into_iter().collect()));
        }
    }
    d.delete.reverse();
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86_400;
    /// 2025-01-01T00:00:00Z.
    const START: u64 = 1_735_689_600;

    fn nightly(days: u64, hour: u64) -> Vec<(String, u64)> {
        (0..days)
            .map(|d| {
                let t = START + d * DAY + hour * 3600;
                (format!("b{d:04}"), t)
            })
            .collect()
    }

    #[test]
    fn the_default_is_thirty_five_daily_twelve_monthly_and_nothing_else() {
        let d = Retention::default();
        assert_eq!((d.daily, d.weekly, d.monthly), (35, 0, 12));
        assert!(d.describe().contains("35 daily") && d.describe().contains("12 monthly"));
    }

    #[test]
    fn after_fourteen_months_of_nightly_runs_it_keeps_35_dailies_and_12_monthlies() {
        // Simulate: a backup every night for 14 months (426 days), pruned after each run.
        let mut kept: Vec<(String, u64)> = Vec::new();
        let policy = Retention::default();
        let mut last = Decision::default();
        for (id, t) in nightly(426, 2) {
            kept.push((id, t));
            last = decide(policy, &kept);
            let gone: BTreeSet<&String> = last.delete.iter().collect();
            kept.retain(|(id, _)| !gone.contains(id));
        }
        assert_eq!(last.kept_by(Why::Daily), 35);
        assert_eq!(last.kept_by(Why::Monthly), 12);
        assert_eq!(last.kept_by(Why::Weekly), 0);
        // The dailies are the 35 most recent nights.
        let newest: Vec<String> = (426 - 35..426).map(|d| format!("b{d:04}")).collect();
        for id in &newest {
            assert!(kept.iter().any(|(k, _)| k == id), "{id} is a recent night");
        }
        // The monthlies are the last night of each of the 12 most recent months; the current
        // month's is the newest backup. Some coincide with the dailies.
        assert!(
            kept.len() >= 35 + 12 - 3 && kept.len() <= 35 + 12,
            "{}",
            kept.len()
        );
        // Nothing older than 13 months survives: the oldest kept is a month end 11 months back.
        let oldest = kept.iter().map(|(_, t)| *t).min().unwrap();
        assert!(START + 426 * DAY - oldest < 13 * 31 * DAY, "{oldest}");
        // Running the policy again over what is left deletes nothing.
        assert!(decide(policy, &kept).delete.is_empty());
    }

    #[test]
    fn weeks_and_months_use_calendar_periods_and_keep_the_newest_of_each() {
        // Two backups on Sunday 2025-01-05 (week 1) and Monday 2025-01-06 (week 2).
        let sun = START + 4 * DAY + 3600;
        let b = vec![
            ("sun".to_string(), sun),
            ("sun-late".to_string(), sun + 7200),
            ("mon".to_string(), sun + DAY),
        ];
        let d = decide(
            Retention {
                daily: 0,
                weekly: 2,
                monthly: 0,
            },
            &b,
        );
        let ids: Vec<&str> = d.keep.iter().map(|(i, _)| i.as_str()).collect();
        assert_eq!(
            ids,
            ["mon", "sun-late"],
            "the newest of each week; 'sun' is dropped"
        );
        assert_eq!(d.delete, ["sun"]);
        // The 1st of a month at 00:00 UTC belongs to the new month.
        let feb1 = START + 31 * DAY;
        let d = decide(
            Retention {
                daily: 0,
                weekly: 0,
                monthly: 5,
            },
            &[("jan".into(), feb1 - 1), ("feb".into(), feb1)],
        );
        assert_eq!(d.keep.len(), 2);
    }

    #[test]
    fn the_newest_backup_is_never_deleted_whatever_the_numbers() {
        let b = nightly(10, 3);
        let d = decide(
            Retention {
                daily: 0,
                weekly: 0,
                monthly: 0,
            },
            &b,
        );
        assert_eq!(d.keep.len(), 1);
        assert_eq!(d.keep[0], ("b0009".to_string(), vec![Why::Newest]));
        assert_eq!(d.delete.len(), 9);
        assert!(decide(Retention::default(), &[]).keep.is_empty());
    }

    #[test]
    fn the_order_of_the_input_does_not_change_the_answer_and_ties_are_stable() {
        let mut b = nightly(40, 1);
        b.push(("tie-a".into(), b[39].1));
        b.push(("tie-b".into(), b[39].1));
        let forward = decide(Retention::default(), &b);
        b.reverse();
        assert_eq!(forward, decide(Retention::default(), &b));
    }

    #[test]
    fn several_backups_a_day_keep_only_the_newest_of_that_day() {
        let day = START + 10 * DAY;
        let b: Vec<(String, u64)> = (0..4)
            .map(|h| (format!("h{h}"), day + h * 6 * 3600))
            .collect();
        let d = decide(
            Retention {
                daily: 3,
                weekly: 0,
                monthly: 0,
            },
            &b,
        );
        assert_eq!(d.keep.len(), 1);
        assert_eq!(d.keep[0].0, "h3");
    }

    /// The privacy promise: a scheduled backup is gone about 13 months after it was taken however
    /// the counts fall, even after an outage or on a sparse schedule, where "the N most recent
    /// periods that have a backup" alone reaches back for years.
    #[test]
    fn no_scheduled_backup_outlives_the_age_bound_after_an_outage_or_on_a_sparse_schedule() {
        let limit = DEFAULT_MAX_AGE_DAYS * DAY;
        // 12 months of nightly backups, a 15-month outage, then one new backup.
        let mut b = nightly(365, 2);
        let now = START + 365 * DAY + 450 * DAY;
        b.push(("resumed".into(), now));
        let counts_only = decide(Retention::default(), &b);
        let oldest = |d: &Decision| {
            d.keep
                .iter()
                .filter_map(|(id, _)| b.iter().find(|(i, _)| i == id).map(|(_, t)| now - t))
                .max()
                .unwrap()
        };
        assert!(
            oldest(&counts_only) > 700 * DAY,
            "counts alone keep two-year-old backups: {}",
            oldest(&counts_only) / DAY
        );
        let bounded = decide_at(Retention::default(), &b, Some((now, limit)));
        assert!(oldest(&bounded) <= limit, "{}", oldest(&bounded) / DAY);
        assert_eq!(bounded.keep.len(), 1, "only the resumed backup is left");
        assert_eq!(bounded.keep.len() + bounded.delete.len(), b.len());
        // A backup every 30 days for two years: nothing older than the bound stays.
        let sparse: Vec<(String, u64)> = (0..24)
            .map(|i| (format!("s{i:02}"), START + i * 30 * DAY))
            .collect();
        let now = START + 24 * 30 * DAY;
        let d = decide_at(Retention::default(), &sparse, Some((now, limit)));
        for (id, _) in &d.keep {
            let t = sparse.iter().find(|(i, _)| i == id).unwrap().1;
            assert!(now - t <= limit, "{id} is {} days old", (now - t) / DAY);
        }
        assert!(!d.delete.is_empty());
        // The newest backup is never deleted, however old it is: it may be all there is.
        let lone = vec![("only".to_string(), START)];
        let d = decide_at(
            Retention::default(),
            &lone,
            Some((START + 1000 * DAY, limit)),
        );
        assert_eq!(d.keep.len(), 1);
        assert!(d.delete.is_empty());
        // No bound given: the counts alone, as before.
        assert_eq!(
            decide_at(Retention::default(), &b, None),
            decide(Retention::default(), &b)
        );
        // A dense, gap-free schedule is untouched by the bound.
        let dense = nightly(426, 2);
        let now = START + 426 * DAY;
        assert_eq!(
            decide_at(Retention::default(), &dense, Some((now, limit))),
            decide(Retention::default(), &dense)
        );
    }

    #[test]
    fn the_age_bound_is_400_days_unless_told_otherwise_and_a_typo_is_an_error() {
        assert_eq!(max_age_from_var(None).unwrap(), Some(400 * DAY));
        assert_eq!(max_age_from_var(Some("")).unwrap(), Some(400 * DAY));
        assert_eq!(max_age_from_var(Some(" 90 ")).unwrap(), Some(90 * DAY));
        assert_eq!(max_age_from_var(Some("0")).unwrap(), None);
        assert_eq!(max_age_from_var(Some("off")).unwrap(), None);
        for bad in ["a year", "-5", "3651", "1.5"] {
            let e = max_age_from_var(Some(bad)).unwrap_err();
            assert!(e.contains("OBOP_BACKUP_MAX_AGE_DAYS"), "{e}");
        }
    }

    #[test]
    fn the_settings_are_numbers_and_a_typo_is_an_error() {
        assert_eq!(
            Retention::from_vars(Some("7"), Some(" 4 "), Some("")).unwrap(),
            Retention {
                daily: 7,
                weekly: 4,
                monthly: 12
            }
        );
        assert_eq!(
            Retention::from_vars(None, None, None).unwrap(),
            Retention::default()
        );
        for bad in ["seven", "-1", "1.5", "99999"] {
            let e = Retention::from_vars(Some(bad), None, None).unwrap_err();
            assert!(
                e.contains("OBOP_BACKUP_KEEP_DAILY") && e.contains("whole number"),
                "{e}"
            );
        }
    }
}
