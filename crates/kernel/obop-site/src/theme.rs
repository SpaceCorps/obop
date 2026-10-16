//! The accent colour of the shop: whatever the owner picks is nudged until text on it and text
//! in it meets WCAG 2.2 AA in light and in dark mode. The same maths as the backoffice colour
//! picker (`web/packages/ui/src/colors.ts`): OKLCH lightness is moved in small steps until every
//! check passes, so the hue the owner chose is kept.

type Rgb = [f64; 3];

fn to_linear(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(v: f64) -> f64 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// `#rgb` or `#rrggbb` to 0..1 channels; anything else is `None`.
pub fn parse_hex(hex: &str) -> Option<Rgb> {
    let h = hex.trim().trim_start_matches('#');
    let h: String = match h.len() {
        3 => h.chars().flat_map(|c| [c, c]).collect(),
        6 => h.to_string(),
        _ => return None,
    };
    if !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(&h, 16).ok()?;
    Some([
        f64::from(n >> 16) / 255.0,
        f64::from((n >> 8) & 255) / 255.0,
        f64::from(n & 255) / 255.0,
    ])
}

pub fn to_hex(rgb: Rgb) -> String {
    let x = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", x(rgb[0]), x(rgb[1]), x(rgb[2]))
}

fn rgb_to_oklab([r, g, b]: Rgb) -> [f64; 3] {
    let (lr, lg, lb) = (to_linear(r), to_linear(g), to_linear(b));
    let l = (0.412_221_470_8 * lr + 0.536_332_536_3 * lg + 0.051_445_992_9 * lb).cbrt();
    let m = (0.211_903_498_2 * lr + 0.680_699_545_1 * lg + 0.107_396_956_6 * lb).cbrt();
    let s = (0.088_302_461_9 * lr + 0.281_718_837_6 * lg + 0.629_978_700_5 * lb).cbrt();
    [
        0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s,
    ]
}

fn oklab_to_linear([l_, a, b]: [f64; 3]) -> Rgb {
    let l = (l_ + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m = (l_ - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s = (l_ - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);
    [
        4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
        -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
        -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701 * s,
    ]
}

#[derive(Debug, Clone, Copy)]
struct Oklch {
    l: f64,
    c: f64,
    h: f64,
}

fn hex_to_oklch(hex: &str) -> Oklch {
    let [l, a, b] = rgb_to_oklab(parse_hex(hex).unwrap_or([0.0; 3]));
    let c = a.hypot(b);
    Oklch {
        l,
        c,
        h: if c < 1e-4 {
            0.0
        } else {
            (b.atan2(a).to_degrees() + 360.0) % 360.0
        },
    }
}

fn in_gamut(rgb: Rgb) -> bool {
    rgb.iter().all(|v| (-0.0005..=1.0005).contains(v))
}

/// OKLCH to hex. An out-of-gamut colour loses chroma, not lightness, until it fits.
fn oklch_to_hex(Oklch { l, c, h }: Oklch) -> String {
    let rad = h.to_radians();
    let mut chroma = c;
    for _ in 0..40 {
        let lin = oklab_to_linear([l, chroma * rad.cos(), chroma * rad.sin()]);
        if in_gamut(lin) || chroma < 1e-4 {
            return to_hex(lin.map(from_linear));
        }
        chroma *= 0.94;
    }
    to_hex([l; 3])
}

/// WCAG 2 relative luminance.
fn luminance(hex: &str) -> f64 {
    let [r, g, b] = parse_hex(hex).unwrap_or([0.0; 3]).map(to_linear);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// WCAG 2 contrast ratio, 1 to 21.
pub fn contrast(a: &str, b: &str) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// `color-mix(in oklab, a pct%, b)` as hex.
fn mix(a: &str, b: &str, pct: f64) -> String {
    let (x, y) = (
        rgb_to_oklab(parse_hex(a).unwrap_or([0.0; 3])),
        rgb_to_oklab(parse_hex(b).unwrap_or([0.0; 3])),
    );
    let t = pct / 100.0;
    let lab = [0, 1, 2].map(|i| x[i] * t + y[i] * (1.0 - t));
    to_hex(oklab_to_linear(lab).map(from_linear))
}

pub const WHITE: &str = "#ffffff";
pub const NEAR_BLACK: &str = "#0c0d12";
pub const AA_TEXT: f64 = 4.5;
pub const AA_UI: f64 = 3.0;

/// Page surfaces of the shop in light and dark mode (the CSS uses the same two pairs).
pub const LIGHT_CANVAS: &str = "#f7f6f2";
pub const LIGHT_PANEL: &str = "#ffffff";
pub const DARK_CANVAS: &str = "#121317";
pub const DARK_PANEL: &str = "#1b1d23";

fn best_on(bg: &str) -> &'static str {
    if contrast(WHITE, bg) >= contrast(NEAR_BLACK, bg) {
        WHITE
    } else {
        NEAR_BLACK
    }
}

/// Move lightness in `dir` until `ok` passes (or the range ends).
fn search(from: Oklch, dir: f64, ok: impl Fn(&str) -> bool) -> String {
    let mut l = from.l;
    for _ in 0..200 {
        let hex = oklch_to_hex(Oklch { l, ..from });
        if ok(&hex) {
            return hex;
        }
        l = (l + dir * 0.005).clamp(0.0, 1.0);
    }
    oklch_to_hex(Oklch { l, ..from })
}

/// One mode of the accent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeAccent {
    /// Fill of buttons.
    pub fill: String,
    /// Text on the fill.
    pub on_fill: String,
    /// Links and accent text on the page surfaces.
    pub ink: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accent {
    pub chosen: String,
    pub light: ModeAccent,
    pub dark: ModeAccent,
}

fn resolve_mode(chosen: &str, dark: bool) -> ModeAccent {
    let (canvas, panel) = if dark {
        (DARK_CANVAS, DARK_PANEL)
    } else {
        (LIGHT_CANVAS, LIGHT_PANEL)
    };
    let lch = hex_to_oklch(chosen);
    let dir = if dark { 1.0 } else { -1.0 };
    let start = if dark {
        Oklch {
            l: lch.l.max(0.62),
            ..lch
        }
    } else {
        lch
    };
    let fill = search(start, dir, |c| {
        contrast(best_on(c), c) >= AA_TEXT
            && contrast(c, panel) >= AA_UI
            && contrast(c, canvas) >= AA_UI
    });
    let ink = search(hex_to_oklch(&fill), dir, |c| {
        contrast(c, panel) >= AA_TEXT
            && contrast(c, canvas) >= AA_TEXT
            && contrast(c, &mix(&fill, panel, 12.0)) >= AA_TEXT
    });
    ModeAccent {
        on_fill: best_on(&fill).to_string(),
        fill,
        ink,
    }
}

/// The accent for a chosen colour; an invalid colour gives the default teal.
pub fn resolve(chosen: &str) -> Accent {
    let hex = parse_hex(chosen).map_or_else(|| DEFAULT_ACCENT.to_string(), to_hex);
    Accent {
        light: resolve_mode(&hex, false),
        dark: resolve_mode(&hex, true),
        chosen: hex,
    }
}

pub const DEFAULT_ACCENT: &str = "#0f766e";

/// A soft background tint for a product card, from the accent hue and a stable number (the hash
/// of the title), so cards differ but stay inside the palette. Returns (light, dark) hex.
pub fn card_tints(chosen: &str, seed: u64) -> (String, String) {
    let base = hex_to_oklch(chosen);
    let shift = (seed % 7) as f64 * 9.0 - 27.0;
    let h = (base.h + shift + 360.0) % 360.0;
    (
        oklch_to_hex(Oklch {
            l: 0.94,
            c: 0.045,
            h,
        }),
        oklch_to_hex(Oklch { l: 0.3, c: 0.05, h }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_matches_the_wcag_reference() {
        assert!((contrast("#000000", "#ffffff") - 21.0).abs() < 1e-6);
        assert!((contrast("#777777", "#ffffff") - 4.48).abs() < 0.01);
    }

    #[test]
    fn hex_parsing() {
        assert_eq!(to_hex(parse_hex("#f80").unwrap()), "#ff8800");
        assert!(parse_hex("red").is_none());
        assert!(parse_hex("#12345").is_none());
        assert!(parse_hex("#gggggg").is_none());
    }

    #[test]
    fn every_accent_passes_aa_in_both_modes() {
        for pick in [
            "#ffff00", "#ffffff", "#000000", "#ff0000", "#00ff00", "#0000ff", "#0f766e", "#e11d48",
            "#f59e0b", "#7c3aed", "#94a3b8", "#fde68a", "#111827", "#6d3fd0",
        ] {
            let a = resolve(pick);
            for (dark, m) in [(false, &a.light), (true, &a.dark)] {
                let (canvas, panel) = if dark {
                    (DARK_CANVAS, DARK_PANEL)
                } else {
                    (LIGHT_CANVAS, LIGHT_PANEL)
                };
                assert!(
                    contrast(&m.on_fill, &m.fill) >= AA_TEXT,
                    "{pick} on-fill dark={dark}"
                );
                assert!(
                    contrast(&m.fill, panel) >= AA_UI,
                    "{pick} fill/panel dark={dark}"
                );
                assert!(
                    contrast(&m.fill, canvas) >= AA_UI,
                    "{pick} fill/canvas dark={dark}"
                );
                assert!(
                    contrast(&m.ink, panel) >= AA_TEXT,
                    "{pick} ink/panel dark={dark}"
                );
                assert!(
                    contrast(&m.ink, canvas) >= AA_TEXT,
                    "{pick} ink/canvas dark={dark}"
                );
            }
        }
    }

    #[test]
    fn an_invalid_accent_falls_back_to_the_default() {
        assert_eq!(resolve("not a colour").chosen, DEFAULT_ACCENT);
        assert_eq!(resolve("javascript:alert(1)").chosen, DEFAULT_ACCENT);
        assert_eq!(resolve("#0F766E").chosen, DEFAULT_ACCENT);
    }

    #[test]
    fn card_tints_are_valid_hex_and_stay_light_or_dark() {
        let (l, d) = card_tints("#0f766e", 3);
        assert!(parse_hex(&l).is_some() && parse_hex(&d).is_some());
        assert!(luminance(&l) > luminance(&d));
    }
}
