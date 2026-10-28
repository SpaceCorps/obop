//! The track rules of docs/spec/mobile.md 5.4, as pure functions: which recorded fixes count and
//! how far the trip went. The device runs the same rules in `web/packages/trip-core`
//! (`src/track.ts`); both read the same file of test vectors, so they cannot drift apart.

/// Mean radius of the Earth in metres (IUGG mean radius R1 of the WGS84 ellipsoid).
pub const EARTH_MEAN_RADIUS_M: f64 = 6_371_008.8;

/// A step above this speed in metres per second (about 252 km/h) is not a real step.
pub const MAX_SPEED_MPS: f64 = 70.0;

/// A fix whose accuracy radius is above this many metres is dropped. The value is Proposed: the
/// spec leaves it open until the field test.
pub const MAX_ACCURACY_M: f64 = 100.0;

/// One recorded position. `t_ms` is milliseconds since the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fix {
    pub seq: i64,
    pub t_ms: f64,
    pub lat: f64,
    pub lon: f64,
    pub accuracy_m: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    pub max_speed_mps: f64,
    pub max_accuracy_m: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_speed_mps: MAX_SPEED_MPS,
            max_accuracy_m: MAX_ACCURACY_M,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    /// The summed distance of the accepted steps in metres, not rounded.
    pub distance_m: f64,
    pub accepted: usize,
    pub skipped_accuracy: usize,
    pub skipped_speed: usize,
    /// Sequence numbers missing between the first and the last stored fix.
    pub seq_gaps: i64,
    pub first: Option<Fix>,
    /// The last accepted fix: the end position of the trip.
    pub last: Option<Fix>,
}

fn radians(degrees: f64) -> f64 {
    (degrees * std::f64::consts::PI) / 180.0
}

/// Great-circle distance in metres (haversine on the WGS84 mean radius); the same operations in
/// the same order as `distanceMeters` of `trip-core`.
pub fn distance_m(a: &Fix, b: &Fix) -> f64 {
    let d_lat = radians(b.lat - a.lat);
    let d_lon = radians(b.lon - a.lon);
    let s_lat = (d_lat / 2.0).sin();
    let s_lon = (d_lon / 2.0).sin();
    let h = s_lat * s_lat + radians(a.lat).cos() * radians(b.lat).cos() * (s_lon * s_lon);
    2.0 * EARTH_MEAN_RADIUS_M * h.sqrt().min(1.0).asin()
}

/// The rules: fixes in order of `seq`; drop a fix whose accuracy radius is above the limit; compare
/// each further fix with the last accepted one and drop it if the step needs a speed above the cap
/// (or goes back in time by any distance); a step of zero metres is accepted at any speed.
pub fn analyse(fixes: &[Fix], options: &Options) -> Analysis {
    let mut sorted: Vec<Fix> = fixes.to_vec();
    sorted.sort_by_key(|f| f.seq);
    let mut seq_gaps = 0;
    for w in sorted.windows(2) {
        let gap = w[1].seq - w[0].seq - 1;
        if gap > 0 {
            seq_gaps += gap;
        }
    }
    let mut out = Analysis {
        distance_m: 0.0,
        accepted: 0,
        skipped_accuracy: 0,
        skipped_speed: 0,
        seq_gaps,
        first: None,
        last: None,
    };
    for p in sorted {
        if p.accuracy_m.is_some_and(|a| a > options.max_accuracy_m) {
            out.skipped_accuracy += 1;
            continue;
        }
        match out.last {
            Some(last) => {
                let d = distance_m(&last, &p);
                let dt = (p.t_ms - last.t_ms) / 1000.0;
                let plausible = if dt > 0.0 {
                    d / dt <= options.max_speed_mps
                } else {
                    d == 0.0
                };
                if !plausible {
                    out.skipped_speed += 1;
                    continue;
                }
                out.distance_m += d;
            }
            None => out.first = Some(p),
        }
        out.accepted += 1;
        out.last = Some(p);
    }
    out
}

impl Analysis {
    /// The distance as the server stores it: metres with two decimals, the canonical text of
    /// `mileage_trip.distance_m`.
    pub fn distance_text(&self) -> String {
        format!("{:.2}", self.distance_m)
    }

    /// The integrity flags: `seq_gap:N speed_jump:N low_accuracy:N`, in that order, a count of
    /// zero leaves its flag out, a clean track has the empty string.
    pub fn integrity(&self) -> String {
        let mut flags = Vec::new();
        if self.seq_gaps > 0 {
            flags.push(format!("seq_gap:{}", self.seq_gaps));
        }
        if self.skipped_speed > 0 {
            flags.push(format!("speed_jump:{}", self.skipped_speed));
        }
        if self.skipped_accuracy > 0 {
            flags.push(format!("low_accuracy:{}", self.skipped_accuracy));
        }
        flags.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value as J;

    /// The vectors the device's `trip-core` runs too (`web/packages/trip-core/src/track.test.ts`).
    const VECTORS: &str =
        include_str!("../../../../web/packages/trip-core/src/track.fixtures.json");

    fn num(v: &J, k: &str) -> f64 {
        v[k].as_f64().unwrap_or_else(|| panic!("{k} in {v}"))
    }

    fn fix(v: &J) -> Fix {
        Fix {
            seq: v["seq"].as_i64().unwrap(),
            t_ms: num(v, "t"),
            lat: num(v, "lat"),
            lon: num(v, "lon"),
            accuracy_m: v.get("accuracyM").and_then(J::as_f64),
        }
    }

    #[test]
    fn the_defaults_are_those_of_the_vector_file() {
        let doc: J = serde_json::from_str(VECTORS).unwrap();
        assert_eq!(num(&doc["defaults"], "maxSpeedMps"), MAX_SPEED_MPS);
        assert_eq!(num(&doc["defaults"], "maxAccuracyM"), MAX_ACCURACY_M);
    }

    #[test]
    fn every_shared_vector_gives_the_same_answer_as_the_device() {
        let doc: J = serde_json::from_str(VECTORS).unwrap();
        let cases = doc["cases"].as_array().unwrap();
        assert!(cases.len() >= 12, "the file lost cases");
        for c in cases {
            let mut options = Options::default();
            if let Some(o) = c.get("options") {
                if let Some(v) = o.get("maxSpeedMps").and_then(J::as_f64) {
                    options.max_speed_mps = v;
                }
                if let Some(v) = o.get("maxAccuracyM").and_then(J::as_f64) {
                    options.max_accuracy_m = v;
                }
            }
            let fixes: Vec<Fix> = c["points"].as_array().unwrap().iter().map(fix).collect();
            let a = analyse(&fixes, &options);
            let want = &c["expect"];
            let name = c["name"].as_str().unwrap();
            assert_eq!(a.distance_text(), want["distance_m"].as_str().unwrap(), "{name}");
            assert_eq!(a.accepted as i64, want["accepted"].as_i64().unwrap(), "{name}");
            assert_eq!(
                a.skipped_accuracy as i64,
                want["skipped_accuracy"].as_i64().unwrap(),
                "{name}"
            );
            assert_eq!(
                a.skipped_speed as i64,
                want["skipped_speed"].as_i64().unwrap(),
                "{name}"
            );
            assert_eq!(a.seq_gaps, want["seq_gaps"].as_i64().unwrap(), "{name}");
            assert_eq!(a.integrity(), want["integrity"].as_str().unwrap(), "{name}");
            assert_eq!(a.first.map(|f| f.seq), want["first_seq"].as_i64(), "{name}");
            assert_eq!(a.last.map(|f| f.seq), want["last_seq"].as_i64(), "{name}");
        }
    }

    #[test]
    fn a_speed_exactly_at_the_cap_counts_and_a_little_over_does_not() {
        let a = Fix {
            seq: 1,
            t_ms: 0.0,
            lat: 0.0,
            lon: 0.0,
            accuracy_m: None,
        };
        let b = Fix {
            seq: 2,
            t_ms: 1000.0,
            lat: 0.0,
            lon: 1.0,
            accuracy_m: None,
        };
        let metres = distance_m(&a, &b);
        let at = analyse(
            &[a, b],
            &Options {
                max_speed_mps: metres,
                ..Options::default()
            },
        );
        assert_eq!(at.skipped_speed, 0);
        let over = analyse(
            &[a, b],
            &Options {
                max_speed_mps: metres * 0.999,
                ..Options::default()
            },
        );
        assert_eq!(over.skipped_speed, 1);
    }

    #[test]
    fn the_answer_does_not_depend_on_the_order_the_fixes_arrive_in() {
        let doc: J = serde_json::from_str(VECTORS).unwrap();
        let c = &doc["cases"].as_array().unwrap()[0];
        let mut fixes: Vec<Fix> = c["points"].as_array().unwrap().iter().map(fix).collect();
        let straight = analyse(&fixes, &Options::default());
        fixes.reverse();
        let before = fixes.clone();
        assert_eq!(analyse(&fixes, &Options::default()), straight);
        assert_eq!(fixes, before, "the input is not reordered in place");
    }
}
