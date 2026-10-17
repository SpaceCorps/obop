//! Translations of the module's own metadata (D52), compiled in from `packages/labels`.

pub fn docs() -> Vec<&'static str> {
    vec![
        include_str!("../../../../packages/labels/de/connectors.toml"),
        include_str!("../../../../packages/labels/fr/connectors.toml"),
        include_str!("../../../../packages/labels/pl/connectors.toml"),
        include_str!("../../../../packages/labels/fi/connectors.toml"),
        include_str!("../../../../packages/labels/sv/connectors.toml"),
        include_str!("../../../../packages/labels/es/connectors.toml"),
    ]
}
