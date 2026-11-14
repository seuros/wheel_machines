//! Thin binding to the bounded hierarchical timing wheel.
mod init;
mod values;

use std::cell::RefCell;

use magnus::{Error, Integer, Ruby, TryConvert, Value, prelude::*, typed_data::Obj};
use wheel_machines::{Drain, Due, Handle, Refusal};

use crate::values::Classes;

const CAPACITY: usize = 1024;

type Core = wheel_machines::Wheel<u64, CAPACITY>;

#[derive(Clone, Copy)]
pub(crate) struct Number(pub(crate) u64);

impl Number {
    /// The count as a `usize`.
    ///
    /// # Errors
    ///
    /// `RangeError` above `usize::MAX`: a count is never silently capped.
    pub(crate) fn count(self, ruby: &Ruby, what: &'static str) -> Result<usize, Error> {
        usize::try_from(self.0).map_err(|_| {
            Error::new(
                ruby.exception_range_error(),
                format!("{what} is past usize ({})", usize::MAX),
            )
        })
    }
}

/// `TypeError` unless `value` is an Integer; the range is the caller's check.
fn integer(value: Value, expected: &'static str) -> Result<Value, Error> {
    if Integer::from_value(value).is_none() {
        return Err(Error::new(
            Ruby::get_with(value).exception_type_error(),
            expected,
        ));
    }
    Ok(value)
}

impl TryConvert for Number {
    fn try_convert(value: Value) -> Result<Self, Error> {
        u64::try_convert(integer(value, "expected an Integer (unsigned 64-bit)")?).map(Self)
    }
}

/// An issuer scope: an Integer in `0..=65535`.
#[derive(Clone, Copy)]
struct Scope(u16);

impl TryConvert for Scope {
    fn try_convert(value: Value) -> Result<Self, Error> {
        u16::try_convert(integer(value, "expected an Integer scope (0..=65535)")?).map(Self)
    }
}

/// A refusal as the Ruby symbol it answers.
fn refused(ruby: &Ruby, refused: Refusal) -> Value {
    ruby.to_symbol(refusal(refused)).as_value()
}

const fn refusal(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::Elapsed => "elapsed",
        Refusal::Full => "full",
        Refusal::Exhausted => "exhausted",
        Refusal::Regressed { .. } => "regressed",
        Refusal::Stale => "stale",
    }
}

/// `WheelMachines::Handle`: the core's `Handle`. It carries the scope of the
/// wheel that issued it; the core refuses it anywhere else.
#[magnus::wrap(class = "WheelMachines::Handle", free_immediately, size)]
struct Issued(Handle);

impl Issued {
    const fn scope(&self) -> u16 {
        self.0.scope()
    }
}

/// Drain `core` into a buffer no larger than what can possibly be due.
fn drain_core(core: &mut Core, budget: usize, limit: usize) -> (Drain, Vec<Due<u64>>) {
    let mut due = vec![Due::default(); limit.min(core.len())];
    let drain = core.drain(budget, &mut due);
    due.truncate(drain.emitted());
    (drain, due)
}

#[magnus::wrap(class = "WheelMachines::Wheel", free_immediately, size)]
struct Wheel {
    core: RefCell<Box<Core>>,
}

impl Wheel {
    fn new() -> Self {
        Self::build(Core::new())
    }

    fn with_scope(scope: Scope) -> Self {
        Self::build(Core::with_scope(scope.0))
    }

    fn starting_at(scope: Scope, now: Number) -> Self {
        Self::build(Core::starting_at(scope.0, now.0))
    }

    fn resumed(ruby: &Ruby, scope: Scope, issued: Number, now: Number) -> Result<Self, Error> {
        Core::resumed(scope.0, issued.0, now.0)
            .map(Self::build)
            .ok_or_else(|| {
                Error::new(
                    ruby.exception_range_error(),
                    "issued is past the last sequence of a scope (2**48 - 1)",
                )
            })
    }

    fn build(core: Core) -> Self {
        Self {
            core: RefCell::new(Box::new(core)),
        }
    }

    fn scope(&self) -> u16 {
        self.core.borrow().scope()
    }

    fn issued(&self) -> u64 {
        self.core.borrow().issued()
    }

    fn register(
        ruby: &Ruby,
        this: Obj<Self>,
        id: Number,
        deadline: Number,
    ) -> Result<Value, Error> {
        this.check_frozen()?;
        let outcome = this.core.borrow_mut().register(id.0, deadline.0);
        Ok(match outcome {
            Ok(handle) => ruby.obj_wrap(Issued(handle)).as_value(),
            Err(refusal) => refused(ruby, refusal),
        })
    }

    fn cancel(ruby: &Ruby, this: Obj<Self>, issued: Obj<Issued>) -> Result<Value, Error> {
        this.check_frozen()?;
        let outcome = this.core.borrow_mut().cancel(issued.0);
        Ok(match outcome {
            Ok(id) => ruby.integer_from_u64(id).as_value(),
            Err(refusal) => refused(ruby, refusal),
        })
    }

    fn deadline(&self, issued: Obj<Issued>) -> Option<u64> {
        self.core.borrow().deadline(issued.0)
    }

    fn advance(ruby: &Ruby, this: Obj<Self>, now: Number) -> Result<Value, Error> {
        this.check_frozen()?;
        let outcome = this.core.borrow_mut().advance(now.0);
        Ok(ruby
            .to_symbol(outcome.map_or_else(refusal, |()| "advanced"))
            .as_value())
    }

    fn drain(ruby: &Ruby, this: Obj<Self>, budget: Number, limit: Number) -> Result<Value, Error> {
        this.check_frozen()?;
        // Everything that can raise happens before the core drains.
        let (classes, budget, limit) = Self::prepare(ruby, budget, limit)?;
        Self::on_copy(ruby, this, |core| {
            let (drain, due) = drain_core(core, budget, limit);
            classes.report(ruby, drain, &due)
        })
    }

    fn tick(
        ruby: &Ruby,
        this: Obj<Self>,
        now: Number,
        budget: Number,
        limit: Number,
    ) -> Result<Value, Error> {
        this.check_frozen()?;
        // Everything that can raise happens before the core moves.
        let (classes, budget, limit) = Self::prepare(ruby, budget, limit)?;
        Self::on_copy(ruby, this, |core| {
            if let Err(refusal) = core.advance(now.0) {
                return Ok(refused(ruby, refusal));
            }
            let (drain, due) = drain_core(core, budget, limit);
            classes.report(ruby, drain, &due)
        })
    }

    /// The raising half of `drain` and `tick`: the classes resolved at init
    /// and the count conversions, done before the core is touched.
    fn prepare(
        ruby: &Ruby,
        budget: Number,
        limit: Number,
    ) -> Result<(Classes, usize, usize), Error> {
        Ok((
            Classes::get(ruby)?,
            budget.count(ruby, "budget")?,
            limit.count(ruby, "limit")?,
        ))
    }

    /// Runs `act`, which builds the Ruby answer, on a copy of the wheel and
    /// keeps the copy only once the answer exists: a raise while building
    /// leaves every due entry in the wheel. Right before the store the
    /// receiver is checked again: frozen meanwhile raises `FrozenError`,
    /// changed meanwhile (a redefined `new` calling back in) raises
    /// `RuntimeError`, and in both cases nothing is applied. No borrow is held
    /// while `act` runs.
    fn on_copy(
        ruby: &Ruby,
        this: Obj<Self>,
        act: impl FnOnce(&mut Core) -> Result<Value, Error>,
    ) -> Result<Value, Error> {
        this.check_frozen()?;
        let before = this.core.borrow().clone();
        let mut after = before.clone();
        let answer = act(&mut after)?;
        this.check_frozen()?;
        let mut held = this.core.borrow_mut();
        if *held != before {
            return Err(Error::new(
                ruby.exception_runtime_error(),
                "the wheel changed while the answer was built; the call was not applied",
            ));
        }
        *held = after;
        Ok(answer)
    }

    fn now(&self) -> u64 {
        self.core.borrow().now()
    }

    fn size(&self) -> usize {
        self.core.borrow().len()
    }

    fn is_empty(&self) -> bool {
        self.core.borrow().is_empty()
    }

    fn next_wake(&self) -> Option<u64> {
        self.core.borrow().next_wake()
    }

    const fn capacity() -> usize {
        CAPACITY
    }
}
