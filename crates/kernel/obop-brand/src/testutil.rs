//! Test support: a fixed-seed generator written here, because no property-test crate is in
//! `Cargo.lock`. The same seed gives the same texts on every run and every host.

/// xorshift64*: a small, well-known generator; the quality does not matter, the repeatability does.
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Rng {
        Rng(seed.max(1))
    }

    pub(crate) fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number in `0..n` (`n` must not be 0).
    pub(crate) fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    pub(crate) fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// Characters that exercise the rules: ASCII, letters of several scripts, marks, digits, the
/// invisible and format characters, spaces, symbols, the characters of the link forms, and a
/// few code points from the ends of the range.
pub(crate) const NASTY: &[char] = &[
    'a',
    'Z',
    'q',
    'é',
    'ß',
    'Ł',
    'ø',
    'Ж',
    'О',
    'א',
    'ع',
    'क',
    'ก',
    '株',
    'ム',
    'ー',
    'ヴ',
    '한',
    'ㄅ',
    '\u{0301}',
    '\u{0302}',
    '\u{0303}',
    '\u{0300}',
    '\u{094D}',
    '0',
    '7',
    '٣',
    '１',
    ' ',
    '.',
    '.',
    ',',
    '-',
    '_',
    '&',
    '\'',
    '"',
    '(',
    ')',
    '!',
    '+',
    '=',
    '<',
    '>',
    ':',
    '/',
    '\\',
    '@',
    '#',
    '$',
    '€',
    '%',
    '*',
    '~',
    '\t',
    '\n',
    '\u{0}',
    '\u{7F}',
    '\u{A0}',
    '\u{AD}',
    '\u{200B}',
    '\u{200C}',
    '\u{200D}',
    '\u{202E}',
    '\u{2060}',
    '\u{3000}',
    '\u{3164}',
    '\u{FE0F}',
    '\u{FEFF}',
    '\u{E0001}',
    '\u{E0041}',
    '\u{E000}',
    '\u{FFF0}',
    '\u{1D400}',
    '\u{02BB}',
    '\u{02BC}',
    '\u{10FFFF}',
    '\u{0378}',
    '\u{1F600}',
    'ｓ',
    '．',
];

/// A text of up to `max` characters from [`NASTY`].
pub(crate) fn text(rng: &mut Rng, max: usize) -> String {
    let n = rng.below(max + 1);
    (0..n).map(|_| *rng.pick(NASTY)).collect()
}
