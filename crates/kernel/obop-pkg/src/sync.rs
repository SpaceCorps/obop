//! Installed packages across server processes (B1, D59).
//!
//! The packages are records, but each process registers their commands in its own dispatcher.
//! So that a module installed, upgraded or removed through one process appears on the others
//! without a restart, every change bumps a shared version number, and each process compares it
//! with the one it last reconciled at (at most once a second, from the dispatcher's entry points)
//! and then registers what is new, replaces what changed and unregisters what is gone.

use crate::installer::Installer;
use obop_storage::Fields;
use serde_json::Value as J;
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;

/// The shared counter that moves whenever the set of installed packages changes.
const VERSION: &str = "modules.version";

/// A package as it is stored: changes with every install or upgrade.
fn fingerprint(f: &Fields) -> String {
    format!(
        "{}@{}",
        f.get("version").and_then(J::as_str).unwrap_or(""),
        f.get("installed_at").and_then(J::as_u64).unwrap_or(0)
    )
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

impl Installer {
    fn fingerprints(&self) -> BTreeMap<String, String> {
        self.stored()
            .into_iter()
            .map(|(id, f)| (id.clone(), fingerprint(&f)))
            .collect()
    }

    /// This process has just registered every stored package (start-up).
    pub(crate) fn note_loaded(&self) {
        *self.registered.lock().unwrap_or_else(|e| e.into_inner()) = self.fingerprints();
        self.seen_version.store(
            self.backend.counter_get(VERSION).unwrap_or(0),
            Ordering::Release,
        );
    }

    /// This process has just installed, upgraded or removed `ids` itself: tell the others, and
    /// remember that it is already up to date for them.
    pub(crate) fn note_local_change(&self, ids: &[String]) {
        let now = self.stored();
        {
            let mut reg = self.registered.lock().unwrap_or_else(|e| e.into_inner());
            for id in ids {
                match now.iter().find(|(i, _)| i == id) {
                    Some((_, f)) => reg.insert(id.clone(), fingerprint(f)),
                    None => reg.remove(id),
                };
            }
        }
        if let Ok(v) = self.backend.counter_add(VERSION, 0, false) {
            // Nobody else changed anything since this process last looked: it is current.
            // Otherwise the next `sync_modules` reconciles the other process's change too.
            if self
                .seen_version
                .compare_exchange(v - 1, v, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                self.last_check_ms.store(0, Ordering::Release);
            }
        }
    }

    /// Bring this process's registered modules in line with the stored packages, if another
    /// process changed them. Cheap when nothing changed: one counter read, at most once a second.
    pub fn sync_modules(&self) {
        let now = now_ms();
        let last = self.last_check_ms.load(Ordering::Acquire);
        if now.saturating_sub(last) < 1000 {
            return;
        }
        self.last_check_ms.store(now, Ordering::Release);
        let Ok(v) = self.backend.counter_get(VERSION) else {
            return;
        };
        if v == self.seen_version.load(Ordering::Acquire) {
            return;
        }
        let Ok(d) = self.d() else { return };
        let _one_at_a_time = self.apply.lock().unwrap_or_else(|e| e.into_inner());
        let stored = self.stored();
        let mut reg = self.registered.lock().unwrap_or_else(|e| e.into_inner());
        // Removed elsewhere.
        for id in reg.keys().cloned().collect::<Vec<_>>() {
            if !stored.iter().any(|(i, _)| *i == id) {
                d.unregister_module(&id);
                reg.remove(&id);
            }
        }
        // Installed or upgraded elsewhere.
        for (id, f) in &stored {
            let fp = fingerprint(f);
            if reg.get(id) == Some(&fp) {
                continue;
            }
            if reg.contains_key(id) {
                d.unregister_module(id);
            }
            self.reregister(id, f);
            reg.insert(id.clone(), fp);
        }
        self.seen_version.store(v, Ordering::Release);
    }
}
