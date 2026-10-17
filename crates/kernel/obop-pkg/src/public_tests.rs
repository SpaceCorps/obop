//! The `[model.public]` declaration (D50): shape, gate and the specs it produces.

use crate::manifest::Manifest;

const SHOP: &str = r#"
[module]
id = "shop"
version = "0.1.0"
name = "Shop"
description = "d"
requires_core = "^0.2"

[[model]]
name = "shop_item"
description = "An item"
read_group = "shop.user"
write_group = "shop.user"

  [model.public]
  read = true
  published_field = "published"
  fields = ["title"]

  [[model.field]]
  name = "title"
  description = "Title"
  class = "none"

  [[model.field]]
  name = "owner"
  description = "Owner"
  class = "internal"
  personal_data = true

  [[model.field]]
  name = "published"
  description = "Published"
  class = "none"
  kind = "bool"

[[model]]
name = "shop_note"
description = "A note"
read_group = "shop.user"
write_group = "shop.user"

  [model.public]
  submit = true
  submit_fields = ["owner"]
  rate_limit_per_hour = 5

  [[model.field]]
  name = "owner"
  description = "Owner"
  class = "internal"
  personal_data = true
"#;

fn err(text: &str) -> String {
    Manifest::parse(text)
        .and_then(|m| m.to_module().map(|_| ()))
        .unwrap_err()
}

#[test]
fn a_valid_public_declaration_becomes_specs() {
    let m = Manifest::parse(SHOP).unwrap();
    m.to_module().unwrap();
    let specs = m.public_specs().unwrap();
    assert_eq!(specs.len(), 2);
    assert_eq!(specs[0].read.as_ref().unwrap().fields, ["title"]);
    assert_eq!(specs[1].submit.as_ref().unwrap().rate_limit_per_hour, 5);
}

#[test]
fn public_read_needs_a_published_field() {
    let bad = SHOP.replace("  published_field = \"published\"\n", "");
    assert!(err(&bad).contains("needs published_field"));
    let bad = SHOP.replace(
        "published_field = \"published\"",
        "published_field = \"title\"",
    );
    assert!(err(&bad).contains("bool or a select"));
    let bad = SHOP.replace(
        "published_field = \"published\"",
        "published_field = \"ghost\"",
    );
    assert!(err(&bad).contains("no field"));
}

#[test]
fn the_gate_refuses_personal_data_in_public_fields() {
    let bad = SHOP.replace("fields = [\"title\"]", "fields = [\"title\", \"owner\"]");
    assert!(err(&bad).contains("class none or public_directory"));
    // Marking it public_directory does not help while it is personal data.
    let bad = bad.replacen("class = \"internal\"", "class = \"public_directory\"", 1);
    assert!(err(&bad).contains("personal data can never be public"));
}

#[test]
fn submit_needs_fields_and_a_rate_limit_and_refuses_sensitive_classes() {
    assert!(err(&SHOP.replace("  rate_limit_per_hour = 5\n", "")).contains("rate_limit_per_hour"));
    assert!(
        err(&SHOP.replace("submit_fields = [\"owner\"]", "submit_fields = []"))
            .contains("submit_fields")
    );
    let bad = SHOP.replace(
        "name = \"owner\"\n  description = \"Owner\"\n  class = \"internal\"",
        "name = \"owner\"\n  description = \"Owner\"\n  class = \"financial\"",
    );
    assert!(err(&bad).contains("cannot collect class financial"));
    assert!(err(&SHOP.replace("submit = true", "submit = false")).contains("need submit = true"));
}

#[test]
fn a_public_table_must_declare_something_and_know_its_keys() {
    let bad = SHOP.replace("read = true", "read = false").replace(
        "  published_field = \"published\"\n  fields = [\"title\"]\n",
        "",
    );
    assert!(err(&bad).contains("declares nothing public"));
    assert!(
        err(&SHOP.replace("read = true", "read = true\n  everyone = true"))
            .contains("invalid obop.toml")
    );
    assert!(
        err(&SHOP.replace("read = true", "read = true\n  order_dir = \"up\""))
            .contains("order_dir")
    );
}

#[test]
fn the_bundled_packages_declare_public_models_that_pass_the_gate() {
    use crate::marketplace::{Bundled, Marketplace};
    for id in ["website", "ecommerce"] {
        let m = Manifest::parse(&Bundled.fetch(id).unwrap()).unwrap();
        m.to_module().unwrap();
        assert!(!m.public_specs().unwrap().is_empty(), "{id}");
    }
}
