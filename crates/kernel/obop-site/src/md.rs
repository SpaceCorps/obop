//! Rich text of pages, posts and products: a restricted Markdown subset.
//!
//! `pulldown-cmark` parses, raw HTML and images are neutralised in the event stream, and
//! `ammonia` has the last word with a strict allow-list. The same events also produce a
//! sanitised Markdown rendering for agents (`?format=md`), so a hostile body cannot smuggle raw
//! HTML or a `javascript:` link into either form.

use ammonia::{Builder, UrlRelative};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::collections::HashSet;

fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH
}

/// The page title is the only `<h1>`: body headings start at `<h2>`. A body that uses `#`
/// is moved down one level as a whole, one that starts at `##` is left alone.
fn demote(l: HeadingLevel, shift: bool) -> HeadingLevel {
    match (l, shift) {
        (HeadingLevel::H1, _) => HeadingLevel::H2,
        (l, false) => l,
        (HeadingLevel::H2, true) => HeadingLevel::H3,
        (HeadingLevel::H3, true) => HeadingLevel::H4,
        (HeadingLevel::H4, true) => HeadingLevel::H5,
        (_, true) => HeadingLevel::H6,
    }
}

fn uses_h1(src: &str) -> bool {
    Parser::new_ext(src, options()).any(|e| {
        matches!(
            e,
            Event::Start(Tag::Heading {
                level: HeadingLevel::H1,
                ..
            })
        )
    })
}

/// A link target the site will emit: http, https, mailto, or a path on this site.
pub fn safe_url(u: &str) -> bool {
    let u = u.trim();
    if u.is_empty() || u.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    if u.starts_with("//") {
        return false;
    }
    if u.starts_with('/') || u.starts_with('#') {
        return true;
    }
    let lower = u.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")
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
        "h2",
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
    ]
    .into_iter()
    .collect();
    b.tags(tags);
    b.add_tag_attributes("a", ["href"]);
    b.add_tag_attributes("code", ["class"]);
    b.add_tag_attributes("ol", ["start"]);
    b.url_schemes(["http", "https", "mailto"].into_iter().collect());
    // Relative links (`/shop`, `#top`) are fine for content the owner writes; the filter below
    // drops protocol-relative ones.
    b.url_relative(UrlRelative::PassThrough);
    b.link_rel(Some("noopener"));
    b.attribute_filter(|el, attr, value| match (el, attr) {
        ("a", "href") => safe_url(value).then(|| value.into()),
        ("code", "class") => {
            let ok = value.strip_prefix("language-").is_some_and(|l| {
                !l.is_empty()
                    && l.len() <= 24
                    && l.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "+-_#".contains(c))
            });
            ok.then(|| value.into())
        }
        ("ol", "start") => value.parse::<u32>().ok().map(|n| n.to_string().into()),
        _ => Some(value.into()),
    });
    b
}

/// Render owner-written Markdown to HTML that is safe to embed in a page.
pub fn render(src: &str) -> String {
    let shift = uses_h1(src);
    let events = Parser::new_ext(src, options()).map(|e| match e {
        // No raw HTML: it is shown as text.
        Event::Html(t) | Event::InlineHtml(t) => Event::Text(t),
        // The data model has no images yet, and a remote image would be a tracking pixel: an
        // image becomes a plain link.
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
            level: demote(level, shift),
            id,
            classes,
            attrs,
        }),
        Event::End(TagEnd::Heading(l)) => Event::End(TagEnd::Heading(demote(l, shift))),
        e => e,
    });
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events);
    sanitizer().clean(&html).to_string()
}

/// Sanitised Markdown for agents: the same restricted subset, re-written from the parse so that
/// nothing the HTML form removes survives here either.
pub fn to_markdown(src: &str) -> String {
    let mut out = String::new();
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut quote = 0usize;
    let mut link_ok: Vec<bool> = Vec::new();
    let mut links: Vec<String> = Vec::new();
    let mut in_code = false;
    let shift = uses_h1(src);
    let pad = |out: &mut String, quote: usize| {
        if !out.ends_with("\n\n") && !out.is_empty() {
            while !out.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }
        let _ = quote;
    };
    for e in Parser::new_ext(src, options()) {
        match e {
            Event::Start(Tag::Heading { level, .. }) => {
                pad(&mut out, quote);
                let n = match demote(level, shift) {
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    _ => 6,
                };
                out.push_str(&"#".repeat(n));
                out.push(' ');
            }
            Event::End(TagEnd::Heading(_)) => out.push_str("\n\n"),
            Event::Start(Tag::Paragraph) => {
                if lists.is_empty() {
                    pad(&mut out, quote);
                }
                if quote > 0 {
                    out.push_str("> ");
                }
            }
            Event::End(TagEnd::Paragraph) => {
                out.push_str(if lists.is_empty() { "\n\n" } else { "\n" })
            }
            Event::Start(Tag::BlockQuote(_)) => quote += 1,
            Event::End(TagEnd::BlockQuote(_)) => quote = quote.saturating_sub(1),
            Event::Start(Tag::List(first)) => lists.push(first),
            Event::End(TagEnd::List(_)) => {
                lists.pop();
                if lists.is_empty() {
                    out.push('\n');
                }
            }
            Event::Start(Tag::Item) => {
                if !out.ends_with('\n') && !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&"  ".repeat(lists.len().saturating_sub(1)));
                match lists.last_mut() {
                    Some(Some(n)) => {
                        out.push_str(&format!("{n}. "));
                        *n += 1;
                    }
                    _ => out.push_str("- "),
                }
            }
            Event::End(TagEnd::Item) => {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
            }
            Event::Start(Tag::Emphasis) | Event::End(TagEnd::Emphasis) => out.push('*'),
            Event::Start(Tag::Strong) | Event::End(TagEnd::Strong) => out.push_str("**"),
            Event::Start(Tag::Strikethrough) | Event::End(TagEnd::Strikethrough) => {
                out.push_str("~~")
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                let ok = safe_url(&dest_url);
                link_ok.push(ok);
                if ok {
                    out.push('[');
                }
                // The target is written after the text, so keep it on a stack.
                links.push(dest_url.to_string());
            }
            Event::End(TagEnd::Link) => {
                let url = links.pop().unwrap_or_default();
                if link_ok.pop().unwrap_or(false) {
                    out.push_str(&format!(
                        "]({})",
                        url.replace(')', "%29").replace(' ', "%20")
                    ));
                }
            }
            // An image is its description.
            Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image) => {}
            Event::Start(Tag::CodeBlock(kind)) => {
                pad(&mut out, quote);
                in_code = true;
                let lang = match kind {
                    CodeBlockKind::Fenced(l) => l
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric() || "+-_#".contains(*c))
                        .take(24)
                        .collect::<String>(),
                    CodeBlockKind::Indented => String::new(),
                };
                out.push_str(&format!("```{lang}\n"));
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code = false;
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("```\n\n");
            }
            Event::Code(t) => out.push_str(&format!("`{}`", t.replace('`', "'"))),
            Event::Text(t) => {
                if in_code {
                    out.push_str(&t.replace("```", "'''"));
                } else {
                    out.push_str(&escape_md(&t));
                }
            }
            // Raw HTML is shown as text.
            Event::Html(t) | Event::InlineHtml(t) => out.push_str(&escape_md(&t)),
            Event::SoftBreak => out.push('\n'),
            Event::HardBreak => out.push_str("  \n"),
            Event::Rule => {
                pad(&mut out, quote);
                out.push_str("---\n\n");
            }
            Event::Start(Tag::TableCell) => out.push_str("| "),
            Event::End(TagEnd::TableCell) => out.push(' '),
            Event::End(TagEnd::TableRow | TagEnd::TableHead) => out.push_str("|\n"),
            Event::End(TagEnd::Table) => out.push('\n'),
            _ => {}
        }
    }
    let mut s = out.trim().to_string();
    s.push('\n');
    s
}

/// Neutralise characters that could start HTML in a Markdown reader.
fn escape_md(s: &str) -> String {
    s.replace('<', "&lt;").replace('>', "&gt;")
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

    fn tag_has_handler(html: &str) -> bool {
        html.split('<').skip(1).any(|t| {
            let tag = t.split('>').next().unwrap_or("").to_lowercase();
            tag.split_whitespace()
                .skip(1)
                .any(|a| a.starts_with("on") && a.contains('='))
        })
    }

    #[test]
    fn renders_the_supported_subset() {
        let h = render("# Title\n\nSome **bold** and `code`.\n\n- a\n- b\n\n> quote\n");
        assert!(h.contains("<h2>Title</h2>"), "{h}");
        assert!(h.contains("<strong>bold</strong>") && h.contains("<li>a</li>"));
        assert!(h.contains("<blockquote>"), "{h}");
        assert!(!h.contains("<h1"), "the page title is the only h1");
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
            "<form action=\"https://evil.test\"><input name=x></form>",
        ] {
            let h = render(src).to_lowercase();
            for bad in [
                "<script", "<svg", "<iframe", "<img", "<style", "<div", "<form", "<input",
            ] {
                assert!(!h.contains(bad), "{src} -> {h}");
            }
            assert!(!tag_has_handler(&h), "{src} -> {h}");
            let m = to_markdown(src);
            assert!(!m.contains('<'), "{src} -> {m}");
        }
    }

    #[test]
    fn dangerous_link_targets_are_removed_from_html_and_markdown() {
        for src in [
            "[x](javascript:alert(1))",
            "[x](JaVaScRiPt:alert(1))",
            "[x](data:text/html;base64,PHNjcmlwdD4=)",
            "[x](vbscript:msgbox)",
            "[x](//evil.test/path)",
            "[x](  javascript:alert(1))",
        ] {
            let h = render(src).to_lowercase();
            assert!(
                !h.contains("javascript:") && !h.contains("data:") && !h.contains("vbscript:"),
                "{src} -> {h}"
            );
            assert!(!h.contains("href=\"//"), "{src} -> {h}");
            let m = to_markdown(src).to_lowercase();
            assert!(
                !m.contains("javascript:")
                    && !m.contains("data:")
                    && !m.contains("vbscript:")
                    && !m.contains("](//"),
                "{src} -> {m}"
            );
        }
    }

    #[test]
    fn owner_links_to_the_site_and_the_web_survive() {
        let h = render(
            "[shop](/shop) [web](https://example.test/a) [mail](mailto:a@b.test) [top](#top)",
        );
        for want in [
            "href=\"/shop\"",
            "href=\"https://example.test/a\"",
            "href=\"mailto:a@b.test\"",
            "href=\"#top\"",
        ] {
            assert!(h.contains(want), "{want} in {h}");
        }
        assert_eq!(
            h.matches("rel=\"noopener\"").count(),
            h.matches("<a ").count()
        );
        let m = to_markdown("[shop](/shop) and [bad](javascript:x)");
        assert!(
            m.contains("[shop](/shop)") && !m.contains("javascript"),
            "{m}"
        );
        assert!(m.contains("bad"), "the text of a refused link stays: {m}");
    }

    #[test]
    fn images_become_links() {
        let h = render("![alt text](https://cdn.example.test/a.png)");
        assert!(
            !h.contains("<img") && h.contains("href=\"https://cdn.example.test/a.png\""),
            "{h}"
        );
        let d = render("![x](data:image/svg+xml;base64,PHN2Zz4=)");
        assert!(!d.contains("data:") && !d.contains("<img"), "{d}");
        assert!(!to_markdown("![alt](https://x.test/a.png)").contains("!["));
    }

    #[test]
    fn markdown_output_keeps_structure() {
        let m = to_markdown(
            "# Title\n\nHello **world**.\n\n1. one\n2. two\n\n- a\n- b\n\n```rust\nlet x = 1;\n```\n",
        );
        assert!(m.starts_with("## Title\n"), "{m}");
        assert!(m.contains("Hello **world**."), "{m}");
        assert!(m.contains("1. one\n2. two"), "{m}");
        assert!(m.contains("- a\n- b"), "{m}");
        assert!(m.contains("```rust\nlet x = 1;\n```"), "{m}");
    }

    #[test]
    fn excerpts_strip_markup() {
        let e = excerpt("# Hi\n\nSome *text* with `code`.\n", 200);
        assert!(e.starts_with("Hi Some text with code"), "{e}");
        assert!(excerpt(&"word ".repeat(200), 50).chars().count() <= 50);
    }
}
