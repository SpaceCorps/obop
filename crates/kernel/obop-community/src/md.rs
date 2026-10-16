//! User-written Markdown to safe HTML: `pulldown-cmark` parses, raw HTML and remote images are
//! neutralised in the event stream, and `ammonia` has the last word with a strict allow-list.

use ammonia::{Builder, UrlRelative};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::collections::HashSet;

fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
}

fn demote(l: HeadingLevel) -> HeadingLevel {
    match l {
        HeadingLevel::H1 | HeadingLevel::H2 => HeadingLevel::H3,
        HeadingLevel::H3 => HeadingLevel::H4,
        HeadingLevel::H4 => HeadingLevel::H5,
        _ => HeadingLevel::H6,
    }
}

fn sanitizer() -> Builder<'static> {
    let mut b = Builder::empty();
    let tags: HashSet<&str> = [
        "p",
        "br",
        "em",
        "strong",
        "del",
        "code",
        "pre",
        "blockquote",
        "ul",
        "ol",
        "li",
        "a",
        "h3",
        "h4",
        "h5",
        "h6",
        "hr",
        "table",
        "thead",
        "tbody",
        "tr",
        "th",
        "td",
        "input",
    ]
    .into_iter()
    .collect();
    b.tags(tags);
    b.add_tag_attributes("a", ["href"]);
    b.add_tag_attributes("code", ["class"]);
    b.add_tag_attributes("ol", ["start"]);
    b.add_tag_attributes("input", ["type", "checked", "disabled"]);
    b.url_schemes(["http", "https", "mailto"].into_iter().collect());
    b.url_relative(UrlRelative::Deny);
    b.link_rel(Some("nofollow ugc noopener"));
    b.attribute_filter(|el, attr, value| match (el, attr) {
        ("code", "class") => {
            let ok = value.strip_prefix("language-").is_some_and(|l| {
                !l.is_empty()
                    && l.len() <= 24
                    && l.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "+-_#".contains(c))
            });
            ok.then(|| value.into())
        }
        ("input", "type") => (value == "checkbox").then(|| value.into()),
        ("input", "disabled") => Some("".into()),
        ("ol", "start") => value.parse::<u32>().ok().map(|n| n.to_string().into()),
        _ => Some(value.into()),
    });
    b
}

/// Render untrusted Markdown to HTML that is safe to embed in a page.
pub fn render(src: &str) -> String {
    let events = Parser::new_ext(src, options()).map(|e| match e {
        // No raw HTML: show it as text.
        Event::Html(t) | Event::InlineHtml(t) => Event::Text(t),
        // No remote images: an image becomes a plain link to it.
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }),
        Event::End(TagEnd::Image) => Event::End(TagEnd::Link),
        Event::Start(Tag::Heading {
            level,
            id,
            classes,
            attrs,
        }) => Event::Start(Tag::Heading {
            level: demote(level),
            id,
            classes,
            attrs,
        }),
        Event::End(TagEnd::Heading(l)) => Event::End(TagEnd::Heading(demote(l))),
        e => e,
    });
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events);
    sanitizer().clean(&html).to_string()
}

/// Number of links in a message (inline, reference and bare autolinks).
pub fn link_count(src: &str) -> usize {
    let mut n = 0;
    let mut depth = 0;
    for e in Parser::new_ext(src, options()) {
        match e {
            Event::Start(Tag::Link { .. } | Tag::Image { .. }) => {
                n += 1;
                depth += 1;
            }
            Event::End(TagEnd::Link | TagEnd::Image) => depth -= 1,
            // Bare URLs in text count too, but not the visible text of a real link.
            Event::Text(t) if depth == 0 => {
                n += t.matches("http://").count() + t.matches("https://").count();
            }
            _ => {}
        }
    }
    n
}

/// Plain-text excerpt for meta descriptions: Markdown stripped, whitespace collapsed.
pub fn excerpt(src: &str, max: usize) -> String {
    let mut out = String::new();
    for e in Parser::new_ext(src, options()) {
        match e {
            Event::Text(t) | Event::Code(t) => {
                out.push_str(&t);
                out.push(' ');
            }
            Event::SoftBreak | Event::HardBreak | Event::End(TagEnd::Paragraph) => out.push(' '),
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_) | CodeBlockKind::Indented)) => {}
            _ => {}
        }
        if out.chars().count() > max + 40 {
            break;
        }
    }
    crate::util::truncate_chars(&crate::util::clean_line(&out), max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(src: &str) -> String {
        render(src)
    }

    /// True if any real tag in `html` carries an `on...=` attribute.
    fn tag_has_handler(html: &str) -> bool {
        html.split('<').skip(1).any(|t| {
            let tag = t.split('>').next().unwrap_or("").to_lowercase();
            tag.split_whitespace()
                .skip(1)
                .any(|a| a.starts_with("on") && a.contains('='))
        })
    }

    #[test]
    fn renders_basic_markdown() {
        let h = clean(
            "# Title\n\nSome **bold** and `code`.\n\n- a\n- b\n\n```rust\nfn main() {}\n```\n",
        );
        assert!(h.contains("<h3>Title</h3>"), "{h}");
        assert!(h.contains("<strong>bold</strong>"));
        assert!(h.contains("<li>a</li>"));
        assert!(h.contains("<code class=\"language-rust\">"), "{h}");
    }

    #[test]
    fn script_tags_and_handlers_are_inert() {
        for src in [
            "<script>alert(1)</script>",
            "hello <img src=x onerror=alert(1)>",
            "<a href=\"x\" onclick=\"alert(1)\">x</a>",
            "<div onmouseover=alert(1)>hi</div>",
            "<iframe src=\"https://evil.test\"></iframe>",
            "<style>body{display:none}</style>",
            "<svg onload=alert(1)></svg>",
            "<svg><script>alert(1)</script></svg>",
        ] {
            let h = clean(src).to_lowercase();
            assert!(
                !h.contains("<script") && !h.contains("<svg") && !h.contains("<iframe"),
                "{src} -> {h}"
            );
            assert!(
                !h.contains("<img") && !h.contains("<style") && !h.contains("<div"),
                "{src} -> {h}"
            );
            assert!(!tag_has_handler(&h), "{src} -> {h}");
        }
    }

    #[test]
    fn dangerous_link_schemes_are_removed() {
        for src in [
            "[x](javascript:alert(1))",
            "[x](JaVaScRiPt:alert(1))",
            "[x](data:text/html;base64,PHNjcmlwdD4=)",
            "[x](vbscript:msgbox)",
            "[x](  javascript:alert(1))",
            "<https://ok.test>[x](javascript:alert(1))",
        ] {
            let h = clean(src).to_lowercase();
            assert!(
                !h.contains("javascript:") && !h.contains("data:") && !h.contains("vbscript:"),
                "{src} -> {h}"
            );
        }
        // Raw HTML is only text, never a link.
        let h = clean("<a href=\"javascript:alert(1)\">x</a>");
        assert!(!h.contains("<a "), "{h}");
        let h = clean("[x](/relative/path)").to_lowercase();
        assert!(
            !h.contains("href=\"/relative"),
            "relative links are denied: {h}"
        );
    }

    #[test]
    fn images_become_links_and_data_urls_vanish() {
        let h = clean("![alt text](https://cdn.example.test/a.png)");
        assert!(!h.contains("<img"), "{h}");
        assert!(h.contains("href=\"https://cdn.example.test/a.png\""), "{h}");
        let d = clean("![x](data:image/svg+xml;base64,PHN2Zz4=)");
        assert!(!d.contains("data:") && !d.contains("<img"), "{d}");
        let s = clean("![x](https://example.test/a.svg)");
        assert!(!s.contains("<img") && !s.contains("<svg"));
    }

    #[test]
    fn user_links_are_nofollow_ugc() {
        let h = clean("[site](https://example.test) and https://auto.example.test");
        assert!(h.contains("rel=\"nofollow ugc noopener\""), "{h}");
        assert_eq!(
            h.matches("rel=\"nofollow ugc noopener\"").count(),
            h.matches("<a ").count()
        );
    }

    #[test]
    fn class_attribute_is_restricted_to_code_languages() {
        let h = clean("<code class=\"evil\">x</code>\n\n```js\n1\n```");
        assert!(!h.contains("<code class=\"evil\""), "{h}");
        assert!(h.contains("language-js"));
    }

    #[test]
    fn counts_links_and_excerpts() {
        assert_eq!(link_count("no links here"), 0);
        assert_eq!(
            link_count("[a](https://a.test) https://b.test ![i](https://c.test/x.png)"),
            3
        );
        let e = excerpt("# Hi\n\nSome *text* with `code`.\n\n```\nblock\n```\n", 200);
        assert!(e.starts_with("Hi Some text with code"), "{e}");
        assert!(excerpt(&"word ".repeat(200), 50).chars().count() <= 50);
    }
}
