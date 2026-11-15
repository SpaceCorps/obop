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
//! with a wrapper, and the last action of the call, commit the unit before they answer; an action
//! without one that is followed by another hands the unit on, and the last one commits for all.
//! An action whose handler was written by hand writes the store itself and cannot take the unit
//! up: it runs after every check of the edits has passed, and the `update` commits the edits
//! when it has answered (two transactions; a refusal of either still leaves nothing stored).
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
    /// The action has a wrapper, so its handler commits before it answers.
    pub wrapped: bool,
}

/// The unit of an update on offer to the action that runs now: for the record `model` `id`.
struct Offer {
    model: String,
    id: String,
    held: Held,
}

#[derive(Default)]
struct Scope {
    /// The actions of this call, until the handler of the update takes them.
    steps: Vec<Step>,
    offer: Option<Offer>,
    /// The action that runs now is followed by another and has no wrapper: it hands the unit on.
    hand_on: bool,
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

/// Run the actions an update of the record `model` `id` implies, each with the unit `held` on
/// offer. Returns what no action committed (for the update to commit) and what each action
/// answered. At the first refusal the unit is dropped and the refusal returned.
pub(crate) fn run(
    steps: &[Step],
    model: &str,
    id: &str,
    held: Held,
) -> R<(Option<Held>, Vec<Value>)> {
    let mut held = Some(held);
    let mut answers = Vec::with_capacity(steps.len());
    for (i, step) in steps.iter().enumerate() {
        let hand_on = i + 1 < steps.len() && !step.wrapped;
        // Nothing of the scope is borrowed while the action runs: it reads the scope itself.
        held = CURRENT.with(|c| match c.borrow_mut().as_mut() {
            Some(s) => {
                s.hand_on = hand_on;
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
        if let Some(back) = CURRENT.with(|c| {
            c.borrow_mut().as_mut().and_then(|s| {
                s.hand_on = false;
                s.offer.take().map(|o| o.held)
            })
        }) {
            held = Some(back);
        }
        answers.push(answer?);
    }
    Ok((held, answers))
}

/// A unit taken up by an action, and what the action does with it when it is done.
pub(crate) struct Adopted {
    pub held: Held,
    /// Hand the unit on ([`give_back`]) and do not commit it: another action follows.
    pub hand_on: bool,
}

/// For the generic handler of an action on the record `model` `id`: the unit of the update that
/// implies it, when there is one on offer for that record. The action goes on in that unit.
pub(crate) fn adopt(model: &str, id: &str) -> Option<Adopted> {
    CURRENT.with(|c| {
        let mut current = c.borrow_mut();
        let scope = current.as_mut()?;
        if !scope
            .offer
            .as_ref()
            .is_some_and(|o| o.model == model && o.id == id)
        {
            return None;
        }
        let held = scope.offer.take()?.held;
        Some(Adopted {
            held,
            hand_on: scope.hand_on,
        })
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
