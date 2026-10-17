//! Connectors: the compliant way data enters and leaves an instance (D54).
//!
//! This crate is the framework-neutral core. It knows nothing about HTTP servers or screens:
//!
//! - [`egress`]: the gate every outbound flow passes (legal basis, consent, minimisation,
//!   treatment by data class, caps, flow log).
//! - [`ingress`]: the matching gate for inbound records, written through the governed commands.
//! - [`policy`]: the per-field decision table.
//! - [`net`]: the one hardened outbound HTTP client (SSRF guard, pinned DNS, no redirects).
//! - [`webhook`]: signatures, replay protection.
//! - [`oauth`]: the authorization code flow with PKCE.
//! - [`secretbox`]: sealing secrets at rest.
//! - [`catalog`]: the provider catalogue, with honest maturity.
//! - [`runtime`]: the pieces wired together over the dispatcher and the store.
//!
//! It provides technical controls that support GDPR compliance. It is not legal advice and it is
//! not a certification: the controller still decides purposes and legal bases, concludes the
//! agreements with recipients and carries the accountability (see docs/CONNECTORS.md).

pub mod catalog;
pub mod checklist;
pub mod egress;
pub mod ingress;
pub mod model;
pub mod net;
pub mod oauth;
pub mod policy;
pub mod queue;
pub mod register;
pub mod runtime;
pub mod secretbox;
pub mod util;
pub mod webhook;

pub use catalog::{Engine, Maturity, Provider};
pub use egress::{Egress, Refusal};
pub use policy::{Policy, Treatment};
pub use runtime::{Config, Runtime};
pub use secretbox::SecretBox;
