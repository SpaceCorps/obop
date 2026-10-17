//! Declarative module packages: the `obop.toml` format, runtime install and removal, the
//! marketplace, and the admin commands that drive them.

pub mod check;
pub mod files;
pub mod installer;
pub mod manifest;
pub mod marketplace;
pub mod resolve;
pub mod semver;
pub mod spdx;
mod sync;
pub mod views;

pub use check::{Report, check_manifest, check_package};
pub use installer::{Installer, PackageInfo, admin_module, register_admin};
pub use manifest::Manifest;
pub use marketplace::{Bundled, Chain, Http, Listing, Marketplace};
#[cfg(test)]
mod dependency_tests;
#[cfg(test)]
mod public_tests;
