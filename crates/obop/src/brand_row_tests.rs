//! With a brand row in the store, an instance sends what it sent without one (docs/spec/branding.md,
//! slice S1a): the record exists, the reader of the dispatcher answers it, and no surface but the
//! logo route (`brand_http_tests`), from slice S1c the pages of the OAuth authorization flow
//! (`oauth_tests/brand.rs`) and, from slice S3, `initialize`, the discovery documents, the title
//! of the OpenAPI document and the Slack message (`brand_docs_tests`) asks the reader; the
//! tests here hold what none of them reads. The golden pins of the first commit of the
//! slice (`pins_*`) hold what an instance with **no** row sends; the tests named
//! `with_a_brand_row_*`, next to each pin, build the same outputs after [`set_brand`] has written a
//! row through `brand::apply` and hold them to the same digests (or, where the output carries a
//! token that changes from run to run, to the same output without the row). A page, a document or
//! a route that starts to read the row without its slice moves one of them. The slice that makes a
//! surface show the name moves the pin of that surface in its own commit, with the reason, and the
//! test beside it then asserts the name.
//!
//! The filter for the family is `cargo test -p obop -- with_a_brand_row` (and `-p obop-auth`,
//! `-p obop-site` for their members). They are not `pins_*`, so the print mode of the pins
//! (`OBOP_PRINT_PINS=1`) does not run them.

use crate::app::App;
use obop_commands::brand::{self, Action};

/// The artwork of the placeholder logo (a plain "M"), in the canonical form the writer prints.
pub(crate) const ARTWORK: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><rect width=\"64\" height=\"64\" rx=\"14\" fill=\"#495670\"/><path d=\"M 16 48 L 16 16 L 24 16 L 32 30 L 40 16 L 48 16 L 48 48 L 41 48 L 41 29 L 32 44 L 23 29 L 23 48 Z\" fill=\"#ffffff\"/></svg>";

/// The values of a customer who has set a brand: a name, a slug, a title in three languages, a
/// message in two and the artwork. Every field a page could show.
pub(crate) const FIELDS: [(&str, &str); 9] = [
    ("name", "MOHV"),
    ("slug", "mohv"),
    ("title_en", "MOHV Mileage"),
    ("title_de", "MOHV Fahrtenbuch"),
    ("title_sv", "MOHV Körjournal"),
    ("message_en", "Welcome to the team"),
    ("message_fi", "Tervetuloa joukkoon"),
    ("message_sv", "Välkommen till laget"),
    ("logo_svg", ARTWORK),
];

/// Write the brand row into the store of `app` through `brand::apply`, as the dispatcher would for
/// `instance_brand.configure`, and check that the reader of its dispatcher now answers it: what a
/// surface would be told if it asked.
pub(crate) fn set_brand(app: &App) {
    let backend = app.store.backend();
    let reader = app.dispatcher.brand();
    assert!(!reader.is_inert(), "the app's dispatcher has a real reader");
    let action = Action::Configure(
        FIELDS
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    );
    brand::apply(&*backend, Some(&reader), "operator", &action, false)
        .unwrap_or_else(|e| panic!("the brand row was refused: {e}"));
    let shown = reader.current();
    assert_eq!(shown.brand().name(), "MOHV", "the reader answers the row");
    assert!(shown.fallbacks().is_empty(), "{:?}", shown.fallbacks());
    assert_eq!(shown.brand().title("sv"), "MOHV Körjournal");
}
