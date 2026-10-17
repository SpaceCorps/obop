//! Updating the instance itself (D63): the release feed, the compatibility of the installed
//! modules with a new core, and the admin commands that ask the control plane to move this
//! instance to a published release.

pub mod commands;
pub mod release;
pub mod updater;

pub use commands::register;
pub use release::{DEFAULT_FEED, Feed, Release, fetch_feed, parse_feed};
pub use updater::{BackupFn, Check, Config, Mode, ReleaseView, Status, Updater};
