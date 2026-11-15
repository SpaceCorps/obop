//! The actions an `update` implies (D48), run with the update as one unit of work.
//!
//! An `update` that changes a governed field is an implicit run of the declared action for that
//! change. The other fields of the call are edits of the record as it was, so the generated
//! `update` checks and writes them into its unit first and then, without committing, offers the
//! unit to each action in turn ([`run`]). The generic handler of an action takes it up
//! ([`adopt`]), runs on the record as edited, and commits the edits and its own writes in one
//! storage transaction. A refusal of any part drops the unit: nothing is stored, no number of a
//! sequence is taken, and the caller gets that refusal.
//!
//! Each action still runs through its own wrapper (`Dispatcher::around`), which may do what
//! cannot be taken back once the handler it wraps has answered (return money, D62). So an action
//! with a wrapper commits the unit before it answers, and so does the last action of the call; an
//! action without one that is followed by another hands the unit on, and the last one commits for
//! all. An action that stores its move before it answers (it has a wrapper, or its handler was
//! written by hand) can therefore only be the last one: an update that would run another action
//! after it is refused before anything is written, because a refusal of the later action could
//! not take the earlier move back.
//!
//! What is left outside the one transaction, and where the store can end up with a part:
//!
//! - An action whose handler was written by hand writes the store itself and cannot take the
//!   unit up. It runs after every check of the edits has passed, and the `update` commits the
//!   edits when it has answered: two transactions. A refusal of the action stores nothing. If
//!   the commit of the edits then fails (a fault of the store, or a uniqueness race that only
//!   the index decides), the move is stored without the edits.
//! - A wrapper that refuses after its handler has answered: the edits and the move are stored,
//!   as the move is when the action is called by itself, and the caller gets the refusal. The
//!   files the edits name are held all the same ([`Refused::stored`]).
//!
//! A commit of the joined unit that fails takes the edits of the update with it. It ends the
//! update with that refusal, whatever the wrapper of the action makes of it, and the action
//! cannot be run again in the same call ([`lost`]).
//!
//! A thread-local carries the steps and the unit, as the write context, the fault scope and the
//! language of a request are carried: the handlers and wrappers between the dispatcher and the
//! engine take a `&Value` and need no new argument. A scope belongs to one run of one command
//! (`Dispatcher::run` opens it), so a command that a wrapper calls through the dispatcher never
//! sees the steps or the unit of the call around it.

use super::unit::{Held, R};
use crate::Value;
use std::cell::RefCell;
use std::marker::PhantomData;

/// One action an update implies, ready to run as the dispatcher runs an action: through its
/// wrapper, as the principal of the call, with the input the guard built for it.
pub(crate) struct Step {
    pub run: Box<dyn Fn() -> R<Value>>,
    /// `model.command` of the action, for a refusal that names it.
    pub command: String,
    /// The action can hand the unit on to the action after it: its handler is the generic one
    /// and it has no wrapper. Any other action stores its move before it answers.
    pub hands_on: bool,
}

/// The unit of an update on offer to the action that runs now: for the record `model` `id`.
struct Offer {
    model: String,
    id: String,
    held: Held,
}

/// The unit of an update whose commit failed in the action of the record `model` `id`.
struct Lost {
    model: String,
    id: String,
    why: String,
}

#[derive(Default)]
struct Scope {
    /// The actions of this call, until the handler of the update takes them.
    steps: Vec<Step>,
    offer: Option<Offer>,
    /// The action that runs now is followed by another: it hands the unit on.
    hand_on: bool,
    /// An action committed the unit: the edits of the update are stored with its move.
    stored: bool,
    lost: Option<Lost>,
}

thread_local! {
    static CURRENT: RefCell<Option<Scope>> = const { RefCell::new(None) };
}

/// Ends the scope of [`open`] when it is dropped, also when the thread unwinds, and restores the
/// scope that was open before it.
pub(crate) struct Open {
    previous: Option<Scope>,
    /// A scope belongs to the thread that opened it.
    _not_send: PhantomData<*const ()>,
}

impl Drop for Open {
    fn drop(&mut self) {
        let previous = self.previous.take();
        CURRENT.with(|c| *c.borrow_mut() = previous);
    }
}

/// Open the scope of one run of a command, with the actions its update implies (none for every
/// other call, whose handlers then see nothing of a scope around it).
pub(crate) fn open(steps: Vec<Step>) -> Open {
    let scope = Scope {
        steps,
        ..Scope::default()
    };
    Open {
        previous: CURRENT.with(|c| c.borrow_mut().replace(scope)),
        _not_send: PhantomData,
    }
}

/// The actions the update of this call implies. They are handed out once: to the generated
/// `update`, which runs them inside its unit of work ([`run`]).
pub(crate) fn take_steps() -> Vec<Step> {
    CURRENT.with(|c| {
        c.borrow_mut()
            .as_mut()
            .map(|s| std::mem::take(&mut s.steps))
            .unwrap_or_default()
    })
}

/// What the actions of an update left when all of them answered.
pub(crate) struct Ran {
    /// What no action committed, for the update to commit.
    pub left: Option<Held>,
    /// What each action answered.
    pub answers: Vec<Value>,
}

/// The refusal that ended the actions of an update.
pub(crate) struct Refused {
    pub why: String,
    /// An action had committed the unit when the refusal came (its wrapper refused after its
    /// handler had answered): the edits of the update are stored with the move.
    pub stored: bool,
}

/// Run the actions an update of the record `model` `id` implies, each with the unit `held` on
/// offer. At the first refusal the unit is dropped and the refusal returned.
pub(crate) fn run(steps: &[Step], model: &str, id: &str, held: Held) -> Result<Ran, Refused> {
    // Every action but the last must hand the unit on. One that stores its move before it
    // answers would leave that move stored when an action after it refused, so such a call is
    // refused here, before any action has run.
    let last = steps.len().saturating_sub(1);
    if let Some(i) = steps[..last].iter().position(|s| !s.hands_on) {
        let (first, next) = (&steps[i].command, &steps[i + 1].command);
        return Err(Refused {
            why: format!(
                "one update cannot run {first} and {next} together: {first} stores its change \
                 before it answers, so the update could not be refused as a whole after it; \
                 send the changes in separate calls"
            ),
            stored: false,
        });
    }
    let mut held = Some(held);
    let mut answers = Vec::with_capacity(steps.len());
    for (i, step) in steps.iter().enumerate() {
        // Nothing of the scope is borrowed while the action runs: it reads the scope itself.
        held = CURRENT.with(|c| match c.borrow_mut().as_mut() {
            Some(s) => {
                s.hand_on = i < last;
                s.offer = held.take().map(|held| Offer {
                    model: model.to_string(),
                    id: id.to_string(),
                    held,
                });
                None
            }
            // No scope is open (the steps came from one, so this does not happen): the unit
            // stays with the update, which commits it after the actions.
            None => held.take(),
        });
        let answer = (step.run)();
        let (back, stored, lost) = CURRENT.with(|c| match c.borrow_mut().as_mut() {
            Some(s) => {
                s.hand_on = false;
                (
                    s.offer.take().map(|o| o.held),
                    s.stored,
                    s.lost.take().map(|l| l.why),
                )
            }
            None => (None, false, None),
        });
        if let Some(back) = back {
            held = Some(back);
        }
        // The commit of the unit failed and the edits went with it: the update ends with that
        // refusal, also when the wrapper of the action answered for it.
        if let Some(why) = lost {
            return Err(Refused { why, stored: false });
        }
        match answer {
            Ok(answer) => answers.push(answer),
            Err(why) => return Err(Refused { why, stored }),
        }
    }
    Ok(Ran {
        left: held,
        answers,
    })
}

/// A unit taken up by an action, and what the action does with it when it is done.
pub(crate) struct Adopted {
    pub held: Held,
    /// Hand the unit on ([`give_back`]) and do not commit it: another action follows.
    pub hand_on: bool,
}

/// For the generic handler of an action on the record `model` `id`: the unit of the update that
/// implies it, when there is one on offer for that record. The action goes on in that unit.
///
/// Refused when the commit of that unit has failed in this call ([`lost`]): a wrapper that runs
/// its handler again would otherwise store the move without the edits of the update.
pub(crate) fn adopt(model: &str, id: &str) -> R<Option<Adopted>> {
    CURRENT.with(|c| {
        let mut current = c.borrow_mut();
        let Some(scope) = current.as_mut() else {
            return Ok(None);
        };
        if let Some(lost) = scope
            .lost
            .as_ref()
            .filter(|l| l.model == model && l.id == id)
        {
            return Err(lost.why.clone());
        }
        if !scope
            .offer
            .as_ref()
            .is_some_and(|o| o.model == model && o.id == id)
        {
            return Ok(None);
        }
        Ok(scope.offer.take().map(|o| Adopted {
            held: o.held,
            hand_on: scope.hand_on,
        }))
    })
}

/// Give a unit that was taken up back to the update: with the action's writes, when another
/// action follows, or as it came, when the action was refused (its wrapper decides what the
/// refusal means for the call).
pub(crate) fn give_back(model: &str, id: &str, held: Held) {
    CURRENT.with(|c| {
        if let Some(scope) = c.borrow_mut().as_mut() {
            scope.offer = Some(Offer {
                model: model.to_string(),
                id: id.to_string(),
                held,
            });
        }
    });
}

/// The action committed the unit it took up: the edits of the update are stored with its move.
pub(crate) fn stored() {
    CURRENT.with(|c| {
        if let Some(scope) = c.borrow_mut().as_mut() {
            scope.stored = true;
        }
    });
}

/// The commit of the unit the action of the record `model` `id` took up failed with `why`. The
/// edits of the update went with the unit, so the update ends with this refusal ([`run`]) and
/// the action is not run again in this call ([`adopt`]).
pub(crate) fn lost(model: &str, id: &str, why: &str) {
    CURRENT.with(|c| {
        if let Some(scope) = c.borrow_mut().as_mut() {
            scope.lost = Some(Lost {
                model: model.to_string(),
                id: id.to_string(),
                why: why.to_string(),
            });
        }
    });
}
