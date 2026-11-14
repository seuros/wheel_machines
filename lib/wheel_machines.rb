# frozen_string_literal: true

require_relative 'wheel_machines/version'

module WheelMachines
  # One due registration: the caller's id and the tick it was registered for.
  # Like the core's `Due::new`, both are Integers in 0..2**64-1: TypeError for
  # a non-Integer, RangeError outside the range.
  Due = Data.define(:id, :deadline) do
    def initialize(id:, deadline:)
      id, deadline = WheelMachines.__send__(:__due, id, deadline)
      super
    end
  end

  # What one drain did: the due entries in order, the cascade moves charged, and
  # why it stopped (:caught_up, :output_full or :budget_spent). The core writes
  # the entries to a caller's buffer and reports `emitted`; here `due` is that
  # buffer, cut to the entries written, stored as a frozen copy.
  #
  # Validated by the core's `Drain::new`: `due` must be an Array of `Due` and
  # `status` a Symbol (TypeError otherwise); an unknown status, a negative
  # `cascaded`, or work past the core's `usize` raises RangeError.
  Drain = Data.define(:due, :cascaded, :status) do
    def initialize(due:, cascaded:, status:)
      due, cascaded, status = WheelMachines.__send__(:__drain, due, cascaded, status)
      super
    end

    # Due entries handed out, the core's `emitted`.
    def emitted = due.size

    # Work units charged against the budget.
    def work = emitted + cascaded

    def caught_up? = status == :caught_up
  end
end

# Native is required: no algorithm fallback. It validates and builds the Due
# and Drain values above, so those classes must exist first.
require 'wheel_machines_native/wheel_machines_native'

module WheelMachines
  private_class_method :__due, :__drain

  # The values are sealed. `initialize` is where each validates, and a
  # subclass could override it, or `allocate` build an instance that never ran
  # it: subclassing raises TypeError and `allocate` is gone. Sealing stops
  # accidents, not reflection (a named subclass's constant is bound before
  # `inherited` raises; `Marshal.load` skips `initialize`), so the native side
  # is the authority: it takes a `Due` only of exactly that class and checks
  # its fields again.
  [Due, Drain].each do |value|
    value.singleton_class.undef_method(:allocate)
    value.define_singleton_method(:inherited) { |_| raise TypeError, "#{value} cannot be subclassed" }
  end
end
