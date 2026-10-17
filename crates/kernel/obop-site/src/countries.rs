//! Names of the countries a shop is likely to name in a tax rate, in the seven languages of the
//! site. A code that is not here is shown as the bare code.

use crate::i18n::Lang;

/// (code, [en, de, fr, pl, fi, sv, es])
const COUNTRIES: &[(&str, [&str; 7])] = &[
    (
        "AT",
        [
            "Austria",
            "Österreich",
            "Autriche",
            "Austria",
            "Itävalta",
            "Österrike",
            "Austria",
        ],
    ),
    (
        "AU",
        [
            "Australia",
            "Australien",
            "Australie",
            "Australia",
            "Australia",
            "Australien",
            "Australia",
        ],
    ),
    (
        "BE",
        [
            "Belgium", "Belgien", "Belgique", "Belgia", "Belgia", "Belgien", "Bélgica",
        ],
    ),
    (
        "BG",
        [
            "Bulgaria",
            "Bulgarien",
            "Bulgarie",
            "Bułgaria",
            "Bulgaria",
            "Bulgarien",
            "Bulgaria",
        ],
    ),
    (
        "CA",
        [
            "Canada", "Kanada", "Canada", "Kanada", "Kanada", "Kanada", "Canadá",
        ],
    ),
    (
        "CH",
        [
            "Switzerland",
            "Schweiz",
            "Suisse",
            "Szwajcaria",
            "Sveitsi",
            "Schweiz",
            "Suiza",
        ],
    ),
    (
        "CY",
        [
            "Cyprus", "Zypern", "Chypre", "Cypr", "Kypros", "Cypern", "Chipre",
        ],
    ),
    (
        "CZ",
        [
            "Czechia",
            "Tschechien",
            "Tchéquie",
            "Czechy",
            "Tšekki",
            "Tjeckien",
            "Chequia",
        ],
    ),
    (
        "DE",
        [
            "Germany",
            "Deutschland",
            "Allemagne",
            "Niemcy",
            "Saksa",
            "Tyskland",
            "Alemania",
        ],
    ),
    (
        "DK",
        [
            "Denmark",
            "Dänemark",
            "Danemark",
            "Dania",
            "Tanska",
            "Danmark",
            "Dinamarca",
        ],
    ),
    (
        "EE",
        [
            "Estonia", "Estland", "Estonie", "Estonia", "Viro", "Estland", "Estonia",
        ],
    ),
    (
        "ES",
        [
            "Spain",
            "Spanien",
            "Espagne",
            "Hiszpania",
            "Espanja",
            "Spanien",
            "España",
        ],
    ),
    (
        "FI",
        [
            "Finland",
            "Finnland",
            "Finlande",
            "Finlandia",
            "Suomi",
            "Finland",
            "Finlandia",
        ],
    ),
    (
        "FR",
        [
            "France",
            "Frankreich",
            "France",
            "Francja",
            "Ranska",
            "Frankrike",
            "Francia",
        ],
    ),
    (
        "GB",
        [
            "United Kingdom",
            "Vereinigtes Königreich",
            "Royaume-Uni",
            "Wielka Brytania",
            "Yhdistynyt kuningaskunta",
            "Storbritannien",
            "Reino Unido",
        ],
    ),
    (
        "GR",
        [
            "Greece",
            "Griechenland",
            "Grèce",
            "Grecja",
            "Kreikka",
            "Grekland",
            "Grecia",
        ],
    ),
    (
        "HR",
        [
            "Croatia",
            "Kroatien",
            "Croatie",
            "Chorwacja",
            "Kroatia",
            "Kroatien",
            "Croacia",
        ],
    ),
    (
        "HU",
        [
            "Hungary", "Ungarn", "Hongrie", "Węgry", "Unkari", "Ungern", "Hungría",
        ],
    ),
    (
        "IE",
        [
            "Ireland", "Irland", "Irlande", "Irlandia", "Irlanti", "Irland", "Irlanda",
        ],
    ),
    (
        "IS",
        [
            "Iceland", "Island", "Islande", "Islandia", "Islanti", "Island", "Islandia",
        ],
    ),
    (
        "IT",
        [
            "Italy", "Italien", "Italie", "Włochy", "Italia", "Italien", "Italia",
        ],
    ),
    (
        "JP",
        [
            "Japan", "Japan", "Japon", "Japonia", "Japani", "Japan", "Japón",
        ],
    ),
    (
        "LI",
        [
            "Liechtenstein",
            "Liechtenstein",
            "Liechtenstein",
            "Liechtenstein",
            "Liechtenstein",
            "Liechtenstein",
            "Liechtenstein",
        ],
    ),
    (
        "LT",
        [
            "Lithuania",
            "Litauen",
            "Lituanie",
            "Litwa",
            "Liettua",
            "Litauen",
            "Lituania",
        ],
    ),
    (
        "LU",
        [
            "Luxembourg",
            "Luxemburg",
            "Luxembourg",
            "Luksemburg",
            "Luxemburg",
            "Luxemburg",
            "Luxemburgo",
        ],
    ),
    (
        "LV",
        [
            "Latvia", "Lettland", "Lettonie", "Łotwa", "Latvia", "Lettland", "Letonia",
        ],
    ),
    (
        "MT",
        [
            "Malta", "Malta", "Malte", "Malta", "Malta", "Malta", "Malta",
        ],
    ),
    (
        "NL",
        [
            "Netherlands",
            "Niederlande",
            "Pays-Bas",
            "Holandia",
            "Alankomaat",
            "Nederländerna",
            "Países Bajos",
        ],
    ),
    (
        "NO",
        [
            "Norway", "Norwegen", "Norvège", "Norwegia", "Norja", "Norge", "Noruega",
        ],
    ),
    (
        "PL",
        [
            "Poland", "Polen", "Pologne", "Polska", "Puola", "Polen", "Polonia",
        ],
    ),
    (
        "PT",
        [
            "Portugal",
            "Portugal",
            "Portugal",
            "Portugalia",
            "Portugali",
            "Portugal",
            "Portugal",
        ],
    ),
    (
        "RO",
        [
            "Romania",
            "Rumänien",
            "Roumanie",
            "Rumunia",
            "Romania",
            "Rumänien",
            "Rumanía",
        ],
    ),
    (
        "SE",
        [
            "Sweden", "Schweden", "Suède", "Szwecja", "Ruotsi", "Sverige", "Suecia",
        ],
    ),
    (
        "SI",
        [
            "Slovenia",
            "Slowenien",
            "Slovénie",
            "Słowenia",
            "Slovenia",
            "Slovenien",
            "Eslovenia",
        ],
    ),
    (
        "SK",
        [
            "Slovakia",
            "Slowakei",
            "Slovaquie",
            "Słowacja",
            "Slovakia",
            "Slovakien",
            "Eslovaquia",
        ],
    ),
    (
        "US",
        [
            "United States",
            "Vereinigte Staaten",
            "États-Unis",
            "Stany Zjednoczone",
            "Yhdysvallat",
            "USA",
            "Estados Unidos",
        ],
    ),
];

/// The name of a country in `lang`, or the bare code for one this table does not know.
pub fn name(lang: Lang, code: &str) -> String {
    let idx = Lang::ALL.iter().position(|l| *l == lang).unwrap_or(0);
    COUNTRIES
        .iter()
        .find(|(c, _)| c.eq_ignore_ascii_case(code))
        .map_or_else(|| code.to_ascii_uppercase(), |(_, n)| n[idx].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_exist_in_every_language_and_codes_are_sorted_and_unique() {
        let mut last = "";
        for (code, names) in COUNTRIES {
            assert!(*code > last, "{code} is out of order");
            last = code;
            assert_eq!(code.len(), 2);
            assert!(names.iter().all(|n| !n.is_empty()));
        }
        assert_eq!(name(Lang::De, "DE"), "Deutschland");
        assert_eq!(name(Lang::Fr, "de"), "Allemagne");
        assert_eq!(name(Lang::En, "ZZ"), "ZZ");
    }
}
