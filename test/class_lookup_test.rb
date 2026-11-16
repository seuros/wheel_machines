# frozen_string_literal: true

require 'minitest/autorun'
require 'open3'
require 'rbconfig'

# The classes of the values the extension wraps (Wheel, Handle) are fixed when
# it loads. Nothing a caller does later, redefining a constant or
# `Object.const_get`, reaches a value being built. Each case runs in a fresh
# process: the class is fixed once per process, at load.
class ClassLookupTest < Minitest::Test
  LIB = File.expand_path('../lib', __dir__)

  PRELUDE = <<~RUBY
    require 'wheel_machines'
    W = WheelMachines
  RUBY

  # Runs `body` after PRELUDE in a new Ruby; answers its last line of output.
  def isolated(body)
    out, err, status = Open3.capture3(RbConfig.ruby, '-I', LIB, '-e', PRELUDE + body)
    assert status.success?, "exited #{status.exitstatus}: #{err}"
    out.lines.last&.chomp
  end

  # Bug: a Handle constant redefined before the first register made the
  # handle an instance of the new class, so #cancel raised TypeError and the
  # timer could never be cancelled.
  def test_a_redefined_handle_constant_does_not_change_the_handle_class
    assert_equal 'true 7 0', isolated(<<~RUBY)
      wheel = W::Wheel.new
      original = W::Handle
      W.send(:remove_const, :Handle)
      W.const_set(:Handle, Class.new)
      handle = wheel.register(7, 100)
      puts [handle.instance_of?(original), wheel.cancel(handle), wheel.size].join(' ')
    RUBY
  end

  # A Wheel built after its constant was redefined is still a Wheel.
  def test_a_redefined_wheel_constant_does_not_change_the_wheel_class
    assert_equal 'true 1', isolated(<<~RUBY)
      original = W::Wheel
      W.send(:remove_const, :Wheel)
      W.const_set(:Wheel, Class.new)
      wheel = original.new
      wheel.register(7, 100)
      puts [wheel.instance_of?(original), wheel.size].join(' ')
    RUBY
  end

  # Bug: building the first value ran `Object.const_get`, so a redefined one
  # could freeze the wheel after the core registered the timer, and the
  # registration was kept. No Ruby runs between the checks and the core call.
  def test_building_a_value_runs_no_const_get
    assert_equal '0 false 1 Wheel Handle', isolated(<<~RUBY)
      hits = 0
      wheel = nil
      Object.define_singleton_method(:const_get) do |*args|
        hits += 1
        wheel&.freeze
        super(*args)
      end
      wheel = W::Wheel.new
      handle = wheel.register(7, 100)
      puts [hits, wheel.frozen?, wheel.size, wheel.class.name.split('::').last,
            handle.class.name.split('::').last].join(' ')
    RUBY
  end

  # Bug: a redefined `Object.const_get` that raised hit an `unwrap` in the
  # extension: an uncatchable fatal, and the process exited.
  def test_a_raising_const_get_does_not_kill_the_process
    assert_equal '7', isolated(<<~RUBY)
      Object.define_singleton_method(:const_get) { |*| raise IOError, 'lookup ran mid-call' }
      wheel = W::Wheel.new
      puts wheel.cancel(wheel.register(7, 100))
    RUBY
  end
end
