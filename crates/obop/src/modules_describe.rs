//! `obop modules describe [FILE...] [--json]`: what a package declares, without a database.

use obop_pkg::describe::{Description, describe_texts, to_json, to_text};
use obop_pkg::marketplace::{Bundled, Marketplace};

/// The description of the files given, or of every bundled package when there are none.
pub fn collect(files: &[&str]) -> Result<Description, String> {
    let mut docs: Vec<(String, String)> = Vec::new();
    if files.is_empty() {
        let mut listing = Bundled.list()?;
        listing.sort_by(|a, b| a.id.cmp(&b.id));
        for l in listing {
            docs.push((l.id.clone(), Bundled.fetch(&l.id)?));
        }
    } else {
        for f in files {
            match obop_pkg::files::read_source(std::path::Path::new(f)) {
                Ok(obop_pkg::files::Source::Package(text)) => docs.push((f.to_string(), text)),
                Ok(obop_pkg::files::Source::Labels { .. }) => {
                    return Err(format!("{f}: a labels file has no models to describe"));
                }
                Err(e) => return Err(format!("{f}: cannot read: {e}")),
            }
        }
    }
    describe_texts(&docs)
}

/// The text to print.
pub fn render(files: &[&str], json: bool) -> Result<String, String> {
    let d = collect(files)?;
    Ok(if json { to_json(&d) } else { to_text(&d) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn model<'a>(v: &'a Value, name: &str) -> &'a Value {
        v["packages"][0]["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["name"] == name)
            .unwrap_or_else(|| panic!("model {name}"))
    }

    fn field<'a>(v: &'a Value, model_name: &str, name: &str) -> &'a Value {
        model(v, model_name)["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == name)
            .unwrap_or_else(|| panic!("{model_name}.{name}"))
    }

    #[test]
    fn accounting_shows_select_ref_sequence_computed_and_the_governed_state() {
        let p = root().join("packages/accounting.toml");
        let out = render(&[p.to_str().unwrap()], true).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["schema_version"], 1);
        assert_eq!(v["packages"][0]["id"], "accounting");
        assert_eq!(v["packages"][0]["requires_core"], "^0.2");
        let code = field(&v, "accounting_account", "code");
        assert_eq!(code["unique"], true);
        assert_eq!(code["required"], true);
        assert_eq!(code["kind"], "text");
        let ty = field(&v, "accounting_account", "account_type");
        assert_eq!(ty["kind"], "select");
        assert!(
            ty["options"]
                .as_array()
                .unwrap()
                .iter()
                .any(|o| o == "equity")
        );
        let mv = model(&v, "accounting_move");
        assert!(mv["state_field"].is_string());
        let post = mv["actions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == "post")
            .unwrap();
        assert_eq!(post["needs_approval"], true);
        let fields = mv["fields"].as_array().unwrap();
        let number = fields
            .iter()
            .find(|f| f["assigned"].is_object())
            .expect("a numbered field");
        assert_eq!(number["assigned"]["kind"], "sequence");
        assert_eq!(number["assigned"]["gapless"], true);
        assert!(
            fields
                .iter()
                .any(|f| f["computed"].as_str().is_some_and(|c| c.contains("sum(")))
        );
        let refs: Vec<_> = fields.iter().filter(|f| f["kind"] == "ref").collect();
        assert!(!refs.is_empty() && refs.iter().all(|f| f["to"].is_string()));
    }

    const SMALL: &str = r#"
[module]
id = "tiny"
name = "Tiny"
version = "1.0.0"
description = "A tiny package"
license = "Apache-2.0"
requires_core = "^0.2"

[module.dependencies]
base = "^0.2"

[[model]]
name = "tiny_item"
description = "An item"
read_group = "base.user"
write_group = "base.user"

  [[model.field]]
  name = "title"
  description = "Title"
  class = "none"
  required = true
  max_length = 80

  [[model.field]]
  name = "price"
  description = "Price"
  class = "none"
  kind = "decimal"
  precision = 12
  scale = 2
  round = "half_up"
  min = "0.00"

  [[model.field]]
  name = "owner"
  description = "Owner"
  class = "internal"
  personal_data = true
  kind = "ref"
  to = "user"
"#;

    fn tiny(tag: &str) -> (std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("obop-describe-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("tiny.toml");
        std::fs::write(&f, SMALL).unwrap();
        let s = f.to_string_lossy().to_string();
        (dir, s)
    }

    #[test]
    fn decimal_limits_ref_and_personal_data_are_reported() {
        let (dir, f) = tiny("a");
        let out = render(&[&f], true);
        let _ = std::fs::remove_dir_all(&dir);
        let v: Value = serde_json::from_str(&out.unwrap()).unwrap();
        let price = field(&v, "tiny_item", "price");
        assert_eq!(price["precision"], 12);
        assert_eq!(price["scale"], 2);
        assert_eq!(price["round"], "half_up");
        assert_eq!(price["limits"]["min"], "0.00");
        let title = field(&v, "tiny_item", "title");
        assert_eq!(title["limits"]["max_length"], 80);
        assert!(title["limits"]["min_length"].is_null());
        let owner = field(&v, "tiny_item", "owner");
        assert_eq!(owner["to"], "user");
        assert_eq!(owner["personal_data"], true);
        assert_eq!(owner["data_class"], "internal");
    }

    #[test]
    fn human_output_is_indented_text_and_default_covers_every_bundled_package() {
        let text = render(&[], false).unwrap();
        assert!(text.contains("package accounting "), "{text}");
        assert!(text.contains("  model accounting_account"));
        assert!(text.contains("unique"));
        let all: Value = serde_json::from_str(&render(&[], true).unwrap()).unwrap();
        assert!(all["packages"].as_array().unwrap().len() >= 23);
    }

    #[test]
    fn json_matches_the_golden_snapshot() {
        let (dir, f) = tiny("g");
        let out = render(&[&f], true).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let golden = root().join("crates/obop/tests/golden/describe_tiny.json");
        if std::env::var("OBOP_UPDATE_GOLDEN").is_ok() {
            std::fs::create_dir_all(golden.parent().unwrap()).unwrap();
            std::fs::write(&golden, &out).unwrap();
        }
        let want = std::fs::read_to_string(&golden).expect("golden file");
        assert_eq!(out, want.replace("\r\n", "\n"));
    }

    #[test]
    fn an_invalid_or_missing_file_is_an_error() {
        let dir = std::env::temp_dir().join(format!("obop-describe-b-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("bad.toml");
        std::fs::write(&bad, format!("{SMALL}\nbogus = 1")).unwrap();
        let e = render(&[bad.to_str().unwrap()], true).unwrap_err();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(e.contains("bad.toml"), "{e}");
        assert!(render(&["/no/such/file.toml"], false).is_err());
    }
}
