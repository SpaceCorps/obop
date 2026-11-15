//! The actions an `update` implies (D48), run with the update as one unit of work.
//!
//! An `update` that changes a governed field is an implicit run of the declared action for that
//! change. The other fields of the call are edits of the record as it was, so the generated
//! `update` checks and writes them into its unit first and then, without committing, offers the
//! unit to each action in turn ([`run`]). The generic handler of an action takes it up
//! ([`adopt`]), runs on the record as edited, and hands the unit on to the action after it; the
//! last one commits the edits and every action's writes in one storage transaction. A refusal
//! of any part drops the unit: nothing is stored, no number of a sequence is taken, and the
//! caller gets that refusal.
//!
//! Only an action that is generic all the way joins the unit ([`Step::joins`]): its handler is
//! the generic one and no module has put a wrapper around it. Code a module wrote by hand (a
//! wrapper, a handler of its own) reads and writes the store directly. It would judge the record
//! without the edits the unit holds, the action would not see what the wrapper wrote, and what
//! it stores before it answers (a move, money returned, D62) could not be taken back when a
//! later part refused. So such an action is run by itself, as a call of the action runs it, and
//! an update that would mix it with an edit or with another action is refused by the generated
//! `update` before anything is written: the caller sends the move alone and the rest in another
//! call.
//!
//! A thread-local carries the steps and the unit, as the write context, the fault scope and the
//! language of a request are carried: the handlers between the dispatcher and the engine take a
//! `&Value` and need no new argument. A scope belongs to one run of one command
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
    /// `model.command` of the action and the governed field whose change implies it, for a
    /// refusal that names them.
    pub command: String,
    pub field: String,
    /// The action goes on in the unit of the update: its handler is the generic one and it has
    /// no wrapper. Any other action is run by itself.
    pub joins: bool,
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
    /// The action that runs now is followed by another: it hands the unit on.
    hand_on: bool,
    /// The scope of a question, not of a run ([`ask`]): the generated `update` says whether
    /// the call is mixed and stops before it writes.
    asking: bool,
    /// What the update answered to the question.
    answer: Option<String>,
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
/// `update`, which runs them inside its unit of work ([`run`]) or, for an action that does not
/// join a unit, by itself.
pub(crate) fn take_steps() -> Vec<Step> {
    CURRENT.with(|c| {
        c.borrow_mut()
            .as_mut()
            .map(|s| std::mem::take(&mut s.steps))
            .unwrap_or_default()
    })
}

/// Ask the generated `update` whether a call with these actions is one it refuses as mixed
/// ([`not_mixed`]). `update` calls its handler, which reads the record, decides and stops:
/// nothing is written and no action runs. The dispatcher asks before it stores the plan of an
/// agent: a plan for a call that can only be refused would be refused when a person approves
/// it, and spent by that. `None` also when the handler was refused before it could say (what
/// refused it refuses the run too).
pub(crate) fn ask(steps: Vec<Step>, update: impl FnOnce()) -> Option<String> {
    let scope = Scope {
        steps,
        asking: true,
        ..Scope::default()
    };
    let _open = Open {
        previous: CURRENT.with(|c| c.borrow_mut().replace(scope)),
        _not_send: PhantomData,
    };
    update();
    CURRENT.with(|c| c.borrow_mut().as_mut().and_then(|s| s.answer.take()))
}

/// For the generated `update`, with what [`not_mixed`] said: true when the dispatcher only
/// asks ([`ask`]). The answer is kept for it, and the update stops here.
pub(crate) fn answer(mixed: &Option<String>) -> bool {
    CURRENT.with(|c| match c.borrow_mut().as_mut() {
        Some(s) if s.asking => {
            s.answer.clone_from(mixed);
            true
        }
        _ => false,
    })
}

/// The refusal of an update whose handler a module wrote by hand and that moves a governed
/// field. The handler knows nothing of the action and writes the store itself, so the two
/// could not be refused as a whole: the caller calls the action by its name.
pub(crate) fn by_hand(update: &str, steps: &[Step]) -> Option<String> {
    let step = steps.first()?;
    Some(format!(
        "`{field}` cannot be changed by {update}: the change runs {command}, which is not part \
         of this update's unit of work, so the call could not be refused as a whole. Call \
         {command}",
        field = step.field,
        command = step.command,
    ))
}

/// The refusal of an update that would mix an action that is run by itself with an edit or with
/// another action. `None` when every action joins the unit, or when the one action is all the
/// update does.
pub(crate) fn not_mixed(steps: &[Step], edited: bool) -> Option<String> {
    let alone = steps.iter().find(|s| !s.joins)?;
    if !edited && steps.len() == 1 {
        return None;
    }
    Some(format!(
        "`{field}` cannot be changed together with other fields in one update: the change runs \
         {command}, which is not part of the update's unit of work, so the call could not be \
         refused as a whole. Send `{field}` by itself and the other changes in a separate call",
        field = alone.field,
        command = alone.command,
    ))
}

/// Run the actions an update of the record `model` `id` implies, each with the unit `held` on
/// offer; every one of them joins the unit ([`not_mixed`]). Returns what no action committed
/// (for the update to commit) and what each action answered. At the first refusal the unit is
/// dropped and the refusal returned.
pub(crate) fn run(
    steps: &[Step],
    model: &str,
    id: &str,
    held: Held,
) -> R<(Option<Held>, Vec<Value>)> {
    debug_assert!(
        steps.iter().all(|s| s.joins),
        "only joining actions share a unit"
    );
    let mut held = Some(held);
    let mut answers = Vec::with_capacity(steps.len());
    for (i, step) in steps.iter().enumerate() {
        // Nothing of the scope is borrowed while the action runs: it reads the scope itself.
        held = CURRENT.with(|c| match c.borrow_mut().as_mut() {
            Some(s) => {
                s.hand_on = i + 1 < steps.len();
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
/// action follows, or as it came, when the action was refused.
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
