//! The language of a REST request, for the texts of a refusal that a package author wrote and
//! translated (D52): the message of an assertion.
//!
//! The surface that serves a request opens a scope for the duration of the call ([`scope`]), the
//! engine reads the language where it words a refusal ([`language`]), and notes which assertion
//! it was ([`note_assertion`]) so that `errors::classify` can name its `code` and `assert_key`.
//! A thread-local carries both, as the write context and the fault scope do: the handlers, guards
//! and wrappers between the surface and the engine need no new argument, and the engine depends
//! on no HTTP type. The scope belongs to the thread that opened it and is not `Send`.
//!
//! Only REST opens a scope. A call from MCP or the command line, or a handler called directly,
//! runs with no scope: no language, nothing noted, the message of the manifest, exactly as before.

use std::cell::RefCell;
use std::marker::PhantomData;

/// The language a manifest is written in (D52: English is the source). A request for it is
/// answered with the manifest's own texts.
pub const MANIFEST_LANGUAGE: &str = "en";

/// The languages obop ships (the same seven the site, the forum and the sign-in pages speak).
pub const LANGUAGES: [&str; 7] = ["en", "de", "fr", "pl", "fi", "sv", "es"];

/// How many assertion refusals one scope remembers; the newest are kept.
const MAX_NOTED: usize = 8;

/// One assertion that refused in this scope: the whole text of the refusal, and its key.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Noted {
    text: String,
    key: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct State {
    lang: Option<&'static str>,
    noted: Vec<Noted>,
}

thread_local! {
    static CURRENT: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Ends the scope of [`scope`] when it is dropped, also when the thread unwinds, and restores the
/// scope that was open before it (scopes nest).
pub struct Scope {
    previous: Option<State>,
    /// A scope belongs to the thread that opened it.
    _not_send: PhantomData<*const ()>,
}

impl Drop for Scope {
    fn drop(&mut self) {
        let previous = self.previous.take();
        CURRENT.with(|c| *c.borrow_mut() = previous);
    }
}

/// Open the scope of one request that asked for `lang` (`None`: it asked for none). `lang` is one
/// of [`LANGUAGES`]; the surface that parsed the request has checked it.
pub fn scope(lang: Option<&'static str>) -> Scope {
    let previous = CURRENT.with(|c| {
        c.borrow_mut().replace(State {
            lang,
            noted: Vec::new(),
        })
    });
    Scope {
        previous,
        _not_send: PhantomData,
    }
}

/// The language the request in scope asked for, if it asked for one and it is not the language
/// of the manifest: the language a refusal is translated into.
pub fn language() -> Option<&'static str> {
    CURRENT
        .with(|c| c.borrow().as_ref().and_then(|s| s.lang))
        .filter(|l| *l != MANIFEST_LANGUAGE)
}

/// Whether a request scope is open on this thread.
pub fn in_scope() -> bool {
    CURRENT.with(|c| c.borrow().is_some())
}

/// Remember that the assertion `key` refused with `text`. Does nothing outside a scope.
pub(crate) fn note_assertion(text: &str, key: Option<&str>) {
    CURRENT.with(|c| {
        if let Some(s) = c.borrow_mut().as_mut() {
            if s.noted.len() == MAX_NOTED {
                s.noted.remove(0);
            }
            s.noted.push(Noted {
                text: text.to_string(),
                key: key.map(String::from),
            });
        }
    });
}

/// The assertion behind a refusal `message`: when the text of an assertion that refused in this
/// scope is part of `message` (the engine wraps the text of an action it started), the key of
/// the newest such assertion. `None`: no assertion refused with that text, or no scope is open.
pub(crate) fn assertion_behind(message: &str) -> Option<Option<String>> {
    CURRENT.with(|c| {
        c.borrow().as_ref().and_then(|s| {
            s.noted
                .iter()
                .rev()
                .find(|n| message.contains(&n.text))
                .map(|n| n.key.clone())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_scope_means_no_language_and_nothing_is_remembered() {
        assert!(!in_scope());
        assert_eq!(language(), None);
        note_assertion("x", Some("k"));
        assert_eq!(assertion_behind("x"), None);
    }

    #[test]
    fn the_manifests_own_language_is_no_language_to_translate_into() {
        let _s = scope(Some("en"));
        assert!(in_scope());
        assert_eq!(language(), None);
        let _s2 = scope(Some("sv"));
        assert_eq!(language(), Some("sv"));
    }

    #[test]
    fn scopes_nest_and_end_with_their_guard() {
        {
            let _outer = scope(Some("de"));
            note_assertion("outer", None);
            {
                let _inner = scope(Some("sv"));
                assert_eq!(language(), Some("sv"));
                // A new scope starts with nothing remembered.
                assert_eq!(assertion_behind("outer"), None);
            }
            assert_eq!(language(), Some("de"));
            assert_eq!(assertion_behind("outer"), Some(None));
        }
        assert!(!in_scope());
        assert_eq!(language(), None);
    }

    #[test]
    fn a_scope_ends_when_the_thread_unwinds() {
        let r = std::panic::catch_unwind(|| {
            let _s = scope(Some("fi"));
            panic!("boom");
        });
        assert!(r.is_err());
        assert!(!in_scope());
    }

    #[test]
    fn an_assertion_is_found_by_its_text_inside_a_longer_refusal() {
        let _s = scope(None);
        note_assertion("A (assertion a failed)", Some("first"));
        note_assertion("B (assertion b failed)", None);
        assert_eq!(
            assertion_behind("A (assertion a failed)"),
            Some(Some("first".into()))
        );
        assert_eq!(
            assertion_behind(
                "x.y: effect 1 (set_state) failed, nothing was changed: B (assertion b failed)"
            ),
            Some(None)
        );
        assert_eq!(assertion_behind("something else"), None);
        // Only the newest are kept.
        for i in 0..20 {
            note_assertion(&format!("n{i:02} (assertion e failed)"), Some("k"));
        }
        assert_eq!(assertion_behind("A (assertion a failed)"), None);
        assert_eq!(
            assertion_behind("n19 (assertion e failed)"),
            Some(Some("k".into()))
        );
        assert_eq!(
            assertion_behind("n12 (assertion e failed)"),
            Some(Some("k".into()))
        );
        assert_eq!(assertion_behind("n11 (assertion e failed)"), None);
    }

    #[test]
    fn the_scope_is_per_thread() {
        let _s = scope(Some("pl"));
        std::thread::spawn(|| {
            assert!(!in_scope());
            assert_eq!(language(), None);
        })
        .join()
        .unwrap();
        assert_eq!(language(), Some("pl"));
    }
}
