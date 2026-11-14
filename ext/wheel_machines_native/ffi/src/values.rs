//! `Due` and `Drain` values: validated through the core at construction, and
//! built from what a drain answered. Both classes are resolved once, at init,
//! and pinned.
use magnus::{Error, RArray, RClass, RModule, Ruby, Symbol, Value, prelude::*};
use matryona_machines::ruby::{Class, Registry, Symbols, exact_array, exact_fields, integer};
use wheel_machines::{Drain, Due, Status};

/// `Due` and `Drain`, as `lib/wheel_machines.rb` defined them before the
/// extension loaded, and `Handle`, which init defines.
pub struct Classes {
    pub handle: Class,
    due: Class,
    drain: Class,
}

static CLASSES: Registry<Classes> = Registry::new("wheel_machines");

pub fn resolve(ruby: &Ruby, module: RModule, handle: RClass) -> Result<(), Error> {
    let classes = Classes {
        handle: Class::pin(ruby, handle),
        due: Class::resolve(ruby, module, "Due")?,
        drain: Class::resolve(ruby, module, "Drain")?,
    };
    CLASSES.set(ruby, classes).map(drop)
}

impl Classes {
    /// The gem's own `Data` classes, resolved at init.
    pub fn get(ruby: &Ruby) -> Result<&'static Self, Error> {
        CLASSES.get(ruby)
    }

    /// Build a `WheelMachines::Drain` holding `WheelMachines::Due` values.
    pub fn report(&self, ruby: &Ruby, drain: Drain, due: &[Due<u64>]) -> Result<Value, Error> {
        let (due_class, drain_class) = (self.due.get(ruby), self.drain.get(ruby));
        let rows = ruby.ary_new_capa(due.len());
        for entry in due {
            let row: Value = due_class.funcall("new", (entry.id, entry.deadline))?;
            rows.push(row)?;
        }
        rows.freeze();
        let status = Statuses::symbol(ruby, drain.status());
        drain_class.funcall("new", (rows, drain.cascaded(), status))
    }
}

matryona_machines::symbols! {
    /// Why a drain stopped, as the Symbol `Drain#status` holds.
    Statuses for Status as "status" {
        CaughtUp => "caught_up",
        OutputFull => "output_full",
        BudgetSpent => "budget_spent",
    }
}

/// What `Due.new` stores: an id and a deadline the core's `Due::new` holds.
pub fn checked_due(id: Value, deadline: Value) -> Result<(u64, u64), Error> {
    let due = Due::new(integer(id, "id")?, integer(deadline, "deadline")?);
    Ok((due.id, due.deadline))
}

/// What `Drain.new` stores: a frozen Array of `WheelMachines::Due`, a cascade
/// count and a status symbol the core's `Drain::new` accepts.
pub fn checked_drain(
    ruby: &Ruby,
    due: Value,
    cascaded: Value,
    status: Value,
) -> Result<(RArray, usize, Symbol), Error> {
    let cascaded = integer(cascaded, "cascaded")?;
    // Read natively from exactly an Array: a subclass is a `TypeError`.
    let rows = exact_array(ruby, due, NOT_DUE)?;
    let due_class = Classes::get(ruby)?.due;
    let copy = ruby.ary_new_capa(rows.len());
    for row in rows {
        copy.push(due_row(ruby, row, due_class)?)?;
    }
    copy.freeze();
    let status = Statuses::parse(ruby, status)?;
    let symbol = Statuses::symbol(ruby, status);
    Drain::new(copy.len(), cascaded, status)
        .map(|_| (copy, cascaded, symbol))
        .ok_or_else(|| {
            Error::new(
                ruby.exception_range_error(),
                "emitted plus cascaded is past usize: no drain charges that much work",
            )
        })
}

const NOT_DUE: &str = "due must be an Array of WheelMachines::Due";

/// `row` when it is exactly a `WheelMachines::Due` (a subclass's constant
/// leaks past the seal) whose fields the core's `Due::new` holds: read
/// straight from the struct, since `Marshal.load` skips `initialize`.
/// `TypeError` or `RangeError` otherwise.
fn due_row(ruby: &Ruby, row: Value, due_class: Class) -> Result<Value, Error> {
    let [id, deadline] = exact_fields(ruby, row, due_class.get(ruby), NOT_DUE)?;
    checked_due(id, deadline)?;
    Ok(row)
}
