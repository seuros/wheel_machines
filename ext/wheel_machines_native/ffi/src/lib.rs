//! Thin binding to the bounded hierarchical timing wheel.
mod init;

use std::{cell::RefCell, sync::Arc};

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

const fn refusal(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::Elapsed => "elapsed",
        Refusal::Full => "full",
        Refusal::Exhausted => "exhausted",
        Refusal::Regressed { .. } => "regressed",
        Refusal::Stale => "stale",
    }
}

/// `WheelMachines::Handle`: the core's `Handle`, tied to the wheel that issued it.
#[magnus::wrap(class = "WheelMachines::Handle", free_immediately, size)]
struct Issued {
    owner: Arc<()>,
    handle: Handle,
}

#[magnus::wrap(class = "WheelMachines::Wheel", free_immediately, size)]
struct Wheel {
    owner: Arc<()>,
    core: RefCell<Box<Core>>,
}

impl Wheel {
    fn new() -> Self {
        Self::starting_at(Number(0))
    }

    fn starting_at(now: Number) -> Self {
        Self {
            owner: Arc::new(()),
            core: RefCell::new(Box::new(Core::starting_at(now.0))),
        }
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
            Ok(handle) => ruby
                .obj_wrap(Issued {
                    owner: Arc::clone(&this.owner),
                    handle,
                })
                .as_value(),
            Err(refused) => ruby.to_symbol(refusal(refused)).as_value(),
        })
    }

    /// A handle issued by another wheel is stale here, whatever its slot says.
    fn cancel(ruby: &Ruby, this: Obj<Self>, issued: Obj<Issued>) -> Result<Value, Error> {
        this.check_frozen()?;
        let outcome = if Arc::ptr_eq(&this.owner, &issued.owner) {
            this.core.borrow_mut().cancel(issued.handle)
        } else {
            Err(Refusal::Stale)
        };
        Ok(match outcome {
            Ok(id) => ruby.integer_from_u64(id).as_value(),
            Err(refused) => ruby.to_symbol(refusal(refused)).as_value(),
        })
    }

    fn deadline(&self, issued: Obj<Issued>) -> Option<u64> {
        if !Arc::ptr_eq(&self.owner, &issued.owner) {
            return None;
        }
        self.core.borrow().deadline(issued.handle)
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
