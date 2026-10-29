//! Declarative module packages: the `obop.toml` format, runtime install and removal, the
//! marketplace, and the admin commands that drive them.

pub mod accounts;
pub mod check;
pub mod devices;
pub mod files;
pub mod installer;
pub mod manifest;
pub mod marketplace;
pub mod migrate;
pub mod resolve;
pub mod semver;
pub mod spdx;
mod sync;
pub mod views;

pub use accounts::register_accounts;
pub use check::{Report, check_manifest, check_package};
pub use devices::register_devices;
pub use installer::{
    CoreCompat, GateReceipt, GateRequest, Installer, MigrationGate, PackageInfo, RollbackInfo,
    UpdateAll, UpdateInfo, admin_module, register_admin,
};
pub use manifest::Manifest;
pub use marketplace::{Bundled, Chain, Http, Listing, Marketplace};
#[cfg(test)]
mod dependency_tests;
#[cfg(test)]
mod ledger_tests;
#[cfg(test)]
mod limits_tests;
#[cfg(test)]
mod public_tests;
#[cfg(test)]
mod update_tests;
