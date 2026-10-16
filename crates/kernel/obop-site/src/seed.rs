//! `obop site seed`: a small demo site with invented content so a fresh instance has something
//! to look at. Everything here is made up: a pretend tea and ceramics shop, no real people, no
//! real addresses. It writes straight into the store, like the other demo seeds (the governed
//! states are set directly, which the commands would not allow).

use obop_commands::{Dispatcher, Store, Value};
use serde_json::{Value as J, json};
use std::collections::BTreeMap;

type Rec = BTreeMap<String, Value>;

fn rec(pairs: &[(&str, Value)]) -> Rec {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

fn t(s: &str) -> Value {
    Value::text(s)
}

fn i(n: i64) -> Value {
    Value::Int(n)
}

fn b(v: bool) -> Value {
    Value::Bool(v)
}

/// What the seed created, one line per kind.
#[derive(Debug)]
pub struct Summary(pub Vec<String>);

/// Merge `site` settings into the instance settings without touching the other keys.
fn set_site_defaults(store: &Store, site: J) -> Result<(), String> {
    let backend = store.backend();
    let mut all: serde_json::Map<String, J> = backend
        .get("obop_setting", "instance")?
        .and_then(|f| f.get("value").and_then(J::as_str).map(String::from))
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    let mut current = match all.remove("site") {
        Some(J::Object(o)) => o,
        _ => serde_json::Map::new(),
    };
    if let J::Object(new) = site {
        for (k, v) in new {
            current.entry(k).or_insert(v);
        }
    }
    all.insert("site".into(), J::Object(current));
    let mut f = obop_storage::Fields::new();
    f.insert("value".into(), J::String(J::Object(all).to_string()));
    if !backend.update("obop_setting", "instance", &f)? {
        backend.insert("obop_setting", "instance", &f)?;
    }
    Ok(())
}

pub fn seed(store: &Store, d: &Dispatcher) -> Result<Summary, String> {
    let has_site = d.has_model("website_page");
    let has_shop = d.has_model("ecommerce_listing") && d.has_model("product");
    if !has_site && !has_shop {
        return Err(
            "install the website and ecommerce packages first: obop modules install-from website, obop modules install-from ecommerce"
                .into(),
        );
    }
    if (has_site && store.count("website_page") > 0)
        || (has_shop && store.count("ecommerce_listing") > 0)
    {
        return Err(
            "this instance already has pages or listings; the seed only fills an empty site".into(),
        );
    }
    let mut out = Vec::new();
    set_site_defaults(
        store,
        json!({
            "title": "Fernhill Goods",
            "tagline": "Small-batch tea, ceramics and gifts",
            "accent": "#2d6a4f",
            "footer_text": "A demo shop with invented products. Nothing here is for sale.",
            "default_lang": "en",
            "shop_currency": "EUR",
            "checkout_note": "This is a demo: no order is fulfilled.",
        }),
    )?;
    out.push("site settings (title, tagline, accent, footer)".to_string());

    if has_site {
        let page = |slug: &str, title: &str, body: &str, desc: &str| {
            store.insert(
                "website_page",
                rec(&[
                    ("slug", t(slug)),
                    ("title", t(title)),
                    ("body", t(body)),
                    ("seo_description", t(desc)),
                    ("published_on", t("2026-09-01")),
                    ("state", t("published")),
                ]),
            )
        };
        let home = page(
            "home",
            "Tea, ceramics and small gifts",
            "We source a few teas we love, work with potters who throw every piece by hand, and pack gifts that take the hurry out of a morning.\n\nEverything is made in small batches, so it is here until it is gone.",
            "Small-batch tea, handmade ceramics and slow-morning gifts from Fernhill Goods.",
        )?;
        let about = page(
            "about",
            "About Fernhill Goods",
            "## Who we are\n\nFernhill Goods is a tiny shop with a short list of things we would happily use ourselves.\n\n## How we work\n\n- We taste every tea before it goes on the shelf.\n- We buy ceramics directly from the people who make them.\n- We answer every message ourselves.\n\n> Good things take the time they take.\n\nQuestions? [Write to us](/contact).",
            "Who is behind Fernhill Goods and how we choose what we sell.",
        )?;
        page(
            "delivery",
            "Delivery and returns",
            "## Delivery\n\nStandard delivery takes three to five working days and is free for orders over 50 euros. Express takes one to two days.\n\n## Returns\n\nTell us within 14 days if something is not right and we will make it right.\n\nThis is a demo shop: nothing is actually shipped.",
            "How long delivery takes, what it costs and how returns work.",
        )?;
        page(
            "contact",
            "Contact",
            "Write to us about an order, a gift or just to say hello. We usually reply within two working days.",
            "Get in touch with Fernhill Goods.",
        )?;
        out.push("4 pages (home, about, delivery, contact)".into());
        let tr = |page: &str, lang: &str, title: &str, body: &str| {
            store.insert(
                "website_page_translation",
                rec(&[
                    ("page_id", t(page)),
                    ("lang", t(lang)),
                    ("title", t(title)),
                    ("body", t(body)),
                    ("published", b(true)),
                ]),
            )
        };
        tr(
            &home,
            "de",
            "Tee, Keramik und kleine Geschenke",
            "Wir wählen wenige Tees aus, die wir lieben, arbeiten mit Töpferinnen und Töpfern, die jedes Stück von Hand drehen, und packen Geschenke, die dem Morgen die Hektik nehmen.\n\nAlles entsteht in kleinen Mengen: Was weg ist, ist weg.",
        )?;
        tr(
            &about,
            "de",
            "Über Fernhill Goods",
            "## Wer wir sind\n\nFernhill Goods ist ein winziger Laden mit einer kurzen Liste von Dingen, die wir selbst gern benutzen.\n\n> Gute Dinge brauchen die Zeit, die sie brauchen.",
        )?;
        tr(
            &about,
            "fr",
            "À propos de Fernhill Goods",
            "## Qui nous sommes\n\nFernhill Goods est une toute petite boutique avec une courte liste d'objets que nous utiliserions volontiers nous-mêmes.\n\n> Les belles choses prennent le temps qu'il leur faut.",
        )?;
        out.push("3 page translations (de, fr)".into());

        let post = |slug: &str, title: &str, summary: &str, body: &str, tags: &str, date: &str| {
            store.insert(
                "website_blog_post",
                rec(&[
                    ("slug", t(slug)),
                    ("title", t(title)),
                    ("author_name", t("Editorial team")),
                    ("byline", t("The Fernhill team")),
                    ("summary", t(summary)),
                    ("body", t(body)),
                    ("tags", t(tags)),
                    ("published_on", t(date)),
                    ("state", t("published")),
                ]),
            )
        };
        let p1 = post(
            "how-we-pick-our-teas",
            "How we pick our teas",
            "A cup, a notebook and a very long afternoon: how a tea earns a place on our shelf.",
            "Every tea starts as a sample in a paper bag. We brew it three ways, write down what we taste and wait a week before we taste it again.\n\n## What we look for\n\n1. A clean, honest first sip.\n2. A finish that makes you want a second cup.\n3. A grower we are happy to name.\n\nMost samples do not make it. The ones that do are on [the shop page](/shop).",
            "tea,sourcing",
            "2026-09-12",
        )?;
        post(
            "caring-for-handmade-ceramics",
            "Caring for handmade ceramics",
            "Handmade stoneware lasts for decades if you treat it kindly. A short guide.",
            "Stoneware is tougher than it looks, but it still prefers a gentle life.\n\n- Warm a pot with hot water before the tea goes in.\n- Hand-wash when you can; the dishwasher is the enemy of glaze.\n- Never move a hot piece onto a cold surface.\n\nA chip is a story, not a failure.",
            "ceramics,care",
            "2026-08-28",
        )?;
        post(
            "a-slower-morning-ritual",
            "A slower morning ritual",
            "Five small steps that turn the first cup of the day into a pause.",
            "You do not need an hour. You need five minutes and a kettle.\n\n1. Boil the water, then let it rest for a minute.\n2. Warm the pot.\n3. Measure the leaves instead of guessing.\n4. Set a timer and look out of the window.\n5. Drink it while it is still too hot to rush.",
            "ritual,tea",
            "2026-07-30",
        )?;
        store.insert(
            "website_blog_post_translation",
            rec(&[
                ("post_id", t(&p1)),
                ("lang", t("de")),
                ("title", t("Wie wir unsere Tees auswählen")),
                ("summary", t("Eine Tasse, ein Notizbuch und ein sehr langer Nachmittag: So kommt ein Tee ins Regal.")),
                ("body", t("Jeder Tee beginnt als Muster in einer Papiertüte. Wir brühen ihn auf drei Arten auf, schreiben auf, was wir schmecken, und probieren ihn eine Woche später noch einmal.")),
                ("published", b(true)),
            ]),
        )?;
        out.push("3 blog posts (1 translated)".into());
        for (n, (label, url)) in [
            ("Shop", "/shop"),
            ("Journal", "/blog"),
            ("About", "/about"),
            ("Delivery", "/delivery"),
            ("Contact", "/contact"),
        ]
        .iter()
        .enumerate()
        {
            store.insert(
                "website_menu_item",
                rec(&[
                    ("label", t(label)),
                    ("url", t(url)),
                    ("sequence", i(n as i64 + 1)),
                    ("visible", b(true)),
                ]),
            )?;
        }
        out.push("5 menu items".into());
    }

    if has_shop {
        let cat = |slug: &str, name: &str, desc: &str, seq: i64| {
            store.insert(
                "ecommerce_category",
                rec(&[
                    ("slug", t(slug)),
                    ("name", t(name)),
                    ("description", t(desc)),
                    ("sequence", i(seq)),
                    ("published", b(true)),
                ]),
            )
        };
        let tea = cat(
            "tea",
            "Tea",
            "Loose-leaf teas, tasted by us before they reach the shelf.",
            1,
        )?;
        let ceramics = cat(
            "ceramics",
            "Ceramics",
            "Pots, cups and drippers thrown by hand.",
            2,
        )?;
        let gifts = cat("gifts", "Gifts", "Ready to give, nicely packed.", 3)?;
        out.push("3 categories".into());
        // (slug, title, summary, description, price, compare_at, category, policy, stock, sku)
        #[allow(clippy::type_complexity)]
        let items: [(
            &str,
            &str,
            &str,
            &str,
            i64,
            Option<i64>,
            &str,
            &str,
            i64,
            &str,
        ); 8] = [
            (
                "sencha-green-tea",
                "Sencha Green Tea, 100 g",
                "Fresh, grassy and bright. Our everyday green.",
                "A first-flush sencha with a clean, grassy sweetness.\n\n- Steep at 70 degrees for 60 seconds\n- 100 g makes about 40 cups",
                1250,
                None,
                &tea,
                "block_when_out",
                40,
                "TEA-SEN-100",
            ),
            (
                "smoked-lapsang-souchong",
                "Smoked Lapsang Souchong, 100 g",
                "Pine smoke and dried fruit. For campfire people.",
                "Dried over pine wood. Bold, smoky and surprisingly smooth with a splash of milk.",
                1400,
                None,
                &tea,
                "block_when_out",
                25,
                "TEA-LAP-100",
            ),
            (
                "jasmine-pearls",
                "Jasmine Pearls, 80 g",
                "Hand-rolled green tea scented with jasmine blossom.",
                "Each pearl unfurls into a leaf as it steeps. Delicate, floral and lightly sweet.",
                1690,
                Some(1990),
                &tea,
                "always_available",
                0,
                "TEA-JAS-080",
            ),
            (
                "stoneware-teapot",
                "Stoneware Teapot, 0.8 l",
                "A sturdy pot with a matte glaze and a lid that stays put.",
                "Thrown by hand and fired twice. The built-in strainer keeps even small leaves out of your cup.\n\nDishwasher safe, but we would hand-wash it.",
                4800,
                None,
                &ceramics,
                "block_when_out",
                6,
                "CER-POT-08",
            ),
            (
                "tea-cups-set-of-two",
                "Tea Cups, set of two",
                "Two small cups with a thumb rest. Made to order.",
                "Two 150 ml cups. Each batch is made to order, so allow a few extra weeks.",
                2600,
                None,
                &ceramics,
                "allow_backorder",
                0,
                "CER-CUP-02",
            ),
            (
                "pour-over-dripper",
                "Pour-over Dripper",
                "A single-hole dripper for a slow, even brew.",
                "Sized for one to two cups. Sold out for now; the next batch is in the kiln.",
                3400,
                None,
                &ceramics,
                "block_when_out",
                0,
                "CER-DRP-01",
            ),
            (
                "slow-morning-gift-box",
                "Gift Box: Slow Morning",
                "Tea, a small cup and a note card, boxed and ready to give.",
                "A gift that says take your time: 50 g of sencha, one stoneware cup and a hand-written card.",
                3900,
                None,
                &gifts,
                "always_available",
                0,
                "GFT-SLM-01",
            ),
            (
                "linen-tea-towel",
                "Linen Tea Towel",
                "Stonewashed linen that gets softer with every wash.",
                "50 by 70 cm, natural undyed linen with a woven edge.",
                1800,
                None,
                &gifts,
                "block_when_out",
                18,
                "GFT-TWL-01",
            ),
        ];
        let mut first = None;
        for (slug, title, summary, desc, price, compare, category, policy, stock, sku) in items {
            let product = store.insert(
                "product",
                rec(&[
                    ("name", t(title)),
                    ("sku", t(sku)),
                    ("list_price_minor", i(price)),
                ]),
            )?;
            let mut fields = rec(&[
                ("product_id", t(&product)),
                ("slug", t(slug)),
                ("title", t(title)),
                ("summary", t(summary)),
                ("description", t(desc)),
                ("category_id", t(category)),
                ("price", i(price)),
                ("currency", t("EUR")),
                ("sku", t(sku)),
                ("stock_policy", t(policy)),
                ("stock_quantity", i(stock)),
                ("published", b(true)),
            ]);
            if let Some(c) = compare {
                fields.insert("compare_at_price".into(), i(c));
            }
            let id = store.insert("ecommerce_listing", fields)?;
            first.get_or_insert(id);
        }
        out.push("8 listings with their products".into());
        if let Some(l) = first {
            store.insert(
                "ecommerce_listing_translation",
                rec(&[
                    ("listing_id", t(&l)),
                    ("lang", t("de")),
                    ("title", t("Sencha Grüntee, 100 g")),
                    ("summary", t("Frisch, grasig und hell. Unser Alltagsgrüntee.")),
                    ("description", t("Ein Sencha der ersten Pflückung mit klarer, grasiger Süße.\n\n- Bei 70 Grad 60 Sekunden ziehen lassen\n- 100 g reichen für etwa 40 Tassen")),
                    ("published", b(true)),
                ]),
            )?;
            out.push("1 listing translation (de)".into());
        }
        for (name, carrier, price, free_over, days) in [
            ("Standard delivery", "Post", 490, Some(5000), 4),
            ("Express delivery", "Courier", 990, None, 2),
        ] {
            let mut f = rec(&[
                ("name", t(name)),
                ("carrier", t(carrier)),
                ("price", i(price)),
                ("delivery_days", i(days)),
                ("active", b(true)),
            ]);
            if let Some(fo) = free_over {
                f.insert("free_over".into(), i(fo));
            }
            store.insert("ecommerce_shipping_method", f)?;
        }
        out.push("2 shipping methods".into());
    }
    Ok(Summary(out))
}
