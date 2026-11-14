//! Private native entry point.
use crate::{
    CAPACITY, Issued, Wheel,
    values::{checked_drain, checked_due},
};
use magnus::{Error, Ruby, function, method, prelude::*};
use wheel_machines::MAX_CASCADES;

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("WheelMachines")?;
    crate::values::resolve(ruby, module)?;
    module.const_set("CAPACITY", CAPACITY)?;
    module.const_set("MAX_CASCADES", MAX_CASCADES)?;
    module.define_singleton_method("__due", function!(checked_due, 2))?;
    module.define_singleton_method("__drain", function!(checked_drain, 3))?;
    let handle = module.define_class("Handle", ruby.class_object())?;
    handle.undef_default_alloc_func();
    handle.define_method("scope", method!(Issued::scope, 0))?;
    let class = module.define_class("Wheel", ruby.class_object())?;
    class.undef_default_alloc_func();
    class.define_singleton_method("new", function!(Wheel::new, 0))?;
    class.define_singleton_method("with_scope", function!(Wheel::with_scope, 1))?;
    class.define_singleton_method("starting_at", function!(Wheel::starting_at, 2))?;
    class.define_singleton_method("resumed", function!(Wheel::resumed, 3))?;
    class.define_method("scope", method!(Wheel::scope, 0))?;
    class.define_method("issued", method!(Wheel::issued, 0))?;
    class.define_method("register", method!(Wheel::register, 2))?;
    class.define_method("cancel", method!(Wheel::cancel, 1))?;
    class.define_method("deadline", method!(Wheel::deadline, 1))?;
    class.define_method("advance", method!(Wheel::advance, 1))?;
    class.define_method("drain", method!(Wheel::drain, 2))?;
    class.define_method("tick", method!(Wheel::tick, 3))?;
    class.define_method("now", method!(Wheel::now, 0))?;
    class.define_method("size", method!(Wheel::size, 0))?;
    class.define_method("empty?", method!(Wheel::is_empty, 0))?;
    class.define_method("next_wake", method!(Wheel::next_wake, 0))?;
    class.define_method("capacity", function!(Wheel::capacity, 0))?;
    Ok(())
}
