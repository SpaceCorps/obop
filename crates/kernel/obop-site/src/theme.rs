//! The accent colour maths lives in `obop_brand::theme` now: the brand's generated tile needs it,
//! and a leaf crate that `obop-commands` uses cannot depend on this crate (which depends on
//! `obop-commands`). Everything here is re-exported, so every caller in this crate is unchanged and
//! the shop's favicon, home page and stylesheet are byte for byte what they were.

pub use obop_brand::theme::*;
