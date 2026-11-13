# frozen_string_literal: true
require 'minitest/autorun'
require 'wheel_machines'

class WheelTest < Minitest::Test
  MAX = 2**64 - 1

  def ids(drain) = drain.due.map(&:id)

  def test_due_ids_in_deadline_then_registration_order
    wheel = WheelMachines::Wheel.new
    [[1, 70], [2, 5], [3, 70], [4, 4096], [5, 5]].each do |id, deadline|
      assert_instance_of WheelMachines::Handle, wheel.register(id, deadline)
    end
    assert_equal 5, wheel.size
    refute_predicate wheel, :empty?
    assert_equal 5, wheel.next_wake
    assert_equal :advanced, wheel.advance(70)
    assert_equal 70, wheel.now
    drain = wheel.drain(64, 64)
    assert_equal [2, 5, 1, 3], ids(drain)
    assert_equal [5, 5, 70, 70], drain.due.map(&:deadline)
    assert_equal :caught_up, drain.status
    assert_predicate drain, :caught_up?
    assert_equal 4096, wheel.next_wake
    assert_equal [4], ids(wheel.tick(4096, 64, 64))
    assert_predicate wheel, :empty?
    assert_nil wheel.next_wake
  end

  def test_drain_and_due_are_frozen_data
    wheel = WheelMachines::Wheel.new
    wheel.register(9, 3)
    drain = wheel.tick(3, 8, 8)
    assert_operator WheelMachines::Drain, :<, Data
    assert_operator WheelMachines::Due, :<, Data
    assert_equal WheelMachines::Due.new(id: 9, deadline: 3), drain.due.first
    assert_predicate drain, :frozen?
    assert_predicate drain.due, :frozen?
    assert_equal 1, drain.work
    assert_equal 1, drain.emitted
    assert_equal 0, drain.cascaded
    assert_equal %i[due cascaded status], WheelMachines::Drain.members
    assert_equal [0, 0], [wheel.drain(8, 8).emitted, wheel.drain(8, 8).work]
  end

  def test_cancel_deadline_and_stale_handles
    wheel = WheelMachines::Wheel.new
    handle = wheel.register(7, 100)
    assert_equal 100, wheel.deadline(handle)
    assert_equal 7, wheel.cancel(handle)
    assert_equal :stale, wheel.cancel(handle)
    assert_nil wheel.deadline(handle)
    replacement = wheel.register(8, 50)
    assert_equal :stale, wheel.cancel(handle)
    assert_equal [1, 50], [wheel.size, wheel.deadline(replacement)]
    drained = wheel.register(9, 10)
    assert_equal [9], ids(wheel.tick(10, 8, 8))
    assert_equal :stale, wheel.cancel(drained)
    assert_equal 1, wheel.size
    assert_equal 8, wheel.cancel(replacement)
  end

  def test_opaque_owner_lifetime_and_gc
    a = WheelMachines::Wheel.new
    b = WheelMachines::Wheel.new
    handle = a.register(1, 10)
    b.register(1, 10)
    assert_equal :stale, b.cancel(handle)
    assert_nil b.deadline(handle)
    assert_equal 1, b.size
    assert_raises(TypeError) { WheelMachines::Handle.allocate }
    assert_raises(TypeError) { WheelMachines::Wheel.allocate }
    assert_raises(TypeError) { handle.dup }
    a = nil
    GC.start
    assert_equal :stale, WheelMachines::Wheel.new.cancel(handle)
  end

  def test_capacity_refuses_without_evicting
    wheel = WheelMachines::Wheel.new
    assert_equal 1024, WheelMachines::CAPACITY
    assert_equal WheelMachines::CAPACITY, wheel.capacity
    handles = WheelMachines::CAPACITY.times.map { |i| wheel.register(i, 100 + i) }
    assert_equal :full, wheel.register(5000, 100)
    assert_equal WheelMachines::CAPACITY, wheel.size
    assert_equal 0, wheel.cancel(handles.first)
    assert_instance_of WheelMachines::Handle, wheel.register(5000, 100)
    assert_equal :stale, wheel.cancel(handles.first)
    assert_equal WheelMachines::CAPACITY, wheel.size
  end

  def test_time_only_moves_forward_and_elapsed_deadlines_are_refused
    wheel = WheelMachines::Wheel.starting_at(100)
    assert_equal 100, wheel.now
    assert_equal :elapsed, wheel.register(1, 100)
    assert_equal :elapsed, wheel.register(1, 0)
    wheel.register(1, 101)
    assert_equal :regressed, wheel.advance(99)
    assert_equal :regressed, wheel.tick(99, 8, 8)
    assert_equal 100, wheel.now
    assert_equal 1, wheel.size
    assert_equal :advanced, wheel.advance(100)
    assert_predicate wheel.drain(8, 8), :caught_up?
    assert_equal 100, wheel.now
  end

  def test_budget_and_limit_make_draining_resumable
    wheel = WheelMachines::Wheel.new
    5.times { |i| wheel.register(i, 9) }
    wheel.advance(9)
    blocked = wheel.drain(8, 0)
    assert_equal [[], :output_full], [blocked.due, blocked.status]
    assert_equal :budget_spent, wheel.drain(0, 8).status
    first = wheel.drain(8, 2)
    assert_equal [[0, 1], :output_full], [ids(first), first.status]
    second = wheel.drain(1, 8)
    assert_equal [[2], :budget_spent], [ids(second), second.status]
    assert_equal 9, wheel.next_wake
    last = wheel.drain(MAX, MAX)
    assert_equal [[3, 4], :caught_up], [ids(last), last.status]
  end

  def test_cascades_are_charged
    assert_equal 10, WheelMachines::MAX_CASCADES
    wheel = WheelMachines::Wheel.new
    wheel.register(1, MAX)
    steps = []
    until (drain = wheel.tick(MAX, 3, 8)).caught_up?
      assert_equal :budget_spent, drain.status
      assert_equal 3, drain.work
      steps << drain.cascaded
    end
    assert_equal [3, 3, 3], steps
    assert_equal [1, [1]], [drain.cascaded, ids(drain)]
    assert_equal :elapsed, wheel.register(2, MAX)
  end

  def test_strict_inputs_before_mutation
    wheel = WheelMachines::Wheel.new
    bad = [1.0, Float::INFINITY, Float::NAN, nil, '1', Object.new]
    bad.each do |value|
      assert_raises(TypeError) { wheel.register(value, 10) }
      assert_raises(TypeError) { wheel.register(1, value) }
      assert_raises(TypeError) { wheel.advance(value) }
      assert_raises(TypeError) { wheel.drain(value, 1) }
      assert_raises(TypeError) { wheel.drain(1, value) }
      assert_raises(TypeError) { wheel.tick(value, 1, 1) }
      assert_raises(TypeError) { wheel.tick(1, 1, value) }
      assert_raises(TypeError) { wheel.cancel(value) }
      assert_raises(TypeError) { wheel.deadline(value) }
      assert_raises(TypeError) { WheelMachines::Wheel.starting_at(value) }
    end
    [-1, 2**64].each do |n|
      assert_raises(RangeError) { wheel.register(n, 10) }
      assert_raises(RangeError) { wheel.register(1, n) }
      assert_raises(RangeError) { wheel.advance(n) }
      assert_raises(RangeError) { wheel.tick(1, n, 1) }
    end
    assert_equal [0, 0], [wheel.now, wheel.size]
  end

  def test_freeze_guards
    wheel = WheelMachines::Wheel.new
    handle = wheel.register(1, 10)
    wheel.freeze
    assert_raises(FrozenError) { wheel.register(2, 10) }
    assert_raises(FrozenError) { wheel.cancel(handle) }
    assert_raises(FrozenError) { wheel.advance(1) }
    assert_raises(FrozenError) { wheel.drain(1, 1) }
    assert_raises(FrozenError) { wheel.tick(1, 1, 1) }
    assert_equal [1, 0, 10, 10], [wheel.size, wheel.now, wheel.deadline(handle), wheel.next_wake]
  end

  def test_gc_stress_at_the_u64_boundary
    previous = GC.stress
    GC.stress = true
    wheel = WheelMachines::Wheel.starting_at(MAX - 1)
    wheel.register(MAX, MAX)
    drain = wheel.tick(MAX, MAX, MAX)
    GC.compact
    assert_equal [WheelMachines::Due.new(MAX, MAX)], drain.due
  ensure
    GC.stress = previous
  end
end
