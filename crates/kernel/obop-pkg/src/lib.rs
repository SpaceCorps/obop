//! Declarative module packages: the `obop.toml` format, runtime install and removal, the
//! marketplace, and the admin commands that drive them.

pub mod installer;
pub mod manifest;
pub mod marketplace;

pub use installer::{Installer, PackageInfo, admin_module, register_admin};
pub use manifest::Manifest;
pub use marketplace::{Bundled, Chain, Http, Listing, Marketplace};
