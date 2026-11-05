//! Instance branding rules (docs/spec/branding.md, PROPOSED; decision D94 is reserved): the character
//! policy of a name, a title and a message, the slug, the brand as a page reads it, the fallback
//! chain, the neutral-wording switch and the comparison with a client's name.
//!
//! A leaf crate: no I/O, and it depends on neither `obop-commands` nor `obop-site`. The reader that
//! keeps the stored row in memory, and the commands that write it, live in `obop-commands`. Every
//! function here is total: whatever text it is given, it returns a value or a refusal and never
//! panics, so a row written around the commands cannot take a page down.

pub mod brand;
pub mod logo;
pub mod policy;
pub mod refusal;
pub mod slug;
pub mod svg;
pub mod theme;
pub mod tile;
mod unicode;

pub use brand::{Brand, DEFAULT_NAME, LANGS, fold, lang_index};
pub use logo::Logo;
pub use policy::{Kind, clean, safe};
pub use refusal::Refusal;

#[cfg(test)]
mod brand_tests;
#[cfg(test)]
mod golden_tests;
#[cfg(test)]
mod policy_tests;
#[cfg(test)]
mod slug_tests;
#[cfg(test)]
mod spike_tests;
#[cfg(test)]
mod svg_tests;
#[cfg(test)]
mod testutil;
#[cfg(test)]
mod tile_tests;
