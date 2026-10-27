//! The commands that read the instance settings, change one key and write the whole object back
//! must not write what they could not read (D74).

use crate::app::App;
use obop_commands::Store;
use obop_storage::testing::{Flaky, Mode};
use serde_json::{Value as J, json};

const STORED: &str = r##"{"theme":{"accent":"#6d3fd0"},"security":{"require_mfa_for_admins":true},"site":{"title":"Shop"}}"##;

fn stored(b: &std::sync::Arc<dyn obop_storage::Backend>) -> J {
    let f = b.get("obop_setting", "instance").unwrap().unwrap();
    serde_json::from_str(f["value"].as_str().unwrap()).unwrap()
}

#[test]
fn site_set_does_not_replace_the_settings_it_could_not_read() {
    let inner = obop_storage::open("memory:").unwrap();
    let flaky = Flaky::new(inner.clone());
    let app = App::with_store(Store::over(obop_storage::guarded(flaky.clone())));
    app.installer.set_settings(STORED).unwrap();
    // The store refuses the first read and answers the next: the old command read "{}", then wrote
    // the one key over the theme, the app order and the security policy, and said "set".
    flaky.refuse_call_after(0);
    let done = crate::site_set_in(&app, "title", json!("Other"));
    flaky.allow_models();
    assert!(done.is_err());
    assert_eq!(stored(&inner), serde_json::from_str::<J>(STORED).unwrap());
    // Read again, the one key changes and the rest stays.
    crate::site_set_in(&app, "title", json!("Other")).unwrap();
    let now = stored(&inner);
    assert_eq!(now["site"]["title"], "Other");
    assert_eq!(now["theme"]["accent"], "#6d3fd0");
    assert_eq!(now["security"]["require_mfa_for_admins"], true);
}

#[test]
fn site_set_does_not_replace_settings_that_are_not_an_object() {
    let inner = obop_storage::open("memory:").unwrap();
    let app = App::with_store(Store::over(inner.clone()));
    let mut f = obop_storage::Fields::new();
    f.insert("value".into(), J::from("not json at all"));
    inner.insert("obop_setting", "instance", &f).unwrap();
    assert!(crate::site_set_in(&app, "title", json!("Other")).is_err());
    assert_eq!(
        inner.get("obop_setting", "instance").unwrap().unwrap()["value"],
        "not json at all"
    );
}

#[test]
fn the_settings_read_answers_the_store_refusal_and_not_an_empty_object() {
    use obop_commands::{Principal, Surface, Value, Vault};
    let inner = obop_storage::open("memory:").unwrap();
    let flaky = Flaky::new(inner.clone());
    let app = App::with_store(Store::over(obop_storage::guarded(flaky.clone())));
    app.installer.set_settings(STORED).unwrap();
    let admin = Principal::user("root", &["platform.admin"]);
    let get = || {
        app.dispatcher.call(
            Surface::Rest,
            &admin,
            &mut Vault::new(),
            "setting.get",
            &Value::map([("id", Value::text("instance"))]),
        )
    };
    assert!(get().is_ok());
    // Shown as "{}", the settings page saved its change over everything else.
    flaky.refuse_model("obop_setting", Mode::Reads);
    let refused = get();
    flaky.allow_models();
    assert!(refused.is_err(), "{refused:?}");
}
