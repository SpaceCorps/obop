//! `base` API crate: types, traits and events only; no logic.

/// Model names other modules may reference.
pub mod models {
    pub const PARTY: &str = "party";
}

/// Durable event types published by `base` (CONVENTIONS section 3.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseEvent {
    PartyErased { party_id: String },
}
