//! `base` implementation: the Contacts app foundation. Declares its registry metadata.

use obop_registry::{CommandDef, FieldDef, ModelDef, Module, Surfaces};
use obop_types::DataClass;

fn field(
    name: &'static str,
    description: &'static str,
    class: DataClass,
    personal: bool,
) -> FieldDef {
    FieldDef {
        name,
        description,
        class: Some(class),
        permission_group: "base.user",
        personal_data: personal,
    }
}

pub fn module() -> Module {
    Module {
        id: "base".into(),
        models: vec![ModelDef {
            name: base_api::models::PARTY,
            description: "A person or organisation the business deals with",
            fields: vec![
                field("name", "Display name", DataClass::PublicDirectory, true),
                field(
                    "email",
                    "Work email address",
                    DataClass::PublicDirectory,
                    true,
                ),
                field(
                    "tax_id",
                    "Tax identifier of a natural person",
                    DataClass::Financial,
                    true,
                ),
                field(
                    "notes",
                    "Free-text internal notes",
                    DataClass::Internal,
                    true,
                ),
            ],
            commands: vec![CommandDef {
                name: "erase",
                description: "Anonymise the party in place and record the erasure",
                permission_group: "privacy.officer",
                surfaces: Surfaces::ALL,
            }],
        }],
        network_destinations: vec![],
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn base_passes_install_gate() {
        assert!(obop_registry::check_module(&super::module()).is_empty());
    }
}
