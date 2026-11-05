//! The generated tile: the logo of an instance that has a name and no artwork (docs/spec/branding.md,
//! section 2, "The generated tile"). One glyph on a rounded square, with the contrast rule the
//! public site already applies (`obop_brand::theme`). It is a template of the server with one
//! escaped glyph, written here and not read by the SVG reader (which refuses `<text>`).

use crate::theme;
use crate::unicode::{category, is_letter};
use icu_properties::props::GeneralCategory as Gc;

/// The default accent of the back office, `#6d3fd0` (`UI/src/theme.ts`, `UI/src/tokens.css`). A
/// stored accent that equals it counts as no accent for the tile.
pub const DEFAULT_VIOLET: &str = "#6d3fd0";

/// The tile's accent when the theme stores none, or only the default violet: the neutral slate
/// of the Slate preset of the theme (`UI/src/theme.ts`).
pub const SLATE: &str = "#495670";

/// The glyph of a name that holds no letter or digit (a row written around the command): the first
/// letter of the default name.
pub const FALLBACK_GLYPH: char = 'O';

/// A stored accent as the screens may use it: exactly `#` and six hex digits, in lower case.
/// Anything else (a name, `#rgb`, a longer string) is no accent.
pub fn valid_accent(stored: &str) -> Option<String> {
    let hex = stored.strip_prefix('#')?;
    (hex.len() == 6 && hex.bytes().all(|c| c.is_ascii_hexdigit()))
        .then(|| format!("#{}", hex.to_ascii_lowercase()))
}

/// The tile's accent and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileAccent {
    /// `#rrggbb`, lower case.
    pub hex: String,
    /// The theme's stored accent, rather than the slate.
    pub from_theme: bool,
}

/// The theme's stored accent when it is a valid `#rrggbb` other than the default violet (compared
/// without regard to case), else the neutral slate. A theme that stores exactly `#6d3fd0` is
/// treated like one that stores nothing: "Set as the default for everyone" saves the whole
/// resolved theme, the default accent included.
pub fn tile_accent(stored: Option<&str>) -> TileAccent {
    match stored.and_then(valid_accent) {
        Some(hex) if hex != DEFAULT_VIOLET => TileAccent {
            hex,
            from_theme: true,
        },
        _ => TileAccent {
            hex: SLATE.to_string(),
            from_theme: false,
        },
    }
}

/// The glyph of a name: its first letter or digit, as one scalar value, in upper case only when
/// the uppercase mapping gives a single scalar (`ß` stays `ß`, it does not become `SS`); `O` when
/// the name holds none.
pub fn glyph(name: &str) -> char {
    let Some(c) = name
        .chars()
        .find(|c| is_letter(category(*c)) || category(*c) == Gc::DecimalNumber)
    else {
        return FALLBACK_GLYPH;
    };
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// The tile for a name. The fill is `theme::resolve(tile accent).light.fill` and the glyph
/// colour its `.light.on_fill`: the search moves a light accent until the contrast rules hold,
/// so the fill can differ from the stored hex. With `size`, the root carries `width` and `height`
/// (the inline logo of a page); the served bytes carry neither.
pub fn svg(name: &str, theme_accent: Option<&str>, size: Option<u32>) -> String {
    let resolved = theme::resolve(&tile_accent(theme_accent).hex);
    let sized = size.map_or(String::new(), |n| format!(" width=\"{n}\" height=\"{n}\""));
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\"{sized} viewBox=\"0 0 64 64\">\
         <rect width=\"64\" height=\"64\" rx=\"14\" fill=\"{}\"/>\
         <text x=\"32\" y=\"44\" font-family=\"Georgia,serif\" font-size=\"36\" \
         font-weight=\"700\" text-anchor=\"middle\" fill=\"{}\">{}</text></svg>",
        resolved.light.fill,
        resolved.light.on_fill,
        escape(&glyph(name).to_string())
    )
}
