//! `Due` and `Drain` values: validated through the core at construction, and
//! built from what a drain answered. Both classes are resolved once, at init,
//! and pinned.
use std::sync::OnceLock;

use magnus::rb_sys::AsRawValue;
use magnus::value::Opaque;
use magnus::{
    Error, RArray, RClass, RModule, RStruct, Ruby, Symbol, TryConvert, Value, prelude::*,
};
use wheel_machines::{Drain, Due, Status};

use crate::Number;

/// `Due` and `Drain`, as `lib/wheel_machines.rb` defined them before the
/// extension loaded.
static CLASSES: OnceLock<[Opaque<RClass>; 2]> = OnceLock::new();

pub fn resolve(ruby: &Ruby, module: RModule) -> Result<(), Error> {
    let due: RClass = module.const_get("Due")?;
    let drain: RClass = module.const_get("Drain")?;
    // Held in a static, so pinned: `GC.compact` must not move them.
    ruby.gc_register_mark_object(due);
    ruby.gc_register_mark_object(drain);
    // A second load keeps the first classes: they are the same constants.
    let _ = CLASSES.get_or_init(|| [due.into(), drain.into()]);
    Ok(())
}

/// The gem's own `Data` classes, resolved at init.
#[derive(Clone, Copy)]
pub struct Classes {
    due: RClass,
    drain: RClass,
}

impl Classes {
    pub fn get(ruby: &Ruby) -> Result<Self, Error> {
        let [due, drain] = CLASSES.get().ok_or_else(|| {
            Error::new(
                ruby.exception_load_error(),
                "the wheel_machines extension is not initialised",
            )
        })?;
        Ok(Self {
            due: ruby.get_inner(*due),
            drain: ruby.get_inner(*drain),
        })
    }

    /// Build a `WheelMachines::Drain` holding `WheelMachines::Due` values.
    pub fn report(&self, ruby: &Ruby, drain: Drain, due: &[Due<u64>]) -> Result<Value, Error> {
        let rows = ruby.ary_new_capa(due.len());
        for entry in due {
            let row: Value = self.due.funcall("new", (entry.id, entry.deadline))?;
            rows.push(row)?;
        }
        rows.freeze();
        let status = ruby.to_symbol(status_name(drain.status()));
        self.drain.funcall("new", (rows, drain.cascaded(), status))
    }
}

const fn status_name(status: Status) -> &'static str {
    match status {
        Status::CaughtUp => "caught_up",
        Status::OutputFull => "output_full",
        Status::BudgetSpent => "budget_spent",
    }
}

/// What `Due.new` stores: an id and a deadline the core's `Due::new` holds.
pub const fn checked_due(id: Number, deadline: Number) -> (u64, u64) {
    let due = Due::new(id.0, deadline.0);
    (due.id, due.deadline)
}

/// What `Drain.new` stores: a frozen Array of `WheelMachines::Due`, a cascade
/// count and a status symbol the core's `Drain::new` accepts.
pub fn checked_drain(
    ruby: &Ruby,
    due: Value,
    cascaded: Number,
    status: Value,
) -> Result<(RArray, usize, Symbol), Error> {
    const NOT_DUE: &str = "due must be an Array of WheelMachines::Due";
    let rows = RArray::from_value(due).ok_or_else(|| type_error(ruby, NOT_DUE))?;
    let due_class = Classes::get(ruby)?.due;
    let copy = ruby.ary_new_capa(rows.len());
    for row in rows {
        copy.push(due_row(ruby, row, due_class)?)?;
    }
    copy.freeze();
    let cascaded = cascaded.count(ruby, "cascaded")?;
    let status = status_from(ruby, status)?;
    let symbol = ruby.to_symbol(status_name(status));
    Drain::new(copy.len(), cascaded, status)
        .map(|_| (copy, cascaded, symbol))
        .ok_or_else(|| {
            Error::new(
                ruby.exception_range_error(),
                "emitted plus cascaded is past usize: no drain charges that much work",
            )
        })
}

/// `row` when it is exactly a `WheelMachines::Due` (a subclass's constant
/// leaks past the seal, so the classes are compared by identity) whose fields
/// the core's `Due::new` holds: read straight from the struct, since
/// `Marshal.load` skips `initialize`. `TypeError` or `RangeError` otherwise.
fn due_row(ruby: &Ruby, row: Value, due_class: RClass) -> Result<Value, Error> {
    if row.class().as_raw() != due_class.as_raw() {
        return Err(type_error(
            ruby,
            "due must be an Array of WheelMachines::Due",
        ));
    }
    let fields = RStruct::try_convert(row)?;
    let id = Number::try_convert(fields.aref::<_, Value>(0)?)?;
    let deadline = Number::try_convert(fields.aref::<_, Value>(1)?)?;
    checked_due(id, deadline);
    Ok(row)
}

fn status_from(ruby: &Ruby, value: Value) -> Result<Status, Error> {
    let Some(symbol) = Symbol::from_value(value) else {
        return Err(type_error(ruby, "status must be a Symbol"));
    };
    match symbol.name()?.as_ref() {
        "caught_up" => Ok(Status::CaughtUp),
        "output_full" => Ok(Status::OutputFull),
        "budget_spent" => Ok(Status::BudgetSpent),
        _ => Err(Error::new(
            ruby.exception_range_error(),
            "status must be :caught_up, :output_full or :budget_spent",
        )),
    }
}

fn type_error(ruby: &Ruby, message: &'static str) -> Error {
    Error::new(ruby.exception_type_error(), message)
}
