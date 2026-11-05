//! Golden pins of every page of the authorization flow, as an end user sees them today (S1a-0 of
//! the instance branding work, docs/spec/branding.md). Taken from `origin/main` before any brand
//! code existed: an instance with no brand row must render these pages byte for byte as they are
//! here, in every language, so a later change that moves one of them is a finding, not a refresh.
//!
//! A pin is the SHA-256 of the page's HTML joined to its content security policy header, in this
//! form (the pages carry nothing random here: the ids, the token and the texts are fixed below):
//!
//! ```text
//! {html}\nContent-Security-Policy: {policy}\n
//! ```
//!
//! So the OAuth pages and the signer pages are pinned as HTML plus the policy header, and nothing
//! else of the answer: their status code and their other headers (`Cache-Control`, the content
//! type and the rest) are NOT pinned by these tests.
//!
//! To read the values again from the tree as it is (after a change that is meant to move them, in
//! the same commit that explains why), run
//!
//! ```text
//! OBOP_PRINT_PINS=1 cargo test -p obop-oauth pins -- --nocapture --test-threads=1
//! ```
//!
//! which prints each table as Rust source instead of asserting it. Print mode is not a check: a
//! pin that runs with `OBOP_PRINT_PINS=1` prints its table and then panics, so a stray variable in
//! the environment cannot give a green run.
//!
//! # Running every pin
//!
//! The pins sit in several crates. These two commands (from the repository root, with a Docker-free
//! `PATH` on this machine) run all of them in assert mode; each filter is an OR of its words:
//!
//! ```text
//! cargo test --locked -p obop-oauth -p obop-auth -p obop-http -p obop-site -- pins signer_pins
//! cargo test --locked -p obop -- pins the_google_drive_page_end_to_end
//! ```
//!
//! The first selects every test whose name has `pins` in it (the pages here, the shop files, the
//! authenticator label and the default issuer, the relying-party name of the OAuth ceremony site)
//! and the signer pin of `obop-http`, whose name has `signer_pins` in it. The second selects the
//! pins of the `obop` crate by `pins`, and the container test
//! `the_google_drive_page_end_to_end_against_a_fake_google`, one scenario of which is the pin of
//! the return page after a Drive sign-in (the scenarios share one fake Google, so they run in one
//! test and that pin has no test name of its own).

use crate::client::Class;
use crate::flow::FlowCtx;
use crate::page::{
    CATALOGUE, LANGS, Rendered, consent_page, error_page, login_page, mfa_page, t as text,
};
use crate::redirect::RedirectForm;
use sha2::{Digest, Sha256};

fn sha(s: &str) -> String {
    Sha256::digest(s.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The pin of a rendered page: its HTML and its policy header.
fn pin_of(r: &Rendered) -> String {
    sha(&format!("{}\nContent-Security-Policy: {}\n", r.html, r.csp))
}

/// Print the table as source (`OBOP_PRINT_PINS=1`), or hold the pages to it.
fn check(table: &str, pinned: &[(&str, &str)], now: &[(String, String)]) {
    if std::env::var("OBOP_PRINT_PINS").is_ok_and(|v| v == "1") {
        let mut out = format!("\n//@pins {table}\nconst {table}: &[(&str, &str)] = &[\n");
        for (name, digest) in now {
            out.push_str(&format!("    (\"{name}\", \"{digest}\"),\n"));
        }
        out.push_str(&format!("];\n//@end {table}\n"));
        println!("{out}");
        panic!(
            "OBOP_PRINT_PINS=1 is print mode, not a check: this run printed the table and proved \
             nothing. Unset it to run the pins."
        );
    }
    assert!(
        !pinned.is_empty(),
        "{table} is empty: print it with OBOP_PRINT_PINS=1"
    );
    let mut moved = Vec::new();
    for (i, (name, digest)) in now.iter().enumerate() {
        match pinned.get(i) {
            Some((n, d)) if n == name && d == digest => {}
            Some((n, _)) if n != name => moved.push(format!("{name}: the table has {n} here")),
            Some(_) => moved.push(format!("{name}: {digest}")),
            None => moved.push(format!("{name}: not in the table")),
        }
    }
    if pinned.len() > now.len() {
        moved.push(format!(
            "the table has {} entries, the test makes {}",
            pinned.len(),
            now.len()
        ));
    }
    assert!(
        moved.is_empty(),
        "{table}: {} of {} pins moved (a page differs from what an instance with no brand row has \
         always sent):\n{}",
        moved.len(),
        now.len(),
        moved.join("\n")
    );
}

/// A flow in the state a page is rendered for. Everything is fixed: no random id, no nonce.
struct Case {
    class: Class,
    form: RedirectForm,
    origin: &'static str,
    host: &'static str,
    client: &'static str,
    user: Option<&'static str>,
    groups: &'static [&'static str],
    preselected: &'static [&'static str],
    microsoft: bool,
    max_days: u32,
}

impl Case {
    fn ctx(&self, lang: &str) -> FlowCtx {
        FlowCtx {
            id: "flow-0001".into(),
            csrf: "csrf-0001".into(),
            lang: lang.into(),
            client_name: self.client.into(),
            redirect_origin: self.origin.into(),
            redirect_host: self.host.into(),
            redirect_form: self.form,
            user: self.user.map(Into::into),
            class: self.class,
            groups: self.groups.iter().map(|g| (*g).into()).collect(),
            preselected: self.preselected.iter().map(|g| (*g).into()).collect(),
            microsoft: self.microsoft,
            max_days: self.max_days,
        }
    }
}

/// An MCP client (Claude) on the web: the sign-in and second-factor pages and the consent page.
const MCP: Case = Case {
    class: Class::Mcp,
    form: RedirectForm::Web,
    origin: "https://claude.ai",
    host: "claude.ai",
    client: "Claude",
    user: Some("ada"),
    groups: &["base.user", "sales.user", "platform.admin"],
    preselected: &["base.user"],
    microsoft: false,
    max_days: 90,
};

/// A mobile app that registered a private-use scheme, with the Microsoft step on its sign-in page.
const MOBILE: Case = Case {
    class: Class::Mobile,
    form: RedirectForm::PrivateUse,
    origin: "org.obop.mileage:",
    host: "org.obop.mileage",
    client: "Mileage",
    user: Some("ada"),
    groups: &["base.user", "sales.user"],
    preselected: &["base.user"],
    microsoft: true,
    max_days: 90,
};

/// A mobile app whose result goes to a program on this computer (a loopback address).
const LOOPBACK: Case = Case {
    form: RedirectForm::Loopback,
    origin: "http://127.0.0.1:53682",
    host: "127.0.0.1:53682",
    ..MOBILE
};

/// The consent page of an account that signs in with Microsoft only, on a phone: capped at 30 days.
const CAPPED: Case = Case {
    user: Some("bob"),
    max_days: 30,
    ..MOBILE
};

/// A page of the table: how to render it in a language.
type Render = Box<dyn Fn(&str) -> Rendered>;

/// The kinds of page, in the order of the table.
fn pages() -> Vec<(&'static str, Render)> {
    let login_mcp = Case { user: None, ..MCP };
    let login_mobile = Case {
        user: None,
        ..MOBILE
    };
    vec![
        (
            "login_mcp",
            Box::new(move |l| login_page(&login_mcp.ctx(l), None)),
        ),
        (
            "login_mobile_ms",
            Box::new(move |l| login_page(&login_mobile.ctx(l), None)),
        ),
        (
            "mfa_code",
            Box::new(|l| mfa_page(&MCP.ctx(l), "mfa-token-0001", None, false)),
        ),
        (
            "mfa_passkey",
            Box::new(|l| mfa_page(&MCP.ctx(l), "mfa-token-0001", None, true)),
        ),
        ("consent_mcp", Box::new(|l| consent_page(&MCP.ctx(l), None))),
        (
            "consent_mobile",
            Box::new(|l| consent_page(&MOBILE.ctx(l), None)),
        ),
        (
            "consent_loopback",
            Box::new(|l| consent_page(&LOOPBACK.ctx(l), None)),
        ),
        (
            "consent_capped",
            Box::new(|l| consent_page(&CAPPED.ctx(l), None)),
        ),
        ("err_client", Box::new(|l| error_page(l, "err.client"))),
        ("err_redirect", Box::new(|l| error_page(l, "err.redirect"))),
        ("err_gone", Box::new(|l| error_page(l, "err.gone"))),
        (
            "err_forbidden",
            Box::new(|l| error_page(l, "err.forbidden")),
        ),
        ("err_busy", Box::new(|l| error_page(l, "err.busy"))),
    ]
}

#[test]
fn pins_the_pages_of_the_flow_are_what_they_were_before_the_brand() {
    let mut now = Vec::new();
    for (kind, render) in pages() {
        for lang in LANGS {
            now.push((format!("{kind}/{lang}"), pin_of(&render(lang))));
        }
    }
    assert_eq!(now.len(), 13 * 7, "13 kinds of page in 7 languages");
    check("PAGE_PINS", PAGE_PINS, &now);
}

/// The five texts that name the product, in the seven languages: 35 cells that the brand work
/// leaves byte for byte as they are (a brand adds neutral twins beside them and changes none).
const NAMES_THE_PRODUCT: [&str; 5] = [
    "login.lead",
    "consent.who_body",
    "consent.f2",
    "consent.who_body_mobile",
    "consent.who_body_mobile_loopback",
];

#[test]
fn pins_the_35_texts_that_name_the_product_are_unchanged() {
    // They are the only texts that hold the name: no other cell of the catalogue says it.
    for (key, cols) in CATALOGUE {
        let names = NAMES_THE_PRODUCT.contains(key);
        for (i, cell) in cols.iter().enumerate() {
            assert_eq!(
                cell.to_lowercase().contains("obop"),
                names,
                "{key} in {}: whether the cell names the product",
                LANGS[i]
            );
        }
    }
    let mut all = String::new();
    let mut cells = 0;
    for key in NAMES_THE_PRODUCT {
        for lang in LANGS {
            all.push_str(&format!("{key}\t{lang}\t{}\n", text(lang, key)));
            cells += 1;
        }
    }
    assert_eq!(cells, 35);
    check(
        "CATALOGUE_PINS",
        CATALOGUE_PINS,
        &[("names_the_product_35_cells".into(), sha(&all))],
    );
}

// The tables. Printed by `OBOP_PRINT_PINS=1` on origin/main 12a59a79 and pasted here.

#[rustfmt::skip]
const PAGE_PINS: &[(&str, &str)] = &[
    ("login_mcp/en", "519e8455a9ecbdf4b7317a8a96d520445d9953799e51de07a097da844b71b0f8"),
    ("login_mcp/de", "9afc1501e1cdc0181cbf3673a0ade6bd3e3d0dbc09a176af4ef37812b8c99669"),
    ("login_mcp/fr", "d15b9b7626478af3b4566d58c4a61efae85494a3b66316a1f51f7e06142a4016"),
    ("login_mcp/pl", "33ef7df6440dc4ed9130969b95fd359b26c00fe011c21cab53a1d5c60dd5747d"),
    ("login_mcp/fi", "5b9f96961fb11ba0c736661ff57092a8c008e34ee097db8ef6443bf24e2f16a0"),
    ("login_mcp/sv", "53cbfe6eeceb3a2a438e9b6752eb05ea5a6ce629813e924fa5529e02b3c98b98"),
    ("login_mcp/es", "a9410c235c2f2292df0518528324262c3e610c7770d71f6687b8777a24b93bb1"),
    ("login_mobile_ms/en", "34f97afdd56b3fa6fcb4a45ecc9a0eb69b1b8ff427acec80cfd4e0d6740f4305"),
    ("login_mobile_ms/de", "76c0dfe6d6bdec1bfcfb2386715c647a38b58e9a09c8da97b9a97cc0321cbc62"),
    ("login_mobile_ms/fr", "39082f4660ca9bbfaf2191b9f20cdccd6510c3455ed64d499c5136345d47aa36"),
    ("login_mobile_ms/pl", "6f8d6ddf40dd0d892050ce100e1687bd033454fda40dd325255506085114acd6"),
    ("login_mobile_ms/fi", "adb55a166ebd2272085d060ad5c83a4bc2093ce024330b3c95f659590f25394d"),
    ("login_mobile_ms/sv", "9e76894bbde8146c2f65f9d312d316bd77dd95f54424a149268f02e408e44366"),
    ("login_mobile_ms/es", "073373c7a87a8c32f77f76cbd3cac12564aaedbfa55cd7e892792d3afc6b52e8"),
    ("mfa_code/en", "ae80e354ccbf72d6ce4ae9deb3a4275e43d2e90d6300280fe6f805bca91a4058"),
    ("mfa_code/de", "604900f44c756e1d0fb688304f5d3ab3723ee2f47d0c54e6aa5b490813a44fc2"),
    ("mfa_code/fr", "a029b2eff0e6e36144c1b46915e9c905f0a5c0c012f4efaca2f675e9d505389b"),
    ("mfa_code/pl", "2120e83431b3ea8fae31e1541f746985cd7cbe41c390ec690f3fb168f2470c12"),
    ("mfa_code/fi", "029b34b3b435ca60eebabc036159ee1d5458cb50d0d3ed40ef387cdf2a8ea00b"),
    ("mfa_code/sv", "3671b38a2be31af5cf00a48662fb1c79ba10e0b863f97efefbc65ff445cac36d"),
    ("mfa_code/es", "72d81514c3ce9052fdf47203532b044a04e9186bae908b602602264d52a66c7f"),
    ("mfa_passkey/en", "8ca604a45973eda13274e94e6322405e459fbc27dd59a8d7a6bf1d5283aa8304"),
    ("mfa_passkey/de", "3834ffe34669c351b2c3452229581410f731ef9a26851187f7b82d63255f6ef0"),
    ("mfa_passkey/fr", "185e541b03abbea8acaaf82ef26ab6dfbde2a284b851693e293c98fe5e19f0d5"),
    ("mfa_passkey/pl", "05e9f9db65f8246db4c7d7b3b3ddfcc70c4afc637d6755cf79e906422460b8e0"),
    ("mfa_passkey/fi", "b8d861e1ed6d50175c2845c739a6d851696665c64769dfb6f8482f22ccb316e0"),
    ("mfa_passkey/sv", "86b449a304ff434ab9f5efc62ee0443c3ad1ca0bcb8576b0b508059d0d0cbec1"),
    ("mfa_passkey/es", "a7093659453174afe2e890cd554d49aead85cf096ff1969872eba92934ac9249"),
    ("consent_mcp/en", "dee4f02a0534b986cbb1994bf33e96d6025929cd5903486dc68e6cb85bf7d1c2"),
    ("consent_mcp/de", "3d460f34236d9aeb203ab38575a97f6110c577f1aa5e5c9fe96a681d56a4e939"),
    ("consent_mcp/fr", "55f165b69bb069d4b8cbd0f42bcf3e1d3632741a26fb6b38a48c104d5b6212dd"),
    ("consent_mcp/pl", "49f8596dc522b596d1f0d6461059fda08611e0350b0f7cdd3ef947f7f1ed003a"),
    ("consent_mcp/fi", "abeb7e7c0ba29a659d98a82024bb755cd8ec0c5baab28641aa265be503eaa3ac"),
    ("consent_mcp/sv", "f2bea28bf8c85feaa0ed4232093dc2b6aa2ce31d67a84b1ea1c6514744337424"),
    ("consent_mcp/es", "5338709345bafed4c23e08526b8f2a293713a1255a4280ef882f69a718499ba0"),
    ("consent_mobile/en", "5aaf3701fdd0b11dac7776f0792d1a176d7cb910d01a4acac7814cbd2545d828"),
    ("consent_mobile/de", "35f0a478e9b4e75aa387ed8f933e62ea38c800187ee9af877beef299d0e0b424"),
    ("consent_mobile/fr", "69edc51140fc156fc76a73f66217d5ef3296cdc872c54e48ad23634b8fed2920"),
    ("consent_mobile/pl", "957d61683a04009b046cfbab4ccdc35c128beb8301b7f24dff63d25ed9dd1183"),
    ("consent_mobile/fi", "b52b50b0b866a304f6518964c4e6154e474a5fff70cb6c9f39ac693cfe9e90f7"),
    ("consent_mobile/sv", "52d7ef069690c9edb6355952622899568072e4d61196df79ce5863664ffc10ca"),
    ("consent_mobile/es", "79c566b9c4c64155fdec5bbb186e94c9bd06e20f5e98e26995d2e37cdb0c3c0a"),
    ("consent_loopback/en", "fddb3200b2df813fda24866ddba7b5c51304a262d603327cdc0a7e48a570d1c0"),
    ("consent_loopback/de", "17c1ccfc164cb275996e7a037d441df3ba2851724be2ba42702a8dd1b6289e32"),
    ("consent_loopback/fr", "e63c1b202aebb4eb5640d765b142a63bc8738ad589ae75330ae3cee65091a04e"),
    ("consent_loopback/pl", "8e83a93b225ee0634ab918f477ada3f39c3cf9f431f288b775f4bddaf838ded2"),
    ("consent_loopback/fi", "557f3254f99d05301489c15d8bde5b0ebc0cb68bc5a593bee28ec57391457945"),
    ("consent_loopback/sv", "733b81fe6700a994f3154f3454d6189d542646485da26dbd7caa3b0acc3ccecb"),
    ("consent_loopback/es", "b25d18bc51dfc0fcbd77131bec18d9ff979753261df1bb4d00ce84d4272dfb84"),
    ("consent_capped/en", "78a3b3ab476b14954e150d40459a7b750b3a422eba1209a8951493308d493637"),
    ("consent_capped/de", "0363ede7ae21bff42bd2f1cd0d6bbc2a218204d7b24e2dd23204d96fae428a19"),
    ("consent_capped/fr", "133843e2d2f8347b409f8890681bd66d13a2b38b3144c2867511606a271c6095"),
    ("consent_capped/pl", "c38b39f18a1d0b70a811934f239d365366b83ebfc7a3b23ab81c1166f9acf1cc"),
    ("consent_capped/fi", "700ee81838d82d1e157d86798e3edc6e6d035f7cebb22e271afca9b98868d254"),
    ("consent_capped/sv", "7c54752ce5cc2c4cc18c263ea41ea377c64ae6d751a1f1028e76670e79a8f2d0"),
    ("consent_capped/es", "9dffb741d774c4cfd342bf5e1524c78f0a4539ff674e8630c91a05cc4c547508"),
    ("err_client/en", "cc08817d44fb3d1d86d079717b3ad0383c0c57fa5dda142f0f91a8c2d8b1380d"),
    ("err_client/de", "d8a92adf02c8b9fb907a8fec3e78c643d222801f8e1c557f19bf890edfcf1d11"),
    ("err_client/fr", "2905370b185dc92d3afc1bcf92df73bff4e1ff9e5822cd25d010164d3ddc61f1"),
    ("err_client/pl", "c4a676f2710374d13a71c2cf3c2d774d7aec8441d891144039c0515c0921d4ac"),
    ("err_client/fi", "413293ca214b04493c5ba8d7a5ca9ee52075f4123746872db16be268bd6960e4"),
    ("err_client/sv", "c8644533176e6c08b938ffe3c6ad453ed4c692945933a69014be3a3da2751d93"),
    ("err_client/es", "6424922340ce3477fa6022e5a5b62e83f14003cfe875fe36a238f2555fc27c49"),
    ("err_redirect/en", "b3c4342a0fdea8f3f80ac9a282b8646b6c77c99aabd1ec740515ee28abbd05b0"),
    ("err_redirect/de", "11b5419697905bb24e163f342cdbf4a037bd3b49f020d5eb969b32ebed8e3316"),
    ("err_redirect/fr", "56f447a1a512967b1cee47580ab5da86b5cf1c99461c396df357aa352c4c5ed6"),
    ("err_redirect/pl", "d1c2e2811ff9a6e51a023459522d6a154c2da33f857cb256e0ab01132e226dca"),
    ("err_redirect/fi", "935bb43b08f547b8da0d01279a04f884420a8a699f76e2001764b923fed91fe0"),
    ("err_redirect/sv", "00bb987a0f1df81c5c165be94eb6a19986e1a747da35b0cc6c7b7e2cc8acc7d0"),
    ("err_redirect/es", "645c42647009c7519fbec6112601a09f9b283e1bf6ee3d4c5da7ed1f25aa18f8"),
    ("err_gone/en", "65800a0a0b4e38178d62f0a22ed56681cac30e88708049cfb77b90ec3fd88e89"),
    ("err_gone/de", "6b1dc3276cc86acc215477b06347b0cb51894bae7a67719ce3643de910b43389"),
    ("err_gone/fr", "41f915b90cb15f6177fbb20236097cc9aad16afa8711a6d67fec729fc054e10f"),
    ("err_gone/pl", "9df2a392245ca223fa07db38b872c8708e842ae7a6beff3e9aedf7990e59505a"),
    ("err_gone/fi", "e39e377c27f4d55c05c7b58decc77b24065a29f594e9015752e6b44588507db0"),
    ("err_gone/sv", "8156aa058d0c7ee2dc31c330de2e12b1c5a9b072dff0c051187f17809dbea202"),
    ("err_gone/es", "573c1e739d81bf65b3d35f4ae5326d1dd0edb1330a5c3a311109e73c39824a0a"),
    ("err_forbidden/en", "2b8eefd1c594b3c38c678095cd83487e345f9ed8a05d9119d2cb9b83ddfb0040"),
    ("err_forbidden/de", "84f1c32b89201ae180c3acd2fce0c7f7b8e6abe43cd4c42ce7cc518568c17cd2"),
    ("err_forbidden/fr", "9d5967313ddd7380382791fb408cdbd63b97ab043fafee275e01123da0a717c4"),
    ("err_forbidden/pl", "307f54c340e170e54224d529bb207c07270bf7f90c03ea46bd6a5d9c8b34be6f"),
    ("err_forbidden/fi", "88710504fd4ab1dcc298c103c12b284214b878156fe3443a4e7e7d6cfdcf3a15"),
    ("err_forbidden/sv", "ae53782e543bb7bd9f60097fcf9509f18421e716f9d1b2de3a8a60734d0a4025"),
    ("err_forbidden/es", "cc25816125607bc1b8f85cf0424bd8a29db726f479be5310c1de34958767065c"),
    ("err_busy/en", "9ef5b12636e1fd38951eb6031c6e9b076ed0d72d791f1cade296e0a8dc97ffad"),
    ("err_busy/de", "8a1ffaa89e32e8a322c7be6b8a60978ed5d3d16d21fec8e735082d03ff56fe72"),
    ("err_busy/fr", "92daba8af81c34776354cd0b56507e036b5202b9bf0d569f827516fee450f71d"),
    ("err_busy/pl", "298af9ec29398fd8dee96540a4fbd0ec2184bb7149463f86d1e185ff76dbd135"),
    ("err_busy/fi", "9821fb5efc35b7392c4684cc89ddd81979b29d41e6cf6043d3d0bcf3e22ac81f"),
    ("err_busy/sv", "ae6c5db07338af24aa05e250c02c94bb04317e8d6c6618d4b30c212dc7428356"),
    ("err_busy/es", "7297a9dad99a8f819e4efa9aed5697407fd96b6cdf11c5492f60439318935e1e"),
];

#[rustfmt::skip]
const CATALOGUE_PINS: &[(&str, &str)] = &[
    ("names_the_product_35_cells", "a6adaab69bc2e356d3d626b7b8541a3afa1b1837cc9a2e9b15b676d584694423"),
];
