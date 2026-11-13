//! Thin binding to the bounded hierarchical timing wheel.
mod init;

use std::cell::RefCell;

use magnus::{Error, Integer, Ruby, TryConvert, Value, prelude::*, typed_data::Obj};
use wheel_machines::{Drain, Due, Handle, Refusal, Status};

const CAPACITY: usize = 1024;

type Core = wheel_machines::Wheel<u64, CAPACITY>;

#[derive(Clone, Copy)]
struct Number(u64);

impl Number {
    /// Counts above the address space behave as "no limit".
    fn count(self) -> usize {
        usize::try_from(self.0).unwrap_or(usize::MAX)
    }
}

impl TryConvert for Number {
    fn try_convert(value: Value) -> Result<Self, Error> {
        if Integer::from_value(value).is_none() {
            return Err(Error::new(
                Ruby::get_with(value).exception_type_error(),
                "expected an Integer (unsigned 64-bit)",
            ));
        }
        u64::try_convert(value).map(Self)
    }
}

/// An issuer scope: an Integer in `0..=65535`.
#[derive(Clone, Copy)]
struct Scope(u16);

impl TryConvert for Scope {
    fn try_convert(value: Value) -> Result<Self, Error> {
        if Integer::from_value(value).is_none() {
            return Err(Error::new(
                Ruby::get_with(value).exception_type_error(),
                "expected an Integer scope (0..=65535)",
            ));
        }
        u16::try_convert(value).map(Self)
    }
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
            Err(refused) => ruby.to_symbol(refusal(refused)).as_value(),
        })
    }

    fn cancel(ruby: &Ruby, this: Obj<Self>, issued: Obj<Issued>) -> Result<Value, Error> {
        this.check_frozen()?;
        let outcome = this.core.borrow_mut().cancel(issued.0);
        Ok(match outcome {
            Ok(id) => ruby.integer_from_u64(id).as_value(),
            Err(refused) => ruby.to_symbol(refusal(refused)).as_value(),
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
        let (drain, due) = this.drain_core(budget, limit);
        report(ruby, drain, &due)
    }

    fn tick(
        ruby: &Ruby,
        this: Obj<Self>,
        now: Number,
        budget: Number,
        limit: Number,
    ) -> Result<Value, Error> {
        this.check_frozen()?;
        if let Err(refused) = this.core.borrow_mut().advance(now.0) {
            return Ok(ruby.to_symbol(refusal(refused)).as_value());
        }
        let (drain, due) = this.drain_core(budget, limit);
        report(ruby, drain, &due)
    }

    /// Drain into a buffer no larger than what can possibly be due, releasing
    /// the borrow before any Ruby object is built.
    fn drain_core(&self, budget: Number, limit: Number) -> (Drain, Vec<Due<u64>>) {
        let mut core = self.core.borrow_mut();
        let mut due = vec![Due::default(); limit.count().min(core.len())];
        let drain = core.drain(budget.count(), &mut due);
        due.truncate(drain.emitted);
        (drain, due)
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

/// Build a `WheelMachines::Drain` holding `WheelMachines::Due` values.
fn report(ruby: &Ruby, drain: Drain, due: &[Due<u64>]) -> Result<Value, Error> {
    let module = ruby.define_module("WheelMachines")?;
    let due_class: Value = module.const_get("Due")?;
    let drain_class: Value = module.const_get("Drain")?;
    let rows = ruby.ary_new_capa(due.len());
    for entry in due {
        let row: Value = due_class.funcall("new", (entry.id, entry.deadline))?;
        rows.push(row)?;
    }
    rows.freeze();
    let status = ruby.to_symbol(match drain.status {
        Status::CaughtUp => "caught_up",
        Status::OutputFull => "output_full",
        Status::BudgetSpent => "budget_spent",
    });
    drain_class.funcall("new", (rows, drain.cascaded, status))
}
