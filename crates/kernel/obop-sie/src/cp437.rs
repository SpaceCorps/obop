//! Code page 437 ("PC8" in SIE), the encoding the SIE 4 files use.

const HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', //
    'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', //
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', //
    '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐', //
    '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧', //
    '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀', //
    'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', //
    '≡', '±', '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

/// Encode text as CP437. Characters with no CP437 form become `?` (the euro sign becomes `E`).
pub fn encode(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| {
            if (c as u32) < 0x80 {
                c as u8
            } else if c == '€' {
                b'E'
            } else {
                HIGH.iter()
                    .position(|&h| h == c)
                    .map_or(b'?', |i| 0x80 + i as u8)
            }
        })
        .collect()
}

/// Decode CP437 bytes to text. Every byte has a character, so this cannot fail.
pub fn decode(b: &[u8]) -> String {
    b.iter()
        .map(|&x| {
            if x < 0x80 {
                x as char
            } else {
                HIGH[(x - 0x80) as usize]
            }
        })
        .collect()
}
