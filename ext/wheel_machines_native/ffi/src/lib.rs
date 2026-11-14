//! Thin binding to the bounded hierarchical timing wheel.
mod init;
mod values;

use std::cell::RefCell;

use magnus::{Error, Ruby, Value, prelude::*, typed_data::Obj};
use matryona_machines::ruby::{exact_typed, integer, keep, symbol};
use wheel_machines::{Drain, Due, Handle, Refusal};

use crate::values::Classes;

const CAPACITY: usize = 1024;

type Core = wheel_machines::Wheel<u64, CAPACITY>;

/// A refusal as the Ruby symbol it answers.
fn refused(ruby: &Ruby, refused: Refusal) -> Value {
    symbol(ruby, refusal(refused)).as_value()
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

    /// The core handle `value` wraps: exactly a `WheelMachines::Handle`, a
    /// subclass instance refused with `TypeError`.
    fn from(ruby: &Ruby, value: Value) -> Result<Handle, Error> {
        let class = Classes::get(ruby)?.handle.get(ruby);
        exact_typed::<Self>(ruby, value, class, "expected a WheelMachines::Handle")
            .map(|issued| issued.0)
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

    fn with_scope(scope: Value) -> Result<Self, Error> {
        Ok(Self::build(Core::with_scope(integer(scope, "scope")?)))
    }

    fn starting_at(scope: Value, now: Value) -> Result<Self, Error> {
        let (scope, now) = (integer(scope, "scope")?, integer(now, "now")?);
        Ok(Self::build(Core::starting_at(scope, now)))
    }

    fn resumed(ruby: &Ruby, scope: Value, issued: Value, now: Value) -> Result<Self, Error> {
        let scope = integer(scope, "scope")?;
        let (issued, now) = (integer(issued, "issued")?, integer(now, "now")?);
        Core::resumed(scope, issued, now)
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

    fn register(ruby: &Ruby, this: Obj<Self>, id: Value, deadline: Value) -> Result<Value, Error> {
        this.check_frozen()?;
        let (id, deadline) = (integer(id, "id")?, integer(deadline, "deadline")?);
        let outcome = this.core.borrow_mut().register(id, deadline);
        Ok(match outcome {
            Ok(handle) => ruby.obj_wrap(Issued(handle)).as_value(),
            Err(refusal) => refused(ruby, refusal),
        })
    }

    fn cancel(ruby: &Ruby, this: Obj<Self>, issued: Value) -> Result<Value, Error> {
        this.check_frozen()?;
        let handle = Issued::from(ruby, issued)?;
        let outcome = this.core.borrow_mut().cancel(handle);
        Ok(match outcome {
            Ok(id) => ruby.integer_from_u64(id).as_value(),
            Err(refusal) => refused(ruby, refusal),
        })
    }

    fn deadline(ruby: &Ruby, this: &Self, issued: Value) -> Result<Option<u64>, Error> {
        let handle = Issued::from(ruby, issued)?;
        Ok(this.core.borrow().deadline(handle))
    }

    fn advance(ruby: &Ruby, this: Obj<Self>, now: Value) -> Result<Value, Error> {
        this.check_frozen()?;
        let now = integer(now, "now")?;
        let outcome = this.core.borrow_mut().advance(now);
        Ok(symbol(ruby, outcome.map_or_else(refusal, |()| "advanced")).as_value())
    }

    /// The answer is built from a copy of the wheel kept only once it exists
    /// (see [`keep`]): a raise while building leaves every due entry in it.
    fn drain(ruby: &Ruby, this: Obj<Self>, budget: Value, limit: Value) -> Result<Value, Error> {
        this.check_frozen()?;
        let (budget, limit) = (integer(budget, "budget")?, integer(limit, "limit")?);
        // Everything that can raise happens before the core drains.
        let classes = Classes::get(ruby)?;
        keep(
            ruby,
            this,
            &this.core,
            |core| drain_core(core, budget, limit),
            |(drain, due)| classes.report(ruby, drain, &due),
        )
    }

    fn tick(
        ruby: &Ruby,
        this: Obj<Self>,
        now: Value,
        budget: Value,
        limit: Value,
    ) -> Result<Value, Error> {
        this.check_frozen()?;
        let now = integer(now, "now")?;
        let (budget, limit) = (integer(budget, "budget")?, integer(limit, "limit")?);
        // Everything that can raise happens before the core moves.
        let classes = Classes::get(ruby)?;
        keep(
            ruby,
            this,
            &this.core,
            |core| core.advance(now).map(|()| drain_core(core, budget, limit)),
            |ticked| match ticked {
                Ok((drain, due)) => classes.report(ruby, drain, &due),
                Err(refusal) => Ok(refused(ruby, refusal)),
            },
        )
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
