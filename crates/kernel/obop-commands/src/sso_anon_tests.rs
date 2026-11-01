//! The sorts of anonymous line of the Microsoft sign-in (D91 Decision 9) and the hourly budget
//! each is counted against. The budget of a sort is a decision of the record (a begun sign-in, a
//! callback that belongs to no sign-in, or a sign-in that went wrong), so it is pinned for every
//! sort here, not only through the call sites that happen to have a test.

use crate::{AnonGroup, AnonLine};

/// A place for every sort, from a `match` without a wildcard: a sort that is added does not
/// compile until it is placed here, and the test below then asks for it in `AnonLine::ALL`.
fn place(line: AnonLine) -> usize {
    match line {
        AnonLine::Start => 0,
        AnonLine::NoFlow => 1,
        AnonLine::Settings => 2,
        AnonLine::Discovery => 3,
        AnonLine::Browser => 4,
        AnonLine::Provider => 5,
        AnonLine::NoCode => 6,
        AnonLine::SwitchedOff => 7,
        AnonLine::Busy => 8,
        AnonLine::TokenUnreachable => 9,
        AnonLine::TokenUnwell => 10,
        AnonLine::TokenRefused => 11,
        AnonLine::KeySet => 12,
        AnonLine::KeySetUnusable => 13,
        AnonLine::IdToken => 14,
        AnonLine::Begin => 15,
    }
}

/// Every sort and the budget it was given: the two named ones, and the fourteen failures (no_code
/// among them: a redirect with no code is a sign-in that went wrong, not a callback with no flow).
const DECIDED: [(AnonLine, AnonGroup); 16] = [
    (AnonLine::Start, AnonGroup::Start),
    (AnonLine::NoFlow, AnonGroup::NoFlow),
    (AnonLine::Settings, AnonGroup::Failed),
    (AnonLine::Discovery, AnonGroup::Failed),
    (AnonLine::Browser, AnonGroup::Failed),
    (AnonLine::Provider, AnonGroup::Failed),
    (AnonLine::NoCode, AnonGroup::Failed),
    (AnonLine::SwitchedOff, AnonGroup::Failed),
    (AnonLine::Busy, AnonGroup::Failed),
    (AnonLine::TokenUnreachable, AnonGroup::Failed),
    (AnonLine::TokenUnwell, AnonGroup::Failed),
    (AnonLine::TokenRefused, AnonGroup::Failed),
    (AnonLine::KeySet, AnonGroup::Failed),
    (AnonLine::KeySetUnusable, AnonGroup::Failed),
    (AnonLine::IdToken, AnonGroup::Failed),
    (AnonLine::Begin, AnonGroup::Failed),
];

#[test]
fn the_list_of_sorts_holds_each_sort_once_and_each_sort_has_the_budget_that_was_decided() {
    // The list that a test forgets the counters by holds every sort, once.
    let mut listed = [0u8; 16];
    for line in AnonLine::ALL {
        listed[place(line)] += 1;
    }
    assert_eq!(
        listed, [1; 16],
        "AnonLine::ALL lists a sort twice or leaves one out"
    );
    // The decisions above name every sort once, too.
    let mut named = [0u8; 16];
    for (line, _) in DECIDED {
        named[place(line)] += 1;
    }
    assert_eq!(
        named, [1; 16],
        "the table of decisions misses a sort or names one twice"
    );
    // And the code agrees with them, sort by sort.
    for (line, group) in DECIDED {
        assert_eq!(line.group(), group, "the budget of {}", line.word());
    }
}

#[test]
fn the_words_that_key_the_sorts_and_the_budgets_differ() {
    let mut words: Vec<&str> = AnonLine::ALL.iter().map(|l| l.word()).collect();
    words.sort_unstable();
    let before = words.len();
    words.dedup();
    assert_eq!(
        words.len(),
        before,
        "two sorts share a word, so they would share a slot"
    );
    let mut groups: Vec<&str> = AnonGroup::ALL.iter().map(|g| g.word()).collect();
    groups.sort_unstable();
    groups.dedup();
    assert_eq!(
        groups.len(),
        AnonGroup::ALL.len(),
        "two budgets share a word"
    );
    // Every budget has a sort, and a sort belongs to a budget of the list.
    for group in AnonGroup::ALL {
        assert!(
            AnonLine::ALL.iter().any(|l| l.group() == group),
            "no sort is counted against the budget {}",
            group.word()
        );
    }
    for line in AnonLine::ALL {
        assert!(AnonGroup::ALL.contains(&line.group()), "{}", line.word());
    }
}
