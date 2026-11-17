//! Company settings of the finance flows, kept in the settings table next to the instance settings
//! but in a row of their own (`obop_setting/company`), one key per section, such as
//! `{"invoicing": {"book_on_post": false}}` or `{"expenses": {"allow_self_approval": true}}`. An
//! instance is one company, so these are the company's settings.
//!
//! Not inside the instance settings object (`obop_setting/instance`): the back office saves that
//! object whole from the copy it read at sign-in (`setting.update`), which would put back an old
//! value of a setting changed since. Only the commands of each section write this row.
//!
//! A change reads the whole object and writes it back with only its section changed; a refused
//! read is an error and nothing is written (D74: never write what could not be read).

use obop_storage::{Backend, Fields};
use serde_json::{Map, Value as J};

const MODEL: &str = "obop_setting";
const ID: &str = "company";
const LOCK: &str = "settings.company";

fn read_all(b: &dyn Backend) -> Result<Map<String, J>, String> {
    let text = b
        .get(MODEL, ID)
        .map_err(|e| format!("cannot read the settings: {e}"))?
        .and_then(|f| f.get("value").and_then(J::as_str).map(String::from))
        .unwrap_or_else(|| "{}".into());
    match serde_json::from_str::<J>(&text) {
        Ok(J::Object(m)) => Ok(m),
        _ => Err("the stored settings are not a JSON object; nothing was changed".into()),
    }
}

/// The section `section` of the instance settings (an empty object when it is not set). A
/// refused read is an error.
pub fn section(b: &dyn Backend, section: &str) -> Result<Map<String, J>, String> {
    Ok(match read_all(b)?.remove(section) {
        Some(J::Object(m)) => m,
        _ => Map::new(),
    })
}

/// A boolean of a section, or `default` when it is not set. A refused read is an error.
pub fn flag(b: &dyn Backend, sec: &str, key: &str, default: bool) -> Result<bool, String> {
    Ok(section(b, sec)?
        .get(key)
        .and_then(J::as_bool)
        .unwrap_or(default))
}

/// Merge `patch` into the section `sec` (a JSON null removes a key) and store the settings.
/// Returns the section as stored.
pub fn update(
    b: &dyn Backend,
    sec: &str,
    patch: &Map<String, J>,
) -> Result<Map<String, J>, String> {
    // Two sections change the same row: without the lock, a change made between this read and
    // this write would be written over (the cross-process lock also holds for several servers on
    // one database).
    let _held = b
        .lock(LOCK)
        .map_err(|e| format!("cannot change the settings now: {e}"))?;
    let mut all = read_all(b)?;
    let mut cur = match all.remove(sec) {
        Some(J::Object(m)) => m,
        _ => Map::new(),
    };
    for (k, v) in patch {
        if v.is_null() {
            cur.remove(k);
        } else {
            cur.insert(k.clone(), v.clone());
        }
    }
    all.insert(sec.to_string(), J::Object(cur.clone()));
    let mut f = Fields::new();
    f.insert("value".into(), J::from(J::Object(all).to_string()));
    if !b.update(MODEL, ID, &f).map_err(|e| e.to_string())? {
        b.insert(MODEL, ID, &f).map_err(|e| e.to_string())?;
    }
    Ok(cur)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn changes_of_two_sections_at_once_are_both_kept() {
        let b: Arc<dyn Backend> = obop_storage::open("memory:").unwrap();
        let threads: Vec<_> = ["invoicing", "expenses", "pos", "sales"]
            .into_iter()
            .map(|sec| {
                let b = b.clone();
                std::thread::spawn(move || {
                    for i in 0..200 {
                        let mut p = Map::new();
                        p.insert(format!("k{i}"), J::from(i));
                        update(&*b, sec, &p).unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        for sec in ["invoicing", "expenses", "pos", "sales"] {
            assert_eq!(section(&*b, sec).unwrap().len(), 200, "{sec}");
        }
    }
}
