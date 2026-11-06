//! The names the module reads and writes under, pinned by their text. Every other test imports the
//! constants from the code under test, so a constant that drifted would pass them all; these do
//! not. A deployed database holds rows under these names, a later step matches on the prefix of
//! `UNREADABLE`, and `LOCK_KEY` is what two processes meet on.

use super::super::{AUDIT_USER, LOCK_KEY, MODEL, ROW_ID, SETTING_ID, SETTING_MODEL, UNREADABLE};
use super::*;

#[test]
fn the_storage_names_are_the_ones_a_deployed_database_holds() {
    assert_eq!(MODEL, "instance_brand");
    assert_eq!(ROW_ID, "instance");
    assert_eq!(LOCK_KEY, "brand.write");
    assert_eq!(AUDIT_USER, "(instance)");
    assert_eq!(
        SETTING_MODEL, "obop_setting",
        "the settings of the instance"
    );
    assert_eq!(SETTING_ID, "instance");
    assert_eq!(UNREADABLE, "the instance branding could not be read");
}

#[test]
fn the_theme_is_read_from_the_row_the_settings_writer_stores() {
    // `Installer::set_settings` stores the text under the model `obop_setting`, the id `instance`,
    // the field `value`. A reader that looked anywhere else would never see the accent, and no test
    // that writes the setting through the same constant would notice. The writer itself is tested in
    // `obop-pkg` against a reader (`the_theme_accent_written_by_the_settings_is_read_by_the_brand`).
    let spy = Spy::memory();
    spy.apply_checked(&[Op::Put {
        model: "obop_setting".into(),
        id: "instance".into(),
        fields: fields(&[("value", s(r##"{"theme":{"accent":"#c2410c"}}"##))]),
    }])
    .unwrap();
    let (r, _) = reader(&spy, LONG);
    assert_eq!(r.current().theme_accent(), Some("#c2410c"));
}

#[test]
fn the_row_is_stored_under_the_model_and_id_of_the_names() {
    let spy = Spy::memory();
    crate::brand::apply(
        &*spy,
        None,
        "ada",
        &crate::brand::Action::Configure(vec![("name".into(), "Acme".into())]),
        false,
    )
    .unwrap();
    let row = spy.get("instance_brand", "instance").unwrap().unwrap();
    assert_eq!(row.get("name"), Some(&s("Acme")));
    let events = spy.list("security_event").unwrap();
    assert_eq!(events[0].1.get("user"), Some(&s("(instance)")));
}
