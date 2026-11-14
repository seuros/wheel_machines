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
    assert_equal [4, 2], [drain.emitted, drain.cascaded]
    assert_equal WheelMachines::Due.new(id: 2, deadline: 5), drain.due.first
    assert_predicate drain, :frozen?
    assert_equal :caught_up, drain.status
    assert_predicate drain, :caught_up?
    assert_equal 4096, wheel.next_wake
    assert_equal [4], ids(wheel.tick(4096, 64, 64))
    assert_predicate wheel, :empty?
    assert_nil wheel.next_wake
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

  # The review's sequence: a handle from one wheel cancelled an unrelated
  # registration in another. In different scopes each refuses the other's.
  def test_handle_from_another_scope_is_refused_and_changes_nothing
    a = WheelMachines::Wheel.with_scope(1)
    b = WheelMachines::Wheel.with_scope(2)
    from_a = a.register(1, 10)
    from_b = b.register(2, 10)
    assert_equal [1, 2, 1, 2], [a.scope, b.scope, from_a.scope, from_b.scope]
    assert_equal :stale, b.cancel(from_a)
    assert_nil b.deadline(from_a)
    assert_equal :stale, a.cancel(from_b)
    assert_nil a.deadline(from_b)
    assert_equal [1, 1, 10, 10], [a.size, b.size, a.deadline(from_a), b.deadline(from_b)]
    assert_equal [1, 2], [a.cancel(from_a), b.cancel(from_b)]
    # The core cannot pick a scope: two wheels left in scope zero issue the
    # same handles, and one honours the other's.
    c = WheelMachines::Wheel.new
    d = WheelMachines::Wheel.new
    handle = c.register(1, 10)
    d.register(2, 10)
    assert_equal [0, 0, 0], [c.scope, d.scope, handle.scope]
    assert_equal 2, d.cancel(handle)
    assert_predicate d, :empty?
  end

  def test_scoped_constructors
    wheel = WheelMachines::Wheel.starting_at(65_535, 100)
    assert_equal [65_535, 100, 0], [wheel.scope, wheel.now, wheel.issued]
    assert_equal 65_535, wheel.register(1, 101).scope
    assert_equal 1, wheel.issued
    plain = WheelMachines::Wheel.new
    assert_equal [0, 0, 0], [plain.scope, plain.now, plain.issued]
    assert_equal [9, 0], [WheelMachines::Wheel.with_scope(9).scope, WheelMachines::Wheel.with_scope(9).now]
    [1.0, nil, '1', Object.new].each do |value|
      assert_raises(TypeError) { WheelMachines::Wheel.with_scope(value) }
      assert_raises(TypeError) { WheelMachines::Wheel.starting_at(value, 0) }
      assert_raises(TypeError) { WheelMachines::Wheel.starting_at(0, value) }
      assert_raises(TypeError) { WheelMachines::Wheel.resumed(value, 0, 0) }
      assert_raises(TypeError) { WheelMachines::Wheel.resumed(0, value, 0) }
      assert_raises(TypeError) { WheelMachines::Wheel.resumed(0, 0, value) }
    end
    [-1, 65_536, 2**64].each do |scope|
      assert_raises(RangeError) { WheelMachines::Wheel.with_scope(scope) }
      assert_raises(RangeError) { WheelMachines::Wheel.starting_at(scope, 0) }
      assert_raises(RangeError) { WheelMachines::Wheel.resumed(scope, 0, 0) }
    end
    assert_raises(RangeError) { WheelMachines::Wheel.starting_at(0, -1) }
    assert_raises(ArgumentError) { WheelMachines::Wheel.starting_at(7) }
  end

  # A fresh wheel counts from one again, so a handle that outlived a restart
  # would name a new registration. A resumed one continues the scope.
  def test_resumed_wheel_continues_its_scope_and_refuses_earlier_handles
    first = WheelMachines::Wheel.starting_at(5, 100)
    old = 3.times.map { |i| first.register(i, 200).tap { |handle| first.cancel(handle) } }
    old << first.register(9, 300)
    assert_equal 4, first.issued

    wheel = WheelMachines::Wheel.resumed(first.scope, first.issued, first.now)
    assert_equal [5, 4, 100, 0], [wheel.scope, wheel.issued, wheel.now, wheel.size]
    assert_nil wheel.next_wake
    fresh = [wheel.register(7, 150), wheel.register(8, 150)]
    assert_equal [5, 5, 6], [fresh.first.scope, fresh.last.scope, wheel.issued]
    old.each do |handle|
      assert_equal :stale, wheel.cancel(handle)
      assert_nil wheel.deadline(handle)
    end
    assert_equal [2, 150, 150], [wheel.size, *fresh.map { |handle| wheel.deadline(handle) }]
    assert_equal [7, 8], fresh.map { |handle| wheel.cancel(handle) }
  end

  def test_resumed_at_the_last_sequence_is_spent_and_past_it_raises
    last = 2**48 - 1
    wheel = WheelMachines::Wheel.resumed(3, last - 1, 0)
    assert_equal :elapsed, wheel.register(1, 0)
    assert_equal last - 1, wheel.issued
    handle = wheel.register(1, 10)
    assert_equal [3, last], [handle.scope, wheel.issued]
    assert_equal :exhausted, wheel.register(2, 10)
    assert_equal [1, last], [wheel.size, wheel.issued]
    spent = WheelMachines::Wheel.resumed(3, last, 0)
    assert_equal :exhausted, spent.register(1, 10)
    assert_equal [0, last], [spent.size, spent.issued]
    assert_raises(RangeError) { WheelMachines::Wheel.resumed(3, last + 1, 0) }
    assert_raises(RangeError) { WheelMachines::Wheel.resumed(3, -1, 0) }
  end

  def test_opaque_handles_outlive_their_wheel
    a = WheelMachines::Wheel.with_scope(1)
    handle = a.register(1, 10)
    assert_raises(TypeError) { WheelMachines::Handle.allocate }
    assert_raises(TypeError) { WheelMachines::Wheel.allocate }
    assert_raises(TypeError) { handle.dup }
    a = nil
    GC.start
    assert_equal 1, handle.scope
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
    wheel = WheelMachines::Wheel.starting_at(0, 100)
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
    wheel = WheelMachines::Wheel.starting_at(0, MAX - 1)
    wheel.register(MAX, MAX)
    drain = wheel.tick(MAX, MAX, MAX)
    GC.compact
    assert_equal [WheelMachines::Due.new(MAX, MAX)], drain.due
  ensure
    GC.stress = previous
  end
end
