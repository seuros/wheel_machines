# frozen_string_literal: true

require_relative 'wheel_machines/version'

module WheelMachines
  # One due registration: the caller's id and the tick it was registered for.
  Due = Data.define(:id, :deadline)

  # What one drain did: the due entries in order, the cascade moves charged, and
  # why it stopped (:caught_up, :output_full or :budget_spent).
  Drain = Data.define(:due, :cascaded, :status) do
    # Work units charged against the budget.
    def work = due.size + cascaded

    def caught_up? = status == :caught_up
  end
end

# Native is required: no algorithm fallback. It builds Due and Drain values, so
# the Data classes above must exist first.
require 'wheel_machines_native/wheel_machines_native'
