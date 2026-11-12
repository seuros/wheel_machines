//! Private native entry point.
use crate::{CAPACITY, Wheel};
use magnus::{Error, Ruby, function, method, prelude::*};
use wheel_machines::MAX_CASCADES;

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("WheelMachines")?;
    module.const_set("CAPACITY", CAPACITY)?;
    module.const_set("MAX_CASCADES", MAX_CASCADES)?;
    let timer = module.define_class("Timer", ruby.class_object())?;
    timer.undef_default_alloc_func();
    let class = module.define_class("Wheel", ruby.class_object())?;
    class.undef_default_alloc_func();
    class.define_singleton_method("new", function!(Wheel::new, 0))?;
    class.define_singleton_method("starting_at", function!(Wheel::starting_at, 1))?;
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
