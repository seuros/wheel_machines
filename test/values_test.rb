# frozen_string_literal: true
require 'minitest/autorun'
require 'wheel_machines'

class ValuesTest < Minitest::Test
  MAX = 2**64 - 1

  def due(id = 1, deadline = 2) = WheelMachines::Due.new(id, deadline)

  def test_due_holds_any_u64_pair_and_nothing_else
    assert_equal [0, MAX], [due(0, MAX).id, due(0, MAX).deadline]
    assert_equal due(3, 4), WheelMachines::Due.new(id: 3, deadline: 4)
    [1.0, nil, '1', Object.new].each do |value|
      assert_raises(TypeError) { due(value, 1) }
      assert_raises(TypeError) { due(1, value) }
    end
    [-1, 2**64].each do |value|
      assert_raises(RangeError) { due(value, 1) }
      assert_raises(RangeError) { due(1, value) }
    end
    assert_raises(ArgumentError) { WheelMachines::Due.new(1) }
  end

  def test_drain_is_validated_by_the_core
    rows = [due]
    assert_operator WheelMachines::Drain, :<, Data
    assert_operator WheelMachines::Due, :<, Data
    assert_equal %i[due cascaded status], WheelMachines::Drain.members
    drain = WheelMachines::Drain.new(rows, 2, :budget_spent)
    assert_equal [1, 2, 3, :budget_spent], [drain.emitted, drain.cascaded, drain.work, drain.status]
    assert_predicate drain.due, :frozen?
    rows << due(5, 6)
    assert_equal 1, drain.emitted, 'a frozen copy, not the caller\'s Array'
    assert_equal drain, WheelMachines::Drain.new(due: [due], cascaded: 2, status: :budget_spent)

    assert_raises(TypeError) { WheelMachines::Drain.new(nil, 0, :caught_up) }
    assert_raises(TypeError) { WheelMachines::Drain.new([[1, 2]], 0, :caught_up) }
    assert_raises(TypeError) { WheelMachines::Drain.new([], 0, 'caught_up') }
    assert_raises(TypeError) { WheelMachines::Drain.new([], 1.0, :caught_up) }
    assert_raises(RangeError) { WheelMachines::Drain.new([], 0, :nope) }
    assert_raises(RangeError) { WheelMachines::Drain.new([], -1, :caught_up) }
    assert_raises(RangeError) { WheelMachines::Drain.new([], 2**64, :caught_up) }
    # One emitted entry plus the most work a usize holds is past what any drain charges.
    assert_raises(RangeError) { WheelMachines::Drain.new([due], MAX, :caught_up) }
    assert_equal MAX, WheelMachines::Drain.new([], MAX, :caught_up).work
  end

  def test_validation_hooks_are_private
    assert_raises(NoMethodError) { WheelMachines.__due(1, 2) }
    assert_raises(NoMethodError) { WheelMachines.__drain([], 0, :caught_up) }
  end

  # The answer to `drain` and `tick` is built from a copy of the wheel kept
  # only once the `Drain` exists: a build that raises loses no due entry, one
  # that calls back in raises RuntimeError, one that freezes the wheel raises
  # FrozenError, and in each case nothing is applied.
  def test_drain_and_tick_lose_nothing_when_the_answer_cannot_be_built
    wheel = WheelMachines::Wheel.new
    wheel.register(7, 5)
    redefining(WheelMachines::Drain, ->(*) { raise IOError }) do
      assert_raises(IOError) { wheel.tick(5, 8, 8) }
      assert_equal [0, 1], [wheel.now, wheel.size]
      wheel.advance(5)
      assert_raises(IOError) { wheel.drain(8, 8) }
      assert_equal 1, wheel.size
    end
    reentrant = ->(original, *args) { wheel.register(9, 9).then { original.call(*args) } }
    redefining(WheelMachines::Due, reentrant) { assert_raises(RuntimeError) { wheel.drain(8, 8) } }
    assert_equal 2, wheel.size
    freezing = ->(original, *args) { wheel.freeze.then { original.call(*args) } }
    redefining(WheelMachines::Drain, freezing) { assert_raises(FrozenError) { wheel.drain(8, 8) } }
    assert_equal [5, 2], [wheel.now, wheel.size]
  end

  # Sealing stops accidents, not reflection: Ruby binds a named subclass's
  # constant before `inherited` raises, so the class leaks. Both forms raise.
  def test_values_are_sealed_and_a_named_subclass_leaks_its_constant
    [WheelMachines::Due, WheelMachines::Drain].each do |value|
      assert_raises(TypeError) { Class.new(value) }
      assert_raises(NoMethodError) { value.allocate }
    end
    assert_raises(TypeError) { self.class.module_eval('class LeakedDue < WheelMachines::Due; end', __FILE__, __LINE__) }
    assert self.class.const_defined?(:LeakedDue, false), 'the constant is bound before inherited raises'
  end

  # A forged Due never gets into a Drain: a leaked subclass's instance is a
  # TypeError, a Marshal frame edited past the check a RangeError.
  def test_a_forged_due_is_refused_at_the_boundary
    assert_raises(TypeError) do
      self.class.module_eval('class ForgedDue < WheelMachines::Due; end', __FILE__, __LINE__)
    end
    ForgedDue.class_eval do
      def initialize(**fields) = Data.instance_method(:initialize).bind_call(self, **fields)
    end
    forged = ForgedDue.new(id: -1, deadline: 2)
    edited = Marshal.load(Marshal.dump(due(5, 2)).sub("i\x0A".b, "i\xFA".b))
    assert_equal(-1, edited.id)
    assert_raises(TypeError) { WheelMachines::Drain.new([forged], 0, :caught_up) }
    assert_raises(RangeError) { WheelMachines::Drain.new([edited], 0, :caught_up) }
  end

  # The classes are resolved at init and pinned, so a compaction moves none
  # of them: every path that builds or checks a value still works after it.
  def test_every_value_path_survives_gc_compact
    skip 'GC.compact is not available' unless GC.respond_to?(:compact)
    GC.compact
    wheel = WheelMachines::Wheel.new
    wheel.register(7, 5)
    wheel.register(8, 9)
    assert_equal [7], wheel.tick(5, 8, 8).due.map(&:id)
    assert_equal [8], wheel.drain(8, 8).due.map(&:id) if wheel.advance(9) == :advanced
    assert_equal 2, WheelMachines::Drain.new([due, due(3, 4)], 0, :caught_up).emitted
  end

  private

  def redefining(klass, body)
    singleton = klass.singleton_class
    singleton.alias_method(:__original_new, :new)
    singleton.remove_method(:new)
    original = klass.method(:__original_new)
    klass.define_singleton_method(:new) { |*args| body.call(original, *args) }
    yield
  ensure
    singleton.remove_method(:new)
    singleton.alias_method(:new, :__original_new)
    singleton.remove_method(:__original_new)
  end
end
